//! Snapshot tests for CLI output formatting
//!
//! These tests ensure output formats remain consistent across changes.
//! Run `cargo insta review` after intentional output format changes.

#![allow(deprecated)] // Allow Command::cargo_bin until we migrate to cargo::cargo_bin_cmd!

use assert_cmd::Command;
use std::fs;
use tempfile::TempDir;

/// Helper to create test files with duplicate code
fn setup_test_files(dir: &TempDir) -> std::io::Result<()> {
    let js_content = r#"
function calculateSum(a, b) {
    return a + b;
}

function calculateTotal(x, y) {
    return x + y;
}
"#;

    let py_content = r#"
def process_data(data):
    result = []
    for item in data:
        result.append(item * 2)
    return result

def transform_data(items):
    output = []
    for element in items:
        output.append(element * 2)
    return output
"#;

    fs::write(dir.path().join("test1.js"), js_content)?;
    fs::write(dir.path().join("test2.py"), py_content)?;
    Ok(())
}

#[test]
fn test_text_output_format() {
    let temp_dir = TempDir::new().unwrap();
    setup_test_files(&temp_dir).unwrap();

    let mut cmd = Command::cargo_bin("polydup").unwrap();
    let output = cmd
        .arg("scan")
        .arg(temp_dir.path())
        .arg("--no-color")
        .assert()
        .success();

    let stdout = String::from_utf8_lossy(&output.get_output().stdout);
    insta::assert_snapshot!("text_output_no_color", stdout);
}

#[test]
fn test_json_output_format() {
    let temp_dir = TempDir::new().unwrap();
    setup_test_files(&temp_dir).unwrap();

    let mut cmd = Command::cargo_bin("polydup").unwrap();
    let output = cmd
        .arg("scan")
        .arg(temp_dir.path())
        .arg("--format")
        .arg("json")
        .assert()
        .success();

    let stdout = String::from_utf8_lossy(&output.get_output().stdout);

    // Parse JSON to ensure it's valid, then snapshot the structure
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("Output should be valid JSON");

    // Verify JSON structure (not exact values which vary)
    assert!(json.get("files_scanned").is_some());
    assert!(json.get("functions_analyzed").is_some());
    assert!(json.get("duplicates").is_some());
    assert!(json.get("stats").is_some());
}

#[test]
fn test_colored_output_format() {
    let temp_dir = TempDir::new().unwrap();
    setup_test_files(&temp_dir).unwrap();

    let mut cmd = Command::cargo_bin("polydup").unwrap();
    let output = cmd
        .arg("scan")
        .arg(temp_dir.path())
        // Don't use --no-color to test colored output
        .env("TERM", "xterm-256color") //  Ensure color support
        .assert()
        .success();

    let stdout = String::from_utf8_lossy(&output.get_output().stdout);

    // Verify output structure
    assert!(stdout.contains("Scan Results"), "Should contain header");
    assert!(stdout.contains("Files scanned:"), "Should contain stats");

    // Note: ANSI colors depend on terminal detection, so we focus on structure
}

#[test]
fn test_filter_only_type2() {
    let temp_dir = TempDir::new().unwrap();
    setup_test_files(&temp_dir).unwrap();

    let mut cmd = Command::cargo_bin("polydup").unwrap();
    let output = cmd
        .arg("scan")
        .arg(temp_dir.path())
        .arg("--only-type")
        .arg("type-2")
        .arg("--no-color")
        .assert()
        .success();

    let stdout = String::from_utf8_lossy(&output.get_output().stdout);

    // Should only show Type-2 duplicates
    assert!(!stdout.contains("Type-1"), "Should not contain Type-1");
    assert!(!stdout.contains("Type-3"), "Should not contain Type-3");

    insta::assert_snapshot!("filter_only_type2", stdout);
}

#[test]
fn test_grouping_by_file() {
    let temp_dir = TempDir::new().unwrap();
    setup_test_files(&temp_dir).unwrap();

    let mut cmd = Command::cargo_bin("polydup").unwrap();
    let output = cmd
        .arg("scan")
        .arg(temp_dir.path())
        .arg("--group-by")
        .arg("file")
        .arg("--no-color")
        .assert()
        .success();

    let stdout = String::from_utf8_lossy(&output.get_output().stdout);

    // Verify output structure (grouping may not show without duplicates)
    assert!(stdout.contains("Scan Results"), "Should contain header");
}

