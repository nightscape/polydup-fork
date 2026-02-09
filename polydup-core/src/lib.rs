//! PolyDup Core - Cross-language duplicate code detection engine
//!
//! This library provides the core functionality for detecting duplicate code
//! across Node.js, Python, and Rust codebases using Tree-sitter parsing,
//! Rabin-Karp/MinHash algorithms, and parallel processing.

mod cache;
mod directives;
mod error;
mod hashing;
mod ignore_rules;
mod parsing;
mod queries;

#[cfg(test)]
mod proptest_fuzzing;

#[cfg(test)]
mod snapshot_tests;

// Re-export public types
pub use cache::{CacheStats, CodeLocation, FileCacheMetadata, HashCache};
pub use directives::{detect_directives, detect_directives_in_file, Directive, FileDirectives};
pub use error::{PolyDupError, Result};
pub use hashing::{
    compute_rolling_hashes, compute_token_edit_distance, compute_token_similarity,
    compute_window_hash, detect_duplicates_with_extension, detect_type3_clones, extend_match,
    normalize, normalize_with_line_numbers, verify_cross_window_match, CloneMatch, RollingHash,
    Token,
};
pub use ignore_rules::{
    compute_duplicate_id, compute_symmetric_duplicate_id, FileRange, IgnoreEntry, IgnoreManager,
};
pub use parsing::{
    extract_functions, extract_javascript_functions, extract_python_functions,
    extract_rust_functions, FunctionNode,
};

use anyhow::Context;
use globset::GlobSet;
use ignore::WalkBuilder;
use once_cell::sync::OnceCell;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tree_sitter::Language;

/// Information about a supported programming language
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LanguageInfo {
    /// Display name of the language
    pub name: &'static str,
    /// File extensions supported (without leading dot)
    pub extensions: &'static [&'static str],
    /// Tree-sitter parser name
    pub parser: &'static str,
    /// Whether Type-3 clone detection is supported
    pub type3_support: bool,
    /// Current support status
    pub status: LanguageStatus,
}

/// Support status for a language
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LanguageStatus {
    /// Full support with dedicated Tree-sitter parser
    Full,
    /// Partial support (e.g., uses another language's parser)
    Partial,
    /// Planned but not yet implemented
    Planned,
}

/// Returns information about all supported programming languages
pub fn get_supported_languages() -> Vec<LanguageInfo> {
    vec![
        LanguageInfo {
            name: "Rust",
            extensions: &["rs"],
            parser: "tree-sitter-rust",
            type3_support: true,
            status: LanguageStatus::Full,
        },
        LanguageInfo {
            name: "Python",
            extensions: &["py", "pyi"],
            parser: "tree-sitter-python",
            type3_support: true,
            status: LanguageStatus::Full,
        },
        LanguageInfo {
            name: "JavaScript",
            extensions: &["js", "mjs", "cjs"],
            parser: "tree-sitter-javascript",
            type3_support: true,
            status: LanguageStatus::Full,
        },
        LanguageInfo {
            name: "TypeScript",
            extensions: &["ts", "mts", "cts"],
            parser: "tree-sitter-javascript",
            type3_support: true,
            status: LanguageStatus::Full,
        },
        LanguageInfo {
            name: "JSX",
            extensions: &["jsx"],
            parser: "tree-sitter-javascript",
            type3_support: true,
            status: LanguageStatus::Full,
        },
        LanguageInfo {
            name: "TSX",
            extensions: &["tsx"],
            parser: "tree-sitter-javascript",
            type3_support: true,
            status: LanguageStatus::Full,
        },
        LanguageInfo {
            name: "Vue",
            extensions: &["vue"],
            parser: "tree-sitter-javascript",
            type3_support: true,
            status: LanguageStatus::Partial,
        },
        LanguageInfo {
            name: "Svelte",
            extensions: &["svelte"],
            parser: "tree-sitter-javascript",
            type3_support: true,
            status: LanguageStatus::Partial,
        },
        LanguageInfo {
            name: "Go",
            extensions: &["go"],
            parser: "tree-sitter-go",
            type3_support: true,
            status: LanguageStatus::Planned,
        },
        LanguageInfo {
            name: "Java",
            extensions: &["java"],
            parser: "tree-sitter-java",
            type3_support: true,
            status: LanguageStatus::Planned,
        },
        LanguageInfo {
            name: "C/C++",
            extensions: &["c", "cc", "cpp", "cxx", "h", "hpp"],
            parser: "tree-sitter-cpp",
            type3_support: true,
            status: LanguageStatus::Planned,
        },
    ]
}

/// Clone type classification
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum CloneType {
    /// Type-1: Exact copies (only whitespace/comments differ)
    #[serde(rename = "type-1")]
    Type1,
    /// Type-2: Structurally identical but renamed identifiers/literals
    #[serde(rename = "type-2")]
    Type2,
    /// Type-3: Near-miss clones with modifications (not yet implemented)
    #[serde(rename = "type-3")]
    Type3,
}

/// Helper function to check if two ranges overlap
fn ranges_overlap(start1: usize, end1: usize, start2: usize, end2: usize) -> bool {
    start1 < end2 && start2 < end1
}

// Stable key for deduplicating matches within the same file pair.
fn canonical_pair_key<'a>(
    func1: &'a FunctionHash,
    func2: &'a FunctionHash,
    source_start: usize,
    target_start: usize,
    length: usize,
) -> (&'a str, &'a str, usize, usize, usize, usize, usize) {
    if func1.file_path.as_ref() < func2.file_path.as_ref() {
        (
            func1.file_path.as_ref(),
            func2.file_path.as_ref(),
            func1.start_line,
            func2.start_line,
            source_start,
            target_start,
            length,
        )
    } else {
        (
            func2.file_path.as_ref(),
            func1.file_path.as_ref(),
            func2.start_line,
            func1.start_line,
            target_start,
            source_start,
            length,
        )
    }
}

/// Represents a detected duplicate code fragment
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DuplicateMatch {
    pub file1: String,
    pub file2: String,
    pub start_line1: usize,
    pub start_line2: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_line1: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_line2: Option<usize>,
    pub length: usize,
    pub similarity: f64,
    pub hash: u64,
    pub clone_type: CloneType,
    /// Edit distance (Type-3 only). None for Type-1/2
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edit_distance: Option<usize>,
    /// Indicates if this duplicate is suppressed by an inline directive
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suppressed_by_directive: Option<bool>,
    /// Token offset within function for file1 (used for ignore ID computation)
    #[serde(skip)]
    token_offset1: Option<usize>,
    /// Token offset within function for file2 (used for ignore ID computation)
    #[serde(skip)]
    token_offset2: Option<usize>,
    /// Token length of the second window (Type-3 may differ from `length`)
    #[serde(skip)]
    target_length: Option<usize>,
    /// Content-based ID for this duplicate (SHA256 of normalized tokens)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duplicate_id: Option<String>,
}

/// Represents a function with its tokens for duplicate detection
#[derive(Debug, Clone)]
struct FunctionHash {
    file_path: Arc<str>, // Shared ownership, cheap to clone
    #[allow(dead_code)] // Kept for potential future reporting improvements
    function_name: Option<String>,
    #[allow(dead_code)] // Kept for byte-level analysis in future
    start_byte: usize,
    #[allow(dead_code)] // Kept for byte-level analysis in future
    end_byte: usize,
    start_line: usize,
    #[allow(dead_code)] // Kept for future detailed reporting
    end_line: usize,
    tokens: Vec<Token>, // Normalized token sequence
    /// Zero-based line offset for each token relative to start_line
    token_line_offsets: Vec<usize>,
    raw_body: String, // Original (unnormalized) function body for Type-1 detection
}

/// Baseline snapshot for comparing duplicate detection across runs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Baseline {
    /// Version of the baseline format
    pub version: String,
    /// Timestamp when baseline was created
    pub created_at: String,
    /// Duplicates that existed at baseline time
    pub duplicates: Vec<DuplicateMatch>,
}

impl Baseline {
    /// Create a new baseline from scan results
    pub fn from_duplicates(duplicates: Vec<DuplicateMatch>) -> Self {
        Self {
            version: env!("CARGO_PKG_VERSION").to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
            duplicates,
        }
    }

    /// Save baseline to a JSON file
    pub fn save_to_file(&self, path: &Path) -> Result<()> {
        let json =
            serde_json::to_string_pretty(self).context("Failed to serialize baseline to JSON")?;
        fs::write(path, json).context("Failed to write baseline file")?;
        Ok(())
    }

    /// Load baseline from a JSON file
    pub fn load_from_file(path: &Path) -> Result<Self> {
        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read baseline file: {}", path.display()))?;
        let baseline: Baseline =
            serde_json::from_str(&content).context("Failed to parse baseline JSON")?;
        Ok(baseline)
    }

    /// Compare current duplicates against baseline and return only new ones
    pub fn find_new_duplicates(&self, current: &[DuplicateMatch]) -> Vec<DuplicateMatch> {
        let baseline_set: std::collections::HashSet<_> =
            self.duplicates.iter().map(duplicate_key).collect();

        current
            .iter()
            .filter(|dup| !baseline_set.contains(&duplicate_key(dup)))
            .cloned()
            .collect()
    }
}

/// Create a unique key for a duplicate match for comparison
fn duplicate_key(dup: &DuplicateMatch) -> (String, String, usize, usize, usize) {
    // Normalize file order for consistent comparison
    let (file1, file2, line1, line2) = if dup.file1 < dup.file2 {
        (
            dup.file1.clone(),
            dup.file2.clone(),
            dup.start_line1,
            dup.start_line2,
        )
    } else {
        (
            dup.file2.clone(),
            dup.file1.clone(),
            dup.start_line2,
            dup.start_line1,
        )
    };
    (file1, file2, line1, line2, dup.length)
}

/// A file that was skipped during scanning
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkippedFile {
    /// Path to the skipped file
    pub path: String,
    /// Reason the file was skipped
    pub reason: String,
}

