//! Regression tests for duplicate_id field and ignore ID matching
//!
//! These tests verify that duplicate IDs are computed correctly during scans
//! and that ignore entries using those IDs actually filter duplicates.

#![allow(deprecated)] // Allow Command::cargo_bin until we migrate to cargo::cargo_bin_cmd!

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

#[test]
fn test_duplicate_id_computed_with_ignore_manager() {
    let temp_dir = TempDir::new().unwrap();

    // Create duplicate files
    let duplicate_code = r#"
function processData(input) {
    const data = Array.isArray(input) ? input : [input];
    const filtered = data.filter(item => item != null);
    const mapped = filtered.map(item => {
        if (typeof item === 'string') {
            return item.toUpperCase();
        }
        return item;
    });
    return mapped;
}
"#;

    fs::write(temp_dir.path().join("file1.js"), duplicate_code).unwrap();
    fs::write(temp_dir.path().join("file2.js"), duplicate_code).unwrap();

    // Initialize to create .polydup-ignore file (empty)
    let mut cmd = Command::cargo_bin("polydup").unwrap();
    cmd.current_dir(temp_dir.path()).arg("init").arg("-y");
    cmd.assert().success();

    // Scan with JSON output to get duplicate_id
    // The duplicate_id should be computed even though there are no ignore entries yet
    let mut cmd = Command::cargo_bin("polydup").unwrap();
    cmd.current_dir(temp_dir.path())
        .arg("scan")
        .arg(".")
        .arg("--format")
        .arg("json");

    let output = cmd.output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    // Parse JSON to extract duplicate_id
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let duplicates = json["duplicates"].as_array().unwrap();

    assert!(!duplicates.is_empty(), "Should find at least one duplicate");

    let duplicate_id = duplicates[0]["duplicate_id"].as_str();
    assert!(
        duplicate_id.is_some(),
        "duplicate_id should be present in scan output"
    );
    let duplicate_id = duplicate_id.unwrap();

    assert!(
        duplicate_id.starts_with("sha256:"),
        "duplicate_id should be a SHA256 hash"
    );
    assert!(
        duplicate_id.len() > 10,
        "duplicate_id should be a full hash"
    );
}

#[test]
fn test_ignore_with_duplicate_id_filters_correctly() {
    let temp_dir = TempDir::new().unwrap();

    // Create duplicate files
    let duplicate_code = r#"
function processData(input) {
    const data = Array.isArray(input) ? input : [input];
    const filtered = data.filter(item => item != null);
    const mapped = filtered.map(item => {
        if (typeof item === 'string') {
            return item.toUpperCase();
        }
        return item;
    });
    return mapped;
}
"#;

    fs::write(temp_dir.path().join("file1.js"), duplicate_code).unwrap();
    fs::write(temp_dir.path().join("file2.js"), duplicate_code).unwrap();

    // Initialize
    let mut cmd = Command::cargo_bin("polydup").unwrap();
    cmd.current_dir(temp_dir.path()).arg("init").arg("-y");
    cmd.assert().success();

    // First scan - get the duplicate_id
    let mut cmd = Command::cargo_bin("polydup").unwrap();
    cmd.current_dir(temp_dir.path())
        .arg("scan")
        .arg(".")
        .arg("--format")
        .arg("json");

    let output = cmd.output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let duplicate_id = json["duplicates"][0]["duplicate_id"]
        .as_str()
        .unwrap()
        .to_string();

    // Add ignore using the exact duplicate_id from scan
    let mut cmd = Command::cargo_bin("polydup").unwrap();
    cmd.current_dir(temp_dir.path())
        .arg("ignore")
        .arg("add")
        .arg(&duplicate_id)
        .arg("--files")
        .arg("file1.js:1-10")
        .arg("--reason")
        .arg("Regression test - should filter duplicate");

    cmd.assert().success();

    // Second scan - should NOT find the duplicate (it's filtered)
    let mut cmd = Command::cargo_bin("polydup").unwrap();
    cmd.current_dir(temp_dir.path())
        .arg("scan")
        .arg(".")
        .arg("--verbose");

    let output = cmd.output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();

    // Should show ignore rules were loaded
    assert!(
        stderr.contains("Ignore rules:") || stderr.contains("duplicate(s) ignored"),
        "Should indicate ignore rules were loaded"
    );

    // Should find 0 duplicates
    assert!(
        stdout.contains("Duplicates found:                                            0")
            || stdout.contains("No duplicates found"),
        "Duplicate should be filtered by ignore entry"
    );

    // Exit code should be 0 (no duplicates found)
    assert!(output.status.success(), "Should exit with code 0");
}

