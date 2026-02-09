//! Tests verifying duplicate_id is always computed, even without .polydup-ignore
//!
//! This ensures users can obtain IDs from scan output to create their first
//! ignore entry using `polydup ignore add <id>`.

#![allow(deprecated)]

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

#[test]
fn test_scan_json_includes_duplicate_id_without_ignore_file() {
    let temp_dir = TempDir::new().unwrap();

    // Create duplicate JavaScript files
    let duplicate_code = r#"
function processData(input) {
    const data = Array.isArray(input) ? input : [input];
    const filtered = data.filter(item => item != null);
    const mapped = filtered.map(x => x * 2);
    const result = mapped.reduce((a, b) => a + b, 0);
    return result;
}
"#;

    fs::write(temp_dir.path().join("file1.js"), duplicate_code).unwrap();
    fs::write(temp_dir.path().join("file2.js"), duplicate_code).unwrap();

    // Run scan with JSON output - NO .polydup-ignore file exists
    let mut cmd = Command::cargo_bin("polydup").unwrap();
    cmd.current_dir(temp_dir.path())
        .arg("scan")
        .arg(".")
        .arg("--format")
        .arg("json");

    let output = cmd.output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    // Parse JSON output
    let json: serde_json::Value = serde_json::from_str(&stdout).expect("Should output valid JSON");

    // Verify duplicates array exists and has entries
    let duplicates = json["duplicates"]
        .as_array()
        .expect("Should have duplicates array");

    assert!(!duplicates.is_empty(), "Should find duplicates");

    // Verify EVERY duplicate has a duplicate_id field
    for (i, dup) in duplicates.iter().enumerate() {
        let duplicate_id = dup["duplicate_id"]
            .as_str()
            .unwrap_or_else(|| panic!("Duplicate {} should have duplicate_id field", i));

        assert!(
            duplicate_id.starts_with("sha256:"),
            "Duplicate {} ID should be SHA256 hash, got: {}",
            i,
            duplicate_id
        );
    }
}

#[test]
fn test_duplicate_id_consistent_across_runs() {
    let temp_dir = TempDir::new().unwrap();

    // Create duplicate JavaScript files
    let duplicate_code = r#"
function processData(input) {
    const data = Array.isArray(input) ? input : [input];
    const filtered = data.filter(item => item != null);
    const mapped = filtered.map(x => x * 2);
    const result = mapped.reduce((a, b) => a + b, 0);
    return result;
}
"#;

    fs::write(temp_dir.path().join("file1.js"), duplicate_code).unwrap();
    fs::write(temp_dir.path().join("file2.js"), duplicate_code).unwrap();

    // First scan
    let output1 = Command::cargo_bin("polydup")
        .unwrap()
        .current_dir(temp_dir.path())
        .arg("scan")
        .arg(".")
        .arg("--format")
        .arg("json")
        .output()
        .unwrap();

    let json1: serde_json::Value =
        serde_json::from_str(&String::from_utf8(output1.stdout).unwrap()).unwrap();

    // Second scan (should produce identical IDs)
    let output2 = Command::cargo_bin("polydup")
        .unwrap()
        .current_dir(temp_dir.path())
        .arg("scan")
        .arg(".")
        .arg("--format")
        .arg("json")
        .output()
        .unwrap();

    let json2: serde_json::Value =
        serde_json::from_str(&String::from_utf8(output2.stdout).unwrap()).unwrap();

    // Compare duplicate IDs
    let id1 = json1["duplicates"][0]["duplicate_id"]
        .as_str()
        .expect("First scan should have duplicate_id");

    let id2 = json2["duplicates"][0]["duplicate_id"]
        .as_str()
        .expect("Second scan should have duplicate_id");

    assert_eq!(id1, id2, "Duplicate IDs should be consistent across scans");
}

