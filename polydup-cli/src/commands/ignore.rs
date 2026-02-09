//! Ignore command implementation
//!
//! This module handles the management of ignored duplicates through the
//! .polydup-ignore file.

use anyhow::{Context, Result};
use colored::*;
use dupe_core::{FileRange, IgnoreEntry, IgnoreManager};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::cli::{IgnoreCommands, OutputFormat};
use crate::config;
use crate::vcs;

/// Execute the ignore command
pub fn run(command: IgnoreCommands, min_block_size: usize, similarity: f64) -> Result<()> {
    // Use the same ignore root as the scan command (git repo root > CWD)
    let ignore_root = vcs::determine_project_root(None)?;
    let mut manager = load_ignore_manager(&ignore_root)?;

    match command {
        IgnoreCommands::Add {
            id,
            files,
            reason,
            added_by,
        } => handle_add_command(AddCommandParams {
            id,
            files,
            reason,
            added_by,
            cli_min_block_size: min_block_size,
            cli_similarity: similarity,
            manager: &mut manager,
            ignore_root: &ignore_root,
        })?,

        IgnoreCommands::List { verbose, format } => {
            handle_list_command(verbose, format, &manager)?;
        }

        IgnoreCommands::Remove { id } => {
            handle_remove_command(&id, &mut manager)?;
        }
    }

    Ok(())
}

/// Load the ignore manager, allowing missing files but failing on invalid ones
fn load_ignore_manager(ignore_root: &Path) -> Result<IgnoreManager> {
    let mut manager = IgnoreManager::new(ignore_root);

    match manager.load() {
        Ok(_) => {}
        Err(e) => {
            let error_msg = e.to_string();
            if !error_msg.contains("No such file") && !error_msg.contains("cannot find") {
                return Err(e).context(
                    "Failed to load .polydup-ignore file. Fix the file before managing ignore rules.",
                );
            }
        }
    }

    Ok(manager)
}

/// Parameters for the 'add' command
struct AddCommandParams<'a> {
    id: Option<String>,
    files: Vec<String>,
    reason: Option<String>,
    added_by: String,
    cli_min_block_size: usize,
    cli_similarity: f64,
    manager: &'a mut IgnoreManager,
    ignore_root: &'a Path,
}

/// Handle the 'add' subcommand
fn handle_add_command(params: AddCommandParams) -> Result<()> {
    let AddCommandParams {
        id,
        files,
        reason,
        added_by,
        cli_min_block_size,
        cli_similarity,
        manager,
        ignore_root,
    } = params;

    // Gather input (interactive if needed)
    let (id, files) = gather_add_input(id, files)?;

    // Resolve the duplicate ID
    let duplicate_id =
        resolve_duplicate_id(id, &files, ignore_root, cli_min_block_size, cli_similarity)?;

    // Parse file ranges and get reason
    let file_ranges = parse_file_ranges(&files)?;
    let reason_final = get_reason(reason)?;

    // Create and save entry
    save_ignore_entry(manager, duplicate_id, file_ranges, reason_final, added_by)?;

    Ok(())
}

/// Gather input for add command (interactive if needed)
fn gather_add_input(
    mut id: Option<String>,
    mut files: Vec<String>,
) -> Result<(Option<String>, Vec<String>)> {
    if id.is_none() && files.is_empty() {
        println!(
            "{}",
            "Interactive mode: provide duplicate details.".yellow()
        );

        let id_input = prompt_input("Duplicate ID (press Enter to compute from files): ")?;
        if !id_input.is_empty() {
            id = Some(id_input);
        }

        while files.is_empty() {
            let files_input = prompt_input(
                "Files containing the duplicate (path:start-end[,path:start-end...]): ",
            )?;

            if !files_input.is_empty() {
                files = files_input
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
                break;
            }

            if id.is_some() {
                break;
            }

            println!(
                "{}",
                "Please provide at least one file range to locate the duplicate.".red()
            );
        }
    }

    Ok((id, files))
}

/// Resolve duplicate ID - either use provided ID or find by scanning
fn resolve_duplicate_id(
    id: Option<String>,
    files: &[String],
    ignore_root: &Path,
    cli_min_block_size: usize,
    cli_similarity: f64,
) -> Result<String> {
    if let Some(id) = id {
        return Ok(id);
    }

    if files.is_empty() {
        anyhow::bail!("Either provide an ID or use --files to generate one");
    }

    find_duplicate_id_by_location(files, ignore_root, cli_min_block_size, cli_similarity)
}

