//! Scan command implementation
//!
//! This module encapsulates the core duplicate scanning logic, including
//! configuration merging, validation, scanning, filtering, and output.

use anyhow::{Context, Result};
use colored::*;
use indicatif::{ProgressBar, ProgressStyle};
use std::path::{Path, PathBuf};

use crate::cli::OutputFormat;
use crate::config;
use crate::reporting::format_text_report;
use crate::vcs;

// Error message formatting helpers to avoid duplication
fn fmt_suggestion() -> String {
    format!("{}", "Suggestion:".bright_yellow().bold())
}

fn fmt_indent() -> String {
    format!("{}", "           ".dimmed())
}

fn fmt_documentation() -> String {
    format!("{}", "Documentation:".bright_blue().bold())
}

/// Configuration for a scan operation
///
/// This struct reduces "argument drilling" by grouping related parameters.
pub struct ScanConfig {
    pub paths: Vec<PathBuf>,
    pub format: OutputFormat,
    pub min_block_size: usize,
    pub similarity: f64,
    pub verbose: bool,
    pub quiet: bool,
    pub exclude: Vec<String>,
    pub enable_type3: bool,
    pub type3_tolerance: f64,
    pub output: Option<PathBuf>,
    pub no_color: bool,
    pub only_type: Vec<String>,
    pub exclude_type: Vec<String>,
    pub group_by: Option<String>,
    pub debug: bool,
    pub save_baseline: Option<PathBuf>,
    pub compare_to: Option<PathBuf>,
    pub git_diff: Option<String>,
    pub enable_directives: bool,
    pub include_tests: bool,
    pub progress: bool,
    pub no_progress: bool,
    pub fail_on_duplicates: bool,
    pub fail_on_new: bool,
    pub show_code: bool,
    pub preview_lines: usize,
    pub suggest_refactoring: bool,
    pub show_ids: bool,
    pub dry_run: bool,
    pub id_length: usize,
    pub full_ids: bool,
}

impl ScanConfig {
    /// Returns true if we should print informational messages (not quiet mode)
    #[inline]
    pub fn should_print(&self) -> bool {
        !self.quiet
    }

    /// Validate the scan configuration
    pub fn validate(&self) -> Result<()> {
        // Validate paths exist
        for path in &self.paths {
            if !path.exists() {
                let mut error_msg = format!("Path does not exist: {}", path.display());
                error_msg.push_str("\n\n");
                error_msg.push_str(&format!("{}", "Suggestion:".bright_yellow().bold()));
                error_msg.push_str(
                    " Check the path spelling and ensure the file or directory exists.\n",
                );
                error_msg.push_str(&format!("{}", "           ".dimmed()));
                error_msg.push_str("Use absolute paths if relative paths are not working.\n");

                if self.debug {
                    error_msg.push_str(&format!("\n{}", "Debug Info:".bright_cyan().bold()));
                    error_msg.push_str(&format!(
                        "\n  Current directory: {}",
                        std::env::current_dir().unwrap_or_default().display()
                    ));
                }

                anyhow::bail!(error_msg);
            }
        }

        // Validate filter flags
        for type_str in &self.only_type {
            if !matches!(type_str.as_str(), "type-1" | "type-2" | "type-3") {
                let mut error_msg = format!("Invalid --only-type value: '{}'", type_str);
                error_msg.push_str("\n\n");
                error_msg.push_str(&fmt_suggestion());
                error_msg.push_str(" Use one of: type-1, type-2, or type-3\n");
                error_msg.push_str(&fmt_indent());
                error_msg.push_str("Example: polydup scan ./src --only-type type-1\n");
                error_msg.push_str(&fmt_indent());
                error_msg.push_str("Multiple types: --only-type type-1,type-2\n\n");
                error_msg.push_str(&fmt_documentation());
                error_msg.push_str(" https://github.com/wiesnerbernard/polydup#clone-types\n");
                anyhow::bail!(error_msg);
            }
        }
        for type_str in &self.exclude_type {
            if !matches!(type_str.as_str(), "type-1" | "type-2" | "type-3") {
                let mut error_msg = format!("Invalid --exclude-type value: '{}'", type_str);
                error_msg.push_str("\n\n");
                error_msg.push_str(&fmt_suggestion());
                error_msg.push_str(" Use one of: type-1, type-2, or type-3\n");
                error_msg.push_str(&fmt_indent());
                error_msg.push_str("Example: polydup scan ./src --exclude-type type-3\n");
                anyhow::bail!(error_msg);
            }
        }

        // Validate group-by flag
        if let Some(ref group_criterion) = self.group_by {
            if !matches!(
                group_criterion.as_str(),
                "file" | "similarity" | "type" | "size"
            ) {
                let mut error_msg = format!("Invalid --group-by value: '{}'", group_criterion);
                error_msg.push_str("\n\n");
                error_msg.push_str(&fmt_suggestion());
                error_msg.push_str(" Use one of: file, similarity, type, or size\n");
                error_msg.push_str(&fmt_indent());
                error_msg.push_str("Example: polydup scan ./src --group-by file\n\n");
                error_msg.push_str(&format!("{}", "Options explained:".bright_white()));
                error_msg.push_str(
                    "\n  • file       - Group by source file (refactoring prioritization)\n",
                );
                error_msg.push_str("  • similarity - Sort by match quality (highest first)\n");
                error_msg.push_str("  • type       - Group by Type-1/Type-2/Type-3\n");
                error_msg.push_str("  • size       - Sort by duplicate length (largest first)\n");
                anyhow::bail!(error_msg);
            }
        }

        // Validate numeric ranges
        if !(0.0..=1.0).contains(&self.similarity) {
            let mut error_msg = format!(
                "Similarity threshold must be between 0.0 and 1.0, got: {}",
                self.similarity
            );
            error_msg.push_str("\n\n");
            error_msg.push_str(&fmt_suggestion());
            error_msg.push_str(" Use a decimal value like 0.85 for 85% similarity\n");
            error_msg.push_str(&fmt_indent());
            error_msg.push_str("Example: polydup scan ./src --similarity 0.9\n");
            anyhow::bail!(error_msg);
        }

        if self.min_block_size == 0 {
            let mut error_msg = "Minimum block size must be greater than 0".to_string();
            error_msg.push_str("\n\n");
            error_msg.push_str(&fmt_suggestion());
            error_msg.push_str(" Use at least 10 tokens for meaningful results\n");
            error_msg.push_str(&fmt_indent());
            error_msg.push_str("Default is 50 tokens. Try values between 25-100.\n");
            error_msg.push_str(&fmt_indent());
            error_msg.push_str("Example: polydup scan ./src --min-tokens 50\n");
            anyhow::bail!(error_msg);
        }

        Ok(())
    }

