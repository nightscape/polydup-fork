//! Ignore management for marking acceptable duplicates.
//!
//! This module provides functionality to:
//! - Load and save `.polydup-ignore` files
//! - Compute content-based IDs for duplicates (SHA256 of normalized tokens)
//! - Check if a duplicate should be ignored
//! - Persist ignore decisions across file renames and refactors

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::error::{PolyDupError, Result};

/// Version of the .polydup-ignore file format
const IGNORE_FILE_VERSION: u32 = 1;

/// Represents a range within a file (e.g., "src/main.rs:10-25")
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileRange {
    pub file: PathBuf,
    pub start_line: usize,
    pub end_line: usize,
}

impl std::fmt::Display for FileRange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}:{}-{}",
            self.file.display(),
            self.start_line,
            self.end_line
        )
    }
}

impl FileRange {
    /// Parse a file range from a string like "src/main.rs:10-25"
    pub fn parse(s: &str) -> Result<Self> {
        let parts: Vec<&str> = s.rsplitn(2, ':').collect();
        if parts.len() != 2 {
            return Err(PolyDupError::IgnoreRule(format!(
                "Invalid file range format: {}",
                s
            )));
        }

        let file = PathBuf::from(parts[1]);
        let range_parts: Vec<&str> = parts[0].split('-').collect();

        if range_parts.len() != 2 {
            return Err(PolyDupError::IgnoreRule(format!(
                "Invalid line range format: {}",
                s
            )));
        }

        let start_line = range_parts[0]
            .parse()
            .map_err(|_| PolyDupError::IgnoreRule("Invalid start line number".to_string()))?;
        let end_line = range_parts[1]
            .parse()
            .map_err(|_| PolyDupError::IgnoreRule("Invalid end line number".to_string()))?;

        if start_line > end_line {
            return Err(PolyDupError::IgnoreRule(format!(
                "Start line ({}) must be <= end line ({})",
                start_line, end_line
            )));
        }

        Ok(FileRange {
            file,
            start_line,
            end_line,
        })
    }
}

/// A single ignore entry representing an acceptable duplicate
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IgnoreEntry {
    /// Content-based ID (SHA256 of normalized token sequence)
    /// This ensures ignores persist across file renames and whitespace changes
    pub id: String,

    /// Files and line ranges where this duplicate appears
    pub files: Vec<FileRange>,

    /// Human-readable reason for ignoring this duplicate
    pub reason: String,

    /// User who added this ignore (email or username)
    pub added_by: String,

    /// Timestamp when this ignore was added
    pub added_at: DateTime<Utc>,
}

impl IgnoreEntry {
    /// Create a new ignore entry with the current timestamp
    pub fn new(id: String, files: Vec<FileRange>, reason: String, added_by: String) -> Self {
        Self {
            id,
            files,
            reason,
            added_by,
            added_at: Utc::now(),
        }
    }

    /// Check if this ignore entry matches the given duplicate ID
    pub fn matches_id(&self, duplicate_id: &str) -> bool {
        self.id == duplicate_id
    }
}

/// Container for the .polydup-ignore file format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IgnoreFile {
    pub version: u32,
    pub ignores: Vec<IgnoreEntry>,
}

impl Default for IgnoreFile {
    fn default() -> Self {
        Self {
            version: IGNORE_FILE_VERSION,
            ignores: Vec::new(),
        }
    }
}

/// Manages loading, saving, and querying ignore entries
pub struct IgnoreManager {
    ignore_file_path: PathBuf,
    ignore_file: IgnoreFile,
    ignored_ids: HashSet<String>,
}

impl IgnoreManager {
    /// Create a new IgnoreManager for the given directory
    pub fn new(directory: &Path) -> Self {
        let ignore_file_path = directory.join(".polydup-ignore");
        Self {
            ignore_file_path,
            ignore_file: IgnoreFile::default(),
            ignored_ids: HashSet::new(),
        }
    }

    /// Load the .polydup-ignore file if it exists
    pub fn load(&mut self) -> Result<()> {
        if !self.ignore_file_path.exists() {
            // No ignore file is not an error - just means nothing is ignored
            return Ok(());
        }

        let contents = std::fs::read_to_string(&self.ignore_file_path).map_err(PolyDupError::Io)?;

        self.ignore_file = toml::from_str(&contents).map_err(|e| {
            PolyDupError::Parsing(format!("Failed to parse .polydup-ignore file: {}", e))
        })?;

        // Validate version
        if self.ignore_file.version > IGNORE_FILE_VERSION {
            return Err(PolyDupError::Config(format!(
                "Unsupported .polydup-ignore version: {} (expected <= {})",
                self.ignore_file.version, IGNORE_FILE_VERSION
            )));
        }

        // Build lookup set for fast ID checks
        self.ignored_ids = self
            .ignore_file
            .ignores
            .iter()
            .map(|entry| entry.id.clone())
            .collect();

        Ok(())
    }

