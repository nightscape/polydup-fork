//! PolyDup CLI - Command-line interface for duplicate code detection
//!
//! Cross-language duplicate code detector using Tree-sitter and Rabin-Karp hashing.
//! Supports Rust, Python, and JavaScript/TypeScript.

use anyhow::Result;
use clap::Parser;

mod cli;
mod commands;
mod config;
mod defaults;
mod detect;
mod init;
mod reporting;
mod vcs;

use cli::{Cli, Commands};

fn main() -> Result<()> {
    let cli = Cli::parse();

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
            };
            commands::scan::run(config)?;
        }
        Some(Commands::Init {
            force,
            non_interactive,
        }) => {
            let args = init::InitArgs {
                force,
                non_interactive,
            };
            init::cmd_init(args)?;
        }
        Some(Commands::Ignore { command }) => {
            commands::ignore::run(command, cli.min_block_size, cli.similarity)?;
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
                std::process::exit(1);
            }
        }
    }

    Ok(())
}