    /// Merge configuration file settings with CLI arguments
    ///
    /// CLI arguments take precedence over config file settings.
    pub fn merge_with_config_file(&mut self) -> Result<()> {
        let config = config::Config::load().context("Failed to load configuration")?;

        if let Some(cfg) = config {
            // Merge min_block_size (use CLI value if non-default)
            if crate::defaults::is_default_min_block_size(self.min_block_size) {
                self.min_block_size = cfg.scan.min_block_size;
            }

            // Merge similarity (use CLI value if non-default)
            if crate::defaults::is_default_similarity(self.similarity) {
                self.similarity = cfg.scan.similarity_threshold;
            }

            // Merge exclude patterns
            for pattern in &cfg.scan.exclude.patterns {
                if !self.exclude.contains(pattern) {
                    self.exclude.push(pattern.clone());
                }
            }
        }

        Ok(())
    }

    /// Print verbose configuration summary
    pub fn print_summary(&self, config_path: Option<&Path>) {
        if self.verbose && self.should_print() {
            eprintln!("PolyDup - Scanning for duplicates");
            eprintln!("  Paths: {:?}", self.paths);
            eprintln!("  Min block size: {} tokens", self.min_block_size);
            eprintln!("  Similarity threshold: {:.1}%", self.similarity * 100.0);
            if let Some(path) = config_path {
                eprintln!("  Config: {}", path.display());
            }
            eprintln!();
        }
    }
}

