//! Integration tests for cache command config file support

use anyhow::Result;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use tempfile::TempDir;

/// Get the path to the polydup binary
fn polydup_bin() -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.pop(); // up to crates/
    path.pop(); // up to workspace root
    path.push("target");
    path.push("debug");
    path.push("polydup");
    path
}

/// Create a test JavaScript file with duplicate code
fn create_test_files(dir: &TempDir) -> Result<()> {
    let file1 = dir.path().join("test1.js");
    fs::write(
        &file1,
        r#"
function calculateSum(a, b) {
    const result = a + b;
    return result;
}

function processData(input) {
    const normalized = input.trim();
    const parsed = JSON.parse(normalized);
    return parsed;
}
"#,
    )?;

    let file2 = dir.path().join("test2.js");
    fs::write(
        &file2,
        r#"
function calculateTotal(x, y) {
    const result = x + y;
    return result;
}

function handleInput(data) {
    const cleaned = data.trim();
    const obj = JSON.parse(cleaned);
    return obj;
}
"#,
    )?;

    Ok(())
}

#[test]
fn test_cache_build_uses_config_defaults() -> Result<()> {
    let temp_dir = TempDir::new()?;
    create_test_files(&temp_dir)?;

    // Create config file with custom thresholds
    let config_path = temp_dir.path().join(".polyduprc.toml");
    fs::write(
        &config_path,
        r#"
[scan]
min_block_size = 15
similarity_threshold = 0.90

[output]
format = "text"
verbose = false
"#,
    )?;

    let cache_path = temp_dir.path().join("test-cache.json");

    // Build cache (should use config values)
    let output = Command::new(polydup_bin())
        .current_dir(temp_dir.path())
        .args(["cache", "build", ".", "-o"])
        .arg(&cache_path)
        .arg("--verbose")
        .output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "Cache build failed:\nstdout: {}\nstderr: {}",
        stdout,
        stderr
    );

    // Verify verbose output shows correct thresholds
    assert!(
        stderr.contains("Threshold: 15 tokens"),
        "Expected threshold 15 from config, stderr: {}",
        stderr
    );
    assert!(
        stderr.contains("Similarity: 90.0%"),
        "Expected similarity 90.0% from config, stderr: {}",
        stderr
    );

    // Verify cache file was created
    assert!(cache_path.exists(), "Cache file should be created");

    // Verify cache info shows correct threshold
    let info_output = Command::new(polydup_bin())
        .current_dir(temp_dir.path())
        .args(["cache", "info", "--cache"])
        .arg(&cache_path)
        .output()?;

    let info_stdout = String::from_utf8_lossy(&info_output.stdout);
    assert!(
        info_stdout.contains("Min tokens:       15"),
        "Cache info should show min_block_size from config, stdout: {}",
        info_stdout
    );

    Ok(())
}

#[test]
fn test_cache_build_without_config_uses_defaults() -> Result<()> {
    let temp_dir = TempDir::new()?;
    create_test_files(&temp_dir)?;

    let cache_path = temp_dir.path().join("test-cache.json");

    // Build cache without config file (should use hardcoded defaults)
    let output = Command::new(polydup_bin())
        .current_dir(temp_dir.path())
        .args(["cache", "build", ".", "-o"])
        .arg(&cache_path)
        .arg("--verbose")
        .output()?;

    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "Cache build failed: {}", stderr);

    // Verify defaults: 50 tokens, 85% similarity
    assert!(
        stderr.contains("Threshold: 50 tokens"),
        "Expected default threshold 50, stderr: {}",
        stderr
    );
    assert!(
        stderr.contains("Similarity: 85.0%"),
        "Expected default similarity 85.0%, stderr: {}",
        stderr
    );

    // Verify cache info shows default threshold
    let info_output = Command::new(polydup_bin())
        .current_dir(temp_dir.path())
        .args(["cache", "info", "--cache"])
        .arg(&cache_path)
        .output()?;

    let info_stdout = String::from_utf8_lossy(&info_output.stdout);
    assert!(
        info_stdout.contains("Min tokens:       50"),
        "Cache info should show default min_block_size, stdout: {}",
        info_stdout
    );

    Ok(())
}

