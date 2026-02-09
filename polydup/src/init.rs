//! Interactive initialization wizard for PolyDup
//!
//! This module provides the `polydup init` command which interactively guides users through
//! setting up PolyDup configuration, CI/CD workflows, and pre-commit hooks.

use anyhow::{Context, Result};
use dialoguer::{theme::ColorfulTheme, Confirm, Input, MultiSelect, Select};
use std::fs;
use std::path::Path;

use crate::config::{CiConfig, Config, ExcludeConfig, OutputConfig, ScanConfig};
use crate::detect::{
    combined_exclude_patterns, detect_environments, recommended_install_command, Environment,
};

/// Arguments for the init command
#[derive(Debug, Clone)]
pub struct InitArgs {
    /// Force overwrite existing configuration
    pub force: bool,
    /// Skip interactive prompts and use defaults
    pub non_interactive: bool,
    /// Only generate CI/CD configuration (skip .polyduprc.toml)
    pub ci_only: bool,
}

/// Run the initialization wizard
pub fn cmd_init(args: InitArgs) -> Result<()> {
    println!("PolyDup Initialization Wizard");
    println!("=============================\n");

    // Detect environments
    let current_dir = std::env::current_dir().context("Failed to get current directory")?;

    let detected_envs =
        detect_environments(&current_dir).context("Failed to detect project environments")?;

    if detected_envs.is_empty() {
        println!("No known project markers detected.");
        println!("Generating generic configuration...\n");
    } else {
        println!("Detected environments:");
        for env in &detected_envs {
            println!("  - {}", env.name());
        }
        println!();
    }

    // Handle --ci-only mode: only generate CI/CD configuration
    if args.ci_only {
        return run_ci_only_mode(&detected_envs, args.non_interactive);
    }

    // Check if config already exists
    if Config::exists() && !args.force {
        let config_path = Config::config_path().unwrap();
        eprintln!(
            "Configuration file already exists: {}",
            config_path.display()
        );
        eprintln!("Use --force to overwrite, or edit it manually.");
        std::process::exit(1);
    }

    // Interactive or non-interactive flow
    let config = if args.non_interactive {
        create_default_config(&detected_envs)
    } else {
        create_interactive_config(&detected_envs)?
    };

    // Save configuration
    let config_path = config
        .save_default()
        .context("Failed to save configuration")?;

    println!("\nConfiguration saved to: {}", config_path.display());

    // Optionally create GitHub Actions workflow
    if !args.non_interactive {
        let create_workflow = Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt("Would you like to create a GitHub Actions workflow?")
            .default(true)
            .interact()?;

        if create_workflow {
            create_github_workflow(&detected_envs)?;
        }
    }

    // Show next steps
    print_next_steps(&detected_envs, &config_path);

    Ok(())
}