/// Execute the scan command
pub fn run(mut config: ScanConfig) -> Result<()> {
    // Set up error reporting based on debug flag
    if config.debug {
        std::env::set_var("RUST_BACKTRACE", "1");
    }

    // Configure colored output based on flags and environment
    // Respect NO_COLOR environment variable (https://no-color.org/)
    if config.no_color || std::env::var("NO_COLOR").is_ok() {
        colored::control::set_override(false);
    }

    // Validate configuration
    config.validate()?;

    // Merge with config file
    config.merge_with_config_file()?;

    // Handle dry-run mode
    if config.dry_run {
        return handle_dry_run(&config);
    }

    // Warn about Type-3 detection with low threshold
    // Type-3 at low thresholds produces excessive false positives (see issue #15)
    const TYPE3_RECOMMENDED_MIN_THRESHOLD: usize = 75;
    if config.enable_type3
        && config.min_block_size < TYPE3_RECOMMENDED_MIN_THRESHOLD
        && config.format == OutputFormat::Text
    {
        eprintln!(
            "{}",
            format!(
                "Warning: Type-3 detection with threshold {} may produce many false positives.",
                config.min_block_size
            )
            .bright_yellow()
        );
        eprintln!(
            "         Consider using --min-tokens {} or higher for Type-3 detection.",
            TYPE3_RECOMMENDED_MIN_THRESHOLD
        );
        eprintln!("         See: https://github.com/wiesnerbernard/polydup/issues/15\n");
    }

    // Show scan configuration if verbose
    config.print_summary(config::Config::config_path().as_deref());

    // Get changed files from git diff if requested
    let changed_files = get_changed_files_if_requested(&config)?;

    let mut git_diff_filtered = false;

    // Create scanner with configuration
    let scanner = create_scanner(&config)?;

    // Show progress indicator for text output (suppress in JSON mode)
    let progress = create_progress_bar(&config);

    // Debug: Show scan configuration
    if config.debug {
        eprintln!("{}", "Debug: Scan Configuration".bright_cyan().bold());
        eprintln!("  Paths: {:?}", config.paths);
        eprintln!("  Min block size: {} tokens", config.min_block_size);
        eprintln!("  Similarity threshold: {:.0}%", config.similarity * 100.0);
        eprintln!("  Type-3 detection: {}", config.enable_type3);
        eprintln!("  Include tests: {}", config.include_tests);
        if changed_files.is_some() {
            eprintln!("  Git-diff mode: Using hash cache if available");
        }
        eprintln!();
    }

    // Perform scan - use cache in git-diff mode if available
    let scan_start = std::time::Instant::now();
    let mut report = if let Some(ref changed) = changed_files {
        // Git-diff mode: Try to use cache for fast scanning
        let cache_path = PathBuf::from(".polydup-cache.json");

        // Fallback helper: run a full scan when cache isn't usable
        let scan_full_project = || -> Result<dupe_core::Report> {
            scanner
                .scan(config.paths.clone())
                .map_err(|e| build_scan_error(e.into(), &config.paths, &config))
        };

        let mut report = if cache_path.exists() {
            // Load cache and use fast path
            match dupe_core::HashCache::load(&cache_path) {
                Ok(mut cache) => {
                    // Validate cache threshold matches scanner threshold
                    if cache.min_block_size != config.min_block_size {
                        if config.verbose && config.should_print() {
                            eprintln!("{}", format!("Warning: Cache was built with threshold {} but scanner is using {}", cache.min_block_size, config.min_block_size).yellow());
                            eprintln!("{}", format!("         Run 'polydup cache build --min-tokens {}' to rebuild cache", config.min_block_size).yellow());
                            eprintln!(
                                "{}",
                                "         Falling back to git-diff scan without cache...".yellow()
                            );
                        }
                        // Fallback: scan entire project then filter to changed files
                        scan_full_project()?
                    } else {
                        // Cache threshold matches, use it
                        if config.verbose && config.should_print() {
                            eprintln!("{}", "Using existing cache: .polydup-cache.json".green());
                            eprintln!(
                                "{}",
                                format!(
                                    "Scanning {} changed files with cached hash lookup...",
                                    changed.len()
                                )
                                .cyan()
                            );
                        }

                        scanner
                            .scan_with_cache(changed.clone(), &mut cache)
                            .map_err(|e| build_scan_error(e.into(), &config.paths, &config))?
                    }
                }
                Err(e) => {
                    if config.verbose && config.should_print() {
                        eprintln!(
                            "{}",
                            format!("Warning: Failed to load cache: {}", e).yellow()
                        );
                        eprintln!(
                            "{}",
                            "Falling back to git-diff scan without cache...".yellow()
                        );
                    }
                    // Fallback: scan entire project then filter to changed files
                    scan_full_project()?
                }
            }
        } else {
            if config.verbose && config.should_print() {
                eprintln!("{}", "No cache found (.polydup-cache.json)".yellow());
                eprintln!(
                    "{}",
                    "Tip: Run 'polydup cache build' to create cache for faster scans".yellow()
                );
                eprintln!(
                    "{}",
                    "Falling back to git-diff scan without cache...".yellow()
                );
            }
            // Fallback: scan entire project then filter to changed files
            scan_full_project()?
        };

        // Ensure git-diff scope is preserved even on cache fallback
        filter_duplicates_by_changed_files(
            &mut report,
            changed,
            config.verbose && config.should_print(),
        )?;
        git_diff_filtered = true;
        report
    } else {
        // Normal mode: scan all files
        scanner
            .scan(config.paths.clone())
            .map_err(|e| build_scan_error(e.into(), &config.paths, &config))?
    };
    let scan_duration = scan_start.elapsed();

    // Debug: Show scan results
    if config.debug {
        eprintln!("{}", "Debug: Scan Results".bright_cyan().bold());
        eprintln!("  Files scanned: {}", report.files_scanned);
        eprintln!("  Functions analyzed: {}", report.functions_analyzed);
        eprintln!("  Duplicates found: {}", report.duplicates.len());
        eprintln!("  Scan duration: {:?}", scan_duration);
        eprintln!("  Total tokens: {}", report.stats.total_tokens);
        eprintln!("  Unique hashes: {}", report.stats.unique_hashes);

        // Show duplicate breakdown by type
        if !report.duplicates.is_empty() {
            let type1_count = report
                .duplicates
                .iter()
                .filter(|d| matches!(d.clone_type, dupe_core::CloneType::Type1))
                .count();
            let type2_count = report
                .duplicates
                .iter()
                .filter(|d| matches!(d.clone_type, dupe_core::CloneType::Type2))
                .count();
            let type3_count = report
                .duplicates
                .iter()
                .filter(|d| matches!(d.clone_type, dupe_core::CloneType::Type3))
                .count();
            eprintln!(
                "  Clone types: Type-1: {}, Type-2: {}, Type-3: {}",
                type1_count, type2_count, type3_count
            );
        }
        eprintln!();
    }

    // Populate report metadata
    report.version = Some(env!("CARGO_PKG_VERSION").to_string());
    report.scan_time = Some(chrono::Utc::now().to_rfc3339());
    report.config = Some(dupe_core::ScanConfig {
        threshold: config.min_block_size,
        similarity: config.similarity,
        type3_enabled: config.enable_type3,
        paths: Some(
            config
                .paths
                .iter()
                .map(|p| p.display().to_string())
                .collect(),
        ),
    });

    // Finish progress indicator
    if let Some(pb) = progress {
        pb.finish_and_clear();
    }

    // Filter duplicates in git-diff mode
    if let Some(ref changed) = changed_files {
        if !git_diff_filtered {
            filter_duplicates_by_changed_files(
                &mut report,
                changed,
                config.verbose && config.should_print(),
            )?;
        }
    }

    // Apply filters
    apply_clone_type_filters(&mut report, &config.only_type, &config.exclude_type);

    // Apply grouping
    apply_grouping(&mut report, config.group_by.as_deref());

    // Handle baseline operations
    handle_baseline_operations(&mut report, &config)?;

    // Generate and output report
    let output_content = generate_output(&report, &config)?;
    write_output(
        &output_content,
        config.output.as_ref(),
        config.verbose,
        config.quiet,
    )?;

    // Handle CI exit codes
    // Exit codes:
    //   0 = Scan completed successfully with no actionable duplicates
    //   1 = Duplicates found (with --fail-on-duplicates or --fail-on-new)
    //   2 = Error (invalid paths, parsing failures, etc.) - handled by anyhow
    let has_duplicates = !report.duplicates.is_empty();

    if config.fail_on_duplicates && has_duplicates {
        if config.should_print() {
            eprintln!(
                "{}",
                format!(
                    "CI Check Failed: {} duplicate(s) found",
                    report.duplicates.len()
                )
                .red()
            );
        }
        std::process::exit(1);
    }

    if config.fail_on_new && has_duplicates {
        // In --compare-to mode, report.duplicates only contains NEW duplicates
        if config.should_print() {
            eprintln!(
                "{}",
                format!(
                    "CI Check Failed: {} new duplicate(s) found since baseline",
                    report.duplicates.len()
                )
                .red()
            );
        }
        std::process::exit(1);
    }

    Ok(())
}