#[test]
fn test_cache_build_with_low_threshold() -> Result<()> {
    let temp_dir = TempDir::new()?;
    create_test_files(&temp_dir)?;

    // Create config with very low threshold (should detect more duplicates)
    let config_path = temp_dir.path().join(".polyduprc.toml");
    fs::write(
        &config_path,
        r#"
[scan]
min_block_size = 3
similarity_threshold = 0.80
"#,
    )?;

    let cache_path = temp_dir.path().join("test-cache.json");

    // Build cache
    let output = Command::new(polydup_bin())
        .current_dir(temp_dir.path())
        .args(["cache", "build", ".", "-o"])
        .arg(&cache_path)
        .output()?;

    assert!(output.status.success(), "Cache build should succeed");

    // Load and verify cache has entries (low threshold should find hashes)
    let cache_content = fs::read_to_string(&cache_path)?;
    let cache: serde_json::Value = serde_json::from_str(&cache_content)?;

    assert_eq!(
        cache["min_block_size"].as_u64(),
        Some(3),
        "Cache should store correct min_block_size"
    );

    let hash_count = cache["hash_index"]
        .as_object()
        .map(|obj| obj.len())
        .unwrap_or(0);
    assert!(
        hash_count > 0,
        "Cache should contain hashes with low threshold"
    );

    Ok(())
}

#[test]
fn test_cache_respects_config_in_parent_directory() -> Result<()> {
    let temp_dir = TempDir::new()?;

    // Create config in parent directory
    let config_path = temp_dir.path().join(".polyduprc.toml");
    fs::write(
        &config_path,
        r#"
[scan]
min_block_size = 20
similarity_threshold = 0.88
"#,
    )?;

    // Create subdirectory with test files
    let subdir = temp_dir.path().join("src");
    fs::create_dir(&subdir)?;

    let file = subdir.join("test.js");
    fs::write(
        &file,
        r#"
function example() {
    console.log("test");
    return true;
}
"#,
    )?;

    let cache_path = subdir.join("cache.json");

    // Build cache from subdirectory (should find parent config)
    let output = Command::new(polydup_bin())
        .current_dir(&subdir)
        .args(["cache", "build", ".", "-o"])
        .arg(&cache_path)
        .arg("--verbose")
        .output()?;

    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "Cache build from subdir failed: {}",
        stderr
    );

    // Verify it found and used parent config
    assert!(
        stderr.contains("Threshold: 20 tokens"),
        "Should find config in parent directory, stderr: {}",
        stderr
    );
    assert!(
        stderr.contains("Similarity: 88.0%"),
        "Should use parent config similarity, stderr: {}",
        stderr
    );

    Ok(())
}

#[test]
fn test_cache_clear_command() -> Result<()> {
    let temp_dir = TempDir::new()?;
    create_test_files(&temp_dir)?;

    let cache_path = temp_dir.path().join("test-cache.json");

    // Build cache first
    let build_output = Command::new(polydup_bin())
        .current_dir(temp_dir.path())
        .args(["cache", "build", ".", "-o"])
        .arg(&cache_path)
        .output()?;

    assert!(build_output.status.success());
    assert!(cache_path.exists());

    // Clear cache
    let clear_output = Command::new(polydup_bin())
        .current_dir(temp_dir.path())
        .args(["cache", "clear", "--cache"])
        .arg(&cache_path)
        .output()?;

    let clear_stdout = String::from_utf8_lossy(&clear_output.stdout);

    assert!(
        clear_output.status.success(),
        "Cache clear failed: {}",
        clear_stdout
    );

    // Verify cache was deleted
    assert!(!cache_path.exists(), "Cache file should be deleted");

    Ok(())
}

#[test]
fn test_cache_info_shows_metadata() -> Result<()> {
    let temp_dir = TempDir::new()?;
    create_test_files(&temp_dir)?;

    let config_path = temp_dir.path().join(".polyduprc.toml");
    fs::write(
        &config_path,
        r#"
[scan]
min_block_size = 30
similarity_threshold = 0.92
"#,
    )?;

    let cache_path = temp_dir.path().join("test-cache.json");

    // Build cache
    Command::new(polydup_bin())
        .current_dir(temp_dir.path())
        .args(["cache", "build", ".", "-o"])
        .arg(&cache_path)
        .output()?;

    // Get cache info
    let info_output = Command::new(polydup_bin())
        .current_dir(temp_dir.path())
        .args(["cache", "info", "--cache"])
        .arg(&cache_path)
        .output()?;

    let info_stdout = String::from_utf8_lossy(&info_output.stdout);

    assert!(info_output.status.success());

    // Verify all expected info is present
    assert!(info_stdout.contains("Cache Information"));
    assert!(info_stdout.contains("Configuration:"));
    assert!(info_stdout.contains("Min tokens:       30"));
    assert!(info_stdout.contains("Statistics:"));
    assert!(info_stdout.contains("Files cached:"));
    assert!(info_stdout.contains("Unique hashes:"));
    assert!(info_stdout.contains("Created at:"));

    Ok(())
}