/// Run CI-only mode: Generate/update CI/CD configuration without touching .polyduprc.toml
fn run_ci_only_mode(environments: &[Environment], non_interactive: bool) -> Result<()> {
    println!("CI-Only Mode: Generating CI/CD configuration\n");

    // If config exists, validate it before generating CI workflows
    if Config::exists() {
        if let Some(config) = Config::load().context("Failed to load existing configuration")? {
            match config.validate() {
                Ok(warnings) => {
                    if warnings.is_empty() {
                        println!("✓ Existing .polyduprc.toml is valid\n");
                    } else {
                        println!("✓ Existing .polyduprc.toml is valid (with warnings):");
                        for warning in warnings {
                            println!("  ⚠ {}", warning);
                        }
                        println!();
                    }
                }
                Err(e) => {
                    eprintln!("⚠ Warning: Existing .polyduprc.toml has validation errors:");
                    eprintln!("  - {}", e);
                    eprintln!(
                        "\nCI workflows will be generated, but may fail with invalid config."
                    );
                    eprintln!("Fix errors with: polydup config validate\n");
                }
            }
        }
    }

    if non_interactive {
        // Non-interactive: Create GitHub Actions workflow with defaults
        create_github_workflow(environments)?;
        println!("\nCI/CD configuration generated.");
        println!("Push your changes to trigger the workflow.");
    } else {
        // Interactive: Let user choose CI platform
        let theme = ColorfulTheme::default();
        let ci_platforms = vec![
            "GitHub Actions",
            "GitLab CI",
            "Azure Pipelines",
            "Jenkins",
            "Other (manual setup)",
        ];

        let selection = Select::with_theme(&theme)
            .with_prompt("Select CI/CD platform")
            .items(&ci_platforms)
            .default(0)
            .interact()?;

        match selection {
            0 => {
                // GitHub Actions - ask if they want to use the action
                let gh_options = vec![
                    "GitHub Action (recommended) - Fast git-diff mode with PR comments",
                    "Manual installation - Full repository scanning",
                ];

                let gh_selection = Select::with_theme(&theme)
                    .with_prompt("Choose GitHub Actions setup")
                    .items(&gh_options)
                    .default(0)
                    .interact()?;

                match gh_selection {
                    0 => {
                        // Use GitHub Action
                        create_github_workflow(environments)?;
                        println!("\n✓ GitHub Actions workflow created.");
                        println!("  Uses: wiesnerbernard/polydup-action@v0.3.0");
                        println!("  Features:");
                        println!("    - Git-diff mode (10-100x faster)");
                        println!("    - Automatic PR comments");
                        println!("    - Configurable fail-on-duplicates");
                        println!("\nPush your changes to trigger the workflow.");
                    }
                    1 => {
                        // Manual installation
                        create_github_workflow_manual(environments)?;
                        println!("\n✓ GitHub Actions workflow created (manual installation).");
                        println!("  Scans: Full repository");
                        println!(
                            "  Note: Consider using the GitHub Action for better performance."
                        );
                        println!("\nPush your changes to trigger the workflow.");
                    }
                    _ => {}
                }
            }
            1 => {
                // GitLab CI
                create_gitlab_ci(environments)?;
                println!("\nGitLab CI configuration created.");
                println!("Push your changes to trigger the pipeline.");
            }
            2 => {
                // Azure Pipelines
                create_azure_pipelines(environments)?;
                println!("\nAzure Pipelines configuration created.");
                println!("Commit and push to trigger the pipeline.");
            }
            3 => {
                // Jenkins
                create_jenkinsfile(environments)?;
                println!("\nJenkinsfile created.");
                println!("Configure your Jenkins job to use this file.");
            }
            4 => {
                // Other
                println!("\nFor manual CI/CD setup, add this to your pipeline:");
                println!("\n  # Install PolyDup");
                println!("  {}", recommended_install_command(environments));
                println!("\n  # Run scan");
                println!("  polydup scan . --format json\n");
            }
            _ => {}
        }
    }

    Ok(())
}

/// Create configuration with default values
fn create_default_config(environments: &[Environment]) -> Config {
    let exclude_patterns = if environments.is_empty() {
        ExcludeConfig::default().patterns
    } else {
        combined_exclude_patterns(environments)
    };

    Config {
        scan: ScanConfig {
            min_block_size: crate::defaults::MIN_BLOCK_SIZE,
            similarity_threshold: crate::defaults::SIMILARITY,
            exclude: ExcludeConfig {
                patterns: exclude_patterns,
            },
        },
        output: OutputConfig::default(),
        ci: CiConfig::default(),
    }
}