/// Get changed files from git diff if requested
fn get_changed_files_if_requested(config: &ScanConfig) -> Result<Option<Vec<PathBuf>>> {
    if let Some(ref diff_range) = config.git_diff {
        if config.should_print() && (config.verbose || config.format == OutputFormat::Text) {
            eprintln!(
                "{}",
                format!(
                    "Git-Diff Mode: Only scanning files changed in {}",
                    diff_range
                )
                .cyan()
            );
        }

        let changed = vcs::get_git_changed_files(
            diff_range,
            config.verbose && config.should_print(),
            config.debug,
        )
        .context(format!(
            "Failed to get changed files from git diff range: {}",
            diff_range
        ))?;

        if changed.is_empty() {
            // No files changed - return empty report
            if config.format == OutputFormat::Json && config.should_print() {
                let empty_report = dupe_core::Report {
                    version: Some(env!("CARGO_PKG_VERSION").to_string()),
                    scan_time: Some(chrono::Utc::now().to_rfc3339()),
                    config: Some(dupe_core::ScanConfig {
                        threshold: config.min_block_size,
                        similarity: config.similarity,
                        type3_enabled: config.enable_type3,
                        paths: Some(
                            config
                                .paths
                                .iter()
                                .map(|p| p.display().to_string())
                                .collect(),
                        ),
                    }),
                    files_scanned: 0,
                    functions_analyzed: 0,
                    duplicates: vec![],
                    skipped_files: vec![],
                    stats: dupe_core::ScanStats {
                        total_lines: 0,
                        total_tokens: 0,
                        unique_hashes: 0,
                        duration_ms: 0,
                        suppressed_by_ignore_file: 0,
                        suppressed_by_directive: 0,
                    },
                };
                let json = serde_json::to_string_pretty(&empty_report)
                    .context("Failed to serialize empty report")?;
                println!("{}", json);
            } else if config.should_print() {
                eprintln!(
                    "{}",
                    "No files changed in the specified range".bright_yellow()
                );
                eprintln!("No duplicates found!");
            }
            std::process::exit(0);
        }

        if config.should_print() {
            if config.verbose {
                eprintln!("  Changed files ({}):", changed.len());
                for file in &changed {
                    eprintln!("    - {}", file.display());
                }
                eprintln!();
            } else if config.format == OutputFormat::Text {
                eprintln!(
                    "  {} file(s) changed, scanning for duplicates...\n",
                    changed.len()
                );
            }
        }

        Ok(Some(changed))
    } else {
        Ok(None)
    }
}

