//! Output formatting and report generation
//!
//! This module handles visual output rendering, including the dashboard-style
//! text reports with box-drawing characters and colored output.

use anyhow::Result;
use colored::*;
use tabled::{builder::Builder, settings::Style};

/// Truncate a duplicate ID to the specified length
///
/// IDs are 64-character SHA256 hashes. This function truncates them
/// to a shorter form (default: 8 characters) for display purposes.
fn truncate_id(id: &str, length: usize) -> String {
    if length >= id.len() {
        id.to_string()
    } else {
        format!("{}...", &id[..length])
    }
}

/// Format human-readable text report as a string
#[allow(clippy::too_many_arguments)]
pub fn format_text_report(
    report: &dupe_core::Report,
    verbose: bool,
    group_by: Option<&str>,
    show_code: bool,
    preview_lines: usize,
    suggest_refactoring: bool,
    show_ids: bool,
    type3_enabled: bool,
    id_length: usize,
) -> Result<String> {
    let mut output = String::new();

    // Build dashboard table using tabled
    let mut builder = Builder::default();
    
    // Header
    builder.push_record([format!("{:^63}", "Scan Results").bright_cyan().bold().to_string()]);
    
    // Basic statistics - format as single entries with padding
    builder.push_record([format!("Files scanned: {}", report.files_scanned.to_string().bold())]);
    builder.push_record([format!("Functions analyzed: {}", report.functions_analyzed.to_string().bold())]);
    builder.push_record([format!(
        "Duplicates found: {}", 
        if report.duplicates.is_empty() {
            report.duplicates.len().to_string().green().bold()
        } else {
            report.duplicates.len().to_string().yellow().bold()
        }
    )]);

    // Show skipped files count (if any)
    if !report.skipped_files.is_empty() {
        builder.push_record([format!("Files skipped: {}", report.skipped_files.len().to_string().yellow())]);
    }

    // Show ignored duplicates count (if any)
    let total_ignored =
        report.stats.suppressed_by_ignore_file + report.stats.suppressed_by_directive;
    if total_ignored > 0 {
        builder.push_record([format!("Duplicates ignored: {}", total_ignored.to_string().dimmed())]);
    }

    // Lines saved estimation (only if duplicates found)
    if !report.duplicates.is_empty() {
        let lines_saved: usize = report.duplicates.iter().map(|d| d.length).sum();
        builder.push_record([format!("Estimated savings: {}", format!("~{} lines", lines_saved).green().bold())]);
    }

    let dashboard = builder
        .build()
        .with(Style::empty()
            .top('═')
            .bottom('═')
            .left('║')
            .right('║')
            .horizontal('═')
            .vertical('║')
            .corner_top_left('╔')
            .corner_top_right('╗')
            .corner_bottom_left('╚')
            .corner_bottom_right('╝')
            .intersection_top('╠')
            .intersection_bottom('╣')
            .intersection('╬')
            .remove_horizontal())
        .to_string();
    
    output.push_str(&dashboard);
    output.push('\n');

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

        // Clone type breakdown section
        let mut clone_builder = Builder::default();
        clone_builder.push_record(["Clone Type Breakdown:".bright_white().to_string()]);

        if type1_count > 0 {
            clone_builder.push_record([format!("  Type-1 (exact): {} groups │ Critical priority", type1_count).red().to_string()]);
        }
        if type2_count > 0 {
            clone_builder.push_record([format!("  Type-2 (renamed): {} groups │ High priority", type2_count).yellow().to_string()]);
        }
        if type3_count > 0 {
            clone_builder.push_record([format!("  Type-3 (modified): {} groups │ Medium priority", type3_count).bright_yellow().to_string()]);
        }

        clone_builder.push_record([format!(
            "  Similarity range: {:.1}% - {:.1}%",
            min_similarity * 100.0,
            max_similarity * 100.0
        )
        .dimmed()
        .to_string()]);

        let clone_table = clone_builder
            .build()
            .with(Style::empty()
                .top('═')
                .bottom('═')
                .left('║')
                .right('║')
                .horizontal('═')
                .vertical('║')
                .corner_top_left('╔')
                .corner_top_right('╗')
                .corner_bottom_left('╚')
                .corner_bottom_right('╝')
                .intersection_top('╠')
                .intersection_bottom('╣')
                .intersection('╬')
                .remove_horizontal())
            .to_string();

        output.push_str(&clone_table);
        output.push('\n');

        // Top offenders section
        append_top_offenders(&mut output, report);
    }

    if verbose {
        append_performance_stats(&mut output, report);
    }

    // Show skipped files details in verbose mode
    if verbose && !report.skipped_files.is_empty() {
        output.push('\n');
        output.push_str(&format!(
            "{}\n",
            format!("Skipped files ({}):", report.skipped_files.len())
                .yellow()
                .bold()
        ));
        for skipped in &report.skipped_files {
            output.push_str(&format!(
                "  {} {}\n",
                format!("- {}:", skipped.path).dimmed(),
                skipped.reason.bright_red()
            ));
        }
    }

    if report.duplicates.is_empty() {
        output.push('\n');
        output.push_str(&format!("{}\n", "✓ No duplicates found!".green().bold()));

        // Suggest Type-3 detection if not enabled
        if !type3_enabled {
            output.push('\n');
            output.push_str(&format!(
                "{}\n",
                "Tip: Enable Type-3 detection to find similar code with structural differences:"
                    .dimmed()
            ));
            output.push_str(&format!(
                "  {}\n",
                "polydup scan . --enable-type3".bright_cyan()
            ));
        }

        return Ok(output);
    }

    output.push('\n');

    // Group duplicates by hash to find multi-way clones
    let multi_way_groups = find_multi_way_groups(&report.duplicates);

    // Render multi-way groups first (highest refactoring value)
    if !multi_way_groups.is_empty() {
        append_multi_way_groups(&mut output, &multi_way_groups, verbose, show_ids, id_length);
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
        // Add section header for clone pairs
        output.push_str(&format!(
            "\n{}\n",
            format!("── Clone Pairs ({} pairs) ──", pairwise_dups.len())
                .bright_cyan()
                .bold()
        ));
        output.push_str(&format!(
            "{}\n",
            "Duplicates appearing in 2 files. Lower refactoring ROI than clone groups."
                .dimmed()
                .italic()
        ));

        append_duplicate_listings(
            &mut output,
            &pairwise_dups,
            group_by,
            verbose,
            show_code,
            preview_lines,
            suggest_refactoring,
            show_ids,
            id_length,
        );
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
fn append_top_offenders(output: &mut String, report: &dupe_core::Report) {
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
        let mut offenders_builder = Builder::default();
        offenders_builder.push_record(["Top Offenders:".bright_white().to_string()]);

        for (idx, (file, count)) in top_offenders.iter().enumerate() {
            // Truncate filename if too long
            let display_name = if file.len() > 40 {
                format!("...{}", &file[file.len() - 37..])
            } else {
                file.clone()
            };

            offenders_builder.push_record([format!("  {}. {} {} duplicates", idx + 1, display_name, count)]);
        }

        let offenders_table = offenders_builder
            .build()
            .with(Style::empty()
                .top('═')
                .bottom('═')
                .left('║')
                .right('║')
                .horizontal('═')
                .vertical('║')
                .corner_top_left('╔')
                .corner_top_right('╗')
                .corner_bottom_left('╚')
                .corner_bottom_right('╝')
                .intersection_top('╠')
                .intersection_bottom('╣')
                .intersection('╬')
                .remove_horizontal())
            .to_string();

        output.push_str(&offenders_table);
        output.push('\n');
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
    /// All instances of this clone: (file, start_line, end_line)
    instances: Vec<(String, usize, Option<usize>)>,
    duplicate_id: Option<String>,
}

/// Find multi-way clone groups (duplicates that appear in 3+ files)
fn find_multi_way_groups(duplicates: &[dupe_core::DuplicateMatch]) -> Vec<MultiWayGroup> {
    use std::collections::HashMap;

    // Group duplicates by hash
    let mut hash_groups: HashMap<u64, Vec<&dupe_core::DuplicateMatch>> = HashMap::new();
    for dup in duplicates {
        hash_groups.entry(dup.hash).or_default().push(dup);
    }

    // Find groups with 3+ unique files
    let mut multi_way_groups = Vec::new();
    for (hash, dups) in hash_groups {
        // Collect all unique file:line pairs with end lines
        // Use HashMap to map (file, start_line) -> end_line
        let mut instances_map: HashMap<(String, usize), Option<usize>> = HashMap::new();
        for dup in &dups {
            instances_map
                .entry((dup.file1.clone(), dup.start_line1))
                .or_insert(dup.end_line1);
            instances_map
                .entry((dup.file2.clone(), dup.start_line2))
                .or_insert(dup.end_line2);
        }

        // Only include if 3+ unique locations
        if instances_map.len() >= 3 {
            // Convert to Vec and sort by file path for consistent display
            let mut instances: Vec<(String, usize, Option<usize>)> = instances_map
                .into_iter()
                .map(|((file, start), end)| (file, start, end))
                .collect();
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
fn append_multi_way_groups(
    output: &mut String,
    groups: &[MultiWayGroup],
    verbose: bool,
    show_ids: bool,
    id_length: usize,
) {
    if !groups.is_empty() {
        output.push_str(&format!(
            "\n{}\n",
            format!(
                "── Clone Groups ({} groups, 3+ instances each) ──",
                groups.len()
            )
            .bright_magenta()
            .bold()
        ));
        output.push_str(&format!(
            "{}\n",
            "Highest refactoring value: consolidating each group eliminates multiple duplicates."
                .dimmed()
                .italic()
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
        for (i, (file, start_line, end_line)) in group.instances.iter().enumerate() {
            let prefix = if i == last_idx { "└─" } else { "├─" };
            let line_range = match end_line {
                Some(end) if *end != *start_line => format!("{}-{}", start_line, end),
                _ => start_line.to_string(),
            };
            output.push_str(&format!(
                "   {} {}:{}\n",
                prefix.bright_black(),
                file,
                line_range.bright_blue()
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

        // Show ID when show_ids is true (truncated by default)
        if show_ids {
            if let Some(ref id) = group.duplicate_id {
                let display_id = truncate_id(id, id_length);
                output.push_str(&format!(
                    "   {} {}\n",
                    "ID:".dimmed(),
                    display_id.bright_cyan()
                ));
            }
        }

        if verbose {
            output.push_str(&format!("      {} {:#x}\n", "Hash:".dimmed(), group.hash));
            if !show_ids {
                if let Some(ref id) = group.duplicate_id {
                    // In verbose mode, always show full ID
                    output.push_str(&format!("      {} {}\n", "ID:".dimmed(), id));
                }
            }
        }
    }
}

/// Append individual duplicate listings to the report
#[allow(clippy::too_many_arguments)]
fn append_duplicate_listings(
    output: &mut String,
    duplicates: &[&dupe_core::DuplicateMatch],
    group_by: Option<&str>,
    verbose: bool,
    show_code: bool,
    preview_lines: usize,
    suggest_refactoring: bool,
    show_ids: bool,
    id_length: usize,
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

        // Show duplicate ID if requested (truncated by default)
        if show_ids {
            if let Some(ref id) = dup.duplicate_id {
                let display_id = truncate_id(id, id_length);
                output.push_str(&format!(
                    "   {} {}\n",
                    "ID:".dimmed(),
                    display_id.bright_cyan()
                ));
            }
        }

        // Show file paths with line ranges
        let line_range1 = match dup.end_line1 {
            Some(end) if end != dup.start_line1 => format!("{}-{}", dup.start_line1, end),
            _ => dup.start_line1.to_string(),
        };
        let line_range2 = match dup.end_line2 {
            Some(end) if end != dup.start_line2 => format!("{}-{}", dup.start_line2, end),
            _ => dup.start_line2.to_string(),
        };
        output.push_str(&format!(
            "   {} {}:{}\n",
            "├─".bright_black(),
            dup.file1,
            line_range1.bright_blue()
        ));
        output.push_str(&format!(
            "   {} {}:{}\n",
            "└─".bright_black(),
            dup.file2,
            line_range2.bright_blue()
        ));

        if verbose {
            output.push_str(&format!("      {} {:#x}\n", "Hash:".dimmed(), dup.hash));
        }

        // Show code preview if enabled
        if show_code {
            let end_line1 = dup.end_line1.unwrap_or(dup.start_line1 + preview_lines);
            append_code_preview(
                output,
                &dup.file1,
                dup.start_line1,
                end_line1,
                preview_lines,
            );
        }

        // Show refactoring suggestions if enabled
        if suggest_refactoring {
            append_refactoring_suggestion(output, dup);
        }
    }
}

/// Read and format a code preview from a file
fn append_code_preview(
    output: &mut String,
    file_path: &str,
    start_line: usize,
    end_line: usize,
    max_lines: usize,
) {
    use std::fs::File;
    use std::io::{BufRead, BufReader};

    let file = match File::open(file_path) {
        Ok(f) => f,
        Err(_) => {
            output.push_str(&format!(
                "   {} {}\n",
                "Code Preview:".bright_black(),
                "(file not accessible)".dimmed()
            ));
            return;
        }
    };

    let reader = BufReader::new(file);
    let lines: Vec<String> = reader
        .lines()
        .skip(start_line.saturating_sub(1))
        .take(end_line.saturating_sub(start_line.saturating_sub(1)))
        .filter_map(|l| l.ok())
        .collect();

    if lines.is_empty() {
        return;
    }

    // Determine how many lines to show
    let display_lines = lines.len().min(max_lines);
    let truncated = lines.len() > max_lines;

    output.push('\n');
    output.push_str(&format!("   {}\n", "Code Preview:".bright_cyan()));

    // Build code preview table using tabled
    let mut code_builder = Builder::default();
    
    // Add header row
    code_builder.push_record([
        "Line".dimmed().to_string(),
        "Code".to_string(),
    ]);

    for (i, line) in lines.iter().take(display_lines).enumerate() {
        let line_num = start_line + i;
        // Truncate very long lines
        let display_line = if line.len() > 60 {
            format!("{}...", &line[..57])
        } else {
            line.to_string()
        };
        code_builder.push_record([
            line_num.to_string().dimmed().to_string(),
            display_line,
        ]);
    }

    if truncated {
        code_builder.push_record([
            "...".dimmed().to_string(),
            format!("({} more lines)", lines.len() - max_lines).dimmed().to_string(),
        ]);
    }

    let code_table = code_builder
        .build()
        .with(Style::empty()
            .top('─')
            .bottom('─')
            .left(' ')
            .right(' ')
            .horizontal('─')
            .vertical('│')
            .corner_top_left('┌')
            .corner_top_right('┐')
            .corner_bottom_left('└')
            .corner_bottom_right('┘')
            .intersection_left('├')
            .intersection_right('┤')
            .intersection_top('┬')
            .intersection_bottom('┴')
            .intersection('┼'))
        .to_string();

    // Add indentation to match original format
    for line in code_table.lines() {
        output.push_str(&format!("   {}\n", line));
    }
}

/// Generate refactoring suggestion based on duplicate characteristics
fn append_refactoring_suggestion(output: &mut String, dup: &dupe_core::DuplicateMatch) {
    let (suggestion, effort, risk) = match dup.clone_type {
        dupe_core::CloneType::Type1 => ("Extract to shared function", "Low", "Low"),
        dupe_core::CloneType::Type2 => {
            if dup.similarity >= 0.95 {
                (
                    "Extract to generic function with type parameters",
                    "Medium",
                    "Low",
                )
            } else {
                (
                    "Consider Strategy pattern or parameterized function",
                    "Medium",
                    "Medium",
                )
            }
        }
        dupe_core::CloneType::Type3 => (
            "Extract common logic, parameterize differences",
            "High",
            "High",
        ),
    };

    // Estimate line savings (rough approximation: tokens / 7 = lines)
    let estimated_lines = dup.length / 7;

    output.push_str(&format!(
        "   {} {}\n",
        "Suggestion:".bright_magenta(),
        suggestion
    ));
    output.push_str(&format!(
        "   {} Effort: {} | Risk: {} | Est. savings: ~{} lines\n",
        " ".repeat(11),
        effort.bright_cyan(),
        match risk {
            "Low" => risk.green(),
            "Medium" => risk.yellow(),
            _ => risk.red(),
        },
        estimated_lines
    ));
}

/// Format SARIF (Static Analysis Results Interchange Format) output
/// for GitHub Code Scanning integration
pub fn format_sarif_report(report: &dupe_core::Report) -> Result<String> {
    let results: Vec<serde_json::Value> = report
        .duplicates
        .iter()
        .map(|dup| {
            let (level, kind) = map_clone_to_sarif(dup);

            serde_json::json!({
                "ruleId": format!("polydup/{}", kind),
                "level": level,
                "message": {
                    "text": format!(
                        "{} duplicate with {} ({:.0}% similar, {} tokens)",
                        kind,
                        dup.file2,
                        dup.similarity * 100.0,
                        dup.length
                    )
                },
                "locations": [{
                    "physicalLocation": {
                        "artifactLocation": {
                            "uri": &dup.file1
                        },
                        "region": {
                            "startLine": dup.start_line1,
                            "endLine": dup.end_line1.unwrap_or(dup.start_line1)
                        }
                    }
                }],
                "relatedLocations": [{
                    "physicalLocation": {
                        "artifactLocation": {
                            "uri": &dup.file2
                        },
                        "region": {
                            "startLine": dup.start_line2,
                            "endLine": dup.end_line2.unwrap_or(dup.start_line2)
                        }
                    },
                    "message": {
                        "text": "Duplicate found here"
                    }
                }]
            })
        })
        .collect();

    let sarif = serde_json::json!({
        "$schema": "https://raw.githubusercontent.com/oasis-tcs/sarif-spec/master/Schemata/sarif-schema-2.1.0.json",
        "version": "2.1.0",
        "runs": [{
            "tool": {
                "driver": {
                    "name": "polydup",
                    "informationUri": "https://github.com/wiesnerbernard/polydup",
                    "version": env!("CARGO_PKG_VERSION"),
                    "rules": [
                        {
                            "id": "polydup/type-1",
                            "name": "ExactDuplicate",
                            "shortDescription": { "text": "Exact code duplicate" },
                            "fullDescription": { "text": "Identical code blocks, differing only in whitespace and comments" },
                            "defaultConfiguration": { "level": "warning" }
                        },
                        {
                            "id": "polydup/type-2",
                            "name": "RenamedDuplicate",
                            "shortDescription": { "text": "Renamed code duplicate" },
                            "fullDescription": { "text": "Structurally identical code with renamed identifiers or literals" },
                            "defaultConfiguration": { "level": "warning" }
                        },
                        {
                            "id": "polydup/type-3",
                            "name": "ModifiedDuplicate",
                            "shortDescription": { "text": "Modified code duplicate" },
                            "fullDescription": { "text": "Similar code with small additions, deletions, or modifications" },
                            "defaultConfiguration": { "level": "note" }
                        }
                    ]
                }
            },
            "results": results
        }]
    });

    serde_json::to_string_pretty(&sarif)
        .map_err(|e| anyhow::anyhow!("SARIF serialization error: {}", e))
}

/// Map clone type to SARIF level and kind
fn map_clone_to_sarif(dup: &dupe_core::DuplicateMatch) -> (&'static str, &'static str) {
    match dup.clone_type {
        dupe_core::CloneType::Type1 => ("warning", "type-1"),
        dupe_core::CloneType::Type2 => ("warning", "type-2"),
        dupe_core::CloneType::Type3 => ("note", "type-3"),
    }
}

/// Format VS Code problem matcher output
/// Format: file:line:column: severity: message
pub fn format_vscode_report(report: &dupe_core::Report) -> Result<String> {
    let mut output = String::new();

    for dup in &report.duplicates {
        let severity = map_clone_to_severity(&dup.clone_type);
        let end_line1 = dup.end_line1.unwrap_or(dup.start_line1);

        output.push_str(&format!(
            "{}:{}:1: {}: {} duplicate found - matches {}:{}-{} ({:.0}% similar, {} tokens)\n",
            dup.file1,
            dup.start_line1,
            severity,
            match dup.clone_type {
                dupe_core::CloneType::Type1 => "Type-1 (exact)",
                dupe_core::CloneType::Type2 => "Type-2 (renamed)",
                dupe_core::CloneType::Type3 => "Type-3 (modified)",
            },
            dup.file2,
            dup.start_line2,
            dup.end_line2.unwrap_or(dup.start_line2),
            dup.similarity * 100.0,
            dup.length
        ));

        // Also report the second location
        output.push_str(&format!(
            "{}:{}:1: {}: duplicate of {}:{}-{}\n",
            dup.file2, dup.start_line2, severity, dup.file1, dup.start_line1, end_line1
        ));
    }

    if report.duplicates.is_empty() {
        output.push_str("No duplicates found.\n");
    }

    Ok(output)
}

/// Format compiler-style output (like rustc/gcc warnings)
pub fn format_compiler_report(report: &dupe_core::Report) -> Result<String> {
    let mut output = String::new();

    for dup in &report.duplicates {
        let severity = map_clone_to_severity(&dup.clone_type);
        let end_line1 = dup.end_line1.unwrap_or(dup.start_line1);

        // rustc-style: severity[code]: message
        output.push_str(&format!(
            "{}[polydup]: {} duplicate detected\n",
            severity,
            match dup.clone_type {
                dupe_core::CloneType::Type1 => "Type-1 (exact)",
                dupe_core::CloneType::Type2 => "Type-2 (renamed)",
                dupe_core::CloneType::Type3 => "Type-3 (modified)",
            }
        ));

        // Arrow pointing to file location
        output.push_str(&format!(
            "  --> {}:{}-{}\n",
            dup.file1, dup.start_line1, end_line1
        ));

        // Note about duplicate
        output.push_str(&format!(
            "   = note: duplicates {}:{}-{}\n",
            dup.file2,
            dup.start_line2,
            dup.end_line2.unwrap_or(dup.start_line2)
        ));

        // Additional info
        output.push_str(&format!(
            "   = info: {:.0}% similar, {} tokens\n\n",
            dup.similarity * 100.0,
            dup.length
        ));
    }

    if report.duplicates.is_empty() {
        output.push_str("No duplicates found.\n");
    } else {
        output.push_str(&format!(
            "Total: {} duplicate(s) found\n",
            report.duplicates.len()
        ));
    }

    Ok(output)
}

/// Map clone type to severity string
fn map_clone_to_severity(clone_type: &dupe_core::CloneType) -> &'static str {
    match clone_type {
        dupe_core::CloneType::Type1 => "warning",
        dupe_core::CloneType::Type2 => "warning",
        dupe_core::CloneType::Type3 => "note",
    }
}
