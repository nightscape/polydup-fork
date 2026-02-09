//! Configuration file handling for PolyDup
//!
//! This module provides functionality to load, save, and manage PolyDup configuration
//! from `.polyduprc.toml` files. Configuration files are discovered by walking up the
//! directory tree from the current directory, similar to `.gitignore` behavior.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// Configuration structure for PolyDup
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    /// Scan configuration
    #[serde(default)]
    pub scan: ScanConfig,

    /// Output configuration
    #[serde(default)]
    pub output: OutputConfig,

    /// CI/CD configuration
    #[serde(default)]
    pub ci: CiConfig,
}

/// Scan-related configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanConfig {
    /// Minimum block size in tokens (default: 50)
    #[serde(default = "default_min_block_size")]
    pub min_block_size: usize,

    /// Similarity threshold (0.0-1.0, default: 0.85)
    #[serde(default = "default_similarity_threshold")]
    pub similarity_threshold: f64,

    /// Exclude patterns configuration
    #[serde(default)]
    pub exclude: ExcludeConfig,
}

/// Exclude patterns configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExcludeConfig {
    /// Glob patterns to exclude from scanning
    #[serde(default = "default_exclude_patterns")]
    pub patterns: Vec<String>,
}

/// Output-related configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputConfig {
    /// Output format: "text" or "json" (default: "text")
    #[serde(default = "default_output_format")]
    pub format: String,

    /// Verbose output (default: false)
    #[serde(default)]
    pub verbose: bool,
}

/// CI/CD-related configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CiConfig {
    /// Enable CI/CD integration (default: false)
    #[serde(default)]
    pub enabled: bool,

    /// Fail build if duplicates found (default: true when enabled)
    #[serde(default = "default_fail_on_duplicates")]
    pub fail_on_duplicates: bool,
}

// Default value functions
fn default_min_block_size() -> usize {
    50
}

fn default_similarity_threshold() -> f64 {
    0.85
}

fn default_output_format() -> String {
    "text".to_string()
}

fn default_fail_on_duplicates() -> bool {
    true
}

fn default_exclude_patterns() -> Vec<String> {
    vec![
        "**/*.test.ts".to_string(),
        "**/*.test.js".to_string(),
        "**/*.spec.ts".to_string(),
        "**/*.spec.js".to_string(),
        "**/__tests__/**".to_string(),
        "**/*.test.py".to_string(),
        "**/test_*.py".to_string(),
        "**/*_test.rs".to_string(),
        "**/tests/**".to_string(),
        "**/*.min.js".to_string(),
    ]
}

impl Default for ScanConfig {
    fn default() -> Self {
        ScanConfig {
            min_block_size: default_min_block_size(),
            similarity_threshold: default_similarity_threshold(),
            exclude: ExcludeConfig::default(),
        }
    }
}

impl Default for ExcludeConfig {
    fn default() -> Self {
        ExcludeConfig {
            patterns: default_exclude_patterns(),
        }
    }
}

impl Default for OutputConfig {
    fn default() -> Self {
        OutputConfig {
            format: default_output_format(),
            verbose: false,
        }
    }
}

impl Default for CiConfig {
    fn default() -> Self {
        CiConfig {
            enabled: false,
            fail_on_duplicates: default_fail_on_duplicates(),
        }
    }
}

impl Config {
    /// Load configuration from a TOML file
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let contents =
            fs::read_to_string(path.as_ref()).context("Failed to read configuration file")?;