/// Handle dry-run mode: validate config and show file counts without scanning
fn handle_dry_run(config: &ScanConfig) -> Result<()> {
    use std::collections::HashMap;

    println!();
    println!(
        "{}",
        "Dry run - no duplicate detection performed"
            .bright_cyan()
            .bold()
    );
    println!();

    // Show configuration
    println!("{}", "Configuration:".bright_white().bold());
    println!(
        "  Paths: {}",
        config
            .paths
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    );
    println!("  Min block size: {} tokens", config.min_block_size);
    println!("  Similarity: {:.0}%", config.similarity * 100.0);
    println!(
        "  Type-3: {}",
        if config.enable_type3 {
            format!(
                "enabled (tolerance: {:.0}%)",
                config.type3_tolerance * 100.0
            )
        } else {
            "disabled".to_string()
        }
    );
    println!("  Exclude patterns: {}", config.exclude.len());
    if !config.exclude.is_empty() {
        for pattern in &config.exclude {
            println!("    - {}", pattern.dimmed());
        }
    }
    println!();

    // Create scanner to collect files
    let scanner = create_scanner(config)?;
    let files = scanner
        .collect_files(config.paths.clone())
        .context("Failed to collect files")?;

    // Count files by extension
    let mut extension_counts: HashMap<String, usize> = HashMap::new();
    for file in &files {
        if let Some(ext) = file.extension().and_then(|e| e.to_str()) {
            *extension_counts.entry(ext.to_string()).or_insert(0) += 1;
        }
    }

    // Show file summary
    println!("{}", "File summary:".bright_white().bold());
    println!(
        "  Total files to scan: {}",
        files.len().to_string().bright_green().bold()
    );

    if !extension_counts.is_empty() {
        println!("  By language:");
        let mut sorted_extensions: Vec<_> = extension_counts.iter().collect();
        sorted_extensions.sort_by(|a, b| b.1.cmp(a.1));
        for (ext, count) in sorted_extensions {
            let lang = match ext.as_str() {
                "rs" => "Rust",
                "py" | "pyi" => "Python",
                "js" | "mjs" | "cjs" => "JavaScript",
                "ts" | "mts" | "cts" => "TypeScript",
                "jsx" => "JSX",
                "tsx" => "TSX",
                "vue" => "Vue",
                "svelte" => "Svelte",
                _ => ext,
            };
            println!("    .{}: {} files ({})", ext, count, lang.dimmed());
        }
    }

    println!();
    println!(
        "{}",
        "Ready to scan. Remove --dry-run to perform duplicate detection.".green()
    );

    Ok(())
}

/// Create and configure the scanner
fn create_scanner(config: &ScanConfig) -> Result<dupe_core::Scanner> {
    let mut scanner = dupe_core::Scanner::with_config(config.min_block_size, config.similarity)
        .context("Failed to initialize scanner")?;

    // Determine the root for loading .polydup-ignore
    // Priority: git repo root of scan target > first scan path > current directory
    let ignore_root = vcs::determine_project_root(Some(&config.paths))?;
    let mut ignore_manager = dupe_core::IgnoreManager::new(&ignore_root);

    // Surface errors when loading ignore file (version mismatch, malformed TOML, etc.)
    // Only treat "file not found" as non-fatal
    match ignore_manager.load() {
        Ok(_) => {
            let ignore_count = ignore_manager.count();
            if config.verbose && config.should_print() && ignore_count > 0 {
                eprintln!("  Ignore rules: {} duplicate(s) ignored", ignore_count);
            }
            scanner = scanner.with_ignore_manager(ignore_manager);
        }
        Err(e) => {
            // Check if it's just a missing file (which is fine)
            let error_msg = e.to_string();
            if error_msg.contains("No such file") || error_msg.contains("cannot find") {
                // No ignore file exists - this is fine, continue without it
            } else {
                // This is a real error (parse failure, version mismatch, etc.)
                return Err(e).context(
                    "Failed to load .polydup-ignore file. Fix the file or remove it to continue.",
                );
            }
        }
    }

    // Enable Type-3 detection if requested
    if config.enable_type3 {
        if config.verbose && config.should_print() {
            eprintln!(
                "  Type-3 detection: enabled (tolerance: {:.1}%)",
                config.type3_tolerance * 100.0
            );
        }
        scanner = scanner
            .with_type3_detection(config.type3_tolerance)
            .context("Invalid Type-3 tolerance")?;
    }

    // Enable directive detection if requested
    if config.enable_directives {
        if config.verbose && config.should_print() {
            eprintln!("  Inline directives: enabled");
        }
        scanner = scanner.with_directives(true);
    }

    // Include test files if requested
    if config.include_tests {
        if config.verbose && config.should_print() {
            eprintln!("  Test files: included");
        }
        scanner = scanner.with_test_files(true);
    }

    // Set exclude patterns if provided
    if !config.exclude.is_empty() {
        scanner = scanner.with_exclude_patterns(config.exclude.clone());
    }

    Ok(scanner)
}