    /// Save the current ignore entries to .polydup-ignore
    pub fn save(&self) -> Result<()> {
        let contents = toml::to_string_pretty(&self.ignore_file).map_err(|e| {
            PolyDupError::Parsing(format!("Failed to serialize ignore file: {}", e))
        })?;

        std::fs::write(&self.ignore_file_path, contents).map_err(PolyDupError::Io)?;

        Ok(())
    }

    /// Check if a duplicate with the given ID should be ignored
    pub fn is_ignored(&self, duplicate_id: &str) -> bool {
        self.ignored_ids.contains(duplicate_id)
    }

    /// Add a new ignore entry
    pub fn add_ignore(&mut self, entry: IgnoreEntry) {
        self.ignored_ids.insert(entry.id.clone());
        self.ignore_file.ignores.push(entry);
    }

    /// Remove an ignore entry by ID (supports prefix matching for short IDs)
    pub fn remove_ignore(&mut self, duplicate_id: &str) -> bool {
        // First try exact match
        if let Some(pos) = self
            .ignore_file
            .ignores
            .iter()
            .position(|e| e.id == duplicate_id)
        {
            self.ignore_file.ignores.remove(pos);
            self.ignored_ids.remove(duplicate_id);
            return true;
        }

        // Try prefix match for short IDs
        if let Some(full_id) = self.find_unique_by_prefix(duplicate_id) {
            if let Some(pos) = self
                .ignore_file
                .ignores
                .iter()
                .position(|e| e.id == full_id)
            {
                self.ignore_file.ignores.remove(pos);
                self.ignored_ids.remove(&full_id);
                return true;
            }
        }

        false
    }

    /// Find a unique ID by prefix (for short ID matching)
    ///
    /// Returns:
    /// - `Some(full_id)` if exactly one ID matches the prefix
    /// - `None` if no IDs match or multiple IDs match (ambiguous)
    pub fn find_unique_by_prefix(&self, prefix: &str) -> Option<String> {
        let matches: Vec<&String> = self
            .ignored_ids
            .iter()
            .filter(|id| id.contains(prefix) || id.ends_with(prefix))
            .collect();

        if matches.len() == 1 {
            Some(matches[0].clone())
        } else {
            None
        }
    }

    /// Find all IDs matching a prefix (for displaying ambiguous matches)
    pub fn find_all_by_prefix(&self, prefix: &str) -> Vec<String> {
        self.ignored_ids
            .iter()
            .filter(|id| id.contains(prefix) || id.ends_with(prefix))
            .cloned()
            .collect()
    }

    /// Get all ignore entries
    pub fn list_ignores(&self) -> &[IgnoreEntry] {
        &self.ignore_file.ignores
    }

    /// Get the number of ignored duplicates
    pub fn count(&self) -> usize {
        self.ignore_file.ignores.len()
    }
}

/// Compute a content-based ID for a duplicate
///
/// This uses SHA256 of the normalized token sequence, ensuring:
/// - Ignores survive file renames
/// - Ignores survive whitespace/comment changes
/// - Two identical code blocks get the same ID
pub fn compute_duplicate_id(normalized_tokens: &[String]) -> String {
    let mut hasher = Sha256::new();

    // Hash the concatenated normalized tokens
    for token in normalized_tokens {
        hasher.update(token.as_bytes());
        hasher.update(b"\n"); // Separator to avoid collisions
    }

    let result = hasher.finalize();
    format!("sha256:{}", hex::encode(result))
}