/// Find a duplicate's ID by scanning and matching the specified location
fn find_duplicate_id_by_location(
    files: &[String],
    ignore_root: &Path,
    cli_min_block_size: usize,
    cli_similarity: f64,
) -> Result<String> {
    println!(
        "{}",
        "Scanning to find duplicate at specified location...".yellow()
    );

    let file_ranges: Vec<FileRange> = files
        .iter()
        .map(|f| FileRange::parse(f).map_err(|e| anyhow::anyhow!("{}", e)))
        .collect::<Result<Vec<_>>>()
        .context("Invalid file range format. Use 'path:start-end'")?;

    let first_range = file_ranges
        .first()
        .ok_or_else(|| anyhow::anyhow!("No files provided"))?;

    let file_path = &first_range.file;
    if !file_path.exists() {
        anyhow::bail!("File not found: {}", file_path.display());
    }

    // Configure scanner
    let (min_block_size, similarity, enable_type3, type3_tolerance) =
        resolve_scan_parameters(cli_min_block_size, cli_similarity)?;

    let mut scanner = dupe_core::Scanner::with_config(min_block_size, similarity)?;

    if enable_type3 {
        scanner = scanner
            .with_type3_detection(type3_tolerance)
            .context("Failed to enable Type-3 detection")?;
    }

    // Enable ignore manager so IDs get computed
    let mut temp_ignore = IgnoreManager::new(ignore_root);
    let _ = temp_ignore.load();
    scanner = scanner.with_ignore_manager(temp_ignore);

    // Scan the project
    let report = scanner.scan(vec![ignore_root.to_path_buf()])?;

    if report.duplicates.is_empty() {
        anyhow::bail!(
            "No duplicates found in {}.\n\
            The --files flag requires at least one duplicate to exist.",
            file_path.display()
        );
    }

    // Find matching duplicate
    find_matching_duplicate(&report.duplicates, first_range, ignore_root)
}

/// Find a duplicate that matches the specified file range
fn find_matching_duplicate(
    duplicates: &[dupe_core::DuplicateMatch],
    target_range: &FileRange,
    ignore_root: &Path,
) -> Result<String> {
    let canonicalize = |path: &Path| -> Result<PathBuf> {
        if path.is_absolute() {
            path.canonicalize().map_err(Into::into)
        } else {
            ignore_root.join(path).canonicalize().map_err(Into::into)
        }
    };

    let target_path = canonicalize(&target_range.file).with_context(|| {
        format!(
            "Failed to canonicalize file path: {}",
            target_range.file.display()
        )
    })?;

    for dup in duplicates {
        let dup_path1 = canonicalize(Path::new(&dup.file1));
        let dup_path2 = canonicalize(Path::new(&dup.file2));

        let overlaps = |start_line: usize, end_line: Option<usize>| {
            let resolved_end =
                end_line.unwrap_or_else(|| start_line.saturating_add(dup.length.saturating_sub(1)));
            start_line <= target_range.end_line && resolved_end >= target_range.start_line
        };

        let matches_file1 = dup_path1
            .as_ref()
            .map(|p| p == &target_path && overlaps(dup.start_line1, dup.end_line1))
            .unwrap_or(false);

        let matches_file2 = dup_path2
            .as_ref()
            .map(|p| p == &target_path && overlaps(dup.start_line2, dup.end_line2))
            .unwrap_or(false);

        if matches_file1 || matches_file2 {
            return dup.duplicate_id.clone().ok_or_else(|| {
                anyhow::anyhow!(
                    "Found duplicate at lines {} in {} but ID was not computed.\n\
                    Try using 'polydup ignore add <id>' with the full ID instead.",
                    dup.start_line1,
                    dup.file1
                )
            });
        }
    }

    anyhow::bail!(
        "No duplicate found at {}:{}-{}.\n\
        Run 'polydup scan {}' to see all duplicates.",
        target_range.file.display(),
        target_range.start_line,
        target_range.end_line,
        target_range.file.display()
    )
}

/// Parse file range strings into FileRange structs
fn parse_file_ranges(files: &[String]) -> Result<Vec<FileRange>> {
    files
        .iter()
        .map(|f| FileRange::parse(f).map_err(|e| anyhow::anyhow!("{}", e)))
        .collect::<Result<Vec<_>>>()
        .context("Invalid file range format")
}

