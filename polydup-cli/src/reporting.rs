//! Output formatting and report generation
//!
//! This module handles visual output rendering, including the dashboard-style
//! text reports with box-drawing characters and colored output.

use anyhow::Result;
use colored::*;

/// Format human-readable text report as a string
pub fn format_text_report(
    report: &dupe_core::Report,
    verbose: bool,
    group_by: Option<&str>,
) -> Result<String> {
    let mut output = String::new();

    // Box-drawing dashboard
    let width = 63;
    output.push_str(&format!(
        "{}\n",
        format!("╔{}╗", "═".repeat(width)).bright_black()
    ));
    output.push_str(&format!(
        "{} {} {}\n",
        "║".bright_black(),
        format!("{:^width$}", "Scan Results", width = width)
            .bright_cyan()
            .bold(),
        "║".bright_black()
    ));
    output.push_str(&format!(
        "{}\n",
        format!("╠{}╣", "═".repeat(width)).bright_black()
    ));

    // Basic statistics
    output.push_str(&format!(
        "{} {:<20} {:>width$} {}\n",
        "║".bright_black(),
        "Files scanned:".bright_white(),
        report.files_scanned.to_string().bold(),
        "║".bright_black(),
        width = width - 22
    ));
    output.push_str(&format!(
        "{} {:<20} {:>width$} {}\n",
        "║".bright_black(),
        "Functions analyzed:".bright_white(),
        report.functions_analyzed.to_string().bold(),
        "║".bright_black(),
        width = width - 22
    ));
    output.push_str(&format!(
        "{} {:<20} {:>width$} {}\n",
        "║".bright_black(),
        "Duplicates found:".bright_white(),
        if report.duplicates.is_empty() {
            report.duplicates.len().to_string().green().bold()
        } else {
            report.duplicates.len().to_string().yellow().bold()
        },
        "║".bright_black(),
        width = width - 22
    ));

    // Lines saved estimation (only if duplicates found)
    if !report.duplicates.is_empty() {
        let lines_saved: usize = report.duplicates.iter().map(|d| d.length).sum();
        output.push_str(&format!(
            "{} {:<20} {:>width$} {}\n",
            "║".bright_black(),
            "Estimated savings:".bright_white(),
            format!("~{} lines", lines_saved).green().bold(),
            "║".bright_black(),
            width = width - 22
        ));
    }

    // Clone type breakdown (if duplicates found)
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

        // Similarity range
        let min_similarity = report
            .duplicates
            .iter()
            .map(|d| d.similarity)
            .min_by(|a, b| a.partial_cmp(b).unwrap())
            .unwrap_or(0.0);
        let max_similarity = report
            .duplicates
            .iter()
            .map(|d| d.similarity)
            .max_by(|a, b| a.partial_cmp(b).unwrap())
            .unwrap_or(0.0);

        output.push_str(&format!(
            "{}\n",
            format!("╠{}╣", "═".repeat(width)).bright_black()
        ));
        output.push_str(&format!(
            "{} {:<width$} {}\n",
            "║".bright_black(),
            "Clone Type Breakdown:".bright_white(),
            "║".bright_black(),
            width = width
        ));

        if type1_count > 0 {
            output.push_str(&format!(
                "{} {} {:>3} {} {} {}\n",
                "║".bright_black(),
                "  Type-1 (exact):".red(),
                type1_count.to_string().red().bold(),
                "groups │".dimmed(),
                "Critical priority".red(),
                "║".bright_black()
            ));
        }
        if type2_count > 0 {
            output.push_str(&format!(
                "{} {} {:>3} {} {} {}\n",
                "║".bright_black(),
                "  Type-2 (renamed):".yellow(),
                type2_count.to_string().yellow().bold(),
                "groups │".dimmed(),
                "High priority    ".yellow(),
                "║".bright_black()
            ));
        }
        if type3_count > 0 {
            output.push_str(&format!(
                "{} {} {:>3} {} {} {}\n",
                "║".bright_black(),
                "  Type-3 (modified):".bright_yellow(),
                type3_count.to_string().bright_yellow().bold(),
                "groups │".dimmed(),
                "Medium priority  ".bright_yellow(),
                "║".bright_black()
            ));
        }

        output.push_str(&format!(
            "{} {:<width$} {}\n",
            "║".bright_black(),
            format!(
                "  Similarity range: {:.1}% - {:.1}%",
                min_similarity * 100.0,
                max_similarity * 100.0
            )
            .dimmed(),
            "║".bright_black(),
            width = width
        ));

        // Top offenders section
        append_top_offenders(&mut output, report, width);
    }

    output.push_str(&format!(
        "{}\n",
        format!("╚{}╝", "═".repeat(width)).bright_black()
    ));

    if verbose {
        append_performance_stats(&mut output, report);
    }

    if report.duplicates.is_empty() {
        output.push('\n');
        output.push_str(&format!("{}\n", "✓ No duplicates found!".green().bold()));
        return Ok(output);
    }

    output.push('\n');
    output.push_str(&format!("{}\n", "Duplicates".bright_cyan().bold()));
    output.push_str(&format!("{}\n", "═".repeat(60).bright_black()));

    // Group duplicates by hash to find multi-way clones
    let multi_way_groups = find_multi_way_groups(&report.duplicates);

    // Render multi-way groups first
    if !multi_way_groups.is_empty() {
        append_multi_way_groups(&mut output, &multi_way_groups, verbose);
    }

    // Render remaining pairwise duplicates
    let grouped_hashes: std::collections::HashSet<u64> =
        multi_way_groups.iter().map(|g| g.hash).collect();
    let pairwise_dups: Vec<_> = report
        .duplicates
        .iter()
        .filter(|d| !grouped_hashes.contains(&d.hash))
        .collect();

    if !pairwise_dups.is_empty() {
        append_duplicate_listings(&mut output, &pairwise_dups, group_by, verbose);
    }

    output.push('\n');
    output.push_str(&format!(
        "{}\n",
        "Tip: Use --format json for machine-readable output"
            .dimmed()
            .italic()
    ));

    Ok(output)
}