/// Determine if progress indicator should be shown
///
/// Progress is shown when:
/// - Explicitly enabled with --progress, OR
/// - Output is to a TTY AND not explicitly disabled AND not quiet/JSON mode
fn should_show_progress(config: &ScanConfig) -> bool {
    // Explicitly disabled
    if config.no_progress {
        return false;
    }

    // Quiet mode or JSON output suppresses progress
    if config.quiet || config.format == OutputFormat::Json {
        return false;
    }

    // Explicitly enabled
    if config.progress {
        return true;
    }

    // Auto-detect: show progress only on TTY
    atty::is(atty::Stream::Stderr)
}

/// Create progress bar for text output with multi-stage messaging
fn create_progress_bar(config: &ScanConfig) -> Option<ProgressBar> {
    if should_show_progress(config) {
        let pb = ProgressBar::new_spinner();
        pb.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.cyan} {elapsed_precise} {msg}")
                .unwrap()
                .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]),
        );

        // Initial message varies based on Type-3 mode
        let initial_msg = if config.enable_type3 {
            "Scanning... (Type-3 detection may take longer)"
        } else {
            "Scanning for duplicates..."
        };
        pb.set_message(initial_msg.to_string());
        pb.enable_steady_tick(std::time::Duration::from_millis(100));
        Some(pb)
    } else {
        None
    }
}

/// Build detailed error message for scan failures
fn build_scan_error(e: anyhow::Error, paths: &[PathBuf], config: &ScanConfig) -> anyhow::Error {
    let mut error_msg = format!("Scan failed: {}", e);
    error_msg.push_str("\n\n");

    // Provide context-specific suggestions
    let error_str = e.to_string().to_lowercase();
    if error_str.contains("permission denied") || error_str.contains("access denied") {
        error_msg.push_str(&fmt_suggestion());
        error_msg.push_str(" Check file permissions for the scanned directory\n");
        error_msg.push_str(&fmt_indent());
        error_msg.push_str(
            "Try running with appropriate access rights or scan a different directory.\n",
        );
    } else if error_str.contains("no such file") || error_str.contains("not found") {
        error_msg.push_str(&fmt_suggestion());
        error_msg.push_str(" Verify the path exists and is accessible\n");
        error_msg.push_str(&fmt_indent());
        error_msg.push_str("Use absolute paths if relative paths are causing issues.\n");
    } else if error_str.contains("parse") || error_str.contains("syntax") {
        error_msg.push_str(&fmt_suggestion());
        error_msg.push_str(" Some files may have syntax errors or unsupported syntax\n");
        error_msg.push_str(&fmt_indent());
        error_msg.push_str("This is usually safe to ignore as PolyDup skips unparseable files.\n");
        error_msg.push_str(&fmt_indent());
        error_msg.push_str("Supported languages: JavaScript, TypeScript, Python, Rust\n\n");
        error_msg.push_str(&fmt_documentation());
        error_msg.push_str(" https://github.com/wiesnerbernard/polydup#language-support\n");
    } else {
        error_msg.push_str(&fmt_suggestion());
        error_msg.push_str(" Try running with --debug for detailed error information\n");
        error_msg.push_str(&fmt_indent());
        error_msg.push_str("Example: polydup scan ./src --debug\n");
    }

    if config.debug {
        error_msg.push_str(&format!("\n{}", "Debug Info:".bright_cyan().bold()));
        error_msg.push_str(&format!("\n  Paths: {:?}", paths));
        error_msg.push_str(&format!("\n  Min block size: {}", config.min_block_size));
        error_msg.push_str(&format!("\n  Similarity: {}", config.similarity));
        error_msg.push_str(&format!("\n  Full error: {:?}", e));
    }

    anyhow::anyhow!(error_msg)
}

