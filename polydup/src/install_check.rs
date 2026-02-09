//! Installation detection and version conflict checking
//!
//! This module detects multiple polydup installations across different
//! package managers and warns about version conflicts.

use anyhow::Result;
use colored::*;
use std::process::Command;

/// Represents a detected polydup installation
#[derive(Debug)]
pub struct Installation {
    pub source: InstallSource,
    pub version: Option<String>,
    pub path: Option<String>,
    pub is_active: bool,
}

/// The source/method of installation
#[derive(Debug, Clone, PartialEq)]
pub enum InstallSource {
    Cargo,
    Pip,
    Npm,
}

impl std::fmt::Display for InstallSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InstallSource::Cargo => write!(f, "cargo"),
            InstallSource::Pip => write!(f, "pip"),
            InstallSource::Npm => write!(f, "npm"),
        }
    }
}

/// Check for all polydup installations across package managers
pub fn detect_installations() -> Vec<Installation> {
    let mut installations = Vec::new();

    // Check for cargo installation (current binary)
    if let Some(install) = detect_cargo_installation() {
        installations.push(install);
    }

    // Check for pip installation
    if let Some(install) = detect_pip_installation() {
        installations.push(install);
    }

    // Check for npm installation
    if let Some(install) = detect_npm_installation() {
        installations.push(install);
    }

    installations
}

/// Detect cargo-installed polydup (the current binary)
fn detect_cargo_installation() -> Option<Installation> {
    // The current binary is the cargo installation
    let version = env!("CARGO_PKG_VERSION").to_string();

    // Try to find the binary path
    let path = std::env::current_exe()
        .ok()
        .map(|p| p.to_string_lossy().to_string());

    Some(Installation {
        source: InstallSource::Cargo,
        version: Some(version),
        path,
        is_active: true, // Cargo is always active since we're running from it
    })
}

/// Detect pip-installed polydup package
fn detect_pip_installation() -> Option<Installation> {
    // Try pip3 first, then pip
    let output = Command::new("pip3")
        .args(["show", "polydup"])
        .output()
        .or_else(|_| Command::new("pip").args(["show", "polydup"]).output())
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Parse version from pip show output
    let version = stdout
        .lines()
        .find(|line| line.starts_with("Version:"))
        .map(|line| line.trim_start_matches("Version:").trim().to_string());

    // Parse location from pip show output
    let path = stdout
        .lines()
        .find(|line| line.starts_with("Location:"))
        .map(|line| line.trim_start_matches("Location:").trim().to_string());

    Some(Installation {
        source: InstallSource::Pip,
        version,
        path,
        is_active: false, // pip package is a library, not CLI
    })
}

/// Detect npm-installed polydup package
fn detect_npm_installation() -> Option<Installation> {
    // Check global npm installation with JSON output and depth=0 to get direct dependencies only
    let output = Command::new("npm")
        .args(["list", "-g", "polydup", "--json", "--depth=0"])
        .output()
        .ok()?;

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Try to parse JSON response using serde_json
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&stdout) {
        // npm list --json returns: { "dependencies": { "polydup": { "version": "0.8.0" } } }
        if let Some(version) = json
            .get("dependencies")
            .and_then(|deps| deps.get("polydup"))
            .and_then(|pkg| pkg.get("version"))
            .and_then(|v| v.as_str())
        {
            return Some(Installation {
                source: InstallSource::Npm,
                version: Some(version.to_string()),
                path: None,
                is_active: false, // npm package is a library, not CLI
            });
        }
    }

    // Fallback: try non-JSON mode for older npm versions or if JSON parsing failed
    let output2 = Command::new("npm")
        .args(["list", "-g", "polydup", "--depth=0"])
        .output()
        .ok()?;

    if !output2.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output2.stdout);
    if !stdout.contains("polydup@") {
        return None;
    }

    // Parse version from npm list output (format: polydup@0.8.0)
    let version = stdout
        .lines()
        .find(|line| line.contains("polydup@"))
        .and_then(|line| {
            // Find the version after the @ symbol
            line.split_whitespace()
                .find(|part| part.contains("polydup@"))
                .and_then(|pkg| pkg.split('@').next_back())
                .map(|v| v.trim().to_string())
        });

    Some(Installation {
        source: InstallSource::Npm,
        version,
        path: None,
        is_active: false, // npm package is a library, not CLI
    })
}