/// Report containing scan results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Report {
    /// PolyDup version
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Scan start time (ISO 8601)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scan_time: Option<String>,
    /// Configuration used for the scan
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<ScanConfig>,
    /// Total number of files scanned
    pub files_scanned: usize,
    /// Total number of functions analyzed
    pub functions_analyzed: usize,
    /// Detected duplicate matches
    pub duplicates: Vec<DuplicateMatch>,
    /// Files that were skipped during scanning (parse errors, permission issues, etc.)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skipped_files: Vec<SkippedFile>,
    /// Scan statistics
    pub stats: ScanStats,
}

/// Configuration used for scanning
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanConfig {
    /// Minimum block size in tokens
    pub threshold: usize,
    /// Similarity threshold (0.0 - 1.0)
    pub similarity: f64,
    /// Type-3 detection enabled
    pub type3_enabled: bool,
    /// Paths scanned
    #[serde(skip_serializing_if = "Option::is_none")]
    pub paths: Option<Vec<String>>,
}

/// Helper for serde skip_serializing_if
fn is_zero(n: &usize) -> bool {
    *n == 0
}

/// Statistics from the scanning process
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanStats {
    /// Total lines of code scanned
    pub total_lines: usize,
    /// Total tokens processed
    pub total_tokens: usize,
    /// Number of unique hashes computed
    pub unique_hashes: usize,
    /// Scan duration in milliseconds
    pub duration_ms: u64,
    /// Number of duplicates suppressed by .polydup-ignore file
    #[serde(default, skip_serializing_if = "is_zero")]
    pub suppressed_by_ignore_file: usize,
    /// Number of duplicates suppressed by inline directives
    #[serde(default, skip_serializing_if = "is_zero")]
    pub suppressed_by_directive: usize,
}

/// Main scanner for detecting duplicates
#[allow(dead_code)] // similarity_threshold reserved for future use
pub struct Scanner {
    /// Minimum code block size to consider (in tokens)
    min_block_size: usize,
    /// Similarity threshold (0.0 - 1.0)
    similarity_threshold: f64,
    /// Glob patterns to exclude from scanning
    exclude_patterns: Vec<String>,
    /// Cached compiled GlobSet for efficient exclude pattern matching
    exclude_glob_set: OnceCell<GlobSet>,
    /// Enable Type-3 (gap-tolerant) clone detection
    enable_type3: bool,
    /// Type-3 similarity tolerance (0.0 - 1.0)
    type3_tolerance: f64,
    /// Ignore manager for filtering false positives
    ignore_manager: Option<IgnoreManager>,
    /// Enable inline directive detection
    enable_directives: bool,
    /// Include test files in scanning (*.test.*, *.spec.*, etc.)
    include_tests: bool,
}

/// Default exclude patterns for test files and build artifacts
fn default_exclude_patterns() -> Vec<String> {
    vec![
        // Test files (excluded by default, enable with --include-tests)
        "**/*.test.ts".to_string(),
        "**/*.test.js".to_string(),
        "**/*.test.tsx".to_string(),
        "**/*.test.jsx".to_string(),
        "**/*.spec.ts".to_string(),
        "**/*.spec.js".to_string(),
        "**/*.spec.tsx".to_string(),
        "**/*.spec.jsx".to_string(),
        "**/__tests__/**".to_string(),
        "**/*.test.py".to_string(),
    ]
}

/// Exclude patterns for build artifacts (always excluded)
fn build_artifact_patterns() -> Vec<String> {
    vec![
        "**/node_modules/**".to_string(),
        "**/target/**".to_string(),
        "**/dist/**".to_string(),
        "**/build/**".to_string(),
        "**/.git/**".to_string(),
    ]
}

impl Scanner {
    /// Creates a new Scanner with default settings
    ///
    /// This is now infallible as there are no I/O or allocation failures.
    pub fn new() -> Self {
        let mut exclude = build_artifact_patterns();
        exclude.extend(default_exclude_patterns());

        Self {
            min_block_size: 50,
            similarity_threshold: 0.85,
            exclude_patterns: exclude,
            exclude_glob_set: OnceCell::new(),
            enable_type3: false,
            type3_tolerance: 0.85,
            ignore_manager: None,
            enable_directives: false,
            include_tests: false,
        }
    }

    /// Creates a new Scanner with custom settings
    pub fn with_config(min_block_size: usize, similarity_threshold: f64) -> Result<Self> {
        let mut exclude = build_artifact_patterns();
        exclude.extend(default_exclude_patterns());

        Ok(Self {
            min_block_size,
            similarity_threshold,
            exclude_patterns: exclude,
            exclude_glob_set: OnceCell::new(),
            enable_type3: false,
            type3_tolerance: 0.85,
            ignore_manager: None,
            enable_directives: false,
            include_tests: false,
        })
    }

    /// Sets custom exclude patterns, replacing the defaults
    pub fn with_exclude_patterns(mut self, patterns: Vec<String>) -> Self {
        self.exclude_patterns = patterns;
        self
    }

    /// Enables test file scanning (removes test file patterns from exclusions)
    pub fn with_test_files(mut self, include: bool) -> Self {
        self.include_tests = include;
        if include {
            // Remove test file patterns from exclusions
            let test_patterns = default_exclude_patterns();
            self.exclude_patterns.retain(|p| !test_patterns.contains(p));
        }
        self
    }

    /// Enables Type-3 clone detection with the specified tolerance
    pub fn with_type3_detection(mut self, tolerance: f64) -> Result<Self> {
        if !(0.0..=1.0).contains(&tolerance) {
            return Err(PolyDupError::Config(
                "Type-3 tolerance must be between 0.0 and 1.0".to_string(),
            ));
        }
        self.enable_type3 = true;
        self.type3_tolerance = tolerance;
        Ok(self)
    }

    /// Sets the ignore manager for filtering false positives
    pub fn with_ignore_manager(mut self, manager: IgnoreManager) -> Self {
        self.ignore_manager = Some(manager);
        self
    }

    /// Enables inline directive detection (// polydup-ignore comments)
    pub fn with_directives(mut self, enabled: bool) -> Self {
        self.enable_directives = enabled;
        self
    }

    /// Collect source files that would be scanned (for dry-run mode)
    ///
    /// Returns a list of file paths that match the scanner's configuration
    /// (supported languages, not excluded, respecting .gitignore, etc.)
    pub fn collect_files(&self, paths: Vec<PathBuf>) -> Result<Vec<PathBuf>> {
        self.collect_source_files(paths)
    }

    /// Scans the given paths and returns a Report with detected duplicates
    ///
    /// Uses Rayon for parallel file processing:
    /// 1. Read and parse files
    /// 2. Extract functions
    /// 3. Normalize and hash function bodies
    /// 4. Compare hashes to find duplicates
    /// 5. Apply directive-based filtering if enabled
    pub fn scan(&self, paths: Vec<PathBuf>) -> Result<Report> {
        use std::time::Instant;
        let start_time = Instant::now();

        // Collect all source files
        let source_files = self.collect_source_files(paths)?;

        // Detect directives if enabled
        let directives_map = self.collect_directives(&source_files);

        // Process files in parallel to extract functions and compute hashes
        let (function_hashes, total_lines, skipped_files) = self.analyze_files(&source_files)?;

        // Find duplicates by comparing hashes
        let (mut duplicates, suppressed_by_ignore_file) =
            self.find_duplicate_hashes(&function_hashes);

        // Apply directive-based filtering
        let suppressed_by_directive = if self.enable_directives && !directives_map.is_empty() {
            self.apply_directive_filtering(&mut duplicates, &directives_map, &function_hashes)
        } else {
            0
        };

        // Calculate statistics
        let stats = self.compute_stats(
            &function_hashes,
            total_lines,
            start_time,
            suppressed_by_ignore_file,
            suppressed_by_directive,
        );

        // files_scanned is the count of successfully scanned files (total - skipped)
        let files_scanned = source_files.len().saturating_sub(skipped_files.len());

        Ok(Report {
            version: None,   // Will be set by CLI
            scan_time: None, // Will be set by CLI
            config: None,    // Will be set by CLI
            files_scanned,
            functions_analyzed: function_hashes.len(),
            duplicates,
            skipped_files,
            stats,
        })
    }

    /// Parallel collection of directives from source files
    fn collect_directives(
        &self,
        source_files: &[PathBuf],
    ) -> HashMap<PathBuf, crate::directives::FileDirectives> {
        if self.enable_directives {
            source_files
                .par_iter()
                .filter_map(|path| {
                    crate::directives::detect_directives_in_file(path)
                        .ok()
                        .map(|d| (path.clone(), d))
                })
                .collect()
        } else {
            HashMap::new()
        }
    }

    /// Analyze files in parallel to extract functions and metadata
    /// Returns (function_hashes, total_lines, skipped_files)
    #[allow(clippy::type_complexity)]
    fn analyze_files(
        &self,
        source_files: &[PathBuf],
    ) -> Result<(Vec<FunctionHash>, usize, Vec<SkippedFile>)> {
        // Collect function hashes, line counts, and track errors
        let results: Vec<(PathBuf, Result<(Vec<FunctionHash>, usize)>)> = source_files
            .par_iter()
            .map(|path| {
                let result = (|| {
                    // Count lines first
                    let content = std::fs::read_to_string(path).map_err(PolyDupError::Io)?;
                    let line_count = content.lines().count();

                    // Process file for functions
                    let hashes = self.process_file_content(path, &content)?;
                    Ok((hashes, line_count))
                })();
                (path.clone(), result)
            })
            .collect();

        // Aggregate results
        let mut all_hashes = Vec::new();
        let mut total_lines = 0;
        let mut skipped_files = Vec::new();

        for (path, res) in results {
            match res {
                Ok((hashes, lines)) => {
                    all_hashes.extend(hashes);
                    total_lines += lines;
                }
                Err(e) => {
                    // Track skipped files with their error reason
                    let reason = match &e {
                        PolyDupError::Io(io_err) => {
                            if io_err.kind() == std::io::ErrorKind::PermissionDenied {
                                "Permission denied".to_string()
                            } else {
                                format!("IO error: {}", io_err)
                            }
                        }
                        PolyDupError::Parsing(msg) => format!("Parse error: {}", msg),
                        PolyDupError::Config(msg) => format!("Config error: {}", msg),
                        PolyDupError::LanguageNotSupported(lang) => {
                            format!("Language not supported: {}", lang)
                        }
                        PolyDupError::LanguageDetection(_) => {
                            "Could not detect language".to_string()
                        }
                        PolyDupError::ParallelExecution(msg) => {
                            format!("Parallel execution error: {}", msg)
                        }
                        PolyDupError::IgnoreRule(msg) => format!("Ignore rule error: {}", msg),
                        PolyDupError::Other(e) => format!("Error: {}", e),
                    };
                    skipped_files.push(SkippedFile {
                        path: path.display().to_string(),
                        reason,
                    });
                }
            }
        }

        Ok((all_hashes, total_lines, skipped_files))
    }

