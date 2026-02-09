//! CLI argument parsing structures
//!
//! This module contains only Clap definitions for command-line arguments.
//! No business logic should be present here.

use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

/// Parse min_block_size with proper error messages for invalid values
fn parse_min_tokens(s: &str) -> Result<usize, String> {
    // Try parsing as i64 first to catch negative numbers
    match s.parse::<i64>() {
        Ok(n) if n < 0 => Err(format!(
            "Minimum token count must be positive, got: {}\n\
             Suggestion: Use a value greater than 0 (e.g., --threshold 50)\n\
             Tip: Typical values range from 25-100 tokens.",
            n
        )),
        Ok(0) => Err("Minimum token count cannot be 0\n\
             Suggestion: Use at least 10 tokens for meaningful results\n\
             Tip: Default is 50 tokens. Try values between 25-100."
            .to_string()),
        Ok(n) => Ok(n as usize),
        Err(_) => Err(format!(
            "Invalid number: '{}'\n\
             Suggestion: Provide a positive integer (e.g., --threshold 50)",
            s
        )),
    }
}

/// Cross-language duplicate code detector
#[derive(Parser)]
#[command(name = "polydup")]
#[command(version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Paths to scan (backward compatibility: works when no subcommand given)
    #[arg(global = true)]
    pub paths: Vec<PathBuf>,

    /// Output format
    #[arg(short = 'f', long = "format", default_value = "text", global = true)]
    pub format: OutputFormat,

    /// Minimum code block size in tokens.
    /// A token is a code element (keyword, identifier, literal, operator).
    /// Comments and whitespace are ignored. Typical function: 50-300 tokens.
    #[arg(
        short = 't',
        long = "min-tokens",
        visible_alias = "threshold",
        alias = "min-block-size",
        default_value = "50",
        global = true,
        value_name = "TOKENS",
        allow_hyphen_values = true,
        value_parser = parse_min_tokens
    )]
    pub min_block_size: usize,

    /// Similarity threshold (0.0-1.0)
    #[arg(
        short = 's',
        long = "similarity",
        default_value = "0.85",
        global = true
    )]
    pub similarity: f64,

    /// Show verbose output
    #[arg(short = 'v', long = "verbose", global = true)]
    pub verbose: bool,

    /// Suppress all output except errors (for CI/CD integration)
    #[arg(short = 'q', long = "quiet", global = true, conflicts_with = "verbose")]
    pub quiet: bool,

    /// Exclude file patterns (glob patterns, e.g., "**/*.test.ts", "**/*.spec.js")
    #[arg(short = 'e', long = "exclude", global = true)]
    pub exclude: Vec<String>,

    /// Enable Type-3 (gap-tolerant) clone detection
    #[arg(long = "enable-type3", global = true)]
    pub enable_type3: bool,

    /// Type-3 similarity tolerance (0.0-1.0)
    #[arg(long = "type3-tolerance", default_value = "0.85", global = true)]
    pub type3_tolerance: f64,

    /// Write output to file instead of stdout
    #[arg(short = 'o', long = "output", global = true)]
    pub output: Option<PathBuf>,

    /// Disable colored output
    #[arg(long = "no-color", global = true)]
    pub no_color: bool,

    /// Only show specific clone type(s) (type-1, type-2, type-3)
    #[arg(long = "only-type", global = true, value_delimiter = ',')]
    pub only_type: Vec<String>,

    /// Exclude specific clone type(s) (type-1, type-2, type-3)
    #[arg(long = "exclude-type", global = true, value_delimiter = ',')]
    pub exclude_type: Vec<String>,

    /// Group duplicates by criterion (file, similarity, type, size)
    #[arg(long = "group-by", global = true)]
    pub group_by: Option<String>,

    /// Show detailed error traces (for debugging)
    #[arg(long = "debug", global = true)]
    pub debug: bool,

    /// Include test files in scan (*.test.*, *.spec.*, etc.)
    #[arg(long = "include-tests", global = true)]
    pub include_tests: bool,

    /// Show progress indicator (auto-detected for TTY, disabled in CI)
    #[arg(long = "progress", global = true, conflicts_with = "no_progress")]
    pub progress: bool,

    /// Disable progress indicator
    #[arg(long = "no-progress", global = true)]
    pub no_progress: bool,
}