/// Create configuration through interactive prompts
fn create_interactive_config(environments: &[Environment]) -> Result<Config> {
    let theme = ColorfulTheme::default();

    // Confirm detected environments
    let selected_envs = if !environments.is_empty() {
        let env_names: Vec<&str> = environments.iter().map(|e| e.name()).collect();
        let defaults: Vec<bool> = vec![true; environments.len()];

        println!("Confirm detected environments:");
        let selections = MultiSelect::with_theme(&theme)
            .items(&env_names)
            .defaults(&defaults)
            .interact()?;

        selections
            .into_iter()
            .map(|i| environments[i])
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };

    // Select similarity threshold
    let threshold_options = vec![
        "Strict (0.95) - Only nearly identical code",
        "Standard (0.85) - Balanced detection (recommended)",
        "Relaxed (0.70) - Catch more similar code",
        "Custom - Enter your own value",
    ];

    let threshold_selection = Select::with_theme(&theme)
        .with_prompt("Select similarity threshold")
        .items(&threshold_options)
        .default(1)
        .interact()?;

    let similarity_threshold = match threshold_selection {
        0 => 0.95,
        1 => 0.85,
        2 => 0.70,
        3 => Input::<f64>::with_theme(&theme)
            .with_prompt("Enter custom threshold (0.0-1.0)")
            .default(0.85)
            .validate_with(|input: &f64| {
                if *input >= 0.0 && *input <= 1.0 {
                    Ok(())
                } else {
                    Err("Threshold must be between 0.0 and 1.0")
                }
            })
            .interact_text()?,
        _ => 0.85,
    };

    // Select minimum block size
    let block_size_options = vec![
        "Small (10 lines) - Catch small duplicates",
        "Medium (50 lines) - Balanced detection (recommended)",
        "Large (100 lines) - Only significant duplicates",
        "Custom - Enter your own value",
    ];

    let block_size_selection = Select::with_theme(&theme)
        .with_prompt("Select minimum block size")
        .items(&block_size_options)
        .default(1)
        .interact()?;

    let min_block_size = match block_size_selection {
        0 => 10,
        1 => 50,
        2 => 100,
        3 => Input::<usize>::with_theme(&theme)
            .with_prompt("Enter custom block size (lines)")
            .default(50)
            .validate_with(|input: &usize| {
                if *input > 0 {
                    Ok(())
                } else {
                    Err("Block size must be greater than 0")
                }
            })
            .interact_text()?,
        _ => 50,
    };

    // Generate exclude patterns
    let exclude_patterns = if selected_envs.is_empty() {
        ExcludeConfig::default().patterns
    } else {
        combined_exclude_patterns(&selected_envs)
    };

    // Ask about custom excludes
    let add_custom_excludes = Confirm::with_theme(&theme)
        .with_prompt("Add custom exclude patterns?")
        .default(false)
        .interact()?;

    let mut final_patterns = exclude_patterns;

    if add_custom_excludes {
        println!("\nEnter exclude patterns (glob format), one per line.");
        println!("Press Enter with empty input to finish.");

        loop {
            let pattern: String = Input::with_theme(&theme)
                .with_prompt("Pattern")
                .allow_empty(true)
                .interact_text()?;

            if pattern.is_empty() {
                break;
            }

            final_patterns.push(pattern);
        }
    }

    // Ask about output format
    let format_options = vec!["text", "json"];
    let format_selection = Select::with_theme(&theme)
        .with_prompt("Default output format")
        .items(&format_options)
        .default(0)
        .interact()?;

    let output_format = format_options[format_selection].to_string();

    // Ask about verbose output
    let verbose = Confirm::with_theme(&theme)
        .with_prompt("Enable verbose output by default?")
        .default(false)
        .interact()?;

    Ok(Config {
        scan: ScanConfig {
            min_block_size,
            similarity_threshold,
            exclude: ExcludeConfig {
                patterns: final_patterns,
            },
        },
        output: OutputConfig {
            format: output_format,
            verbose,
        },
        ci: CiConfig::default(),
    })
}