#[test]
fn test_can_create_ignore_entry_from_scan_output() {
    let temp_dir = TempDir::new().unwrap();

    // Create duplicate JavaScript files
    let duplicate_code = r#"
function processData(input) {
    const data = Array.isArray(input) ? input : [input];
    const filtered = data.filter(item => item != null);
    const mapped = filtered.map(x => x * 2);
    const result = mapped.reduce((a, b) => a + b, 0);
    return result;
}
"#;

    fs::write(temp_dir.path().join("file1.js"), duplicate_code).unwrap();
    fs::write(temp_dir.path().join("file2.js"), duplicate_code).unwrap();

    // Step 1: Run scan and extract duplicate_id
    let output = Command::cargo_bin("polydup")
        .unwrap()
        .current_dir(temp_dir.path())
        .arg("scan")
        .arg(".")
        .arg("--format")
        .arg("json")
        .output()
        .unwrap();

    let json: serde_json::Value =
        serde_json::from_str(&String::from_utf8(output.stdout).unwrap()).unwrap();

    let duplicate_id = json["duplicates"][0]["duplicate_id"]
        .as_str()
        .expect("Should have duplicate_id in scan output");

    // Step 2: Use that ID to create an ignore entry
    let mut cmd = Command::cargo_bin("polydup").unwrap();
    cmd.current_dir(temp_dir.path())
        .arg("ignore")
        .arg("add")
        .arg(duplicate_id)
        .arg("--files")
        .arg("file1.js:1-10")
        .arg("--reason")
        .arg("Extracted from scan output");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("✓ Duplicate added to ignore list"));

    // Step 3: Verify the ignore file was created with the correct ID
    let ignore_content = fs::read_to_string(temp_dir.path().join(".polydup-ignore"))
        .expect("Should create .polydup-ignore file");

    assert!(
        ignore_content.contains(duplicate_id),
        "Ignore file should contain the duplicate ID from scan output"
    );
}

#[test]
fn test_type3_duplicates_also_have_ids_without_ignore_file() {
    let temp_dir = TempDir::new().unwrap();

    // Create near-miss duplicate files (Type-3)
    let code1 = r#"
function processData(input) {
    const data = Array.isArray(input) ? input : [input];
    const filtered = data.filter(item => item != null);
    const mapped = filtered.map(x => x * 2);
    const result = mapped.reduce((a, b) => a + b, 0);
    return result;
}
"#;

    let code2 = r#"
function processData(input) {
    const data = Array.isArray(input) ? input : [input];
    const filtered = data.filter(item => item !== undefined);
    const mapped = filtered.map(x => x * 3);
    const result = mapped.reduce((a, b) => a + b, 1);
    return result;
}
"#;

    fs::write(temp_dir.path().join("file1.js"), code1).unwrap();
    fs::write(temp_dir.path().join("file2.js"), code2).unwrap();

    // Run scan with Type-3 detection - NO .polydup-ignore file exists
    let mut cmd = Command::cargo_bin("polydup").unwrap();
    cmd.current_dir(temp_dir.path())
        .arg("scan")
        .arg(".")
        .arg("--enable-type3")
        .arg("--format")
        .arg("json");

    let output = cmd.output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    let json: serde_json::Value = serde_json::from_str(&stdout).expect("Should output valid JSON");

    // Find Type-3 duplicates
    let duplicates = json["duplicates"]
        .as_array()
        .expect("Should have duplicates array");

    if !duplicates.is_empty() {
        // If Type-3 clones are detected, they should have IDs
        for (i, dup) in duplicates.iter().enumerate() {
            if dup["clone_type"].as_str() == Some("type-3") {
                let duplicate_id = dup["duplicate_id"]
                    .as_str()
                    .unwrap_or_else(|| panic!("Type-3 duplicate {} should have duplicate_id", i));

                assert!(
                    duplicate_id.starts_with("sha256:"),
                    "Type-3 duplicate {} ID should be SHA256 hash",
                    i
                );
            }
        }
    }
}
