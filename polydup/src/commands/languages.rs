//! Languages command implementation
//!
//! Lists supported programming languages with their file extensions and support status.

use anyhow::Result;
use colored::*;
use dupe_core::{get_supported_languages, LanguageStatus};
use serde::Serialize;

use crate::cli::OutputFormat;

/// Configuration for the languages command
pub struct LanguagesConfig {
    pub format: OutputFormat,
    pub full_only: bool,
}

/// JSON output structure
#[derive(Serialize)]
struct LanguagesOutput {
    languages: Vec<LanguageEntry>,
    total: usize,
    full_support: usize,
}

#[derive(Serialize)]
struct LanguageEntry {
    name: &'static str,
    extensions: Vec<&'static str>,
    parser: &'static str,
    type3_support: bool,
    status: &'static str,
}

/// Run the languages command
pub fn run(config: LanguagesConfig) -> Result<()> {
    let languages = get_supported_languages();

    // Filter if --full-only is specified
    let filtered: Vec<_> = if config.full_only {
        languages
            .into_iter()
            .filter(|l| l.status == LanguageStatus::Full)
            .collect()
    } else {
        languages
    };

    let full_count = filtered
        .iter()
        .filter(|l| l.status == LanguageStatus::Full)
        .count();

    match config.format {
        OutputFormat::Json | OutputFormat::Sarif => {
            // SARIF format falls back to JSON for languages list
            let output = LanguagesOutput {
                languages: filtered
                    .iter()
                    .map(|l| LanguageEntry {
                        name: l.name,
                        extensions: l.extensions.to_vec(),
                        parser: l.parser,
                        type3_support: l.type3_support,
                        status: match l.status {
                            LanguageStatus::Full => "full",
                            LanguageStatus::Partial => "partial",
                            LanguageStatus::Planned => "planned",
                        },
                    })
                    .collect(),
                total: filtered.len(),
                full_support: full_count,
            };
            println!("{}", serde_json::to_string_pretty(&output)?);
        }
        OutputFormat::Text | OutputFormat::Vscode | OutputFormat::Compiler => {
            // IDE formats fall back to text for languages list
            println!("Supported Languages:");
            println!();

            for lang in &filtered {
                let status_icon = match lang.status {
                    LanguageStatus::Full => "✓".green().to_string(),
                    LanguageStatus::Partial => "◐".yellow().to_string(),
                    LanguageStatus::Planned => "○".bright_black().to_string(),
                };

                let extensions = lang
                    .extensions
                    .iter()
                    .map(|e| format!(".{}", e))
                    .collect::<Vec<_>>()
                    .join(", ");

                let status_text = match lang.status {
                    LanguageStatus::Full => "",
                    LanguageStatus::Partial => " (partial)",
                    LanguageStatus::Planned => " (planned)",
                };

                println!(
                    "  {} {:<20} {}{}",
                    status_icon, lang.name, extensions, status_text
                );
            }

            println!();
            println!(
                "Total: {} languages ({} with full support)",
                filtered.len(),
                full_count
            );
            println!();
            println!("Legend:");
            println!(
                "  {} Full      - Complete Tree-sitter parsing support",
                "✓".green()
            );
            println!(
                "  {} Partial   - Uses another language's parser (e.g., Vue uses JavaScript)",
                "◐".yellow()
            );
            println!("  {} Planned   - Not yet implemented", "○".bright_black());
        }
    }

    Ok(())
}
