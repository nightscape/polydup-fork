//! Upgrade command implementation
//!
//! Detects installation method and upgrades polydup to the latest version.

use anyhow::{Context, Result};
use colored::*;
use std::env;
use std::path::Path;
use std::process::Command;

/// Current version from Cargo.toml
const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Installation method detected
#[derive(Debug, Clone, PartialEq)]
pub enum InstallMethod {
    Cargo,
    Homebrew,
    Unknown,
}

impl std::fmt::Display for InstallMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InstallMethod::Cargo => write!(f, "cargo"),
            InstallMethod::Homebrew => write!(f, "homebrew"),
            InstallMethod::Unknown => write!(f, "unknown"),
        }
    }
}

/// Execute the upgrade command
pub fn run(check_only: bool, force: bool) -> Result<()> {
    println!();
    println!("{}", "Checking for updates...".bright_cyan());

    // Check for latest version on crates.io
    let latest_version = get_latest_version()?;

    println!(
        "  {} {}",
        "Current version:".dimmed(),
        CURRENT_VERSION.bright_white()
    );
    println!(
        "  {} {}",
        "Latest version: ".dimmed(),
        latest_version.bright_white()
    );

    // Compare versions
    let current =
        semver::Version::parse(CURRENT_VERSION).context("Failed to parse current version")?;
    let latest =
        semver::Version::parse(&latest_version).context("Failed to parse latest version")?;

    if current >= latest && !force {
        println!();
        println!(
            "{} {}",
            "OK".green().bold(),
            "Already running the latest version.".green()
        );
        return Ok(());
    }

    if check_only {
        println!();
        println!(
            "{} {} -> {}",
            "Update available:".yellow(),
            CURRENT_VERSION.dimmed(),
            latest_version.bright_green()
        );
        println!(
            "{}",
            "Run 'polydup upgrade' to install the update.".dimmed()
        );
        return Ok(());
    }

    // Detect installation method
    let install_method = detect_installation_method();

    println!();
    println!(
        "{} {} (via {})",
        "Upgrading".bright_cyan(),
        format!("{} -> {}", CURRENT_VERSION, latest_version).bright_white(),
        install_method.to_string().dimmed()
    );
    println!();

    // Perform upgrade
    match install_method {
        InstallMethod::Cargo => upgrade_via_cargo(force)?,
        InstallMethod::Homebrew => upgrade_via_homebrew()?,
        InstallMethod::Unknown => {
            println!(
                "{} {}",
                "Warning:".yellow().bold(),
                "Could not detect installation method.".yellow()
            );
            println!();
            println!("Please upgrade manually using one of:");
            println!("  {} cargo install polydup --force", "cargo:".dimmed());
            println!("  {} brew upgrade polydup", "brew: ".dimmed());
            return Ok(());
        }
    }

    println!();
    println!(
        "{} {}",
        "OK".green().bold(),
        format!("Successfully upgraded to v{}", latest_version).green()
    );

    Ok(())
}

/// Get the latest version from crates.io
fn get_latest_version() -> Result<String> {
    // Use ureq to fetch from crates.io API
    let url = "https://crates.io/api/v1/crates/polydup";

    let response: serde_json::Value = ureq::get(url)
        .header("User-Agent", "polydup-cli")
        .call()
        .context("Failed to connect to crates.io")?
        .into_body()
        .read_json()
        .context("Failed to parse crates.io response")?;

    let version = response["crate"]["max_version"]
        .as_str()
        .context("Could not find version in crates.io response")?
        .to_string();

    Ok(version)
}

/// Detect how polydup was installed
fn detect_installation_method() -> InstallMethod {
    // Get the current executable path
    let current_exe = match env::current_exe() {
        Ok(path) => path,
        Err(_) => return InstallMethod::Unknown,
    };

    // Check if it's in Cargo's bin directory
    if is_cargo_install(&current_exe) {
        return InstallMethod::Cargo;
    }

    // Check if it's a Homebrew installation
    if is_homebrew_install(&current_exe) {
        return InstallMethod::Homebrew;
    }

    InstallMethod::Unknown
}

/// Check if the binary is installed via Cargo
fn is_cargo_install(exe_path: &Path) -> bool {
    // Check if path contains .cargo/bin
    if let Some(path_str) = exe_path.to_str() {
        if path_str.contains(".cargo/bin") {
            return true;
        }
    }

    // Also check CARGO_HOME
    if let Ok(cargo_home) = env::var("CARGO_HOME") {
        if exe_path.starts_with(&cargo_home) {
            return true;
        }
    }

    // Check default cargo home
    if let Some(home) = dirs_next::home_dir() {
        let cargo_bin = home.join(".cargo").join("bin");
        if exe_path.starts_with(&cargo_bin) {
            return true;
        }
    }

    false
}

/// Check if the binary is installed via Homebrew
fn is_homebrew_install(exe_path: &Path) -> bool {
    if let Some(path_str) = exe_path.to_str() {
        // Check common Homebrew paths
        if path_str.contains("/homebrew/") || path_str.contains("/Cellar/") {
            return true;
        }
    }

    // Try running brew list to check
    if Command::new("brew")
        .args(["list", "polydup"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        return true;
    }

    false
}

/// Upgrade via cargo install
fn upgrade_via_cargo(force: bool) -> Result<()> {
    let mut args = vec!["install", "polydup"];
    if force {
        args.push("--force");
    }

    println!(
        "{} cargo {}",
        "Running:".dimmed(),
        args.join(" ").bright_white()
    );
    println!();

    let status = Command::new("cargo")
        .args(&args)
        .status()
        .context("Failed to run cargo install")?;

    if !status.success() {
        anyhow::bail!("cargo install failed with exit code: {:?}", status.code());
    }

    Ok(())
}

/// Upgrade via Homebrew
fn upgrade_via_homebrew() -> Result<()> {
    println!("{} brew upgrade polydup", "Running:".dimmed());
    println!();

    let status = Command::new("brew")
        .args(["upgrade", "polydup"])
        .status()
        .context("Failed to run brew upgrade")?;

    if !status.success() {
        anyhow::bail!("brew upgrade failed with exit code: {:?}", status.code());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_install_method_display() {
        assert_eq!(InstallMethod::Cargo.to_string(), "cargo");
        assert_eq!(InstallMethod::Homebrew.to_string(), "homebrew");
        assert_eq!(InstallMethod::Unknown.to_string(), "unknown");
    }
}