        toml::from_str(&contents).context("Failed to parse TOML configuration")
    }

    /// Load configuration from the current directory or parent directories
    ///
    /// Walks up the directory tree looking for `.polyduprc.toml` files,
    /// similar to how Git looks for `.gitignore` files.
    pub fn load() -> Result<Option<Self>> {
        let current_dir = std::env::current_dir().context("Failed to get current directory")?;

        Self::find_and_load(&current_dir)
    }

    /// Find and load configuration file by walking up the directory tree
    fn find_and_load(start_dir: &Path) -> Result<Option<Self>> {
        let mut current = Some(start_dir);

        while let Some(dir) = current {
            let config_path = dir.join(".polyduprc.toml");

            if config_path.exists() {
                return Ok(Some(Self::load_from_file(config_path)?));
            }

            current = dir.parent();
        }

        Ok(None)
    }

    /// Save configuration to a TOML file
    pub fn save<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let toml_string =
            toml::to_string_pretty(self).context("Failed to serialize configuration")?;

        fs::write(path.as_ref(), toml_string).context("Failed to write configuration file")?;

        Ok(())
    }

    /// Save configuration to `.polyduprc.toml` in the current directory
    pub fn save_default(&self) -> Result<PathBuf> {
        let current_dir = std::env::current_dir().context("Failed to get current directory")?;
        let config_path = current_dir.join(".polyduprc.toml");

        self.save(&config_path)?;

        Ok(config_path)
    }

    /// Check if configuration file exists in current or parent directories
    pub fn exists() -> bool {
        if let Ok(current_dir) = std::env::current_dir() {
            Self::find_config_path(&current_dir).is_some()
        } else {
            false
        }
    }

    /// Find configuration file path without loading it
    fn find_config_path(start_dir: &Path) -> Option<PathBuf> {
        let mut current = Some(start_dir);

        while let Some(dir) = current {
            let config_path = dir.join(".polyduprc.toml");

            if config_path.exists() {
                return Some(config_path);
            }

            current = dir.parent();
        }

        None
    }

    /// Get the path to the configuration file if it exists
    pub fn config_path() -> Option<PathBuf> {
        std::env::current_dir()
            .ok()
            .and_then(|dir| Self::find_config_path(&dir))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::defaults;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.scan.min_block_size, defaults::MIN_BLOCK_SIZE);
        assert_eq!(config.scan.similarity_threshold, defaults::SIMILARITY);
        assert_eq!(config.output.format, "text");
        assert!(!config.output.verbose);
        assert!(!config.ci.enabled);
        assert!(config.ci.fail_on_duplicates);
    }

    #[test]
    fn test_save_and_load() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let config_path = temp_dir.path().join(".polyduprc.toml");

        let config = Config::default();
        config.save(&config_path)?;

        let loaded = Config::load_from_file(&config_path)?;
        assert_eq!(loaded.scan.min_block_size, config.scan.min_block_size);
        assert_eq!(
            loaded.scan.similarity_threshold,
            config.scan.similarity_threshold
        );

        Ok(())
    }

    #[test]
    fn test_find_config_in_parent() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let config_path = temp_dir.path().join(".polyduprc.toml");

        // Create config in parent
        let config = Config::default();
        config.save(&config_path)?;

        // Create subdirectory
        let sub_dir = temp_dir.path().join("subdir");
        fs::create_dir(&sub_dir)?;

        // Should find config in parent
        let found = Config::find_and_load(&sub_dir)?;
        assert!(found.is_some());

        Ok(())
    }

    #[test]
    fn test_parse_custom_toml() -> Result<()> {
        let toml_content = r#"
[scan]
min_block_size = 100
similarity_threshold = 0.95

[scan.exclude]
patterns = ["**/node_modules/**", "**/target/**"]

[output]
format = "json"
verbose = true

[ci]
enabled = true
fail_on_duplicates = false
"#;

        let temp_dir = TempDir::new()?;
        let config_path = temp_dir.path().join(".polyduprc.toml");

        let mut file = fs::File::create(&config_path)?;
        file.write_all(toml_content.as_bytes())?;

        let config = Config::load_from_file(&config_path)?;
        assert_eq!(config.scan.min_block_size, 100);
        assert_eq!(config.scan.similarity_threshold, 0.95);
        assert_eq!(config.output.format, "json");
        assert!(config.output.verbose);
        assert!(config.ci.enabled);
        assert!(!config.ci.fail_on_duplicates);
        assert_eq!(config.scan.exclude.patterns.len(), 2);

        Ok(())
    }
}
