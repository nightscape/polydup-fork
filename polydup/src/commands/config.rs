//! Configuration management commands
//!
//! Provides commands to validate, view, and manage PolyDup configuration files.

use anyhow::{Context, Result};
use std::path::PathBuf;

use crate::cli::ConfigCommands;
use crate::config::Config;

/// Run the config command
pub fn run(command: ConfigCommands) -> Result<()> {
    match command {
        ConfigCommands::Validate { config_path } => {
            validate_config(config_path)?;
        }
        ConfigCommands::Path => {
            show_config_path()?;
        }
        ConfigCommands::Show { config_path } => {
            show_config(config_path)?;
        }
    }

    Ok(())
}

/// Validate configuration file
fn validate_config(config_path: Option<PathBuf>) -> Result<()> {
    let config = load_config(config_path)?;

    // Validate configuration
    match config.validate() {
        Ok(warnings) => {
            println!("✓ Configuration is valid");
            println!();
            println!("{}", config.summary());

            if !warnings.is_empty() {
                println!();
                println!("Warnings:");
                for warning in warnings {
                    println!("  ⚠ {}", warning);
                }
            }
        }
        Err(e) => {
            eprintln!("✗ Configuration is invalid");
            eprintln!();
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }

    Ok(())
}

/// Show configuration file path
fn show_config_path() -> Result<()> {
    match Config::config_path() {
        Some(path) => {
            println!("{}", path.display());
        }
        None => {
            eprintln!("No configuration file found.");
            eprintln!();
            eprintln!("Run 'polydup init' to create one.");
            std::process::exit(1);
        }
    }

    Ok(())
}

/// Show configuration summary
fn show_config(config_path: Option<PathBuf>) -> Result<()> {
    let config = load_config(config_path)?;

    println!("{}", config.summary());

    Ok(())
}

/// Load configuration from file or search for it
fn load_config(config_path: Option<PathBuf>) -> Result<Config> {
    let config = if let Some(path) = config_path {
        Config::load_from_file(&path)
            .with_context(|| format!("Failed to load config from {}", path.display()))?
    } else {
        match Config::load()? {
            Some(config) => config,
            None => {
                eprintln!("No configuration file found.");
                eprintln!();
                eprintln!("Run 'polydup init' to create one.");
                std::process::exit(1);
            }
        }
    };

    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_validate_valid_config() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let config_path = temp_dir.path().join(".polyduprc.toml");

        let config = Config::default();
        config.save(&config_path)?;

        let loaded = Config::load_from_file(&config_path)?;
        let result = loaded.validate();

        assert!(result.is_ok());

        Ok(())
    }

    #[test]
    fn test_validate_invalid_similarity() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let config_path = temp_dir.path().join(".polyduprc.toml");

        let toml_content = r#"
[scan]
min_block_size = 50
similarity_threshold = 1.5
"#;

        fs::write(&config_path, toml_content)?;

        let config = Config::load_from_file(&config_path)?;
        let result = config.validate();

        assert!(result.is_err());

        Ok(())
    }

    #[test]
    fn test_validate_invalid_format() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let config_path = temp_dir.path().join(".polyduprc.toml");

        let toml_content = r#"
[output]
format = "xml"
"#;

        fs::write(&config_path, toml_content)?;

        let config = Config::load_from_file(&config_path)?;
        let result = config.validate();

        assert!(result.is_err());

        Ok(())
    }
}