/// Filter duplicates to only show those involving changed files
fn filter_duplicates_by_changed_files(
    report: &mut dupe_core::Report,
    changed: &[PathBuf],
    verbose: bool,
) -> Result<()> {
    // Normalize paths relative to repo root
    let repo_root = vcs::find_git_repo_root().unwrap_or(std::env::current_dir()?);
    let normalize_path = |p: &str| -> String {
        let path = Path::new(p);
        let repo_relative = if path.is_absolute() {
            path.strip_prefix(&repo_root).unwrap_or(path)
        } else {
            path
        };

        repo_relative
            .to_string_lossy()
            .trim_start_matches("./")
            .to_string()
    };

    let changed_set: std::collections::HashSet<String> = changed
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .map(|s| normalize_path(&s))
        .collect();

    report.duplicates.retain(|dup| {
        let file1_normalized = normalize_path(&dup.file1);
        let file2_normalized = normalize_path(&dup.file2);
        changed_set.contains(&file1_normalized) || changed_set.contains(&file2_normalized)
    });

    if verbose {
        eprintln!(
            "  Git-diff filter: {} duplicate(s) involve changed files",
            report.duplicates.len()
        );
    }

    Ok(())
}

/// Apply clone type filters to the report
fn apply_clone_type_filters(
    report: &mut dupe_core::Report,
    only_type: &[String],
    exclude_type: &[String],
) {
    if !only_type.is_empty() || !exclude_type.is_empty() {
        report.duplicates.retain(|dup| {
            let type_str = match dup.clone_type {
                dupe_core::CloneType::Type1 => "type-1",
                dupe_core::CloneType::Type2 => "type-2",
                dupe_core::CloneType::Type3 => "type-3",
            };

            // If only_type is specified, include only those types
            if !only_type.is_empty() && !only_type.contains(&type_str.to_string()) {
                return false;
            }

            // If exclude_type is specified, exclude those types
            if exclude_type.contains(&type_str.to_string()) {
                return false;
            }

            true
        });
    }
}

/// Apply grouping/sorting to the report
fn apply_grouping(report: &mut dupe_core::Report, group_by: Option<&str>) {
    if let Some(criterion) = group_by {
        match criterion {
            "file" => {
                // Sort by file1, then file2
                report
                    .duplicates
                    .sort_by(|a, b| a.file1.cmp(&b.file1).then(a.file2.cmp(&b.file2)));
            }
            "similarity" => {
                // Sort by similarity (descending), then by length (descending)
                report.duplicates.sort_by(|a, b| {
                    b.similarity
                        .partial_cmp(&a.similarity)
                        .unwrap()
                        .then(b.length.cmp(&a.length))
                });
            }
            "type" => {
                // Sort by clone type (Type-1, Type-2, Type-3), then similarity
                report.duplicates.sort_by(|a, b| {
                    let type_order = |ct: &dupe_core::CloneType| match ct {
                        dupe_core::CloneType::Type1 => 0,
                        dupe_core::CloneType::Type2 => 1,
                        dupe_core::CloneType::Type3 => 2,
                    };
                    type_order(&a.clone_type)
                        .cmp(&type_order(&b.clone_type))
                        .then(b.similarity.partial_cmp(&a.similarity).unwrap())
                });
            }
            "size" => {
                // Sort by length (descending), then similarity (descending)
                report.duplicates.sort_by(|a, b| {
                    b.length
                        .cmp(&a.length)
                        .then(b.similarity.partial_cmp(&a.similarity).unwrap())
                });
            }
            _ => {} // Already validated
        }
    }
}

