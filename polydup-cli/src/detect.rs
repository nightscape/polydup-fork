//! Environment detection for automatic configuration
//!
//! This module detects the programming environment(s) in use by checking for
//! marker files (package.json, Cargo.toml, etc.) and provides environment-specific
//! defaults for exclude patterns and configuration.

use anyhow::Result;
use std::path::Path;

/// Detected programming environment
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Environment {
    /// Node.js / JavaScript / TypeScript project
    NodeJs,
    /// Rust project
    Rust,
    /// Python project
    Python,
    /// Go project
    Go,
    /// Java project
    Java,
    /// Ruby project
    Ruby,
}

impl Environment {
    /// Get display name for the environment
    pub fn name(&self) -> &'static str {
        match self {
            Environment::NodeJs => "Node.js",
            Environment::Rust => "Rust",
            Environment::Python => "Python",
            Environment::Go => "Go",
            Environment::Java => "Java",
            Environment::Ruby => "Ruby",
        }
    }

    /// Get environment-specific exclude patterns
    pub fn exclude_patterns(&self) -> Vec<String> {
        match self {
            Environment::NodeJs => vec![
                "**/node_modules/**".to_string(),
                "**/dist/**".to_string(),
                "**/build/**".to_string(),
                "**/*.test.js".to_string(),
                "**/*.test.ts".to_string(),
                "**/*.spec.js".to_string(),
                "**/*.spec.ts".to_string(),
                "**/__tests__/**".to_string(),
                "**/*.min.js".to_string(),
            ],
            Environment::Rust => vec![
                "**/target/**".to_string(),
                "**/*_test.rs".to_string(),
                "**/tests/**".to_string(),
            ],
            Environment::Python => vec![
                "**/__pycache__/**".to_string(),
                "**/*.pyc".to_string(),
                "**/.venv/**".to_string(),
                "**/venv/**".to_string(),
                "**/.pytest_cache/**".to_string(),
                "**/test_*.py".to_string(),
                "**/*_test.py".to_string(),
            ],
            Environment::Go => vec!["**/vendor/**".to_string(), "**/*_test.go".to_string()],
            Environment::Java => vec![
                "**/target/**".to_string(),
                "**/build/**".to_string(),
                "**/.gradle/**".to_string(),
                "**/bin/**".to_string(),
                "**/*Test.java".to_string(),
            ],
            Environment::Ruby => vec![
                "**/vendor/**".to_string(),
                "**/.bundle/**".to_string(),
                "**/*_spec.rb".to_string(),
                "**/spec/**".to_string(),
            ],
        }
    }

    /// Get recommended install command for this environment
    pub fn install_command(&self) -> &'static str {
        match self {
            Environment::NodeJs => "npm install -g @polydup/core",
            Environment::Rust => "cargo install polydup-cli",
            Environment::Python => "pip install polydup",
            Environment::Go => "cargo install polydup-cli",
            Environment::Java => "cargo install polydup-cli",
            Environment::Ruby => "cargo install polydup-cli",
        }
    }

    /// Get alternative install methods for this environment
    pub fn alt_install_methods(&self) -> Vec<&'static str> {
        match self {
            Environment::NodeJs => vec![
                "Download binary from GitHub releases",
                "cargo install polydup-cli",
            ],
            Environment::Rust => vec!["Download binary from GitHub releases"],
            Environment::Python => vec![
                "cargo install polydup-cli",
                "Download binary from GitHub releases",
            ],
            _ => vec!["Download binary from GitHub releases"],
        }
    }
}

/// Detect programming environments in the given directory
pub fn detect_environments<P: AsRef<Path>>(dir: P) -> Result<Vec<Environment>> {
    let dir = dir.as_ref();
    let mut environments = Vec::new();

    // Check for Node.js
    if dir.join("package.json").exists()
        || dir.join("yarn.lock").exists()
        || dir.join("pnpm-lock.yaml").exists()
    {
        environments.push(Environment::NodeJs);
    }

    // Check for Rust
    if dir.join("Cargo.toml").exists() {
        environments.push(Environment::Rust);
    }

    // Check for Python
    if dir.join("pyproject.toml").exists()
        || dir.join("setup.py").exists()
        || dir.join("requirements.txt").exists()
        || dir.join("Pipfile").exists()
    {
        environments.push(Environment::Python);
    }

    // Check for Go
    if dir.join("go.mod").exists() {
        environments.push(Environment::Go);
    }

    // Check for Java
    if dir.join("pom.xml").exists()
        || dir.join("build.gradle").exists()
        || dir.join("build.gradle.kts").exists()
    {
        environments.push(Environment::Java);
    }

    // Check for Ruby
    if dir.join("Gemfile").exists() {
        environments.push(Environment::Ruby);
    }

    Ok(environments)
}