/// Create GitHub Actions workflow file
fn create_github_workflow(_environments: &[Environment]) -> Result<()> {
    let workflow_dir = Path::new(".github/workflows");
    fs::create_dir_all(workflow_dir).context("Failed to create .github/workflows directory")?;

    let workflow_path = workflow_dir.join("polydup.yml");

    if workflow_path.exists() {
        let overwrite = Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt(format!(
                "{} already exists. Overwrite?",
                workflow_path.display()
            ))
            .default(false)
            .interact()?;

        if !overwrite {
            println!("Skipping GitHub Actions workflow creation.");
            return Ok(());
        }
    }

    let workflow_content = generate_github_workflow();

    fs::write(&workflow_path, workflow_content)
        .context("Failed to write GitHub Actions workflow")?;

    println!(
        "GitHub Actions workflow created: {}",
        workflow_path.display()
    );

    Ok(())
}

/// Create GitHub Actions workflow file with manual installation
fn create_github_workflow_manual(environments: &[Environment]) -> Result<()> {
    let workflow_dir = Path::new(".github/workflows");
    fs::create_dir_all(workflow_dir).context("Failed to create .github/workflows directory")?;

    let workflow_path = workflow_dir.join("polydup.yml");

    if workflow_path.exists() {
        let overwrite = Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt(format!(
                "{} already exists. Overwrite?",
                workflow_path.display()
            ))
            .default(false)
            .interact()?;

        if !overwrite {
            println!("Skipping GitHub Actions workflow creation.");
            return Ok(());
        }
    }

    let install_command = recommended_install_command(environments);
    let workflow_content = generate_github_workflow_manual(install_command);

    fs::write(&workflow_path, workflow_content)
        .context("Failed to write GitHub Actions workflow")?;

    println!(
        "GitHub Actions workflow created: {}",
        workflow_path.display()
    );

    Ok(())
}

/// Generate GitHub Actions workflow content
fn generate_github_workflow() -> String {
    r#"name: PolyDup Duplicate Detection

on:
  pull_request:
    branches: [ main, master, develop ]

permissions:
  contents: read
  pull-requests: write  # Required for PR comments

jobs:
  duplicate-check:
    runs-on: ubuntu-latest
    name: Detect Duplicate Code

    steps:
      - name: Checkout code
        uses: actions/checkout@v4
        with:
          fetch-depth: 0  # Required for git-diff comparison

      - name: Run PolyDup
        uses: wiesnerbernard/polydup-action@v0.3.0
        with:
          threshold: 50
          similarity: 0.85
          fail-on-duplicates: true
          comment-on-pr: true

# This workflow uses the PolyDup GitHub Action for best CI experience:
# - Fast git-diff mode (scans all files, filters to changed)
# - Automatic PR comments with duplicate code reports
# - Configurable fail-on-duplicates behavior
# - No manual installation required
#
# Configuration options:
#   threshold: Minimum code block size in tokens (default: 50)
#   similarity: Similarity threshold 0.0-1.0 (default: 0.85)
#   fail-on-duplicates: Fail check if duplicates found (default: true)
#   comment-on-pr: Post results as PR comment (default: true)
#
# For more options, see: https://github.com/wiesnerbernard/polydup-action
"#
    .to_string()
}

/// Generate GitHub Actions workflow content with manual installation
fn generate_github_workflow_manual(install_command: &str) -> String {
    format!(
        r#"name: PolyDup Duplicate Detection

on:
  pull_request:
    branches: [ main, master, develop ]

jobs:
  duplicate-check:
    runs-on: ubuntu-latest
    name: Detect Duplicate Code

    steps:
      - name: Checkout code
        uses: actions/checkout@v4

      - name: Install PolyDup
        run: {}

      - name: Run duplicate detection
        id: polydup
        run: |
          polydup scan . --format json > polydup-report.json
          DUPLICATES=$(jq '.duplicates | length' polydup-report.json)
          echo "duplicates=$DUPLICATES" >> $GITHUB_OUTPUT

          if [ "$DUPLICATES" -gt 0 ]; then
            echo "::error::Found $DUPLICATES duplicate code block(s)"
            exit 1
          fi

      - name: Upload report
        if: always()
        uses: actions/upload-artifact@v4
        with:
          name: polydup-report
          path: polydup-report.json

# This workflow uses manual installation and scans the full repository.
# For better performance, consider using the GitHub Action:
#   uses: wiesnerbernard/polydup-action@v0.3.0
# which provides:
#   - 10-100x faster git-diff mode
#   - Automatic PR comments
#   - Better CI integration
"#,
        install_command
    )
}

