//! Version update detection and notification
//!
//! Checks crates.io for newer versions and displays color-coded notifications
//! based on semver distance (major, minor, patch).

use colored::*;
use semver::Version;
use update_informer::{registry, Check};

/// Package name on crates.io
const CRATE_NAME: &str = "polydup";

/// Current version from Cargo.toml
const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Check for updates and display notification if available
pub fn check_for_updates() {
    // Skip if NO_COLOR is set or if check is disabled via env var
    if std::env::var("NO_COLOR").is_ok() || std::env::var("POLYDUP_NO_UPDATE_CHECK").is_ok() {
        return;
    }

    // Use update-informer with crates.io registry
    // It caches results for 24 hours by default
    let informer = update_informer::new(registry::Crates, CRATE_NAME, CURRENT_VERSION);

    if let Some(new_version) = informer.check_version().ok().flatten() {
        display_update_notification(&new_version.to_string());
    }
}

/// Display color-coded update notification based on semver distance
fn display_update_notification(new_version_str: &str) {
    let current = match Version::parse(CURRENT_VERSION) {
        Ok(v) => v,
        Err(_) => return,
    };

    let new_version = match Version::parse(new_version_str) {
        Ok(v) => v,
        Err(_) => return,
    };

    // Determine update type based on semver
    let (icon, message, style) = if new_version.major > current.major {
        // Major update - breaking changes possible
        (
            "!!",
            format!(
                "Major update available: {} -> {}",
                CURRENT_VERSION, new_version_str
            ),
            UpdateStyle::Major,
        )
    } else if new_version.minor > current.minor {
        // Minor update - new features
        (
            "->",
            format!(
                "Update available: {} -> {}",
                CURRENT_VERSION, new_version_str
            ),
            UpdateStyle::Minor,
        )
    } else {
        // Patch update - bug fixes
        (
            "->",
            format!(
                "Patch available: {} -> {}",
                CURRENT_VERSION, new_version_str
            ),
            UpdateStyle::Patch,
        )
    };

    eprintln!();
    match style {
        UpdateStyle::Major => {
            eprintln!("{} {}", icon.red().bold(), message.red().bold());
            eprintln!(
                "   {}",
                "Breaking changes may be present. Review changelog before updating.".red()
            );
        }
        UpdateStyle::Minor => {
            eprintln!("{} {}", icon.yellow(), message.yellow());
            eprintln!("   {}", "New features available.".yellow());
        }
        UpdateStyle::Patch => {
            eprintln!("{} {}", icon.dimmed(), message.dimmed());
            eprintln!("   {}", "Bug fixes available.".dimmed());
        }
    }

    eprintln!(
        "   {}",
        "Run 'polydup upgrade' or 'cargo install polydup' to update.".dimmed()
    );
    eprintln!();
}

enum UpdateStyle {
    Major,
    Minor,
    Patch,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_current_version_is_valid_semver() {
        assert!(Version::parse(CURRENT_VERSION).is_ok());
    }
}
