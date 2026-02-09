//! CLI argument parsing structures
//!
//! This module contains only Clap definitions for command-line arguments.
//! No business logic should be present here.

use clap::{CommandFactory, Parser, Subcommand, ValueEnum};
use clap_complete::Shell;
use std::path::PathBuf;

/// Version string that includes installation source indicator
const VERSION_STRING: &str = concat!(env!("CARGO_PKG_VERSION"), " (cargo)");

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

/// Parse type3_tolerance with proper error messages for invalid values
fn parse_type3_tolerance(s: &str) -> Result<f64, String> {
    match s.parse::<f64>() {
        Ok(n) if n < 0.0 => Err(format!(
            "Type-3 tolerance must be between 0.0 and 1.0, got: {}\n\
             Suggestion: Use a value like 0.85 for 85% tolerance\n\
             Tip: Higher values = stricter matching, Lower values = more matches",
            n
        )),
        Ok(n) if n > 1.0 => Err(format!(
            "Type-3 tolerance must be between 0.0 and 1.0, got: {}\n\
             Suggestion: Use a value like 0.85 for 85% tolerance\n\
             Tip: Higher values = stricter matching, Lower values = more matches",
            n
        )),
        Ok(n) => Ok(n),
        Err(_) => Err(format!(
            "Invalid number: '{}'\n\
             Suggestion: Provide a decimal value between 0.0 and 1.0 (e.g., --type3-tolerance 0.85)",
            s
        )),
    }
}

/// Cross-language duplicate code detector
#[derive(Parser)]
#[command(name = "polydup")]
#[command(version = VERSION_STRING)]
#[command(about = "Cross-language duplicate code detector supporting JS/TS, Python, and Rust")]
#[command(after_help = "\
DETECTION TYPES:
  Type-1  Exact duplicates (always enabled)
  Type-2  Renamed duplicates (always enabled) - same structure, different names
  Type-3  Gap-tolerant duplicates - similar code with small differences
          Enable with: --enable-type3 --type3-tolerance 0.85

EXAMPLES:
  polydup scan ./src                    Basic scan for exact and renamed clones
  polydup scan ./src --enable-type3     Include Type-3 (gap-tolerant) detection
  polydup scan ./src --format json      JSON output for CI/CD integration
  polydup scan ./src --show-ids         Show duplicate IDs for ignore commands