/// Display installation check results
pub fn display_installations(installations: &[Installation], verbose: bool) -> Result<()> {
    let cli_version = env!("CARGO_PKG_VERSION");

    println!("{}", "PolyDup Installation Check".bright_cyan().bold());
    println!("{}", "═".repeat(40).bright_black());

    // Show current CLI version prominently
    println!(
        "\n{} {} {}",
        "CLI Version:".bright_white(),
        cli_version.green().bold(),
        "(active)".dimmed()
    );

    if installations.len() <= 1 {
        println!("\n{}", "No additional installations detected.".dimmed());
        return Ok(());
    }

    println!("\n{}", "Detected Installations:".bright_white());

    for install in installations {
        let version_str = install.version.as_deref().unwrap_or("unknown");

        let status = if install.is_active {
            "(active)".green()
        } else {
            "(library only)".dimmed()
        };

        let source_icon = match install.source {
            InstallSource::Cargo => "📦",
            InstallSource::Pip => "🐍",
            InstallSource::Npm => "📦",
        };

        println!(
            "  {} {}: {} {}",
            source_icon,
            install.source.to_string().bright_yellow(),
            version_str.bold(),
            status
        );

        if verbose {
            if let Some(ref path) = install.path {
                println!("      {}", path.dimmed());
            }
        }
    }

    // Check for version mismatches
    let versions: Vec<_> = installations
        .iter()
        .filter_map(|i| i.version.as_ref())
        .collect();

    let all_same = versions.windows(2).all(|w| w[0] == w[1]);

    if !all_same && versions.len() > 1 {
        println!();
        println!(
            "{} {}",
            "⚠️".yellow(),
            "Version mismatch detected!".yellow().bold()
        );
        println!(
            "   {}",
            "Different versions may cause unexpected behavior.".dimmed()
        );
        println!(
            "   {} cargo install polydup --force",
            "Update CLI:".bright_white()
        );
    }

    // Show helpful note about library packages
    let has_library_packages = installations
        .iter()
        .any(|i| matches!(i.source, InstallSource::Pip | InstallSource::Npm));

    if has_library_packages {
        println!();
        println!("{}", "Note:".bright_white());
        println!(
            "  {}",
            "pip and npm packages are library bindings for programmatic use.".dimmed()
        );
        println!("  {}", "For CLI usage, use: cargo install polydup".dimmed());
    }

    Ok(())
}

/// Run installation check command
pub fn run() -> Result<()> {
    let installations = detect_installations();
    display_installations(&installations, true)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_npm_json_output() {
        // Simulate npm list --json output
        let json_output = r#"{
            "dependencies": {
                "polydup": {
                    "version": "0.8.1"
                }
            }
        }"#;

        let json: serde_json::Value = serde_json::from_str(json_output).unwrap();
        let version = json
            .get("dependencies")
            .and_then(|deps| deps.get("polydup"))
            .and_then(|pkg| pkg.get("version"))
            .and_then(|v| v.as_str());

        assert_eq!(version, Some("0.8.1"));
    }

    #[test]
    fn test_parse_npm_json_missing_package() {
        let json_output = r#"{
            "dependencies": {
                "other-package": {
                    "version": "1.0.0"
                }
            }
        }"#;

        let json: serde_json::Value = serde_json::from_str(json_output).unwrap();
        let version = json
            .get("dependencies")
            .and_then(|deps| deps.get("polydup"))
            .and_then(|pkg| pkg.get("version"))
            .and_then(|v| v.as_str());

        assert_eq!(version, None);
    }

    #[test]
    fn test_parse_npm_text_output() {
        let text_output = "
/usr/local/lib
├── polydup@0.8.1
└── some-other-package@1.0.0
";

        let version = text_output
            .lines()
            .find(|line| line.contains("polydup@"))
            .and_then(|line| {
                line.split_whitespace()
                    .find(|part| part.contains("polydup@"))
                    .and_then(|pkg| pkg.split('@').next_back())
                    .map(|v| v.trim().to_string())
            });

        assert_eq!(version, Some("0.8.1".to_string()));
    }

    #[test]
    fn test_install_source_display() {
        assert_eq!(InstallSource::Cargo.to_string(), "cargo");
        assert_eq!(InstallSource::Pip.to_string(), "pip");
        assert_eq!(InstallSource::Npm.to_string(), "npm");
    }
}