#[derive(Subcommand)]
#[allow(clippy::large_enum_variant)]
pub enum Commands {
    /// Scan for duplicate code (default command)
    Scan {
        /// Paths to scan (files or directories)
        #[arg(required = true)]
        paths: Vec<PathBuf>,

        /// Output format
        #[arg(short = 'f', long = "format", default_value = "text")]
        format: OutputFormat,

        /// Minimum code block size in tokens.
        /// A token is a code element (keyword, identifier, literal, operator).
        /// Comments and whitespace are ignored. Typical function: 50-300 tokens.
        #[arg(
            short = 't',
            long = "min-tokens",
            visible_alias = "threshold",
            alias = "min-block-size",
            default_value = "50",
            value_name = "TOKENS",
            allow_hyphen_values = true,
            value_parser = parse_min_tokens
        )]
        min_block_size: usize,

        /// Similarity threshold (0.0-1.0)
        #[arg(short = 's', long = "similarity", default_value = "0.85")]
        similarity: f64,

        /// Show verbose output
        #[arg(short = 'v', long = "verbose")]
        verbose: bool,

        /// Suppress all output except errors (for CI/CD integration)
        #[arg(short = 'q', long = "quiet", conflicts_with = "verbose")]
        quiet: bool,

        /// Exclude file patterns (glob patterns)
        #[arg(short = 'e', long = "exclude")]
        exclude: Vec<String>,

        /// Enable Type-3 (gap-tolerant) clone detection
        #[arg(long = "enable-type3")]
        enable_type3: bool,

        /// Type-3 similarity tolerance (0.0-1.0)
        #[arg(long = "type3-tolerance", default_value = "0.85")]
        type3_tolerance: f64,

        /// Write output to file instead of stdout
        #[arg(short = 'o', long = "output")]
        output: Option<PathBuf>,

        /// Disable colored output
        #[arg(long = "no-color")]
        no_color: bool,

        /// Only show specific clone type(s) (type-1, type-2, type-3)
        #[arg(long = "only-type", value_delimiter = ',')]
        only_type: Vec<String>,

        /// Exclude specific clone type(s) (type-1, type-2, type-3)
        #[arg(long = "exclude-type", value_delimiter = ',')]
        exclude_type: Vec<String>,

        /// Group duplicates by criterion (file, similarity, type, size)
        #[arg(long = "group-by")]
        group_by: Option<String>,

        /// Show detailed error traces (for debugging)
        #[arg(long = "debug")]
        debug: bool,

        /// Save scan results as baseline for future comparisons
        #[arg(long = "save-baseline")]
        save_baseline: Option<PathBuf>,

        /// Compare current scan against a baseline file (show only new duplicates)
        #[arg(long = "compare-to")]
        compare_to: Option<PathBuf>,

        /// Only scan files changed in git diff range (e.g., "origin/main..HEAD")
        #[arg(long = "git-diff", value_name = "RANGE")]
        git_diff: Option<String>,

        /// Enable inline directive detection (// polydup-ignore comments)
        #[arg(long = "enable-directives")]
        enable_directives: bool,

        /// Include test files in scan (*.test.*, *.spec.*, etc.)
        #[arg(long = "include-tests")]
        include_tests: bool,

        /// Show progress indicator (auto-detected for TTY, disabled in CI)
        #[arg(long = "progress", conflicts_with = "no_progress")]
        progress: bool,

        /// Disable progress indicator
        #[arg(long = "no-progress")]
        no_progress: bool,
    },

    /// Initialize PolyDup configuration
    Init {
        /// Force overwrite existing configuration
        #[arg(long = "force")]
        force: bool,

        /// Skip interactive prompts and use defaults
        #[arg(short = 'y', long = "yes")]
        non_interactive: bool,
    },

    /// Manage ignored duplicates
    Ignore {
        #[command(subcommand)]
        command: IgnoreCommands,
    },
}

#[derive(Subcommand)]
pub enum IgnoreCommands {
    /// Add a duplicate to the ignore list
    Add {
        /// Duplicate ID to ignore (from scan output)
        #[arg(value_name = "ID")]
        id: Option<String>,

        /// Files containing the duplicate (format: "path:start-end")
        #[arg(long = "files", value_delimiter = ',')]
        files: Vec<String>,

        /// Reason for ignoring this duplicate
        #[arg(short = 'r', long = "reason")]
        reason: Option<String>,

        /// Who is adding this ignore entry
        #[arg(long = "added-by", default_value_t = get_current_user())]
        added_by: String,
    },

    /// List all ignored duplicates
    List {
        /// Show detailed information
        #[arg(short = 'v', long = "verbose")]
        verbose: bool,

        /// Output format
        #[arg(short = 'f', long = "format", default_value = "text")]
        format: OutputFormat,
    },

    /// Remove a duplicate from the ignore list
    Remove {
        /// Duplicate ID to remove
        #[arg(value_name = "ID", required = true)]
        id: String,
    },
}

#[derive(Clone, ValueEnum, PartialEq)]
pub enum OutputFormat {
    /// Human-readable text output
    Text,
    /// JSON output for scripting
    Json,
}

/// Get the current user's name for ignore entries
pub fn get_current_user() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "unknown".to_string())
}