/// Generate combined exclude patterns from multiple environments
pub fn combined_exclude_patterns(environments: &[Environment]) -> Vec<String> {
    let mut patterns: Vec<String> = environments
        .iter()
        .flat_map(|env| env.exclude_patterns())
        .collect();

    // Remove duplicates while preserving order
    patterns.sort();
    patterns.dedup();

    patterns
}

/// Get a recommended install command based on detected environments
pub fn recommended_install_command(environments: &[Environment]) -> &'static str {
    if environments.is_empty() {
        // Universal install method
        return "cargo install polydup-cli";
    }

    // Prioritize Rust if detected (native install)
    if environments.contains(&Environment::Rust) {
        return "cargo install polydup-cli";
    }

    // Otherwise use the first detected environment's method
    environments[0].install_command()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_detect_nodejs() -> Result<()> {
        let temp_dir = TempDir::new()?;
        fs::write(temp_dir.path().join("package.json"), "{}")?;

        let envs = detect_environments(temp_dir.path())?;
        assert!(envs.contains(&Environment::NodeJs));

        Ok(())
    }

    #[test]
    fn test_detect_rust() -> Result<()> {
        let temp_dir = TempDir::new()?;
        fs::write(temp_dir.path().join("Cargo.toml"), "[package]")?;

        let envs = detect_environments(temp_dir.path())?;
        assert!(envs.contains(&Environment::Rust));

        Ok(())
    }

    #[test]
    fn test_detect_python() -> Result<()> {
        let temp_dir = TempDir::new()?;
        fs::write(temp_dir.path().join("pyproject.toml"), "[tool]")?;

        let envs = detect_environments(temp_dir.path())?;
        assert!(envs.contains(&Environment::Python));

        Ok(())
    }

    #[test]
    fn test_detect_monorepo() -> Result<()> {
        let temp_dir = TempDir::new()?;
        fs::write(temp_dir.path().join("package.json"), "{}")?;
        fs::write(temp_dir.path().join("Cargo.toml"), "[package]")?;
        fs::write(temp_dir.path().join("requirements.txt"), "")?;

        let envs = detect_environments(temp_dir.path())?;
        assert_eq!(envs.len(), 3);
        assert!(envs.contains(&Environment::NodeJs));
        assert!(envs.contains(&Environment::Rust));
        assert!(envs.contains(&Environment::Python));

        Ok(())
    }

    #[test]
    fn test_combined_exclude_patterns() {
        let envs = vec![Environment::NodeJs, Environment::Rust];
        let patterns = combined_exclude_patterns(&envs);

        // Should contain patterns from both environments
        assert!(patterns.contains(&"**/node_modules/**".to_string()));
        assert!(patterns.contains(&"**/target/**".to_string()));

        // Should not have duplicates
        let unique_count = patterns.len();
        let mut sorted = patterns.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(unique_count, sorted.len());
    }

    #[test]
    fn test_environment_names() {
        assert_eq!(Environment::NodeJs.name(), "Node.js");
        assert_eq!(Environment::Rust.name(), "Rust");
        assert_eq!(Environment::Python.name(), "Python");
    }

    #[test]
    fn test_recommended_install_command() {
        let envs = vec![Environment::NodeJs, Environment::Rust];
        assert_eq!(
            recommended_install_command(&envs),
            "cargo install polydup-cli"
        );

        let envs = vec![Environment::Python];
        assert_eq!(recommended_install_command(&envs), "pip install polydup");

        let envs = vec![];
        assert_eq!(
            recommended_install_command(&envs),
            "cargo install polydup-cli"
        );
    }
}