For more information: https://github.com/wiesnerbernard/polydup")]
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

    /// Enable Type-3 (gap-tolerant) clone detection.
    /// Finds similar code with insertions, deletions, or modifications.
    /// Use with --type3-tolerance to control sensitivity (default: 0.85).
    #[arg(long = "enable-type3", global = true)]
    pub enable_type3: bool,

    /// Type-3 similarity tolerance (0.0-1.0)
    #[arg(
        long = "type3-tolerance",
        default_value = "0.85",
        global = true,
        value_parser = parse_type3_tolerance
    )]
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

    /// Show debug output: scan config, timing, token counts, clone type breakdown
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

    /// Show code preview snippets for duplicates
    #[arg(short = 'c', long = "show-code", global = true)]
    pub show_code: bool,

    /// Maximum lines to show in code preview (default: 10)
    #[arg(long = "preview-lines", default_value = "10", global = true)]
    pub preview_lines: usize,

    /// Disable update check notification
    #[arg(long = "no-update-check", global = true)]
    pub no_update_check: bool,

    /// Show duplicate IDs in text output (for use with ignore commands)
    #[arg(long = "show-ids", global = true)]
    pub show_ids: bool,

    /// Validate configuration and show file counts without performing scan
    #[arg(long = "dry-run", global = true)]
    pub dry_run: bool,

    /// Length of truncated duplicate IDs (default: 8, full: 64)
    #[arg(
        long = "id-length",
        default_value = "8",
        global = true,
        value_name = "N"
    )]
    pub id_length: usize,

    /// Show full duplicate IDs (64 characters) instead of truncated
    #[arg(long = "full-ids", global = true)]
    pub full_ids: bool,
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

        /// Enable Type-3 (gap-tolerant) clone detection.
        /// Finds similar code with insertions, deletions, or modifications.
        /// Use with --type3-tolerance to control sensitivity (default: 0.85).
        #[arg(long = "enable-type3")]
        enable_type3: bool,

        /// Type-3 similarity tolerance (0.0-1.0)
        #[arg(
            long = "type3-tolerance",
            default_value = "0.85",
            value_parser = parse_type3_tolerance
        )]
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

        /// Filter to duplicates involving files changed in git diff range (e.g., "origin/main..HEAD")
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

        /// Exit with code 1 if any duplicates are found (for CI)
        #[arg(long = "fail-on-duplicates")]
        fail_on_duplicates: bool,

        /// Exit with code 1 if new duplicates are found compared to baseline (requires --compare-to)
        #[arg(long = "fail-on-new", requires = "compare_to")]
        fail_on_new: bool,

        /// Show code preview snippets for duplicates
        #[arg(short = 'c', long = "show-code")]
        show_code: bool,

        /// Maximum lines to show in code preview (default: 10)
        #[arg(long = "preview-lines", default_value = "10")]
        preview_lines: usize,

        /// Show refactoring suggestions for duplicates
        #[arg(long = "suggest-refactoring")]
        suggest_refactoring: bool,

        /// Show duplicate IDs in text output (for use with ignore commands)
        #[arg(long = "show-ids")]
        show_ids: bool,

        /// Validate configuration and show file counts without performing scan
        #[arg(long = "dry-run")]
        dry_run: bool,

        /// Length of truncated duplicate IDs (default: 8, full: 64)
        #[arg(long = "id-length", default_value = "8", value_name = "N")]
        id_length: usize,

        /// Show full duplicate IDs (64 characters) instead of truncated
        #[arg(long = "full-ids")]
        full_ids: bool,
    },

    /// Initialize PolyDup configuration
    Init {
        /// Force overwrite existing configuration
        #[arg(long = "force")]
        force: bool,

        /// Skip interactive prompts and use defaults
        #[arg(short = 'y', long = "yes")]
        non_interactive: bool,

        /// Only generate CI/CD configuration (skip .polyduprc.toml)
        #[arg(long = "ci-only")]
        ci_only: bool,
    },

    /// Manage configuration file
    Config {
        #[command(subcommand)]
        command: ConfigCommands,
    },

    /// Manage hash cache for fast scanning
    Cache {
        #[command(subcommand)]
        command: crate::commands::cache::CacheCommand,
    },

    /// Manage ignored duplicates
    Ignore {
        #[command(subcommand)]
        command: IgnoreCommands,
    },

    /// Check for multiple polydup installations and version conflicts
    CheckInstall,

    /// List supported programming languages
    Languages {
        /// Output format
        #[arg(short = 'f', long = "format", default_value = "text")]
        format: OutputFormat,

        /// Show only languages with full support
        #[arg(long = "full-only")]
        full_only: bool,
    },

    /// Upgrade polydup to the latest version
    Upgrade {
        /// Only check for updates, don't install
        #[arg(long = "check-only")]
        check_only: bool,

        /// Force reinstallation even if already up-to-date
        #[arg(long = "force")]
        force: bool,
    },

    /// Generate shell completions for bash, zsh, fish, or powershell
    Completions {
        /// Shell to generate completions for
        #[arg(value_enum)]
        shell: Shell,
    },

    /// Watch for file changes and re-scan for duplicates
    Watch {
        /// Paths to watch (files or directories)
        #[arg(required = true)]
        paths: Vec<PathBuf>,

        /// Minimum code block size in tokens
        #[arg(
            short = 't',
            long = "min-tokens",
            visible_alias = "threshold",
            default_value = "50",
            value_name = "TOKENS"
        )]
        min_block_size: usize,

        /// Similarity threshold (0.0-1.0)
        #[arg(short = 's', long = "similarity", default_value = "0.85")]
        similarity: f64,

        /// Exclude file patterns (glob patterns)
        #[arg(short = 'e', long = "exclude")]
        exclude: Vec<String>,

        /// Debounce delay in milliseconds (default: 500ms)
        #[arg(long = "debounce", default_value = "500")]
        debounce_ms: u64,

        /// Clear terminal before each scan
        #[arg(long = "clear")]
        clear: bool,

        /// Show verbose output
        #[arg(short = 'v', long = "verbose")]
        verbose: bool,

        /// Disable colored output
        #[arg(long = "no-color")]
        no_color: bool,
    },
}

#[derive(Subcommand)]
pub enum ConfigCommands {
    /// Validate configuration file
    Validate {
        /// Path to configuration file (default: search for .polyduprc.toml)
        #[arg(long = "config")]
        config_path: Option<PathBuf>,
    },

    /// Show configuration file path
    Path,

    /// Show configuration summary
    Show {
        /// Path to configuration file (default: search for .polyduprc.toml)
        #[arg(long = "config")]
        config_path: Option<PathBuf>,
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
    /// SARIF output for GitHub Code Scanning integration
    Sarif,
    /// VS Code problem matcher format (file:line:col: severity: message)
    Vscode,
    /// Compiler-style output (like rustc/gcc warnings)
    Compiler,
}

/// Get the current user's name for ignore entries
pub fn get_current_user() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "unknown".to_string())
}

/// Generate shell completions and write to stdout
pub fn generate_completions(shell: Shell) {
    let mut cmd = Cli::command();
    clap_complete::generate(shell, &mut cmd, "polydup", &mut std::io::stdout());
}