    /// Filter duplicates based on directives
    /// Returns the count of duplicates that were suppressed
    fn apply_directive_filtering(
        &self,
        duplicates: &mut Vec<DuplicateMatch>,
        directives_map: &HashMap<PathBuf, crate::directives::FileDirectives>,
        function_hashes: &[FunctionHash],
    ) -> usize {
        let original_count = duplicates.len();
        for dup in duplicates.iter_mut() {
            let suppressed = self.is_suppressed_by_directive(dup, directives_map, function_hashes);
            if suppressed {
                dup.suppressed_by_directive = Some(true);
            }
        }

        // Filter out suppressed duplicates (they shouldn't appear in reports or fail CI)
        duplicates.retain(|dup| dup.suppressed_by_directive != Some(true));
        original_count - duplicates.len()
    }

    /// Compute scan statistics
    fn compute_stats(
        &self,
        function_hashes: &[FunctionHash],
        total_lines: usize,
        start_time: std::time::Instant,
        suppressed_by_ignore_file: usize,
        suppressed_by_directive: usize,
    ) -> ScanStats {
        let total_tokens: usize = function_hashes.iter().map(|fh| fh.tokens.len()).sum();

        let unique_hashes: usize = {
            let mut hash_set = std::collections::HashSet::new();
            for fh in function_hashes {
                // Compute rolling hashes just for statistics
                let hashes = compute_rolling_hashes(&fh.tokens, self.min_block_size);
                for (hash, _) in hashes {
                    hash_set.insert(hash);
                }
            }
            hash_set.len()
        };

        let duration_ms = start_time.elapsed().as_millis() as u64;

        ScanStats {
            total_lines,
            total_tokens,
            unique_hashes,
            duration_ms,
            suppressed_by_ignore_file,
            suppressed_by_directive,
        }
    }

    /// Collects all source files from the given paths
    ///
    /// Uses the `ignore` crate to respect .gitignore, .ignore files,
    /// and common ignore patterns (node_modules, target, etc.)
    fn collect_source_files(&self, paths: Vec<PathBuf>) -> Result<Vec<PathBuf>> {
        let mut files = Vec::new();

        for path in paths {
            if path.is_file() {
                if self.is_supported_file(&path) && !self.is_excluded(&path) {
                    files.push(path);
                }
            } else if path.is_dir() {
                // Use ignore crate's WalkBuilder to respect .gitignore
                let walker = WalkBuilder::new(&path)
                    .git_ignore(true) // Respect .gitignore
                    .git_global(true) // Respect global gitignore
                    .git_exclude(true) // Respect .git/info/exclude
                    .ignore(true) // Respect .ignore files
                    .hidden(false) // Don't skip hidden files (e.g., .config/)
                    .parents(true) // Respect parent .gitignore files
                    .build();

                for entry in walker {
                    match entry {
                        Ok(entry) => {
                            let path = entry.path();
                            if path.is_file()
                                && self.is_supported_file(path)
                                && !self.is_excluded(path)
                            {
                                files.push(path.to_path_buf());
                            }
                        }
                        Err(err) => {
                            // Log but don't fail on individual entry errors
                            eprintln!("Warning: Failed to access path: {}", err);
                        }
                    }
                }
            }
        }

        Ok(files)
    }