/// Create GitLab CI configuration file
fn create_gitlab_ci(environments: &[Environment]) -> Result<()> {
    let config_path = Path::new(".gitlab-ci.yml");

    if config_path.exists() {
        let overwrite = Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt(format!(
                "{} already exists. Overwrite?",
                config_path.display()
            ))
            .default(false)
            .interact()?;

        if !overwrite {
            println!("Skipping GitLab CI configuration creation.");
            return Ok(());
        }
    }

    let install_command = recommended_install_command(environments);
    let config_content = format!(
        r#"# PolyDup Duplicate Detection
stages:
  - test

polydup:
  stage: test
  image: rust:latest
  script:
    - {}
    - polydup scan . --format json > polydup-report.json || true
  artifacts:
    when: always
    paths:
      - polydup-report.json
    expire_in: 30 days
  allow_failure: true
"#,
        install_command
    );

    fs::write(config_path, config_content).context("Failed to write GitLab CI configuration")?;

    println!("GitLab CI configuration created: {}", config_path.display());

    Ok(())
}

/// Create Azure Pipelines configuration file
fn create_azure_pipelines(environments: &[Environment]) -> Result<()> {
    let config_path = Path::new("azure-pipelines.yml");

    if config_path.exists() {
        let overwrite = Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt(format!(
                "{} already exists. Overwrite?",
                config_path.display()
            ))
            .default(false)
            .interact()?;

        if !overwrite {
            println!("Skipping Azure Pipelines configuration creation.");
            return Ok(());
        }
    }

    let install_command = recommended_install_command(environments);
    let config_content = format!(
        r#"# PolyDup Duplicate Detection
trigger:
  - main
  - master
  - develop

pool:
  vmImage: 'ubuntu-latest'

steps:
  - script: {}
    displayName: 'Install PolyDup'

  - script: |
      polydup scan . --format json > polydup-report.json || true
    displayName: 'Run duplicate detection'

  - task: PublishBuildArtifacts@1
    condition: always()
    inputs:
      PathtoPublish: 'polydup-report.json'
      ArtifactName: 'polydup-report'
"#,
        install_command
    );

    fs::write(config_path, config_content)
        .context("Failed to write Azure Pipelines configuration")?;

    println!(
        "Azure Pipelines configuration created: {}",
        config_path.display()
    );

    Ok(())
}

/// Create Jenkinsfile
fn create_jenkinsfile(environments: &[Environment]) -> Result<()> {
    let config_path = Path::new("Jenkinsfile");

    if config_path.exists() {
        let overwrite = Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt(format!(
                "{} already exists. Overwrite?",
                config_path.display()
            ))
            .default(false)
            .interact()?;

        if !overwrite {
            println!("Skipping Jenkinsfile creation.");
            return Ok(());
        }
    }

    let install_command = recommended_install_command(environments);
    let config_content = format!(
        r#"// PolyDup Duplicate Detection
pipeline {{
    agent any

    stages {{
        stage('Install PolyDup') {{
            steps {{
                sh '{}'
            }}
        }}

        stage('Duplicate Detection') {{
            steps {{
                sh 'polydup scan . --format json > polydup-report.json || true'
            }}
        }}
    }}

    post {{
        always {{
            archiveArtifacts artifacts: 'polydup-report.json', allowEmptyArchive: true
        }}
    }}
}}
"#,
        install_command
    );

    fs::write(config_path, config_content).context("Failed to write Jenkinsfile")?;

    println!("Jenkinsfile created: {}", config_path.display());

    Ok(())
}

