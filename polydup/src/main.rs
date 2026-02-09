//! PolyDup CLI - Command-line interface for duplicate code detection
//!
//! Cross-language duplicate code detector using Tree-sitter and Rabin-Karp hashing.
//! Supports Rust, Python, and JavaScript/TypeScript.
//!
//! # Exit Codes
//!
//! - `0`: Success, no duplicates found (or duplicates found but not in fail mode)
//! - `1`: Duplicates found with `--fail-on-duplicates` or `--fail-on-new`
//! - `2`: Error (invalid paths, parsing failures, git failures, etc.)

use anyhow::Result;
use clap::Parser;

mod cli;
mod commands;
mod config;
mod defaults;
mod detect;
mod init;
mod install_check;
mod reporting;
mod vcs;
mod version_check;

use cli::{Cli, Commands};

/// Exit code for errors (invalid args, file errors, git failures, etc.)
const EXIT_ERROR: i32 = 2;

fn main() {
    if let Err(e) = run() {
        eprintln!("Error: {:#}", e);
        std::process::exit(EXIT_ERROR);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();

    // Check for updates (unless disabled)
    if !cli.no_update_check {
        version_check::check_for_updates();
    }

    // Handle commands (with backward compatibility for old-style invocation)
    match cli.command {
        Some(Commands::Scan {
            paths,
            format,
            min_block_size,
            similarity,
            verbose,
            quiet,
            exclude,
            enable_type3,
            type3_tolerance,
            output,
            no_color,
            only_type,
            exclude_type,
            group_by,
            debug,
            save_baseline,
            compare_to,
            git_diff,
            enable_directives,
            include_tests,
            progress,
            no_progress,
            fail_on_duplicates,
            fail_on_new,
            show_code,
            preview_lines,
            suggest_refactoring,
            show_ids,
            dry_run,
            id_length,
            full_ids,
        }) => {
            let config = commands::scan::ScanConfig {
                paths,
                format,
                min_block_size,
                similarity,
                verbose,
                quiet,
                exclude,
                enable_type3,
                type3_tolerance,
                output,
                no_color,
                only_type,
                exclude_type,
                group_by,
                debug,
                save_baseline,
                compare_to,
                git_diff,
                enable_directives,
                include_tests,
                progress,
                no_progress,
                fail_on_duplicates,
                fail_on_new,
                show_code,
                preview_lines,
                suggest_refactoring,
                show_ids,
                dry_run,
                id_length,
                full_ids,
            };
            commands::scan::run(config)?;
        }
        Some(Commands::Init {
            force,
            non_interactive,
            ci_only,
        }) => {
            let args = init::InitArgs {
                force,
                non_interactive,
                ci_only,
            };
            init::cmd_init(args)?;
        }
        Some(Commands::Config { command }) => {
            commands::config::run(command)?;
        }
        Some(Commands::Cache { command }) => {
            command.execute()?;
        }
        Some(Commands::Ignore { command }) => {
            commands::ignore::run(command, cli.min_block_size, cli.similarity)?;
        }
        Some(Commands::CheckInstall) => {
            install_check::run()?;
        }
        Some(Commands::Languages { format, full_only }) => {
            let config = commands::languages::LanguagesConfig { format, full_only };
            commands::languages::run(config)?;
        }
        Some(Commands::Upgrade { check_only, force }) => {
            commands::upgrade::run(check_only, force)?;
        }
        Some(Commands::Completions { shell }) => {
            cli::generate_completions(shell);
        }
        Some(Commands::Watch {
            paths,
            min_block_size,
            similarity,
            exclude,
            debounce_ms,
            clear,
            verbose,
            no_color,
        }) => {
            let config = commands::watch::WatchConfig {
                paths,
                min_block_size,
                similarity,
                exclude,
                debounce_ms,
                clear,
                verbose,
                no_color,
            };
            commands::watch::run(config)?;
        }
        None => {
            // Backward compatibility: if no subcommand but paths provided, run scan
            if !cli.paths.is_empty() {
                let config = commands::scan::ScanConfig {
                    paths: cli.paths,
                    format: cli.format,
                    min_block_size: cli.min_block_size,
                    similarity: cli.similarity,
                    verbose: cli.verbose,
                    quiet: cli.quiet,
                    exclude: cli.exclude,
                    enable_type3: cli.enable_type3,
                    type3_tolerance: cli.type3_tolerance,
                    output: cli.output,
                    no_color: cli.no_color,
                    only_type: cli.only_type,
                    exclude_type: cli.exclude_type,
                    group_by: cli.group_by,
                    debug: cli.debug,
                    save_baseline: None,
                    compare_to: None,
                    git_diff: None,
                    enable_directives: false,
                    include_tests: cli.include_tests,
                    progress: cli.progress,
                    no_progress: cli.no_progress,
                    fail_on_duplicates: false,
                    fail_on_new: false,
                    show_code: cli.show_code,
                    preview_lines: cli.preview_lines,
                    suggest_refactoring: false,
                    show_ids: cli.show_ids,
                    dry_run: cli.dry_run,
                    id_length: cli.id_length,
                    full_ids: cli.full_ids,
                };
                commands::scan::run(config)?;
            } else {
                eprintln!("Error: No command specified");
                eprintln!();
                eprintln!("Usage:");
                eprintln!("  polydup scan <PATHS>...    Scan for duplicate code");
                eprintln!("  polydup init               Initialize configuration");
                eprintln!();
                eprintln!("For backward compatibility, you can also use:");
                eprintln!("  polydup <PATHS>...         Same as 'polydup scan'");
                std::process::exit(EXIT_ERROR);
            }
        }
    }

    Ok(())
}