    /// Checks if a file is a supported source file
    fn is_supported_file(&self, path: &Path) -> bool {
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            matches!(
                ext,
                "rs" | "py"
                    | "pyi"
                    | "js"
                    | "mjs"
                    | "cjs"
                    | "ts"
                    | "mts"
                    | "cts"
                    | "jsx"
                    | "tsx"
                    | "vue"
                    | "svelte"
            )
        } else {
            false
        }
    }

    /// Checks if a file matches any exclude patterns
    fn is_excluded(&self, path: &Path) -> bool {
        // Get or build the cached GlobSet
        let glob_set = self.exclude_glob_set.get_or_init(|| {
            use globset::{Glob, GlobSetBuilder};

            let mut builder = GlobSetBuilder::new();
            for pattern in &self.exclude_patterns {
                if let Ok(glob) = Glob::new(pattern) {
                    builder.add(glob);
                }
            }

            builder.build().unwrap_or_else(|_| GlobSet::empty())
        });

        glob_set.is_match(path)
    }

    /// Processes a single file content and returns function hashes
    fn process_file_content(&self, path: &Path, code: &str) -> Result<Vec<FunctionHash>> {
        let lang = self.detect_language(path)?;
        let functions = extract_functions(code, lang)?;

        // Use Arc<str> for efficient sharing across all functions in this file
        let file_path: Arc<str> = path.to_string_lossy().to_string().into();
        let mut function_hashes = Vec::new();

        for func in functions {
            // Store both raw body (for Type-1) and normalized tokens (for Type-2)
            let raw_body = func.body.clone();
            let (tokens, token_line_offsets) = normalize_with_line_numbers(&func.body);

            // Skip if too small
            if tokens.len() < self.min_block_size {
                continue;
            }

            // Store the full token sequence for extension-based detection
            function_hashes.push(FunctionHash {
                file_path: Arc::clone(&file_path), // Cheap pointer clone
                function_name: func.name.clone(),
                start_byte: func.start_byte,
                end_byte: func.end_byte,
                start_line: func.start_line,
                end_line: func.end_line,
                tokens,
                token_line_offsets,
                raw_body,
            });
        }

        Ok(function_hashes)
    }

    /// Detects the Tree-sitter Language from file extension
    fn detect_language(&self, path: &Path) -> Result<Language> {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .ok_or_else(|| PolyDupError::LanguageDetection(path.to_path_buf()))?;

        match ext {
            "rs" => Ok(tree_sitter_rust::language()),
            "py" | "pyi" => Ok(tree_sitter_python::language()),
            "js" | "mjs" | "cjs" | "jsx" | "ts" | "mts" | "cts" | "tsx" | "vue" | "svelte" => {
                Ok(tree_sitter_javascript::language())
            }
            _ => Err(PolyDupError::LanguageNotSupported(ext.to_string())),
        }
    }

    /// Computes the inclusive line span for a token window within a function
    fn compute_line_span(
        &self,
        func: &FunctionHash,
        start_offset: usize,
        length: usize,
    ) -> (usize, usize) {
        let start_line = func
            .token_line_offsets
            .get(start_offset)
            .map(|offset| func.start_line + offset)
            .unwrap_or(func.start_line + start_offset);

        let end_index = start_offset + length.saturating_sub(1);
        let end_line = func
            .token_line_offsets
            .get(end_index)
            .map(|offset| func.start_line + offset)
            .unwrap_or(func.start_line + end_index);

        (start_line, end_line)
    }

    /// Finds duplicate code using greedy extension algorithm
    ///
    /// Orchestrates the detection pipeline:
    /// 1. Type-1/2 detection (exact and renamed clones)
    /// 2. Type-3 detection (near-miss clones with gaps)
    /// 3. Duplicate ID computation
    /// 4. Ignore filtering
    ///
    /// Returns (duplicates, suppressed_by_ignore_file_count)
    fn find_duplicate_hashes(
        &self,
        function_hashes: &[FunctionHash],
    ) -> (Vec<DuplicateMatch>, usize) {
        // Type alias for pair deduplication keys
        type SeenPairKey<'a> = (&'a str, &'a str, usize, usize, usize, usize, usize);

        // Shared state for deduplication across Type-1/2 and Type-3
        let mut seen_pairs: std::collections::HashSet<SeenPairKey<'_>> =
            std::collections::HashSet::new();

        // Phase 1: Type-1/2 detection
        let mut duplicates = self.find_type12_duplicates(function_hashes, &mut seen_pairs);

        // Phase 2: Type-3 detection (if enabled)
        if self.enable_type3 {
            self.find_type3_duplicates(function_hashes, &seen_pairs, &mut duplicates);
        }

        // Phase 3: Compute IDs for all duplicates
        self.compute_duplicate_ids(function_hashes, &mut duplicates);

        // Phase 4: Filter out ignored duplicates
        let suppressed_count = self.filter_ignored_duplicates(&mut duplicates);

        (duplicates, suppressed_count)
    }

    /// Detects Type-1 (exact) and Type-2 (renamed) clones
    ///
    /// Compares all function pairs using hash-based detection with greedy extension.
    fn find_type12_duplicates<'a>(
        &self,
        function_hashes: &'a [FunctionHash],
        seen_pairs: &mut std::collections::HashSet<(
            &'a str,
            &'a str,
            usize,
            usize,
            usize,
            usize,
            usize,
        )>,
    ) -> Vec<DuplicateMatch> {
        let mut duplicates = Vec::new();

        for i in 0..function_hashes.len() {
            for j in (i + 1)..function_hashes.len() {
                let func1 = &function_hashes[i];
                let func2 = &function_hashes[j];

                let matches = self.find_clones_between_functions(func1, func2);

                for clone_match in matches {
                    let pair_key = canonical_pair_key(
                        func1,
                        func2,
                        clone_match.source_start,
                        clone_match.target_start,
                        clone_match.length,
                    );

                    if seen_pairs.contains(&pair_key) {
                        continue;
                    }
                    seen_pairs.insert(pair_key);

                    // Compute hash for reporting
                    let match_hash = Self::compute_match_hash(
                        &func1.tokens[clone_match.source_start
                            ..clone_match.source_start + clone_match.length],
                    );

                    let clone_type = self.classify_clone_type(&func1.raw_body, &func2.raw_body);

                    let (actual_start1, actual_end1) =
                        self.compute_line_span(func1, clone_match.source_start, clone_match.length);
                    let (actual_start2, actual_end2) =
                        self.compute_line_span(func2, clone_match.target_start, clone_match.length);

                    // Skip same location (overlapping function boundaries)
                    if func1.file_path == func2.file_path && actual_start1 == actual_start2 {
                        continue;
                    }

                    duplicates.push(DuplicateMatch {
                        file1: func1.file_path.to_string(),
                        file2: func2.file_path.to_string(),
                        start_line1: actual_start1,
                        start_line2: actual_start2,
                        end_line1: Some(actual_end1),
                        end_line2: Some(actual_end2),
                        length: clone_match.length,
                        similarity: clone_match.similarity,
                        hash: match_hash,
                        clone_type,
                        edit_distance: None,
                        suppressed_by_directive: None,
                        token_offset1: Some(clone_match.source_start),
                        token_offset2: Some(clone_match.target_start),
                        target_length: Some(clone_match.length),
                        duplicate_id: None,
                    });
                }
            }
        }

        duplicates
    }

    /// Detects Type-3 (gap-tolerant) clones using edit distance
    ///
    /// Finds near-miss clones that have insertions, deletions, or modifications.
    fn find_type3_duplicates<'a>(
        &self,
        function_hashes: &'a [FunctionHash],
        seen_pairs: &std::collections::HashSet<(
            &'a str,
            &'a str,
            usize,
            usize,
            usize,
            usize,
            usize,
        )>,
        duplicates: &mut Vec<DuplicateMatch>,
    ) {
        let mut type3_candidates = Vec::new();

        for i in 0..function_hashes.len() {
            for j in (i + 1)..function_hashes.len() {
                let func1 = &function_hashes[i];
                let func2 = &function_hashes[j];

                let type3_matches = detect_type3_clones(
                    &func1.tokens,
                    &func2.tokens,
                    self.min_block_size,
                    self.type3_tolerance,
                );

                for clone_match in type3_matches {
                    let pair_key = canonical_pair_key(
                        func1,
                        func2,
                        clone_match.source_start,
                        clone_match.target_start,
                        clone_match.length,
                    );

                    if seen_pairs.contains(&pair_key) {
                        continue;
                    }

                    type3_candidates.push((func1, func2, clone_match));
                }
            }
        }

        // Deduplicate overlapping Type-3 matches
        let deduplicated = self.deduplicate_overlapping_matches(type3_candidates);

        // Convert to DuplicateMatch
        for (func1, func2, clone_match) in deduplicated {
            let (actual_start1, actual_end1) =
                self.compute_line_span(func1, clone_match.source_start, clone_match.length);
            let (actual_start2, actual_end2) =
                self.compute_line_span(func2, clone_match.target_start, clone_match.target_length);

            // Skip self-matches: same file and same starting line indicates
            // the algorithm matched a code block against itself
            if func1.file_path == func2.file_path && actual_start1 == actual_start2 {
                continue;
            }

            let window1 = &func1.tokens
                [clone_match.source_start..clone_match.source_start + clone_match.length];
            let window2 = &func2.tokens
                [clone_match.target_start..clone_match.target_start + clone_match.target_length];
            let edit_dist = hashing::compute_token_edit_distance(window1, window2);

            let match_hash = Self::compute_match_hash(window1);

            duplicates.push(DuplicateMatch {
                file1: func1.file_path.to_string(),
                file2: func2.file_path.to_string(),
                start_line1: actual_start1,
                start_line2: actual_start2,
                end_line1: Some(actual_end1),
                end_line2: Some(actual_end2),
                length: clone_match.length,
                similarity: clone_match.similarity,
                hash: match_hash,
                clone_type: CloneType::Type3,
                edit_distance: Some(edit_dist),
                suppressed_by_directive: None,
                token_offset1: Some(clone_match.source_start),
                token_offset2: Some(clone_match.target_start),
                target_length: Some(clone_match.target_length),
                duplicate_id: None,
            });
        }
    }

    /// Computes content-based IDs for all duplicates
    ///
    /// IDs are SHA256 hashes of normalized tokens, enabling persistent ignore rules.
    fn compute_duplicate_ids(
        &self,
        function_hashes: &[FunctionHash],
        duplicates: &mut [DuplicateMatch],
    ) {
        for dup in duplicates.iter_mut() {
            if dup.duplicate_id.is_some() {
                continue;
            }

            let tokens1 = self.extract_duplicate_tokens(
                function_hashes,
                &dup.file1,
                dup.start_line1,
                dup.end_line1,
                dup.token_offset1,
                dup.length,
            );

            let tokens2 = self.extract_duplicate_tokens(
                function_hashes,
                &dup.file2,
                dup.start_line2,
                dup.end_line2,
                dup.token_offset2,
                dup.target_length.unwrap_or(dup.length),
            );

            if let Some(tokens1) = tokens1 {
                let id = if let Some(tokens2) = tokens2 {
                    ignore_rules::compute_symmetric_duplicate_id(&tokens1, &tokens2)
                } else {
                    ignore_rules::compute_duplicate_id(&tokens1)
                };
                dup.duplicate_id = Some(id);
            }
        }
    }

    /// Extracts normalized token strings for a duplicate region
    fn extract_duplicate_tokens(
        &self,
        function_hashes: &[FunctionHash],
        file: &str,
        reported_start: usize,
        reported_end: Option<usize>,
        token_offset: Option<usize>,
        length: usize,
    ) -> Option<Vec<String>> {
        function_hashes.iter().find_map(|fh| {
            if fh.file_path.as_ref() != file
                || fh.start_line > reported_start
                || reported_start > fh.end_line
            {
                return None;
            }

            let start_offset = match token_offset {
                Some(offset) if offset + length <= fh.tokens.len() => Some(offset),
                _ => self.infer_token_offset(fh, reported_start, reported_end, length),
            }?;

            if start_offset + length > fh.tokens.len() {
                return None;
            }

            Some(
                fh.tokens
                    .iter()
                    .skip(start_offset)
                    .take(length)
                    .map(|t| t.as_hash_string().to_string())
                    .collect(),
            )
        })
    }

    /// Attempts to derive the token offset from reported lines when it's missing (e.g., older caches).
    fn infer_token_offset(
        &self,
        func_hash: &FunctionHash,
        reported_start: usize,
        reported_end: Option<usize>,
        length: usize,
    ) -> Option<usize> {
        let start_line_offset = reported_start.checked_sub(func_hash.start_line)?;
        let end_line = reported_end.unwrap_or(reported_start);

        func_hash
            .token_line_offsets
            .iter()
            .enumerate()
            .filter_map(|(idx, line_offset)| {
                if *line_offset != start_line_offset {
                    return None;
                }

                let end_idx = idx.checked_add(length.checked_sub(1)?)?;
                let end_offset = func_hash.token_line_offsets.get(end_idx)?;
                if func_hash.start_line + *end_offset == end_line {
                    Some(idx)
                } else {
                    None
                }
            })
            .next()
    }

    /// Filters out duplicates that are in the ignore list
    fn filter_ignored_duplicates(&self, duplicates: &mut Vec<DuplicateMatch>) -> usize {
        let original_count = duplicates.len();
        if let Some(ref ignore_manager) = self.ignore_manager {
            duplicates.retain(|dup| {
                if let Some(ref id) = dup.duplicate_id {
                    !ignore_manager.is_ignored(id)
                } else {
                    // If we couldn't compute an ID, keep the duplicate (fail open)
                    true
                }
            });
        }
        original_count - duplicates.len()
    }

    /// Computes a hash for a token slice (used for match reporting)
    fn compute_match_hash(tokens: &[Token]) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        tokens.hash(&mut hasher);
        hasher.finish()
    }

    /// Checks if a duplicate is suppressed by an inline directive
    ///
    /// Directives suppress the entire function they're placed before, so we check
    /// if the owning function has a directive, not the duplicate's specific lines.
    fn is_suppressed_by_directive(
        &self,
        dup: &DuplicateMatch,
        directives_map: &HashMap<PathBuf, crate::directives::FileDirectives>,
        function_hashes: &[FunctionHash],
    ) -> bool {
        // Check if either file has a directive suppressing this duplicate
        let file1_path = PathBuf::from(&dup.file1);
        let file2_path = PathBuf::from(&dup.file2);

        // Check file1 - use the owning function's start line for directive lookup
        if let Some(directives) = directives_map.get(&file1_path) {
            let func_start =
                self.find_owning_function_start(&dup.file1, dup.start_line1, function_hashes);
            // Use function start for directive check (directives apply to whole function)
            let check_line = func_start.unwrap_or(dup.start_line1);

            if directives.is_suppressed(check_line, check_line).is_some() {
                return true;
            }
        }

        // Check file2 - use the owning function's start line for directive lookup
        if let Some(directives) = directives_map.get(&file2_path) {
            let func_start =
                self.find_owning_function_start(&dup.file2, dup.start_line2, function_hashes);
            // Use function start for directive check (directives apply to whole function)
            let check_line = func_start.unwrap_or(dup.start_line2);

            if directives.is_suppressed(check_line, check_line).is_some() {
                return true;
            }
        }

        false
    }

    /// Finds the start line of the function containing a given line
    fn find_owning_function_start(
        &self,
        file: &str,
        line: usize,
        function_hashes: &[FunctionHash],
    ) -> Option<usize> {
        function_hashes
            .iter()
            .find(|fh| {
                fh.file_path.as_ref() == file && fh.start_line <= line && line <= fh.end_line
            })
            .map(|fh| fh.start_line)
    }

    /// Deduplicates overlapping Type-3 matches by keeping only the longest match per region
    ///
    /// Groups matches by (file1, file2, func1_line, func2_line) to handle same-file clones properly.
    /// Merges overlapping regions, keeping the longest match with the highest similarity score.
    /// Overlap requires BOTH source AND target ranges to overlap.
    fn deduplicate_overlapping_matches<'a>(
        &self,
        candidates: Vec<(&'a FunctionHash, &'a FunctionHash, CloneMatch)>,
    ) -> Vec<(&'a FunctionHash, &'a FunctionHash, CloneMatch)> {
        if candidates.is_empty() {
            return Vec::new();
        }

        // Track which matches have been merged
        let mut used = vec![false; candidates.len()];
        let mut deduplicated = Vec::new();

        for i in 0..candidates.len() {
            if used[i] {
                continue;
            }

            let (func1, func2, current) = &candidates[i];
            let mut best_match = (*func1, *func2, current.clone());
            used[i] = true;

            // Find all overlapping matches (iterate until no more overlaps found)
            // This handles transitive overlaps: A overlaps B, B overlaps C
            let mut found_overlap = true;
            while found_overlap {
                found_overlap = false;

                for j in (i + 1)..candidates.len() {
                    if used[j] {
                        continue;
                    }

                    let (f1, f2, candidate) = &candidates[j];

                    // Only merge if same function pair (by file path and line number)
                    let same_pair = (func1.file_path == f1.file_path
                        && func2.file_path == f2.file_path
                        && func1.start_line == f1.start_line
                        && func2.start_line == f2.start_line)
                        || (func1.file_path == f2.file_path
                            && func2.file_path == f1.file_path
                            && func1.start_line == f2.start_line
                            && func2.start_line == f1.start_line);

                    if !same_pair {
                        continue;
                    }

                    // Check if overlapping with CURRENT best_match (not original)
                    // This ensures transitive overlaps are handled correctly
                    let source_overlap = ranges_overlap(
                        best_match.2.source_start,
                        best_match.2.source_start + best_match.2.length,
                        candidate.source_start,
                        candidate.source_start + candidate.length,
                    );
                    let target_overlap = ranges_overlap(
                        best_match.2.target_start,
                        best_match.2.target_start + best_match.2.target_length,
                        candidate.target_start,
                        candidate.target_start + candidate.target_length,
                    );

                    if source_overlap && target_overlap {
                        let best_span = best_match.2.length.max(best_match.2.target_length);
                        let candidate_span = candidate.length.max(candidate.target_length);

                        // Keep the match that covers more tokens overall, breaking ties by similarity
                        if candidate_span > best_span
                            || (candidate_span == best_span
                                && candidate.similarity > best_match.2.similarity)
                        {
                            best_match = (*f1, *f2, candidate.clone());
                            found_overlap = true; // Need another pass to check against new best
                        }
                        used[j] = true;
                    }
                }
            }

            deduplicated.push(best_match);
        }

        deduplicated
    }

    /// Classifies a clone as Type-1 (exact) or Type-2 (renamed)
    fn classify_clone_type(&self, raw1: &str, raw2: &str) -> CloneType {
        // Normalize whitespace for comparison (avoid intermediate Vec allocation)
        let normalized1 = raw1.split_whitespace().collect::<String>();
        let normalized2 = raw2.split_whitespace().collect::<String>();

        // If raw code is identical (ignoring whitespace), it's Type-1 (exact copy)
        if normalized1 == normalized2 {
            CloneType::Type1
        } else {
            // Otherwise, it's Type-2 (renamed identifiers/literals)
            CloneType::Type2
        }
    }

    /// Finds clone matches between two functions using extension algorithm
    fn find_clones_between_functions(
        &self,
        func1: &FunctionHash,
        func2: &FunctionHash,
    ) -> Vec<CloneMatch> {
        use std::collections::HashMap;

        let mut matches = Vec::new();
        let mut hash_map: HashMap<u64, Vec<usize>> = HashMap::new();

        // Index all windows in func1
        let mut i = 0;
        while i <= func1.tokens.len().saturating_sub(self.min_block_size) {
            let hash = hashing::compute_window_hash(&func1.tokens[i..i + self.min_block_size]);
            hash_map.entry(hash).or_default().push(i);
            i += 1;
        }

        // Search for matches in func2
        let mut j = 0;
        while j <= func2.tokens.len().saturating_sub(self.min_block_size) {
            let hash = hashing::compute_window_hash(&func2.tokens[j..j + self.min_block_size]);

            if let Some(func1_positions) = hash_map.get(&hash) {
                for &func1_pos in func1_positions {
                    // Verify exact match using shared utility
                    if hashing::verify_cross_window_match(
                        &func1.tokens,
                        &func2.tokens,
                        func1_pos,
                        j,
                        self.min_block_size,
                    ) {
                        // Greedy extension using shared utility
                        let extension = hashing::extend_match(
                            &func1.tokens,
                            &func2.tokens,
                            func1_pos,
                            j,
                            self.min_block_size,
                        );

                        let total_length = self.min_block_size + extension;

                        matches.push(CloneMatch {
                            source_start: func1_pos,
                            target_start: j,
                            length: total_length,
                            target_length: total_length,
                            similarity: 1.0, // Exact match
                        });

                        // Skip ahead
                        j += extension.max(1);
                        break;
                    }
                }
            }

            j += 1;
        }

        matches
    }

    fn add_hashes_to_cache(&self, function_hashes: &[FunctionHash], cache: &mut HashCache) {
        for func_hash in function_hashes {
            let hashes = compute_rolling_hashes(&func_hash.tokens, self.min_block_size);

            for (hash, offset) in hashes {
                let end_token_idx = offset + self.min_block_size;
                let (start_line, end_line) =
                    self.compute_line_span(func_hash, offset, self.min_block_size);

                let location = CodeLocation {
                    file_path: func_hash.file_path.to_string(),
                    start_line,
                    end_line,
                    token_offset: Some(offset),
                    token_length: self.min_block_size,
                    tokens: func_hash.tokens[offset..end_token_idx].to_vec(),
                    raw_source: func_hash.raw_body.clone(),
                };

                cache.add_hash(hash, location);
            }
        }
    }

    /// Build a hash cache from the given paths
    ///
    /// Scans all files and builds a persistent cache of rolling hashes.
    /// This enables fast incremental scanning and git-diff mode.
    pub fn build_cache(&self, paths: Vec<PathBuf>) -> Result<HashCache> {
        let mut cache = HashCache::new(self.min_block_size);

        // Collect all source files
        let source_files = self.collect_source_files(paths)?;

        // Process each file and add to cache
        for file_path in source_files {
            let content = match std::fs::read_to_string(&file_path) {
                Ok(c) => c,
                Err(_) => continue, // Skip files we can't read
            };

            let function_hashes = match self.process_file_content(&file_path, &content) {
                Ok(fh) => fh,
                Err(_) => continue, // Skip files we can't parse
            };

            self.add_hashes_to_cache(&function_hashes, &mut cache);
        }

        Ok(cache)
    }

    /// Scan with cache lookup (for git-diff mode)
    ///
    /// Scans only the changed files, then looks up their hashes in the cache
    /// to find duplicates against the entire codebase.
    pub fn scan_with_cache(
        &self,
        changed_files: Vec<PathBuf>,
        cache: &mut HashCache,
    ) -> Result<Report> {
        use std::time::Instant;
        let start_time = Instant::now();

        // Ensure we don't match against stale cache entries
        let stale_files = cache.invalidate_stale_files();
        let normalize_path =
            |path: &Path| path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let changed_set: HashSet<PathBuf> =
            changed_files.iter().map(|p| normalize_path(p)).collect();

        if !stale_files.is_empty() {
            // Rebuild cache entries that were invalidated so unchanged files
            // remain available for lookups.
            let stale_paths: Vec<PathBuf> = stale_files
                .into_iter()
                .filter_map(|path| {
                    let raw_path = PathBuf::from(&path);
                    let normalized = normalize_path(&raw_path);

                    if !normalized.exists() || changed_set.contains(&normalized) {
                        return None;
                    }

                    Some(raw_path)
                })
                .collect();

            if !stale_paths.is_empty() {
                let (stale_hashes, _, _) = self.analyze_files(&stale_paths)?;
                self.add_hashes_to_cache(&stale_hashes, cache);
            }
        }

        // Only scan the changed files
        let (function_hashes, total_lines, skipped_files) = self.analyze_files(&changed_files)?;

        // Find duplicates by looking up in cache
        let mut duplicates = Vec::new();
        let mut cached_hits_by_file: HashMap<String, Vec<CodeLocation>> = HashMap::new();
        let mut cached_function_hashes: Vec<FunctionHash> = Vec::new();

        for func_hash in &function_hashes {
            let hashes = compute_rolling_hashes(&func_hash.tokens, self.min_block_size);

            for (hash, offset) in hashes {
                // Look up this hash in the cache
                if let Some(cached_locations) = cache.lookup(hash) {
                    for cached_loc in cached_locations {
                        // Normalize both paths for comparison (handle relative vs absolute)
                        let changed_file_path = Path::new(func_hash.file_path.as_ref())
                            .canonicalize()
                            .unwrap_or_else(|_| {
                                Path::new(func_hash.file_path.as_ref()).to_path_buf()
                            });
                        let cached_file_path = Path::new(&cached_loc.file_path)
                            .canonicalize()
                            .unwrap_or_else(|_| Path::new(&cached_loc.file_path).to_path_buf());

                        // Skip if same file (we'll find those via normal duplicate detection)
                        if changed_file_path == cached_file_path {
                            continue;
                        }

                        cached_hits_by_file
                            .entry(cached_loc.file_path.clone())
                            .or_default()
                            .push(cached_loc.clone());

                        // Calculate line numbers for the match in changed file
                        let start_token_idx = offset;
                        let end_token_idx =
                            (offset + self.min_block_size).min(func_hash.tokens.len());

                        let start_line_offset =
                            if start_token_idx < func_hash.token_line_offsets.len() {
                                func_hash.token_line_offsets[start_token_idx]
                            } else {
                                0
                            };

                        let end_line_offset = if end_token_idx > 0
                            && end_token_idx - 1 < func_hash.token_line_offsets.len()
                        {
                            func_hash.token_line_offsets[end_token_idx - 1]
                        } else {
                            start_line_offset
                        };

                        // Create duplicate match
                        let similarity = compute_token_similarity(
                            &func_hash.tokens[start_token_idx..end_token_idx],
                            &cached_loc.tokens,
                        );

                        if similarity >= self.similarity_threshold {
                            let clone_type = if func_hash.raw_body == cached_loc.raw_source {
                                CloneType::Type1
                            } else {
                                CloneType::Type2
                            };

                            duplicates.push(DuplicateMatch {
                                file1: func_hash.file_path.to_string(),
                                file2: cached_loc.file_path.clone(),
                                start_line1: func_hash.start_line + start_line_offset,
                                start_line2: cached_loc.start_line,
                                end_line1: Some(func_hash.start_line + end_line_offset),
                                end_line2: Some(cached_loc.end_line),
                                length: self.min_block_size,
                                similarity,
                                hash,
                                clone_type,
                                edit_distance: None,
                                suppressed_by_directive: None,
                                token_offset1: Some(offset),
                                token_offset2: cached_loc.token_offset,
                                target_length: Some(cached_loc.token_length),
                                duplicate_id: None,
                            });
                        }
                    }
                }
            }
        }

        // Run Type-3 detection between changed files and any cached functions that matched hashes
        if self.enable_type3 && !cached_hits_by_file.is_empty() {
            let mut seen_functions: HashSet<(String, usize)> = HashSet::new();

            for locations in cached_hits_by_file.values() {
                for loc in locations {
                    let token_offset = match loc.token_offset {
                        Some(offset) => offset,
                        None => continue,
                    };

                    let normalized_path = normalize_path(Path::new(&loc.file_path));
                    if changed_set.contains(&normalized_path) {
                        continue;
                    }

                    let (tokens, token_line_offsets) = normalize_with_line_numbers(&loc.raw_source);
                    if tokens.len() < self.min_block_size
                        || token_offset >= token_line_offsets.len()
                    {
                        continue;
                    }

                    let line_offset = token_line_offsets[token_offset];
                    let start_line = loc.start_line.saturating_sub(line_offset);
                    let key = (loc.file_path.clone(), start_line);

                    if !seen_functions.insert(key.clone()) {
                        continue;
                    }

                    let end_line =
                        start_line + token_line_offsets.last().copied().unwrap_or_default();

                    cached_function_hashes.push(FunctionHash {
                        file_path: Arc::<str>::from(key.0),
                        function_name: None,
                        start_byte: 0,
                        end_byte: 0,
                        start_line,
                        end_line,
                        tokens,
                        token_line_offsets,
                        raw_body: loc.raw_source.clone(),
                    });
                }
            }

            if !cached_function_hashes.is_empty() {
                // Type alias for pair deduplication keys (shared with main detection path)
                type SeenPairKey<'a> = (&'a str, &'a str, usize, usize, usize, usize, usize);

                let mut seen_pairs: HashSet<SeenPairKey<'_>> = HashSet::new();

                for dup in &duplicates {
                    if let (Some(offset1), Some(offset2)) = (dup.token_offset1, dup.token_offset2) {
                        if let (Some(func1), Some(func2)) = (
                            function_hashes.iter().find(|fh| {
                                fh.file_path.as_ref() == dup.file1.as_str()
                                    && fh.start_line <= dup.start_line1
                                    && dup.start_line1 <= fh.end_line
                            }),
                            cached_function_hashes.iter().find(|fh| {
                                fh.file_path.as_ref() == dup.file2.as_str()
                                    && fh.start_line <= dup.start_line2
                                    && dup.start_line2 <= fh.end_line
                            }),
                        ) {
                            seen_pairs.insert(canonical_pair_key(
                                func1, func2, offset1, offset2, dup.length,
                            ));
                        }
                    }
                }

                let mut type3_candidates = Vec::new();

                for func1 in &function_hashes {
                    for func2 in &cached_function_hashes {
                        let type3_matches = detect_type3_clones(
                            &func1.tokens,
                            &func2.tokens,
                            self.min_block_size,
                            self.type3_tolerance,
                        );

                        for clone_match in type3_matches {
                            let pair_key = canonical_pair_key(
                                func1,
                                func2,
                                clone_match.source_start,
                                clone_match.target_start,
                                clone_match.length,
                            );

                            if seen_pairs.contains(&pair_key) {
                                continue;
                            }

                            type3_candidates.push((func1, func2, clone_match));
                        }
                    }
                }

                let deduplicated = self.deduplicate_overlapping_matches(type3_candidates);

                for (func1, func2, clone_match) in deduplicated {
                    let (actual_start1, actual_end1) =
                        self.compute_line_span(func1, clone_match.source_start, clone_match.length);
                    let (actual_start2, actual_end2) = self.compute_line_span(
                        func2,
                        clone_match.target_start,
                        clone_match.target_length,
                    );

                    if func1.file_path == func2.file_path && actual_start1 == actual_start2 {
                        continue;
                    }

                    let window1 = &func1.tokens
                        [clone_match.source_start..clone_match.source_start + clone_match.length];
                    let window2 = &func2.tokens[clone_match.target_start
                        ..clone_match.target_start + clone_match.target_length];

                    let edit_dist = hashing::compute_token_edit_distance(window1, window2);
                    let match_hash = Self::compute_match_hash(window1);

                    duplicates.push(DuplicateMatch {
                        file1: func1.file_path.to_string(),
                        file2: func2.file_path.to_string(),
                        start_line1: actual_start1,
                        start_line2: actual_start2,
                        end_line1: Some(actual_end1),
                        end_line2: Some(actual_end2),
                        length: clone_match.length,
                        similarity: clone_match.similarity,
                        hash: match_hash,
                        clone_type: CloneType::Type3,
                        edit_distance: Some(edit_dist),
                        suppressed_by_directive: None,
                        token_offset1: Some(clone_match.source_start),
                        token_offset2: Some(clone_match.target_start),
                        target_length: Some(clone_match.target_length),
                        duplicate_id: None,
                    });
                }
            }
        }

        // Also find duplicates within the changed files themselves
        let (intra_duplicates, _) = self.find_duplicate_hashes(&function_hashes);
        duplicates.extend(intra_duplicates);

        // Deduplicate
        duplicates.sort_by(|a, b| {
            (&a.file1, &a.file2, a.start_line1, a.start_line2).cmp(&(
                &b.file1,
                &b.file2,
                b.start_line1,
                b.start_line2,
            ))
        });
        duplicates.dedup_by(|a, b| {
            a.file1 == b.file1
                && a.file2 == b.file2
                && a.start_line1 == b.start_line1
                && a.start_line2 == b.start_line2
        });

        // Hydrate function metadata for any files that only exist in the cache so we
        // can compute duplicate IDs and apply directive-based suppression.
        let mut lookup_function_hashes = function_hashes.clone();
        if !cached_function_hashes.is_empty() {
            lookup_function_hashes.extend(cached_function_hashes.clone());
        }
        let hashed_files: HashSet<&str> = lookup_function_hashes
            .iter()
            .map(|fh| fh.file_path.as_ref())
            .collect();

        let mut missing_files: HashSet<String> = HashSet::new();
        for dup in &duplicates {
            if !hashed_files.contains(dup.file1.as_str()) {
                missing_files.insert(dup.file1.clone());
            }
            if !hashed_files.contains(dup.file2.as_str()) {
                missing_files.insert(dup.file2.clone());
            }
        }

        if !missing_files.is_empty() {
            let missing_paths: Vec<PathBuf> = missing_files.iter().map(PathBuf::from).collect();
            let (mut extra_hashes, _, _) = self.analyze_files(&missing_paths)?;
            lookup_function_hashes.append(&mut extra_hashes);
        }

        // Compute IDs and filter against .polydup-ignore
        self.compute_duplicate_ids(&lookup_function_hashes, &mut duplicates);
        let suppressed_by_ignore_file = self.filter_ignored_duplicates(&mut duplicates);

        // Apply inline directive filtering for both changed and cached files
        let suppressed_by_directive = if self.enable_directives && !duplicates.is_empty() {
            let directive_paths: HashSet<PathBuf> = lookup_function_hashes
                .iter()
                .map(|fh| PathBuf::from(fh.file_path.as_ref()))
                .collect();
            let directives_map =
                self.collect_directives(&directive_paths.into_iter().collect::<Vec<_>>());

            if !directives_map.is_empty() {
                self.apply_directive_filtering(
                    &mut duplicates,
                    &directives_map,
                    &lookup_function_hashes,
                )
            } else {
                0
            }
        } else {
            0
        };

        // Refresh cache with the newly scanned files so future runs stay incremental
        self.add_hashes_to_cache(&function_hashes, cache);

        // Calculate statistics
        let stats = self.compute_stats(
            &function_hashes,
            total_lines,
            start_time,
            suppressed_by_ignore_file,
            suppressed_by_directive,
        );

        // files_scanned is the count of successfully scanned files (total - skipped)
        let files_scanned = changed_files.len().saturating_sub(skipped_files.len());

        Ok(Report {
            version: None,
            scan_time: None,
            config: None,
            files_scanned,
            functions_analyzed: function_hashes.len(),
            duplicates,
            skipped_files,
            stats,
        })
    }
}