/// Print next steps for the user
fn print_next_steps(environments: &[Environment], config_path: &Path) {
    println!("\n{}", "=".repeat(60));
    println!("Next Steps");
    println!("{}", "=".repeat(60));

    println!("\n1. Install PolyDup locally (recommended):");

    if environments.is_empty() {
        println!("   cargo install polydup-cli");
    } else {
        let install_cmd = recommended_install_command(environments);
        println!("   {}", install_cmd);

        if environments.len() > 1 {
            println!("\n   Alternative install methods:");
            for env in environments {
                for alt in env.alt_install_methods() {
                    println!("   - {}", alt);
                }
            }
        }
    }

    println!("\n2. Try scanning your project:");
    println!("   polydup scan ./src");

    println!("\n3. Configuration file location:");
    println!("   {}", config_path.display());
    println!("   Edit this file to customize settings.");

    println!("\n4. CI/CD Integration:");
    if Path::new(".github/workflows/polydup.yml").exists() {
        println!("   ✓ GitHub Actions workflow created.");
        println!("   Push your changes to trigger the workflow.");
        println!();
        println!("   Note: CI is disabled by default in .polyduprc.toml");
        println!("   To enable CI features (exit codes, fail on duplicates), set:");
        println!("   [ci]");
        println!("   enabled = true");
    } else {
        println!("   No CI workflow created yet.");
        println!("   To add CI/CD integration:");
        println!("   - Run: polydup init --ci-only");
        println!("   - Or manually set up using documentation");
        println!();
        println!("   Note: CI features are disabled by default in .polyduprc.toml");
        println!("   To enable, edit your config and set: [ci] enabled = true");
    }

    println!("\n5. Documentation:");
    println!("   Visit https://github.com/wiesnerbernard/polydup for more info.");

    println!("\n{}", "=".repeat(60));
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_create_default_config() {
        let envs = vec![Environment::NodeJs, Environment::Rust];
        let config = create_default_config(&envs);

        assert_eq!(config.scan.min_block_size, crate::defaults::MIN_BLOCK_SIZE);
        assert_eq!(
            config.scan.similarity_threshold,
            crate::defaults::SIMILARITY
        );
        assert!(!config.scan.exclude.patterns.is_empty());
    }

    #[test]
    fn test_generate_github_workflow() {
        let workflow = generate_github_workflow();
        assert!(workflow.contains("name: PolyDup Duplicate Detection"));
        assert!(workflow.contains("wiesnerbernard/polydup-action@v0.3.0"));
        assert!(workflow.contains("fail-on-duplicates: true"));
        assert!(workflow.contains("comment-on-pr: true"));
    }

    #[test]
    fn test_generate_github_workflow_manual() {
        let workflow = generate_github_workflow_manual("cargo install polydup");
        assert!(workflow.contains("name: PolyDup Duplicate Detection"));
        assert!(workflow.contains("cargo install polydup"));
        assert!(workflow.contains("polydup scan"));
        assert!(workflow.contains("jq '.duplicates | length'"));
    }

    #[test]
    #[ignore = "requires terminal for dialoguer prompts"]
    fn test_create_github_workflow() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let original_dir = std::env::current_dir()?;
        std::env::set_current_dir(temp_dir.path())?;

        let envs = vec![Environment::Rust];
        create_github_workflow(&envs)?;

        let workflow_path = temp_dir.path().join(".github/workflows/polydup.yml");
        assert!(workflow_path.exists());

        let content = fs::read_to_string(workflow_path)?;
        assert!(content.contains("PolyDup"));

        std::env::set_current_dir(original_dir)?;
        Ok(())
    }

    #[ignore = "requires terminal for dialoguer prompts"]
    #[test]
    fn test_ci_only_does_not_create_config() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let original_dir = std::env::current_dir()?;
        std::env::set_current_dir(temp_dir.path())?;

        let args = InitArgs {
            force: false,
            non_interactive: true,
            ci_only: true,
        };

        cmd_init(args)?;

        // Verify .polyduprc.toml NOT created
        let config_exists = temp_dir.path().join(".polyduprc.toml").exists();
        assert!(
            !config_exists,
            ".polyduprc.toml should not be created in ci-only mode"
        );

        // Verify workflow created
        let workflow_path = temp_dir.path().join(".github/workflows/polydup.yml");
        assert!(
            workflow_path.exists(),
            "GitHub Actions workflow should be created"
        );

        std::env::set_current_dir(original_dir)?;
        Ok(())
    }

    #[test]
    fn test_ci_only_noninteractive() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let original_dir = std::env::current_dir()?;
        std::env::set_current_dir(temp_dir.path())?;

        let envs = vec![Environment::NodeJs];
        run_ci_only_mode(&envs, true)?;

        // Verify .polyduprc.toml NOT created
        let config_exists = temp_dir.path().join(".polyduprc.toml").exists();
        assert!(!config_exists, ".polyduprc.toml should not be created");

        // Verify GitHub Actions workflow created (default for non-interactive)
        let workflow_path = temp_dir.path().join(".github/workflows/polydup.yml");
        assert!(
            workflow_path.exists(),
            "GitHub Actions workflow should be created"
        );

        std::env::set_current_dir(original_dir)?;
        Ok(())
    }

    // Note: These tests may fail in non-interactive environments (CI/pre-commit hooks)
    // due to dialoguer prompts. They work fine when run locally.

    #[test]
    #[ignore = "requires terminal for dialoguer prompts"]
    fn test_ci_only_gitlab() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let original_dir = std::env::current_dir()?;
        std::env::set_current_dir(temp_dir.path())?;

        let envs = vec![Environment::Python];
        create_gitlab_ci(&envs)?;

        let gitlab_path = temp_dir.path().join(".gitlab-ci.yml");
        assert!(gitlab_path.exists(), "GitLab CI file should exist");

        let content = fs::read_to_string(gitlab_path)?;
        assert!(content.contains("PolyDup"));
        assert!(content.contains("stages:"));
        assert!(content.contains("polydup scan"));

        std::env::set_current_dir(original_dir)?;
        Ok(())
    }

    #[test]
    #[ignore = "requires terminal for dialoguer prompts"]
    fn test_ci_only_azure() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let original_dir = std::env::current_dir()?;
        std::env::set_current_dir(temp_dir.path())?;

        let envs = vec![Environment::Rust];
        create_azure_pipelines(&envs)?;

        let azure_path = temp_dir.path().join("azure-pipelines.yml");
        assert!(azure_path.exists(), "Azure Pipelines file should exist");

        let content = fs::read_to_string(azure_path)?;
        assert!(content.contains("PolyDup"));
        assert!(content.contains("vmImage"));
        assert!(content.contains("polydup scan"));

        std::env::set_current_dir(original_dir)?;
        Ok(())
    }

    #[ignore = "requires terminal for dialoguer prompts"]
    #[test]
    fn test_ci_only_jenkins() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let original_dir = std::env::current_dir()?;
        std::env::set_current_dir(temp_dir.path())?;

        let envs = vec![Environment::NodeJs];
        create_jenkinsfile(&envs)?;

        let jenkins_path = temp_dir.path().join("Jenkinsfile");
        assert!(jenkins_path.exists(), "Jenkinsfile should exist");

        let content = fs::read_to_string(jenkins_path)?;
        assert!(content.contains("PolyDup"));
        assert!(content.contains("pipeline"));
        assert!(content.contains("polydup scan"));

        std::env::set_current_dir(original_dir)?;
        Ok(())
    }
}