/// Compute a symmetric ID for a pair of token windows.
///
/// When both sides normalize to the same token sequence (Type-1/2 clones),
/// this returns the legacy single-window ID to keep existing ignore files valid.
pub fn compute_symmetric_duplicate_id(
    normalized_tokens1: &[String],
    normalized_tokens2: &[String],
) -> String {
    let id1 = compute_duplicate_id(normalized_tokens1);
    let id2 = compute_duplicate_id(normalized_tokens2);

    // Preserve legacy IDs when both windows hash the same (or collide)
    if id1 == id2 {
        return id1;
    }

    let (first, second) = if id1 <= id2 { (id1, id2) } else { (id2, id1) };

    let mut hasher = Sha256::new();
    hasher.update(first.as_bytes());
    hasher.update(b"\n");
    hasher.update(second.as_bytes());

    let result = hasher.finalize();
    format!("sha256:{}", hex::encode(result))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_range_parse() {
        let range = FileRange::parse("src/main.rs:10-25").unwrap();
        assert_eq!(range.file, PathBuf::from("src/main.rs"));
        assert_eq!(range.start_line, 10);
        assert_eq!(range.end_line, 25);
    }

    #[test]
    fn test_file_range_parse_invalid() {
        assert!(FileRange::parse("invalid").is_err());
        assert!(FileRange::parse("src/main.rs").is_err());
        assert!(FileRange::parse("src/main.rs:10").is_err());
        assert!(FileRange::parse("src/main.rs:25-10").is_err()); // start > end
    }

    #[test]
    fn test_file_range_display() {
        let range = FileRange {
            file: PathBuf::from("src/lib.rs"),
            start_line: 5,
            end_line: 15,
        };
        assert_eq!(range.to_string(), "src/lib.rs:5-15");
    }

    #[test]
    fn test_compute_duplicate_id() {
        let tokens1 = vec!["fn".to_string(), "$$ID".to_string(), "$$NUM".to_string()];
        let tokens2 = vec!["fn".to_string(), "$$ID".to_string(), "$$NUM".to_string()];
        let tokens3 = vec!["fn".to_string(), "$$ID".to_string(), "$$STR".to_string()];

        let id1 = compute_duplicate_id(&tokens1);
        let id2 = compute_duplicate_id(&tokens2);
        let id3 = compute_duplicate_id(&tokens3);

        assert_eq!(id1, id2, "Same tokens should produce same ID");
        assert_ne!(id1, id3, "Different tokens should produce different IDs");
        assert!(id1.starts_with("sha256:"), "ID should have sha256 prefix");
    }

    #[test]
    fn test_compute_duplicate_id_symmetric_same_tokens() {
        let tokens = vec!["a".to_string(), "b".to_string()];

        let symmetric = compute_symmetric_duplicate_id(&tokens, &tokens);
        let single = compute_duplicate_id(&tokens);

        assert_eq!(
            symmetric, single,
            "Symmetric ID should match legacy ID when windows are identical"
        );
    }

    #[test]
    fn test_compute_duplicate_id_symmetric_order_independent() {
        let tokens_a = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let tokens_b = vec![
            "a".to_string(),
            "b".to_string(),
            "c".to_string(),
            "d".to_string(),
        ];

        let id1 = compute_symmetric_duplicate_id(&tokens_a, &tokens_b);
        let id2 = compute_symmetric_duplicate_id(&tokens_b, &tokens_a);

        assert_eq!(id1, id2, "Symmetric ID should ignore argument order");
        assert_ne!(
            id1,
            compute_duplicate_id(&tokens_a),
            "Should incorporate both windows when they differ"
        );
    }

    #[test]
    fn test_ignore_entry_creation() {
        let files = vec![FileRange {
            file: PathBuf::from("src/main.rs"),
            start_line: 1,
            end_line: 10,
        }];

        let entry = IgnoreEntry::new(
            "sha256:abc123".to_string(),
            files.clone(),
            "License header".to_string(),
            "user@example.com".to_string(),
        );

        assert_eq!(entry.id, "sha256:abc123");
        assert_eq!(entry.files, files);
        assert_eq!(entry.reason, "License header");
        assert_eq!(entry.added_by, "user@example.com");
    }

    #[test]
    fn test_ignore_manager_basic() {
        let temp_dir = std::env::temp_dir();
        let mut manager = IgnoreManager::new(&temp_dir);

        // Initially no ignores
        assert_eq!(manager.count(), 0);
        assert!(!manager.is_ignored("sha256:test"));

        // Add an ignore
        let entry = IgnoreEntry::new(
            "sha256:test".to_string(),
            vec![],
            "Test".to_string(),
            "test@example.com".to_string(),
        );
        manager.add_ignore(entry);

        assert_eq!(manager.count(), 1);
        assert!(manager.is_ignored("sha256:test"));
        assert!(!manager.is_ignored("sha256:other"));

        // Remove the ignore
        assert!(manager.remove_ignore("sha256:test"));
        assert_eq!(manager.count(), 0);
        assert!(!manager.is_ignored("sha256:test"));
    }

    #[test]
    fn test_ignore_manager_remove_nonexistent() {
        let temp_dir = std::env::temp_dir();
        let mut manager = IgnoreManager::new(&temp_dir);

        assert!(!manager.remove_ignore("sha256:nonexistent"));
    }
}