/// Append top offenders section to the report
fn append_top_offenders(output: &mut String, report: &dupe_core::Report, width: usize) {
    use std::collections::HashMap;
    let mut file_counts: HashMap<String, usize> = HashMap::new();

    // Count duplicates per file (each duplicate affects 2 files)
    for dup in &report.duplicates {
        *file_counts.entry(dup.file1.clone()).or_insert(0) += 1;
        *file_counts.entry(dup.file2.clone()).or_insert(0) += 1;
    }

    // Sort by count descending, take top 5
    let mut top_offenders: Vec<(String, usize)> = file_counts.into_iter().collect();
    top_offenders.sort_by(|a, b| b.1.cmp(&a.1));
    top_offenders.truncate(5);

    if !top_offenders.is_empty() {
        output.push_str(&format!(
            "{}\n",
            format!("╠{}╣", "═".repeat(width)).bright_black()
        ));
        output.push_str(&format!(
            "{} {:<width$} {}\n",
            "║".bright_black(),
            "Top Offenders:".bright_white(),
            "║".bright_black(),
            width = width
        ));

        for (idx, (file, count)) in top_offenders.iter().enumerate() {
            // Truncate filename if too long
            let display_name = if file.len() > 40 {
                format!("...{}", &file[file.len() - 37..])
            } else {
                file.clone()
            };

            output.push_str(&format!(
                "{} {} {:<43} {:>3} {} {}\n",
                "║".bright_black(),
                format!("  {}.", idx + 1).dimmed(),
                display_name,
                count,
                "duplicates".dimmed(),
                "║".bright_black()
            ));
        }
    }
}

/// Append performance statistics to the report
fn append_performance_stats(output: &mut String, report: &dupe_core::Report) {
    output.push('\n');
    output.push_str(&format!("{}\n", "Performance:".bright_white()));
    output.push_str(&format!(
        "  {:<15} {}ms\n",
        "Duration:".dimmed(),
        report.stats.duration_ms
    ));
    output.push_str(&format!(
        "  {:<15} {}\n",
        "Total tokens:".dimmed(),
        report.stats.total_tokens
    ));
    output.push_str(&format!(
        "  {:<15} {}\n",
        "Unique hashes:".dimmed(),
        report.stats.unique_hashes
    ));
}

/// A multi-way clone group (same code in 3+ files)
struct MultiWayGroup {
    hash: u64,
    clone_type: dupe_core::CloneType,
    length: usize,
    /// All instances of this clone: (file, start_line)
    instances: Vec<(String, usize)>,
    duplicate_id: Option<String>,
}

/// Find multi-way clone groups (duplicates that appear in 3+ files)
fn find_multi_way_groups(duplicates: &[dupe_core::DuplicateMatch]) -> Vec<MultiWayGroup> {
    use std::collections::{HashMap, HashSet};

    // Group duplicates by hash
    let mut hash_groups: HashMap<u64, Vec<&dupe_core::DuplicateMatch>> = HashMap::new();
    for dup in duplicates {
        hash_groups.entry(dup.hash).or_default().push(dup);
    }

    // Find groups with 3+ unique files
    let mut multi_way_groups = Vec::new();
    for (hash, dups) in hash_groups {
        // Collect all unique file:line pairs using HashSet for O(1) lookups
        let mut instances_set: HashSet<(String, usize)> = HashSet::new();
        for dup in &dups {
            instances_set.insert((dup.file1.clone(), dup.start_line1));
            instances_set.insert((dup.file2.clone(), dup.start_line2));
        }

        // Only include if 3+ unique locations
        if instances_set.len() >= 3 {
            // Convert to Vec and sort by file path for consistent display
            let mut instances: Vec<(String, usize)> = instances_set.into_iter().collect();
            instances.sort_by(|a, b| a.0.cmp(&b.0));

            let first_dup = dups[0];
            multi_way_groups.push(MultiWayGroup {
                hash,
                clone_type: first_dup.clone_type.clone(),
                length: first_dup.length,
                instances,
                duplicate_id: first_dup.duplicate_id.clone(),
            });
        }
    }

    // Sort groups by instance count (descending), then by length
    multi_way_groups.sort_by(|a, b| {
        b.instances
            .len()
            .cmp(&a.instances.len())
            .then(b.length.cmp(&a.length))
    });

    multi_way_groups
}