impl Default for Scanner {
    fn default() -> Self {
        Self::new() // Infallible now, no panic possible
    }
}

/// Public API: Find duplicates in the given file paths
///
/// # Arguments
/// * `paths` - Vector of file paths to scan
///
/// # Returns
/// * `Result<Report>` - Scan report with detected duplicates
pub fn find_duplicates(paths: Vec<String>) -> Result<Report> {
    let scanner = Scanner::new();
    let path_bufs: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
    scanner.scan(path_bufs)
}

/// Public API with custom configuration
pub fn find_duplicates_with_config(
    paths: Vec<String>,
    min_block_size: usize,
    similarity_threshold: f64,
) -> Result<Report> {
    let scanner = Scanner::with_config(min_block_size, similarity_threshold)?;
    let path_bufs: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
    scanner.scan(path_bufs)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to create a FunctionHash for testing with sequential line offsets
    fn make_test_function(
        file: &str,
        start_line: usize,
        tokens: Vec<Token>,
        raw_body: &str,
    ) -> FunctionHash {
        let token_line_offsets: Vec<usize> = (0..tokens.len()).collect();
        FunctionHash {
            file_path: Arc::<str>::from(file),
            function_name: None,
            start_byte: 0,
            end_byte: 0,
            start_line,
            end_line: start_line + tokens.len(),
            tokens,
            token_line_offsets,
            raw_body: raw_body.to_string(),
        }
    }

    /// Helper to create a FunctionHash with all tokens on the same line
    fn make_test_function_same_line(
        file: &str,
        start_line: usize,
        end_line: usize,
        tokens: Vec<Token>,
        raw_body: &str,
    ) -> FunctionHash {
        let token_line_offsets: Vec<usize> = vec![0; tokens.len()];
        FunctionHash {
            file_path: Arc::<str>::from(file),
            function_name: None,
            start_byte: 0,
            end_byte: 0,
            start_line,
            end_line,
            tokens,
            token_line_offsets,
            raw_body: raw_body.to_string(),
        }
    }

    /// Helper to create simple expression tokens for testing: keyword id op id ;
    fn make_expr_tokens(keyword: &str, op: &str) -> Vec<Token> {
        vec![
            Token::Keyword(keyword.into()),
            Token::Identifier,
            Token::Operator(op.into()),
            Token::Identifier,
            Token::Punctuation(";".into()),
        ]
    }

    #[test]
    fn test_scanner_creation() {
        let _scanner = Scanner::new(); // Infallible
    }

    #[test]
    fn test_scanner_with_config() {
        let scanner = Scanner::with_config(30, 0.9);
        assert!(scanner.is_ok());
        let s = scanner.unwrap();
        assert_eq!(s.min_block_size, 30);
        assert_eq!(s.similarity_threshold, 0.9);
    }

    #[test]
    fn test_type3_tolerance_validation() {
        assert!(Scanner::new().with_type3_detection(0.9).is_ok());
        assert!(Scanner::new().with_type3_detection(1.2).is_err());
        assert!(Scanner::new().with_type3_detection(-0.1).is_err());
    }

    #[test]
    fn test_type3_not_dropped_when_functions_share_offsets() {
        fn make_function(
            file: &str,
            start_line: usize,
            tokens: Vec<Token>,
            raw_body: &str,
        ) -> FunctionHash {
            let token_line_offsets: Vec<usize> = (0..tokens.len()).collect();
            FunctionHash {
                file_path: Arc::<str>::from(file),
                function_name: None,
                start_byte: 0,
                end_byte: 0,
                start_line,
                end_line: start_line + tokens.len(),
                tokens,
                token_line_offsets,
                raw_body: raw_body.to_string(),
            }
        }

        let scanner = Scanner::with_config(3, 0.85)
            .unwrap()
            .with_type3_detection(0.6)
            .unwrap();

        let type1_tokens = vec![
            Token::Keyword("return".into()),
            Token::NumberLiteral,
            Token::Punctuation(";".into()),
        ];
        let near_tokens_a = vec![
            Token::Keyword("compute".into()),
            Token::Identifier,
            Token::Identifier,
        ];
        let near_tokens_b = vec![
            Token::Keyword("compute".into()),
            Token::Identifier,
            Token::NumberLiteral,
        ];

        let functions = vec![
            make_function("file_a.rs", 10, type1_tokens.clone(), "return 1;"),
            make_function("file_b.rs", 20, type1_tokens, "return 1;"),
            make_function("file_a.rs", 200, near_tokens_a, "compute(x, y)"),
            make_function("file_b.rs", 300, near_tokens_b, "compute(x, 1)"),
        ];

        let (duplicates, _) = scanner.find_duplicate_hashes(&functions);

        let type1_present = duplicates.iter().any(|d| {
            matches!(d.clone_type, CloneType::Type1 | CloneType::Type2)
                && d.start_line1 == 10
                && d.start_line2 == 20
        });
        assert!(
            type1_present,
            "expected Type-1/2 match for the first function pair"
        );

        let type3_present = duplicates.iter().any(|d| {
            matches!(d.clone_type, CloneType::Type3) && d.start_line1 == 200 && d.start_line2 == 300
        });
        assert!(
            type3_present,
            "Type-3 match between later functions should not be deduped"
        );

        assert_eq!(
            duplicates.len(),
            2,
            "should keep both the Type-1/2 and Type-3 matches"
        );
    }

    #[test]
    fn test_type3_reports_token_offsets_in_start_lines() {
        let scanner = Scanner::with_config(3, 0.85)
            .unwrap()
            .with_type3_detection(0.75)
            .unwrap();

        let functions = vec![
            make_test_function_same_line(
                "file_a.rs",
                100,
                105,
                make_expr_tokens("let", "+"),
                "let a = b + c;",
            ),
            make_test_function_same_line(
                "file_b.rs",
                200,
                205,
                make_expr_tokens("mut", "-"),
                "let a = b - c;",
            ),
        ];

        let (duplicates, _) = scanner.find_duplicate_hashes(&functions);

        let type3 = duplicates
            .iter()
            .find(|d| matches!(d.clone_type, CloneType::Type3))
            .expect("expected a Type-3 duplicate match");

        assert_eq!(
            type3.start_line1, 100,
            "should report the actual source line even when tokens share a line"
        );
        assert_eq!(
            type3.start_line2, 200,
            "should report the actual target line even when tokens share a line"
        );
        assert_eq!(type3.token_offset1, Some(1));
        assert_eq!(type3.token_offset2, Some(1));
    }

    #[test]
    fn type3_duplicate_ids_are_symmetric() {
        use tempfile::TempDir;

        let tokens_a = make_expr_tokens("let", "+");
        // tokens_b has an extra identifier to create a Type-3 (near-miss) clone
        let mut tokens_b = make_expr_tokens("let", "-");
        tokens_b.push(Token::Identifier);

        let func_a = make_test_function("file_a.rs", 10, tokens_a.clone(), "fn file_a.rs() {}");
        let func_b = make_test_function("file_b.rs", 20, tokens_b.clone(), "fn file_b.rs() {}");

        let temp_dir = TempDir::new().unwrap();
        let scanner = Scanner::with_config(3, 0.85)
            .unwrap()
            .with_type3_detection(0.75)
            .unwrap()
            .with_ignore_manager(IgnoreManager::new(temp_dir.path()));

        let (forward, _) = scanner.find_duplicate_hashes(&[func_a.clone(), func_b.clone()]);
        let (reverse, _) = scanner.find_duplicate_hashes(&[func_b, func_a]);

        let id_forward = forward
            .into_iter()
            .find(|d| matches!(d.clone_type, CloneType::Type3))
            .and_then(|d| d.duplicate_id)
            .expect("expected a Type-3 duplicate ID");

        let id_reverse = reverse
            .into_iter()
            .find(|d| matches!(d.clone_type, CloneType::Type3))
            .and_then(|d| d.duplicate_id)
            .expect("expected a Type-3 duplicate ID");

        assert_eq!(
            id_forward, id_reverse,
            "Type-3 IDs should not depend on function order"
        );
    }

    #[test]
    fn type3_does_not_report_self_matches() {
        // Regression test for issue #71: Type-3 detection was reporting functions
        // as duplicates of themselves (same file, same line on both sides)
        let scanner = Scanner::with_config(3, 0.85)
            .unwrap()
            .with_type3_detection(0.75)
            .unwrap();

        // Create two functions in the SAME file with the SAME starting line
        // This simulates the bug where Type-3 matched a function against itself
        let tokens = make_expr_tokens("let", "+");
        let func1 = make_test_function_same_line("same_file.rs", 28, 35, tokens.clone(), "fn a()");
        let func2 = make_test_function_same_line("same_file.rs", 28, 35, tokens, "fn a()");

        let (duplicates, _) = scanner.find_duplicate_hashes(&[func1, func2]);

        // Should NOT report any duplicates since both map to the same file:line
        let self_matches: Vec<_> = duplicates
            .iter()
            .filter(|d| d.file1 == d.file2 && d.start_line1 == d.start_line2)
            .collect();

        assert!(
            self_matches.is_empty(),
            "Type-3 should never report self-matches (same file and line). Found: {:?}",
            self_matches
        );
    }

    #[test]
    fn type3_still_detects_same_file_different_line_duplicates() {
        // Ensure the self-match fix doesn't break legitimate same-file duplicates
        let scanner = Scanner::with_config(3, 0.85)
            .unwrap()
            .with_type3_detection(0.75)
            .unwrap();

        // Two similar functions in the SAME file but DIFFERENT lines
        let tokens1 = make_expr_tokens("let", "+");
        let mut tokens2 = make_expr_tokens("let", "-");
        tokens2.push(Token::Identifier); // Make it Type-3 (not exact)

        let func1 = make_test_function_same_line("same_file.rs", 10, 15, tokens1, "fn first()");
        let func2 = make_test_function_same_line("same_file.rs", 50, 55, tokens2, "fn second()");

        let (duplicates, _) = scanner.find_duplicate_hashes(&[func1, func2]);

        let same_file_different_line: Vec<_> = duplicates
            .iter()
            .filter(|d| d.file1 == d.file2 && d.start_line1 != d.start_line2)
            .collect();

        assert!(
            !same_file_different_line.is_empty(),
            "Type-3 should still detect duplicates in the same file at different lines"
        );
    }

    #[test]
    fn duplicate_matches_store_actual_end_lines() {
        let scanner = Scanner::with_config(2, 0.85).unwrap();

        let tokens = vec![
            Token::Keyword("fn".into()),
            Token::Identifier,
            Token::Identifier,
            Token::Punctuation("{".into()),
            Token::Punctuation("}".into()),
        ];

        let func1 = FunctionHash {
            file_path: Arc::<str>::from("file_a.rs"),
            function_name: None,
            start_byte: 0,
            end_byte: 0,
            start_line: 10,
            end_line: 14,
            tokens: tokens.clone(),
            token_line_offsets: vec![0, 0, 1, 1, 2],
            raw_body: "fn a() {}".to_string(),
        };

        let func2 = FunctionHash {
            file_path: Arc::<str>::from("file_b.rs"),
            function_name: None,
            start_byte: 0,
            end_byte: 0,
            start_line: 20,
            end_line: 24,
            tokens,
            token_line_offsets: vec![0, 1, 1, 2, 2],
            raw_body: "fn b() {}".to_string(),
        };

        let (duplicates, _) = scanner.find_duplicate_hashes(&[func1, func2]);
        let dup = duplicates.first().expect("expected a duplicate match");

        assert_eq!(dup.start_line1, 10);
        assert_eq!(dup.start_line2, 20);
        assert_eq!(dup.end_line1, Some(12));
        assert_eq!(dup.end_line2, Some(22));
    }

    #[test]
    fn scan_with_cache_prunes_stale_entries() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_a = temp_dir.path().join("a.js");
        let file_b = temp_dir.path().join("b.js");

        let shared_fn = r#"
        function shared() {
          return 1 + 1;
        }
        "#;
        std::fs::write(&file_a, shared_fn).unwrap();
        std::fs::write(&file_b, shared_fn).unwrap();

        let scanner = Scanner::with_config(3, 0.85).unwrap();
        let mut cache = scanner
            .build_cache(vec![file_a.clone(), file_b.clone()])
            .unwrap();

        // Change the non-diff file so its cached hashes are outdated
        std::thread::sleep(std::time::Duration::from_millis(1100));
        std::fs::write(&file_b, "const unrelated = 42;\n").unwrap();

        let report = scanner
            .scan_with_cache(vec![file_a.clone()], &mut cache)
            .unwrap();

        assert!(
            report.duplicates.is_empty(),
            "stale cache entries should be invalidated before lookup"
        );
    }

    #[test]
    fn scan_with_cache_repopulates_changed_entries() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_a = temp_dir.path().join("a.js");

        let original = r#"
        function shared() {
          return 1 + 1;
        }
        "#;

        let updated = r#"
        function shared() {
          return 7 + 8;
        }
        "#;

        std::fs::write(&file_a, original).unwrap();

        let scanner = Scanner::with_config(3, 0.85).unwrap();
        let mut cache = scanner.build_cache(vec![file_a.clone()]).unwrap();

        std::thread::sleep(std::time::Duration::from_millis(1100));
        std::fs::write(&file_a, updated).unwrap();

        let file_a_str = file_a.to_string_lossy().to_string();
        assert!(
            cache.file_needs_rescan(&file_a_str),
            "modified files should be considered stale before cache lookup"
        );

        scanner
            .scan_with_cache(vec![file_a.clone()], &mut cache)
            .unwrap();

        let cached_entries: Vec<&CodeLocation> = cache
            .hash_index
            .values()
            .flat_map(|locs| locs.iter())
            .filter(|loc| loc.file_path == file_a_str)
            .collect();

        assert!(
            !cached_entries.is_empty(),
            "changed files should be added back into the cache after rescan"
        );
        assert!(
            cached_entries
                .iter()
                .any(|loc| loc.raw_source.contains("return 7 + 8;")),
            "cache should contain hashes for the refreshed file contents"
        );
        assert!(
            cache.file_metadata.contains_key(&file_a_str),
            "file metadata should be refreshed after rescanning changed files"
        );
    }

    #[test]
    fn scan_with_cache_rehydrates_stale_unchanged_files() {
        let temp_dir = tempfile::tempdir().unwrap();
        let changed_file = temp_dir.path().join("changed.js");
        let unchanged_file = temp_dir.path().join("unchanged.js");

        let shared_fn = r#"
        function shared() {
          return 1 + 1;
        }
        "#;

        std::fs::write(&changed_file, shared_fn).unwrap();
        std::fs::write(&unchanged_file, shared_fn).unwrap();

        let scanner = Scanner::with_config(3, 0.85).unwrap();
        let mut cache = scanner
            .build_cache(vec![temp_dir.path().to_path_buf()])
            .unwrap();

        // Simulate a restored cache where file mtimes no longer match.
        std::thread::sleep(std::time::Duration::from_millis(1100));
        std::fs::write(
            &changed_file,
            r#"
        function shared() {
          return 1 + 1;
        }
        function another() {
          return 1 + 1;
        }
        "#,
        )
        .unwrap();
        std::fs::write(&unchanged_file, shared_fn).unwrap();

        let report = scanner
            .scan_with_cache(vec![changed_file.clone()], &mut cache)
            .unwrap();

        assert!(
            report.duplicates.iter().any(|dup| {
                (dup.file1.ends_with("changed.js") && dup.file2.ends_with("unchanged.js"))
                    || (dup.file1.ends_with("unchanged.js") && dup.file2.ends_with("changed.js"))
            }),
            "invalidated entries should be rebuilt so unchanged files still match against diffs"
        );
    }

    #[test]
    fn scan_with_cache_respects_ignore_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_a = temp_dir.path().join("a.js");
        let file_b = temp_dir.path().join("b.js");

        let shared_fn = r#"
        function shared() {
          return 1 + 1;
        }
        "#;
        std::fs::write(&file_a, shared_fn).unwrap();
        std::fs::write(&file_b, shared_fn).unwrap();

        let base_scanner = Scanner::with_config(3, 0.85).unwrap();
        let mut cache = base_scanner
            .build_cache(vec![temp_dir.path().to_path_buf()])
            .unwrap();

        let initial_report = base_scanner
            .scan_with_cache(vec![file_a.clone()], &mut cache)
            .unwrap();
        assert!(
            !initial_report.duplicates.is_empty(),
            "expected an initial duplicate to seed ignore entries"
        );
        let ignored_ids: Vec<String> = initial_report
            .duplicates
            .iter()
            .map(|d| {
                d.duplicate_id
                    .clone()
                    .expect("expected cache path to compute duplicate IDs")
            })
            .collect();

        let mut manager = IgnoreManager::new(temp_dir.path());
        for id in ignored_ids {
            manager.add_ignore(IgnoreEntry::new(
                id,
                vec![],
                "test ignore".to_string(),
                "tester".to_string(),
            ));
        }

        let scanner = base_scanner.with_ignore_manager(manager);
        let report = scanner
            .scan_with_cache(vec![file_a.clone()], &mut cache)
            .unwrap();

        assert!(
            report.duplicates.is_empty(),
            "duplicates present in .polydup-ignore should be filtered when using cache"
        );
    }

    #[test]
    fn scan_with_cache_uses_symmetric_ids_for_existing_ignores() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_a = temp_dir.path().join("a.js");
        let file_b = temp_dir.path().join("b.js");

        let shared_fn = r#"
        function shared() {
          return 1 + 1;
        }
        "#;
        std::fs::write(&file_a, shared_fn).unwrap();
        std::fs::write(&file_b, shared_fn).unwrap();

        let base_scanner = Scanner::with_config(7, 0.85).unwrap();
        let mut cache = base_scanner
            .build_cache(vec![temp_dir.path().to_path_buf()])
            .unwrap();

        let baseline_report = base_scanner
            .scan(vec![temp_dir.path().to_path_buf()])
            .unwrap();
        let baseline_id = baseline_report
            .duplicates
            .first()
            .and_then(|dup| dup.duplicate_id.clone())
            .expect("expected duplicate IDs from full scans");
        let baseline_id_for_ignore = baseline_id.clone();

        let mut manager = IgnoreManager::new(temp_dir.path());
        manager.add_ignore(IgnoreEntry::new(
            baseline_id_for_ignore,
            vec![],
            "test ignore".to_string(),
            "tester".to_string(),
        ));

        let scanner = base_scanner.with_ignore_manager(manager);
        let report = scanner
            .scan_with_cache(vec![file_a.clone()], &mut cache)
            .unwrap();

        assert!(
            report.duplicates.is_empty(),
            "cached scans should honor ignores generated from full scans"
        );
    }

    #[test]
    fn scan_with_cache_respects_directives_from_cached_files() {
        let temp_dir = tempfile::tempdir().unwrap();
        let changed_file = temp_dir.path().join("changed.js");
        let cached_file = temp_dir.path().join("cached.js");

        let suppressed_fn = r#"
        // polydup-ignore: generated code
        function shared() {
          return 1 + 1;
        }
        "#;

        let changed_fn = r#"
        function shared() {
          return 1 + 1;
        }
        "#;

        std::fs::write(&cached_file, suppressed_fn).unwrap();
        std::fs::write(&changed_file, changed_fn).unwrap();

        let scanner = Scanner::with_config(3, 0.85).unwrap().with_directives(true);
        let mut cache = scanner
            .build_cache(vec![temp_dir.path().to_path_buf()])
            .unwrap();

        let report = scanner
            .scan_with_cache(vec![changed_file.clone()], &mut cache)
            .unwrap();

        assert!(
            report.duplicates.is_empty(),
            "duplicates suppressed by directives in cached files should stay suppressed when using cache"
        );
    }

    #[test]
    fn scan_with_cache_runs_type3_detection_against_cached_files() {
        let temp_dir = tempfile::tempdir().unwrap();
        let changed_file = temp_dir.path().join("changed.js");
        let cached_file = temp_dir.path().join("cached.js");

        let cached_fn = r#"
        function cached() {
          step1();
          step2();
          step3();
          step4();
          step5();
        }
        "#;

        let changed_fn = r#"
        function cached() {
          step1();
          step2();
          insert_gap();
          step3();
          step4();
          step5();
        }
        "#;

        std::fs::write(&cached_file, cached_fn).unwrap();
        std::fs::write(&changed_file, changed_fn).unwrap();

        let scanner = Scanner::with_config(3, 0.8)
            .unwrap()
            .with_type3_detection(0.8)
            .unwrap();
        let mut cache = scanner
            .build_cache(vec![temp_dir.path().to_path_buf()])
            .unwrap();

        let report = scanner
            .scan_with_cache(vec![changed_file.clone()], &mut cache)
            .unwrap();

        assert!(
            report.duplicates.iter().any(|dup| {
                matches!(dup.clone_type, CloneType::Type3)
                    && dup.file1.ends_with("changed.js")
                    && dup.file2.ends_with("cached.js")
            }),
            "Type-3 should run for cached comparisons so near-miss clones surface in git-diff mode"
        );
    }

    #[test]
    fn test_find_duplicates_empty() {
        let result = find_duplicates(vec![]);
        assert!(result.is_ok());
        let report = result.unwrap();
        assert_eq!(report.duplicates.len(), 0);
    }

    #[test]
    fn test_is_supported_file() {
        let scanner = Scanner::new();

        assert!(scanner.is_supported_file(Path::new("test.rs")));
        assert!(scanner.is_supported_file(Path::new("test.py")));
        assert!(scanner.is_supported_file(Path::new("test.js")));
        assert!(scanner.is_supported_file(Path::new("test.ts")));
        assert!(!scanner.is_supported_file(Path::new("test.txt")));
        assert!(!scanner.is_supported_file(Path::new("test.md")));
    }

    #[test]
    fn test_detect_language() {
        let scanner = Scanner::new();

        assert!(scanner.detect_language(Path::new("test.rs")).is_ok());
        assert!(scanner.detect_language(Path::new("test.py")).is_ok());
        assert!(scanner.detect_language(Path::new("test.js")).is_ok());
        assert!(scanner.detect_language(Path::new("test.txt")).is_err());
    }
}