#[test]
fn test_duplicate_id_consistency_across_scans() {
    let temp_dir = TempDir::new().unwrap();

    // Create duplicate files
    let duplicate_code = r#"
function processData(input) {
    const data = Array.isArray(input) ? input : [input];
    const filtered = data.filter(item => item != null);
    const mapped = filtered.map(item => {
        if (typeof item === 'string') {
            return item.toUpperCase();
        }
        return item;
    });
    return mapped;
}
"#;

    fs::write(temp_dir.path().join("file1.js"), duplicate_code).unwrap();
    fs::write(temp_dir.path().join("file2.js"), duplicate_code).unwrap();

    // Initialize
    let mut cmd = Command::cargo_bin("polydup").unwrap();
    cmd.current_dir(temp_dir.path()).arg("init").arg("-y");
    cmd.assert().success();

    // First scan
    let mut cmd = Command::cargo_bin("polydup").unwrap();
    cmd.current_dir(temp_dir.path())
        .arg("scan")
        .arg(".")
        .arg("--format")
        .arg("json");

    let output1 = cmd.output().unwrap();
    let stdout1 = String::from_utf8(output1.stdout).unwrap();
    let json1: serde_json::Value = serde_json::from_str(&stdout1).unwrap();
    let id1 = json1["duplicates"][0]["duplicate_id"].as_str().unwrap();

    // Second scan (should produce same ID)
    let mut cmd = Command::cargo_bin("polydup").unwrap();
    cmd.current_dir(temp_dir.path())
        .arg("scan")
        .arg(".")
        .arg("--format")
        .arg("json");

    let output2 = cmd.output().unwrap();
    let stdout2 = String::from_utf8(output2.stdout).unwrap();
    let json2: serde_json::Value = serde_json::from_str(&stdout2).unwrap();
    let id2 = json2["duplicates"][0]["duplicate_id"].as_str().unwrap();

    assert_eq!(
        id1, id2,
        "duplicate_id should be consistent across multiple scans"
    );
}

#[test]
fn test_duplicate_id_matches_ignore_computation() {
    let temp_dir = TempDir::new().unwrap();

    // Create duplicate files
    let duplicate_code = r#"
function processData(input) {
    const data = Array.isArray(input) ? input : [input];
    const filtered = data.filter(item => item != null);
    const mapped = filtered.map(item => {
        if (typeof item === 'string') {
            return item.toUpperCase();
        }
        return item;
    });
    return mapped;
}
"#;

    fs::write(temp_dir.path().join("file1.js"), duplicate_code).unwrap();
    fs::write(temp_dir.path().join("file2.js"), duplicate_code).unwrap();

    // Initialize
    let mut cmd = Command::cargo_bin("polydup").unwrap();
    cmd.current_dir(temp_dir.path()).arg("init").arg("-y");
    cmd.assert().success();

    // Get duplicate_id from scan
    let mut cmd = Command::cargo_bin("polydup").unwrap();
    cmd.current_dir(temp_dir.path())
        .arg("scan")
        .arg(".")
        .arg("--format")
        .arg("json");

    let output = cmd.output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let scan_id = json["duplicates"][0]["duplicate_id"].as_str().unwrap();

    // Add ignore with that ID
    let mut cmd = Command::cargo_bin("polydup").unwrap();
    cmd.current_dir(temp_dir.path())
        .arg("ignore")
        .arg("add")
        .arg(scan_id)
        .arg("--files")
        .arg("file1.js:1-10")
        .arg("--reason")
        .arg("Test");

    cmd.assert().success();

    // Read the ignore file to verify the ID was stored correctly
    let ignore_content = fs::read_to_string(temp_dir.path().join(".polydup-ignore")).unwrap();

    assert!(
        ignore_content.contains(scan_id),
        "Ignore file should contain the exact duplicate_id from scan"
    );

    // Scan again - should be filtered
    let mut cmd = Command::cargo_bin("polydup").unwrap();
    cmd.current_dir(temp_dir.path()).arg("scan").arg(".");

    cmd.assert().success().stdout(predicate::str::contains(
        "Duplicates found:                                            0",
    ));
}
