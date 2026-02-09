//! Tests for .polydup-ignore error handling
//!
//! Verifies that malformed or unsupported ignore files cause the scan to fail
//! with a clear error message rather than silently proceeding without ignore rules.

#![allow(deprecated)]

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

#[test]
fn test_scan_fails_on_malformed_ignore_file() {
    let temp_dir = TempDir::new().unwrap();

    // Create a duplicate JavaScript file
    let duplicate_code = r#"
function processData(input) {
    const data = Array.isArray(input) ? input : [input];
    return data.filter(item => item != null);
}
"#;

    fs::write(temp_dir.path().join("file1.js"), duplicate_code).unwrap();
    fs::write(temp_dir.path().join("file2.js"), duplicate_code).unwrap();

    // Create a malformed .polydup-ignore file
    let malformed_toml = r#"
version = 1
[[ignores]]
id = "test-id"
# Missing required 'files' field - malformed entry
reason = "Test"
added_by = "test"
added_at = 2024-01-01T00:00:00Z
"#;

    fs::write(temp_dir.path().join(".polydup-ignore"), malformed_toml).unwrap();

    // Run scan - should fail with clear error message
    let mut cmd = Command::cargo_bin("polydup").unwrap();
    cmd.current_dir(temp_dir.path()).arg("scan").arg(".");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains(
            "Failed to load .polydup-ignore file",
        ))
        .stderr(predicate::str::contains(
            "Fix the file or remove it to continue",
        ));
}

#[test]
fn test_scan_fails_on_unsupported_version() {
    let temp_dir = TempDir::new().unwrap();

    // Create a duplicate JavaScript file
    let duplicate_code = r#"
function processData(input) {
    const data = Array.isArray(input) ? input : [input];
    return data.filter(item => item != null);
}
"#;

    fs::write(temp_dir.path().join("file1.js"), duplicate_code).unwrap();
    fs::write(temp_dir.path().join("file2.js"), duplicate_code).unwrap();

    // Create a .polydup-ignore file with unsupported version
    let future_version_toml = r#"
version = 999
ignores = []
"#;

    fs::write(temp_dir.path().join(".polydup-ignore"), future_version_toml).unwrap();

    // Run scan - should fail with version error
    let mut cmd = Command::cargo_bin("polydup").unwrap();
    cmd.current_dir(temp_dir.path()).arg("scan").arg(".");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains(
            "Unsupported .polydup-ignore version",
        ))
        .stderr(predicate::str::contains("999"));
}

#[test]
fn test_scan_succeeds_when_ignore_file_missing() {
    let temp_dir = TempDir::new().unwrap();

    // Create test files
    fs::write(temp_dir.path().join("file1.js"), "console.log('hello');").unwrap();

    // No .polydup-ignore file - should succeed (not crash or fail)
    let mut cmd = Command::cargo_bin("polydup").unwrap();
    cmd.current_dir(temp_dir.path()).arg("scan").arg(".");

    // Should succeed - missing ignore file is not an error
    cmd.assert().success();
}

#[test]
fn test_scan_succeeds_with_valid_ignore_file() {
    let temp_dir = TempDir::new().unwrap();

    // Create test files
    fs::write(temp_dir.path().join("file1.js"), "console.log('hello');").unwrap();

    // Create a valid .polydup-ignore file
    let valid_toml = r#"
version = 1

[[ignores]]
id = "sha256:abc123"
files = [{file = "file1.js", start_line = 1, end_line = 10}]
reason = "Test ignore"
added_by = "test"
added_at = "2024-01-01T00:00:00Z"
"#;

    fs::write(temp_dir.path().join(".polydup-ignore"), valid_toml).unwrap();

    // Run scan - should succeed and load ignore file without errors
    let mut cmd = Command::cargo_bin("polydup").unwrap();
    cmd.current_dir(temp_dir.path())
        .arg("scan")
        .arg(".")
        .arg("--verbose");

    // Should succeed - valid ignore file should load
    cmd.assert().success();
}

#[test]
fn test_ignore_add_fails_on_malformed_existing_file() {
    let temp_dir = TempDir::new().unwrap();

    // Create a malformed .polydup-ignore file
    let malformed_toml = "version = 1\n[[ignores]\nid = ";

    fs::write(temp_dir.path().join(".polydup-ignore"), malformed_toml).unwrap();

    // Try to add an ignore entry - should fail
    let mut cmd = Command::cargo_bin("polydup").unwrap();
    cmd.current_dir(temp_dir.path())
        .arg("ignore")
        .arg("add")
        .arg("test-id")
        .arg("--files")
        .arg("test.js:1-10")
        .arg("--reason")
        .arg("Test");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains(
            "Failed to load .polydup-ignore file",
        ))
        .stderr(predicate::str::contains(
            "Fix the file before managing ignore rules",
        ));
}

#[test]
fn test_ignore_list_fails_on_unsupported_version() {
    let temp_dir = TempDir::new().unwrap();

    // Create a .polydup-ignore file with unsupported version
    let future_version_toml = r#"
version = 999
ignores = []
"#;

    fs::write(temp_dir.path().join(".polydup-ignore"), future_version_toml).unwrap();

    // Try to list ignores - should fail
    let mut cmd = Command::cargo_bin("polydup").unwrap();
    cmd.current_dir(temp_dir.path()).arg("ignore").arg("list");

    cmd.assert().failure().stderr(predicate::str::contains(
        "Unsupported .polydup-ignore version",
    ));
}

#[test]
fn test_ignore_add_succeeds_when_file_missing() {
    let temp_dir = TempDir::new().unwrap();

    // No .polydup-ignore file - should succeed and create it
    let mut cmd = Command::cargo_bin("polydup").unwrap();
    cmd.current_dir(temp_dir.path())
        .arg("ignore")
        .arg("add")
        .arg("test-id")
        .arg("--files")
        .arg("test.js:1-10")
        .arg("--reason")
        .arg("Test");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("✓ Duplicate added to ignore list"));

    // Verify file was created
    assert!(temp_dir.path().join(".polydup-ignore").exists());
}