/// Append multi-way clone groups to the report
fn append_multi_way_groups(output: &mut String, groups: &[MultiWayGroup], verbose: bool) {
    if !groups.is_empty() {
        output.push_str(&format!(
            "\n{}\n",
            format!("── Multi-Way Clone Groups ({} groups) ──", groups.len())
                .bright_magenta()
                .bold()
        ));
    }

    for (idx, group) in groups.iter().enumerate() {
        let idx = idx + 1; // 1-based indexing

        let clone_type_str = match group.clone_type {
            dupe_core::CloneType::Type1 => "Type-1 (exact)".to_string().red(),
            dupe_core::CloneType::Type2 => "Type-2 (renamed)".to_string().yellow(),
            dupe_core::CloneType::Type3 => "Type-3 (modified)".to_string().bright_yellow(),
        };

        let estimated_savings = (group.instances.len() - 1) * group.length;

        output.push_str(&format!(
            "\n{} {} | {} | {} instances\n",
            format!("{}.", idx).bright_black(),
            clone_type_str,
            format!("{} tokens", group.length).dimmed(),
            group.instances.len().to_string().bright_cyan().bold(),
        ));

        // Show all instances
        let last_idx = group.instances.len() - 1;
        for (i, (file, line)) in group.instances.iter().enumerate() {
            let prefix = if i == last_idx { "└─" } else { "├─" };
            output.push_str(&format!(
                "   {} {}:{}\n",
                prefix.bright_black(),
                file,
                line.to_string().bright_blue()
            ));
        }

        // Show estimated savings
        output.push_str(&format!(
            "   {} {}\n",
            "★".bright_green(),
            format!(
                "Estimated savings: ~{} tokens ({} of {} can be refactored)",
                estimated_savings,
                group.instances.len() - 1,
                group.instances.len()
            )
            .green()
        ));

        if verbose {
            output.push_str(&format!("      {} {:#x}\n", "Hash:".dimmed(), group.hash));
            if let Some(ref id) = group.duplicate_id {
                output.push_str(&format!("      {} {}\n", "ID:".dimmed(), id));
            }
        }
    }
}

/// Append individual duplicate listings to the report
fn append_duplicate_listings(
    output: &mut String,
    duplicates: &[&dupe_core::DuplicateMatch],
    group_by: Option<&str>,
    verbose: bool,
) {
    // Track current group for group headers
    let mut current_group: Option<String> = None;

    for (idx, dup) in duplicates.iter().enumerate() {
        // Add group header if grouping is enabled
        if let Some(criterion) = group_by {
            let group_key = match criterion {
                "file" => dup.file1.clone(),
                "similarity" => format!("{:.0}%", dup.similarity * 100.0),
                "type" => match dup.clone_type {
                    dupe_core::CloneType::Type1 => "Type-1 (exact)".to_string(),
                    dupe_core::CloneType::Type2 => "Type-2 (renamed)".to_string(),
                    dupe_core::CloneType::Type3 => "Type-3 (modified)".to_string(),
                },
                "size" => format!("{} tokens", dup.length),
                _ => String::new(),
            };

            if current_group.as_ref() != Some(&group_key) {
                current_group = Some(group_key.clone());
                output.push_str(&format!(
                    "\n{}\n",
                    format!("─── {} ───", group_key).bright_white().bold()
                ));
            }
        }

        let clone_type_str = match dup.clone_type {
            dupe_core::CloneType::Type1 => "Type-1 (exact)".to_string().red(),
            dupe_core::CloneType::Type2 => "Type-2 (renamed)".to_string().yellow(),
            dupe_core::CloneType::Type3 => "Type-3 (modified)".to_string().bright_yellow(),
        };

        // Add suppression indicator if applicable
        let suppression_indicator = if dup.suppressed_by_directive == Some(true) {
            format!(" {}", "[SUPPRESSED]".dimmed().italic())
        } else {
            String::new()
        };

        output.push_str(&format!(
            "\n{} {}{} | {} | {}\n",
            format!("{}.", idx + 1).bright_black(),
            clone_type_str,
            suppression_indicator,
            format!("Similarity: {:.1}%", dup.similarity * 100.0).dimmed(),
            format!("Length: {} tokens", dup.length).dimmed()
        ));

        // Show file paths with line numbers
        output.push_str(&format!(
            "   {} {}:{}\n",
            "├─".bright_black(),
            dup.file1,
            dup.start_line1.to_string().bright_blue()
        ));
        output.push_str(&format!(
            "   {} {}:{}\n",
            "└─".bright_black(),
            dup.file2,
            dup.start_line2.to_string().bright_blue()
        ));

        if verbose {
            output.push_str(&format!("      {} {:#x}\n", "Hash:".dimmed(), dup.hash));
        }
    }
}