/// Handle baseline save and compare operations
fn handle_baseline_operations(report: &mut dupe_core::Report, config: &ScanConfig) -> Result<()> {
    // Save baseline if requested
    if let Some(ref baseline_path) = config.save_baseline {
        let baseline = dupe_core::Baseline::from_duplicates(report.duplicates.clone());
        baseline.save_to_file(baseline_path).map_err(|e| {
            let mut error_msg = format!(
                "Failed to save baseline to {}: {}",
                baseline_path.display(),
                e
            );
            error_msg.push_str("\n\n");
            error_msg.push_str(&format!("{}", "Suggestion:".bright_yellow().bold()));
            error_msg.push_str(" Check write permissions and ensure parent directory exists\n");

            if config.debug {
                error_msg.push_str(&format!("\n{}", "Debug Info:".bright_cyan().bold()));
                error_msg.push_str(&format!("\n  Baseline path: {}", baseline_path.display()));
                error_msg.push_str(&format!("\n  Full error: {:?}", e));
            }

            anyhow::anyhow!(error_msg)
        })?;

        if config.should_print() && (config.verbose || config.format == OutputFormat::Text) {
            eprintln!(
                "{}",
                format!("Baseline saved to: {}", baseline_path.display()).green()
            );
            eprintln!("  {} duplicates recorded", report.duplicates.len());
        }
    }

    // Compare against baseline if specified
    let original_count = report.duplicates.len();
    if let Some(ref baseline_path) = config.compare_to {
        let baseline = dupe_core::Baseline::load_from_file(baseline_path).map_err(|e| {
            let mut error_msg = format!(
                "Failed to load baseline from {}: {}",
                baseline_path.display(),
                e
            );
            error_msg.push_str("\n\n");
            error_msg.push_str(&format!("{}", "Suggestion:".bright_yellow().bold()));
            error_msg.push_str(" Ensure the baseline file exists and is valid JSON\n");
            error_msg.push_str(&format!("{}", "           ".dimmed()));
            error_msg.push_str(&format!(
                "Create a baseline first: polydup scan ./src --save-baseline {}\n",
                baseline_path.display()
            ));

            if config.debug {
                error_msg.push_str(&format!("\n{}", "Debug Info:".bright_cyan().bold()));
                error_msg.push_str(&format!("\n  Baseline path: {}", baseline_path.display()));
                error_msg.push_str(&format!("\n  Full error: {:?}", e));
            }

            anyhow::anyhow!(error_msg)
        })?;

        // Filter to show only new duplicates
        report.duplicates = baseline.find_new_duplicates(&report.duplicates);

        if config.should_print() && (config.verbose || config.format == OutputFormat::Text) {
            eprintln!(
                "{}",
                format!("Comparing against baseline: {}", baseline_path.display()).cyan()
            );
            eprintln!(
                "  {} total duplicates, {} new since baseline",
                original_count,
                report.duplicates.len()
            );
        }
    }

    Ok(())
}

/// Generate output string based on format
fn generate_output(report: &dupe_core::Report, config: &ScanConfig) -> Result<String> {
    use crate::reporting::{format_compiler_report, format_sarif_report, format_vscode_report};

    match config.format {
        OutputFormat::Json => {
            serde_json::to_string_pretty(report).context("Failed to serialize results to JSON")
        }
        OutputFormat::Text => format_text_report(
            report,
            config.verbose,
            config.group_by.as_deref(),
            config.show_code,
            config.preview_lines,
            config.suggest_refactoring,
            config.show_ids,
            config.enable_type3,
            if config.full_ids {
                64
            } else {
                config.id_length
            },
        ),
        OutputFormat::Sarif => format_sarif_report(report),
        OutputFormat::Vscode => format_vscode_report(report),
        OutputFormat::Compiler => format_compiler_report(report),
    }
}

/// Write output to file or stdout
///
/// In quiet mode, output is only written to a file (if specified).
/// Stdout output is suppressed when quiet=true and no output file is specified.
fn write_output(
    content: &str,
    output_path: Option<&PathBuf>,
    verbose: bool,
    quiet: bool,
) -> Result<()> {
    if let Some(path) = output_path {
        std::fs::write(path, content).map_err(|e| {
            let mut error_msg = format!("Failed to write output to {}: {}", path.display(), e);
            error_msg.push_str("\n\n");

            let error_kind = e.kind();
            match error_kind {
                std::io::ErrorKind::PermissionDenied => {
                    error_msg.push_str(&format!("{}", "Suggestion:".bright_yellow().bold()));
                    error_msg.push_str(" Check write permissions for the target directory\n");
                    error_msg.push_str(&format!("{}", "           ".dimmed()));
                    error_msg.push_str(
                        "Ensure you have permission to create/modify files in this location.\n",
                    );
                }
                std::io::ErrorKind::NotFound => {
                    error_msg.push_str(&format!("{}", "Suggestion:".bright_yellow().bold()));
                    error_msg.push_str(" The parent directory may not exist\n");
                    error_msg.push_str(&format!("{}", "           ".dimmed()));
                    error_msg.push_str(&format!(
                        "Try creating the directory first: mkdir -p {}\n",
                        path.parent()
                            .map(|p| p.display().to_string())
                            .unwrap_or_default()
                    ));
                }
                std::io::ErrorKind::AlreadyExists => {
                    error_msg.push_str(&format!("{}", "Suggestion:".bright_yellow().bold()));
                    error_msg.push_str(" A directory exists with this name\n");
                    error_msg.push_str(&format!("{}", "           ".dimmed()));
                    error_msg.push_str(
                        "Choose a different filename or specify a file within the directory.\n",
                    );
                }
                _ => {
                    error_msg.push_str(&format!("{}", "Suggestion:".bright_yellow().bold()));
                    error_msg.push_str(" Verify the path is valid and accessible\n");
                    error_msg.push_str(&format!("{}", "           ".dimmed()));
                    error_msg.push_str("Try using an absolute path or checking disk space.\n");
                }
            }

            anyhow::anyhow!(error_msg)
        })?;

        if verbose && !quiet {
            eprintln!("Output written to: {}", path.display());
        }
    } else if !quiet {
        // Only print to stdout when not in quiet mode
        println!("{}", content);
    }

    Ok(())
}
