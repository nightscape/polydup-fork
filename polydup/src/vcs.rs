//! Version control system integration
//!
//! This module handles Git operations for differential scanning and
//! repository metadata queries.

use anyhow::{Context, Result};
use colored::*;
use std::path::PathBuf;
use std::process::Command;

/// Find the root directory of the git repository (from current working directory)
pub fn find_git_repo_root() -> Option<PathBuf> {
    find_git_repo_root_for_path(None)
}

/// Find the root directory of the git repository containing the given path
///
/// If `path` is None, uses the current working directory.
/// If `path` is a file, uses its parent directory.
pub fn find_git_repo_root_for_path(path: Option<&std::path::Path>) -> Option<PathBuf> {
    let mut cmd = Command::new("git");

    if let Some(p) = path {
        // Use the directory containing the path (or the path itself if it's a directory)
        let dir = if p.is_file() {
            p.parent().map(|d| d.to_path_buf())
        } else {
            Some(p.to_path_buf())
        };

        if let Some(d) = dir {
            cmd.arg("-C").arg(d);
        }
    }

    cmd.args(["rev-parse", "--show-toplevel"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| {
            let root = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if root.is_empty() {
                None
            } else {
                Some(PathBuf::from(root))
            }
        })
}

/// Determine the project root directory for ignore file operations
///
/// Priority:
/// 1. Git repo root of the first path (if in a git repo)
/// 2. First path itself (if it's a directory)
/// 3. Parent of first path (if it's a file)
/// 4. Current working directory (fallback)
///
/// This is used to ensure consistent location for .polydup-ignore files
/// across scan and ignore commands.
pub fn determine_project_root(paths: Option<&[PathBuf]>) -> Result<PathBuf> {
    // Try to use the first path as context
    if let Some(paths) = paths {
        if let Some(first_path) = paths.first() {
            // Try to find git repo root from this path
            if let Some(repo_root) = find_git_repo_root_for_path(Some(first_path)) {
                return Ok(repo_root);
            }

            // Not in a git repo - use the path itself (or its parent if it's a file)
            if first_path.is_dir() {
                return Ok(first_path.clone());
            } else if let Some(parent) = first_path.parent() {
                return Ok(parent.to_path_buf());
            }
        }
    }

    // Try git repo root from current directory
    if let Some(repo_root) = find_git_repo_root() {
        return Ok(repo_root);
    }

    // Fallback to current directory
    std::env::current_dir().context("Failed to get current directory")
}

/// Get list of files changed in git diff range
pub fn get_git_changed_files(diff_range: &str, verbose: bool, debug: bool) -> Result<Vec<PathBuf>> {
    // Get git repo root to resolve relative paths
    let repo_root = find_git_repo_root()
        .context("Failed to find git repository root. Is this a git repository?")?;

    // Run git diff --name-only to get changed files
    let output = Command::new("git")
        .args(["diff", "--name-only", "--diff-filter=ACMR", diff_range])
        .output()
        .context(
            "Failed to execute git diff command. Is git installed and is this a git repository?",
        )?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let mut error_msg = format!("Git diff command failed: {}", stderr.trim());
        error_msg.push_str("\n\n");
        error_msg.push_str(&format!("{}", "Suggestion:".bright_yellow().bold()));
        error_msg.push_str(" Verify the git diff range is valid\n");
        error_msg.push_str(&format!("{}", "           ".dimmed()));
        error_msg.push_str("Example: origin/main..HEAD or main..feature-branch\n");
        error_msg.push_str(&format!("{}", "           ".dimmed()));
        error_msg.push_str("Run 'git log --oneline' to see available commits\n");

        if debug {
            error_msg.push_str(&format!("\n{}", "Debug Info:".bright_cyan().bold()));
            error_msg.push_str(&format!(
                "\n  Command: git diff --name-only --diff-filter=ACMR {}",
                diff_range
            ));
            error_msg.push_str(&format!(
                "\n  Exit code: {}",
                output.status.code().unwrap_or(-1)
            ));
            error_msg.push_str(&format!("\n  Stderr: {}", stderr));
        }

        anyhow::bail!(error_msg);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let git_relative_files: Vec<&str> = stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();

    if debug && !git_relative_files.is_empty() {
        eprintln!(
            "{}",
            format!("  Git repo root: {}", repo_root.display()).dimmed()
        );
        eprintln!(
            "{}",
            format!("  Raw git diff files: {:?}", git_relative_files).dimmed()
        );
    }

    // Convert git-relative paths to absolute paths and filter out non-existent files
    let mut changed_files: Vec<PathBuf> = git_relative_files
        .into_iter()
        .map(|relative| repo_root.join(relative))
        .filter(|path| path.exists()) // Filter out deleted files
        .collect();

    // Remove duplicates
    changed_files.sort();
    changed_files.dedup();

    if verbose && !changed_files.is_empty() {
        eprintln!(
            "  {} file(s) changed, scanning for duplicates...",
            changed_files.len()
        );
    }

    if debug && changed_files.is_empty() && !stdout.is_empty() {
        eprintln!(
            "{}",
            "  Warning: Git reported changed files but none exist on disk. They may have been deleted.".yellow()
        );
    }

    Ok(changed_files)
}