/// Get reason for ignoring (prompt if not provided)
fn get_reason(reason: Option<String>) -> Result<String> {
    let reason_str = match reason {
        Some(r) => r,
        None => prompt_input("Reason for ignoring (optional): ")?,
    };

    Ok(if reason_str.is_empty() {
        "No reason provided".to_string()
    } else {
        reason_str
    })
}

/// Create and save an ignore entry
fn save_ignore_entry(
    manager: &mut IgnoreManager,
    duplicate_id: String,
    file_ranges: Vec<FileRange>,
    reason: String,
    added_by: String,
) -> Result<()> {
    let entry = IgnoreEntry {
        id: duplicate_id.clone(),
        files: file_ranges,
        reason,
        added_by,
        added_at: chrono::Utc::now(),
    };

    manager.add_ignore(entry);
    manager.save().context("Failed to save ignore file")?;

    println!("{}", "✓ Duplicate added to ignore list".green());
    println!("  ID: {}", duplicate_id.bright_blue());

    Ok(())
}

fn prompt_input(prompt: &str) -> Result<String> {
    print!("{}", prompt.cyan());
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(input.trim().to_string())
}

/// Resolve scan parameters for rescan, honoring config file overrides
///
/// Returns (min_block_size, similarity, enable_type3, type3_tolerance)
fn resolve_scan_parameters(
    cli_min_block_size: usize,
    cli_similarity: f64,
) -> Result<(usize, f64, bool, f64)> {
    let mut min_block_size = cli_min_block_size;
    let mut similarity = cli_similarity;

    if let Some(cfg) = config::Config::load()? {
        if crate::defaults::is_default_min_block_size(min_block_size) {
            min_block_size = cfg.scan.min_block_size;
        }

        if crate::defaults::is_default_similarity(similarity) {
            similarity = cfg.scan.similarity_threshold;
        }
    }

    // Always enable Type-3 detection for ignore add rescanning
    let enable_type3 = true;
    let type3_tolerance = crate::defaults::TYPE3_TOLERANCE;

    Ok((min_block_size, similarity, enable_type3, type3_tolerance))
}

/// Handle the 'list' subcommand
fn handle_list_command(
    verbose: bool,
    format: OutputFormat,
    manager: &dupe_core::IgnoreManager,
) -> Result<()> {
    let entries = manager.list_ignores();

    if entries.is_empty() {
        println!("{}", "No ignored duplicates configured.".yellow());
        println!();
        println!("To ignore specific duplicates, run a scan and use:");
        println!("  {}", "polydup ignore add <duplicate-id>".bright_cyan());
        println!();
        println!("Alternatively, use the --files flag to specify locations:");
        println!(
            "  {}",
            "polydup ignore add --files path/to/file.ts:10-25".bright_cyan()
        );
        return Ok(());
    }

    match format {
        OutputFormat::Json => {
            let json = serde_json::to_string_pretty(&entries)
                .context("Failed to serialize ignore list")?;
            println!("{}", json);
        }
        OutputFormat::Text => {
            println!(
                "{}",
                format!("Ignored Duplicates ({})", entries.len()).bold()
            );
            println!();

            for (idx, entry) in entries.iter().enumerate() {
                println!(
                    "{} {}",
                    format!("{}.", idx + 1).bright_black(),
                    entry.id.bright_blue()
                );
                println!("   {} {}", "Reason:".dimmed(), entry.reason);
                println!("   {} {}", "Added by:".dimmed(), entry.added_by);
                println!(
                    "   {} {}",
                    "Added at:".dimmed(),
                    entry.added_at.format("%Y-%m-%d %H:%M:%S UTC")
                );

                if verbose {
                    println!("   {}", "Files:".dimmed());
                    for file_range in &entry.files {
                        println!("     {} {}", "•".bright_black(), file_range);
                    }
                } else if !entry.files.is_empty() {
                    println!("   {} {} file(s)", "Files:".dimmed(), entry.files.len());
                }
                println!();
            }
        }
    }

    Ok(())
}

/// Handle the 'remove' subcommand
fn handle_remove_command(id: &str, manager: &mut dupe_core::IgnoreManager) -> Result<()> {
    let removed = manager.remove_ignore(id);

    if removed {
        manager.save().context("Failed to save ignore file")?;
        println!("{}", "✓ Duplicate removed from ignore list".green());
    } else {
        println!("{}", format!("✗ Duplicate ID not found: {}", id).red());
        std::process::exit(1);
    }

    Ok(())
}
