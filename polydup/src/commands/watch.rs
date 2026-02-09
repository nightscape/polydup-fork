//! Watch mode for continuous duplicate detection
//!
//! Monitors files for changes and re-runs duplicate detection automatically.

use anyhow::{Context, Result};
use colored::*;
use notify_debouncer_mini::{new_debouncer, notify::RecursiveMode, DebouncedEventKind};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::channel;
use std::sync::Arc;
use std::time::{Duration, Instant};

use dupe_core::Scanner;

/// Configuration for watch mode
pub struct WatchConfig {
    pub paths: Vec<PathBuf>,
    pub min_block_size: usize,
    pub similarity: f64,
    pub exclude: Vec<String>,
    pub debounce_ms: u64,
    pub clear: bool,
    pub verbose: bool,
    pub no_color: bool,
}

/// Run the watch command
pub fn run(config: WatchConfig) -> Result<()> {
    // Respect --no-color flag and NO_COLOR environment variable
    if config.no_color || std::env::var("NO_COLOR").is_ok() {
        colored::control::set_override(false);
    }

    // Set up Ctrl+C handler
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();

    ctrlc::set_handler(move || {
        r.store(false, Ordering::SeqCst);
    })
    .context("Failed to set Ctrl+C handler")?;

    // Print header
    print_header(&config);

    // Validate paths
    for path in &config.paths {
        if !path.exists() {
            anyhow::bail!("Path does not exist: {}", path.display());
        }
    }

    // Initial scan
    let mut last_duplicate_count = perform_scan(&config, None)?;

    // Set up file watcher
    let (tx, rx) = channel();
    let debounce_duration = Duration::from_millis(config.debounce_ms);

    let mut debouncer =
        new_debouncer(debounce_duration, tx).context("Failed to create file watcher")?;

    // Watch all paths
    for path in &config.paths {
        debouncer
            .watcher()
            .watch(path, RecursiveMode::Recursive)
            .with_context(|| format!("Failed to watch path: {}", path.display()))?;
    }

    println!(
        "\n{} Watching {} for changes... (press {} to stop)\n",
        ">>".dimmed(),
        format_paths(&config.paths),
        "Ctrl+C".cyan()
    );

    // Main event loop
    while running.load(Ordering::SeqCst) {
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(Ok(events)) => {
                // Collect unique changed files
                let changed_files: HashSet<PathBuf> = events
                    .iter()
                    .filter(|e| matches!(e.kind, DebouncedEventKind::Any))
                    .filter_map(|e| {
                        let path = &e.path;
                        // Only track source files
                        if is_source_file(path) && !is_excluded(path, &config.exclude) {
                            Some(path.clone())
                        } else {
                            None
                        }
                    })
                    .collect();

                if !changed_files.is_empty() {
                    if config.clear {
                        clear_terminal();
                    }

                    let timestamp = chrono::Local::now().format("%H:%M:%S");
                    println!(
                        "{} {} changed: {}",
                        format!("[{}]", timestamp).dimmed(),
                        "File".yellow(),
                        format_changed_files(&changed_files)
                    );

                    // Re-scan
                    match perform_scan(&config, Some(&changed_files)) {
                        Ok(new_count) => {
                            print_change_summary(last_duplicate_count, new_count);
                            last_duplicate_count = new_count;
                        }
                        Err(e) => {
                            eprintln!("{} Scan failed: {}", "Error:".red().bold(), e);
                        }
                    }
                }
            }
            Ok(Err(error)) => {
                eprintln!("{} Watch error: {:?}", "Warning:".yellow(), error);
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                // Normal timeout, continue loop
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                break;
            }
        }
    }

    println!("\n{} Watch mode stopped.", "[ok]".green());
    Ok(())
}