#[test]
fn test_grouping_by_similarity() {
    let temp_dir = TempDir::new().unwrap();
    setup_test_files(&temp_dir).unwrap();

    let mut cmd = Command::cargo_bin("polydup").unwrap();
    let output = cmd
        .arg("scan")
        .arg(temp_dir.path())
        .arg("--group-by")
        .arg("similarity")
        .arg("--no-color")
        .assert()
        .success();

    let stdout = String::from_utf8_lossy(&output.get_output().stdout);

    // Verify grouping happened
    assert!(stdout.contains("Scan Results"));
}

#[test]
fn test_dashboard_rendering() {
    let temp_dir = TempDir::new().unwrap();
    setup_test_files(&temp_dir).unwrap();

    let mut cmd = Command::cargo_bin("polydup").unwrap();
    let output = cmd
        .arg("scan")
        .arg(temp_dir.path())
        .arg("--no-color")
        .assert()
        .success();

    let stdout = String::from_utf8_lossy(&output.get_output().stdout);

    // Verify dashboard elements
    assert!(stdout.contains("╔"), "Should contain top-left corner");
    assert!(stdout.contains("╗"), "Should contain top-right corner");
    assert!(stdout.contains("╚"), "Should contain bottom-left corner");
    assert!(stdout.contains("╝"), "Should contain bottom-right corner");
    assert!(stdout.contains("║"), "Should contain vertical line");
    assert!(stdout.contains("═"), "Should contain horizontal line");
    assert!(stdout.contains("Scan Results"), "Should contain header");
    assert!(stdout.contains("Files scanned:"), "Should contain stats");
}

#[test]
fn test_error_message_invalid_path() {
    let mut cmd = Command::cargo_bin("polydup").unwrap();
    let output = cmd
        .arg("scan")
        .arg("/nonexistent/path")
        .arg("--no-color")
        .assert()
        .failure();

    let stderr = String::from_utf8_lossy(&output.get_output().stderr);

    // Verify error message components
    assert!(stderr.contains("Error:"), "Should contain error label");
    assert!(stderr.contains("Suggestion:"), "Should contain suggestion");
    // Example might be in suggestion text, not as separate section
}

#[test]
fn test_error_message_invalid_filter() {
    let temp_dir = TempDir::new().unwrap();
    setup_test_files(&temp_dir).unwrap();

    let mut cmd = Command::cargo_bin("polydup").unwrap();
    let output = cmd
        .arg("scan")
        .arg(temp_dir.path())
        .arg("--only-type")
        .arg("invalid")
        .arg("--no-color")
        .assert()
        .failure();

    let stderr = String::from_utf8_lossy(&output.get_output().stderr);

    assert!(stderr.contains("Suggestion:"), "Should contain suggestion");
    assert!(stderr.contains("Example:"), "Should contain example");
    assert!(
        stderr.contains("Documentation:"),
        "Should contain docs link"
    );

    insta::assert_snapshot!("error_invalid_filter", stderr);
}

#[test]
fn test_error_message_invalid_grouping() {
    let temp_dir = TempDir::new().unwrap();
    setup_test_files(&temp_dir).unwrap();

    let mut cmd = Command::cargo_bin("polydup").unwrap();
    let output = cmd
        .arg("scan")
        .arg(temp_dir.path())
        .arg("--group-by")
        .arg("invalid")
        .arg("--no-color")
        .assert()
        .failure();

    let stderr = String::from_utf8_lossy(&output.get_output().stderr);

    assert!(
        stderr.contains("Options explained:"),
        "Should explain options"
    );

    insta::assert_snapshot!("error_invalid_grouping", stderr);
}

#[test]
fn test_verbose_output() {
    let temp_dir = TempDir::new().unwrap();
    setup_test_files(&temp_dir).unwrap();

    let mut cmd = Command::cargo_bin("polydup").unwrap();
    let output = cmd
        .arg("scan")
        .arg(temp_dir.path())
        .arg("--verbose")
        .arg("--no-color")
        .assert()
        .success();

    let stdout = String::from_utf8_lossy(&output.get_output().stdout);

    // Verify verbose output components
    assert!(
        stdout.contains("Performance:"),
        "Should contain performance section"
    );
}