/// Perform a duplicate scan and return the count
fn perform_scan(config: &WatchConfig, changed_files: Option<&HashSet<PathBuf>>) -> Result<usize> {
    let start = Instant::now();

    let scanner = Scanner::with_config(config.min_block_size, config.similarity)
        .context("Failed to configure scanner")?;

    let report = scanner.scan(config.paths.clone()).context("Scan failed")?;

    let duration = start.elapsed();
    let duplicate_count = report.duplicates.len();

    let timestamp = chrono::Local::now().format("%H:%M:%S");

    if changed_files.is_some() {
        // Incremental scan output
        println!(
            "{} {} Re-scanned in {}: {} duplicates",
            format!("[{}]", timestamp).dimmed(),
            "[ok]".green(),
            format_duration(duration),
            if duplicate_count == 0 {
                "0".green().to_string()
            } else {
                duplicate_count.to_string().yellow().to_string()
            }
        );
    } else {
        // Initial scan output
        println!(
            "\n{} Initial scan complete in {}\n",
            "[ok]".green(),
            format_duration(duration)
        );
        println!(
            "  {} files scanned, {} functions analyzed",
            report.files_scanned.to_string().cyan(),
            report.functions_analyzed.to_string().cyan()
        );
        println!(
            "  {} duplicates found\n",
            if duplicate_count == 0 {
                "0".green().to_string()
            } else {
                duplicate_count.to_string().yellow().to_string()
            }
        );

        // Show duplicates summary if any
        if config.verbose && duplicate_count > 0 {
            println!("  {}", "Duplicates:".underline());
            for dup in report.duplicates.iter().take(5) {
                println!(
                    "    {} {} <-> {}",
                    "-".dimmed(),
                    format!("{}:{}", dup.file1, dup.start_line1).white(),
                    format!("{}:{}", dup.file2, dup.start_line2).white()
                );
            }
            if duplicate_count > 5 {
                println!("    {} ... and {} more", "-".dimmed(), duplicate_count - 5);
            }
            println!();
        }
    }

    Ok(duplicate_count)
}

/// Print the watch mode header
fn print_header(config: &WatchConfig) {
    println!();
    println!(
        "{}",
        "╔═══════════════════════════════════════════════════════════╗".cyan()
    );
    println!(
        "{}",
        "║                    PolyDup Watch Mode                     ║".cyan()
    );
    println!(
        "{}",
        "╚═══════════════════════════════════════════════════════════╝".cyan()
    );
    println!();
    println!("  {} {}", "Paths:".dimmed(), format_paths(&config.paths));
    println!(
        "  {} {} tokens",
        "Min block:".dimmed(),
        config.min_block_size
    );
    println!(
        "  {} {}%",
        "Similarity:".dimmed(),
        (config.similarity * 100.0) as u32
    );
    println!("  {} {}ms", "Debounce:".dimmed(), config.debounce_ms);
}

/// Print change summary comparing old and new duplicate counts
fn print_change_summary(old_count: usize, new_count: usize) {
    if new_count > old_count {
        let diff = new_count - old_count;
        println!(
            "         {} +{} duplicate{} detected",
            "[!]".yellow(),
            diff,
            if diff == 1 { "" } else { "s" }
        );
    } else if new_count < old_count {
        let diff = old_count - new_count;
        println!(
            "         {} -{} duplicate{} resolved",
            "[ok]".green(),
            diff,
            if diff == 1 { "" } else { "s" }
        );
    }
    println!();
}

/// Format a duration for display
fn format_duration(duration: Duration) -> String {
    let ms = duration.as_millis();
    if ms < 1000 {
        format!("{}ms", ms)
    } else {
        format!("{:.1}s", duration.as_secs_f64())
    }
}

/// Format paths for display
fn format_paths(paths: &[PathBuf]) -> String {
    paths
        .iter()
        .map(|p| p.display().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Format changed files for display
fn format_changed_files(files: &HashSet<PathBuf>) -> String {
    let file_names: Vec<_> = files
        .iter()
        .filter_map(|p| p.file_name())
        .map(|n| n.to_string_lossy().to_string())
        .take(3)
        .collect();

    if files.len() > 3 {
        format!("{} (+{} more)", file_names.join(", "), files.len() - 3)
    } else {
        file_names.join(", ")
    }
}

/// Check if a path is a source file we care about
fn is_source_file(path: &std::path::Path) -> bool {
    let source_extensions = ["rs", "js", "ts", "jsx", "tsx", "py", "vue", "svelte"];

    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| source_extensions.contains(&ext))
        .unwrap_or(false)
}

/// Check if a path matches any exclude pattern
fn is_excluded(path: &std::path::Path, exclude_patterns: &[String]) -> bool {
    let path_str = path.to_string_lossy();

    for pattern in exclude_patterns {
        // Simple glob matching for common patterns
        if let Some(suffix) = pattern.strip_prefix("**/") {
            if path_str.contains(suffix) {
                return true;
            }
        } else if let Some(prefix) = pattern.strip_suffix("/**") {
            if path_str.contains(prefix) {
                return true;
            }
        } else if path_str.contains(pattern) {
            return true;
        }
    }

    // Default exclusions
    let default_excludes = [
        "node_modules",
        "target",
        ".git",
        "__pycache__",
        "dist",
        "build",
    ];
    for exclude in default_excludes {
        if path_str.contains(exclude) {
            return true;
        }
    }

    false
}

/// Clear the terminal
fn clear_terminal() {
    print!("\x1B[2J\x1B[1;1H");
}
