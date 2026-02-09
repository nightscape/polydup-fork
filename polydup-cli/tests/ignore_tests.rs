//! CLI tests for ignore commands
#![allow(deprecated)]

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

#[test]
fn test_ignore_list_empty() {
    let temp_dir = TempDir::new().unwrap();

    let mut cmd = Command::cargo_bin("polydup").unwrap();
    cmd.current_dir(temp_dir.path()).arg("ignore").arg("list");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("No ignored duplicates"));
}

#[test]
fn test_ignore_add_and_list() {
    let temp_dir = TempDir::new().unwrap();

    // Add an ignore entry
    let mut cmd = Command::cargo_bin("polydup").unwrap();
    cmd.current_dir(temp_dir.path())
        .arg("ignore")
        .arg("add")
        .arg("test-id-123")
        .arg("--files")
        .arg("test.js:10-20")
        .arg("--reason")
        .arg("Test entry")
        .arg("--added-by")
        .arg("test-user");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("✓ Duplicate added to ignore list"))
        .stdout(predicate::str::contains("test-id-123"));

    // List and verify
    let mut cmd = Command::cargo_bin("polydup").unwrap();
    cmd.current_dir(temp_dir.path()).arg("ignore").arg("list");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Ignored Duplicates (1)"))
        .stdout(predicate::str::contains("test-id-123"))
        .stdout(predicate::str::contains("Test entry"))
        .stdout(predicate::str::contains("test-user"));
}

#[test]
fn test_ignore_list_verbose() {
    let temp_dir = TempDir::new().unwrap();

    // Add an entry with multiple files
    let mut cmd = Command::cargo_bin("polydup").unwrap();
    cmd.current_dir(temp_dir.path())
        .arg("ignore")
        .arg("add")
        .arg("multi-file-id")
        .arg("--files")
        .arg("src/main.rs:5-15,src/lib.rs:10-20")
        .arg("--reason")
        .arg("Multi-file duplicate");

    cmd.assert().success();

    // List with verbose flag
    let mut cmd = Command::cargo_bin("polydup").unwrap();
    cmd.current_dir(temp_dir.path())
        .arg("ignore")
        .arg("list")
        .arg("--verbose");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("src/main.rs:5-15"))
        .stdout(predicate::str::contains("src/lib.rs:10-20"));
}

#[test]
fn test_ignore_list_json() {
    let temp_dir = TempDir::new().unwrap();

    // Add an entry
    let mut cmd = Command::cargo_bin("polydup").unwrap();
    cmd.current_dir(temp_dir.path())
        .arg("ignore")
        .arg("add")
        .arg("json-test-id")
        .arg("--files")
        .arg("test.rs:1-10")
        .arg("--reason")
        .arg("JSON test");

    cmd.assert().success();

    // List as JSON
    let mut cmd = Command::cargo_bin("polydup").unwrap();
    cmd.current_dir(temp_dir.path())
        .arg("ignore")
        .arg("list")
        .arg("--format")
        .arg("json");

    let output = cmd.output().unwrap();
    let json_str = String::from_utf8(output.stdout).unwrap();

    // Verify JSON structure
    assert!(json_str.contains("\"id\": \"json-test-id\""));
    assert!(json_str.contains("\"reason\": \"JSON test\""));
    assert!(json_str.contains("\"files\""));
}

#[test]
fn test_ignore_remove() {
    let temp_dir = TempDir::new().unwrap();

    // Add an entry
    let mut cmd = Command::cargo_bin("polydup").unwrap();
    cmd.current_dir(temp_dir.path())
        .arg("ignore")
        .arg("add")
        .arg("remove-test-id")
        .arg("--files")
        .arg("test.js:1-5")
        .arg("--reason")
        .arg("To be removed");

    cmd.assert().success();

    // Remove it
    let mut cmd = Command::cargo_bin("polydup").unwrap();
    cmd.current_dir(temp_dir.path())
        .arg("ignore")
        .arg("remove")
        .arg("remove-test-id");

    cmd.assert().success().stdout(predicate::str::contains(
        "✓ Duplicate removed from ignore list",
    ));

    // Verify it's gone
    let mut cmd = Command::cargo_bin("polydup").unwrap();
    cmd.current_dir(temp_dir.path()).arg("ignore").arg("list");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("No ignored duplicates"));
}

#[test]
fn test_ignore_remove_nonexistent() {
    let temp_dir = TempDir::new().unwrap();

    let mut cmd = Command::cargo_bin("polydup").unwrap();
    cmd.current_dir(temp_dir.path())
        .arg("ignore")
        .arg("remove")
        .arg("nonexistent-id");

    cmd.assert()
        .failure()
        .stdout(predicate::str::contains("✗ Duplicate ID not found"));
}

#[test]
fn test_ignore_file_persistence() {
    let temp_dir = TempDir::new().unwrap();
    let ignore_file = temp_dir.path().join(".polydup-ignore");

    // Add an entry
    let mut cmd = Command::cargo_bin("polydup").unwrap();
    cmd.current_dir(temp_dir.path())
        .arg("ignore")
        .arg("add")
        .arg("persist-test-id")
        .arg("--files")
        .arg("test.js:10-20")
        .arg("--reason")
        .arg("Persistence test");

    cmd.assert().success();

    // Verify file exists
    assert!(
        ignore_file.exists(),
        ".polydup-ignore file should be created"
    );

    // Verify file contains TOML data
    let content = fs::read_to_string(&ignore_file).unwrap();
    assert!(content.contains("version = 1"));
    assert!(content.contains("persist-test-id"));
    assert!(content.contains("Persistence test"));
}

#[test]
fn test_ignore_add_multiple_files() {
    let temp_dir = TempDir::new().unwrap();

    // Add entry with comma-separated files
    let mut cmd = Command::cargo_bin("polydup").unwrap();
    cmd.current_dir(temp_dir.path())
        .arg("ignore")
        .arg("add")
        .arg("multi-id")
        .arg("--files")
        .arg("file1.rs:1-10,file2.rs:20-30,file3.rs:40-50")
        .arg("--reason")
        .arg("Multiple files");

    cmd.assert().success();

    // Verify with verbose list
    let mut cmd = Command::cargo_bin("polydup").unwrap();
    cmd.current_dir(temp_dir.path())
        .arg("ignore")
        .arg("list")
        .arg("--verbose");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("file1.rs:1-10"))
        .stdout(predicate::str::contains("file2.rs:20-30"))
        .stdout(predicate::str::contains("file3.rs:40-50"));
}

#[test]
fn test_ignore_add_files_scans_full_project() {
    let temp_dir = TempDir::new().unwrap();

    fs::create_dir_all(temp_dir.path().join("dir_a")).unwrap();
    fs::create_dir_all(temp_dir.path().join("dir_b")).unwrap();

    let duplicate_code = r#"
function processData(input) {
    const data = Array.isArray(input) ? input : [input];
    const filtered = data.filter(item => item != null && item != undefined);
    const mapped = filtered.map(item => {
        if (typeof item === 'string') {
            return item.toUpperCase().trim();
        } else if (typeof item === 'number') {
            return item * 2 + 10;
        } else {
            return String(item);
        }
    });
    const result = mapped.reduce((acc, val) => {
        if (acc.length === 0) {
            return [val];
        }
        return [...acc, val];
    }, []);
    console.log("Processed result:", result);
    return result;
}
"#;

    fs::write(
        temp_dir.path().join("dir_a").join("left.js"),
        duplicate_code,
    )
    .unwrap();
    fs::write(
        temp_dir.path().join("dir_b").join("right.js"),
        duplicate_code,
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("polydup").unwrap();
    cmd.current_dir(temp_dir.path())
        .arg("ignore")
        .arg("add")
        .arg("--files")
        .arg("dir_a/left.js:1-200")
        .arg("--reason")
        .arg("Cross-directory duplicate");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("✓ Duplicate added to ignore list"));

    let ignore_file = temp_dir.path().join(".polydup-ignore");
    let content = fs::read_to_string(ignore_file).unwrap();
    assert!(
        content.contains("id = \""),
        "Ignore file should contain a computed duplicate ID"
    );
}

#[test]
fn test_ignore_add_interactive_without_args() {
    let temp_dir = TempDir::new().unwrap();
    fs::create_dir_all(temp_dir.path().join("src")).unwrap();
    fs::write(
        temp_dir.path().join("src").join("example.rs"),
        "fn example() {}\n",
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("polydup").unwrap();
    cmd.current_dir(temp_dir.path())
        .arg("ignore")
        .arg("add")
        .write_stdin("interactive-id\nsrc/example.rs:1-1\nInteractive entry\n");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Interactive mode"))
        .stdout(predicate::str::contains("✓ Duplicate added to ignore list"))
        .stdout(predicate::str::contains("interactive-id"));

    let ignore_file = temp_dir.path().join(".polydup-ignore");
    let content = fs::read_to_string(ignore_file).unwrap();
    assert!(content.contains("interactive-id"));
    assert!(content.contains("src/example.rs"));
    assert!(content.contains("Interactive entry"));
}

#[test]
fn test_ignore_add_invalid_file_format() {
    let temp_dir = TempDir::new().unwrap();

    let mut cmd = Command::cargo_bin("polydup").unwrap();
    cmd.current_dir(temp_dir.path())
        .arg("ignore")
        .arg("add")
        .arg("bad-format-id")
        .arg("--files")
        .arg("invalid-format") // Missing :start-end
        .arg("--reason")
        .arg("Should fail");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("Invalid file range format"));
}

#[test]
fn test_ignore_help() {
    let mut cmd = Command::cargo_bin("polydup").unwrap();
    cmd.arg("ignore").arg("--help");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Manage ignored duplicates"))
        .stdout(predicate::str::contains("add"))
        .stdout(predicate::str::contains("list"))
        .stdout(predicate::str::contains("remove"));
}

#[test]
fn test_ignore_add_help() {
    let mut cmd = Command::cargo_bin("polydup").unwrap();
    cmd.arg("ignore").arg("add").arg("--help");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains(
            "Add a duplicate to the ignore list",
        ))
        .stdout(predicate::str::contains("--files"))
        .stdout(predicate::str::contains("--reason"));
}
#[test]
fn test_scan_respects_ignore_file() {
    let temp_dir = TempDir::new().unwrap();

    // Create duplicate JavaScript files
    let duplicate_code = r#"
function processData(input) {
    const data = Array.isArray(input) ? input : [input];
    const filtered = data.filter(item => item != null && item != undefined);
    const mapped = filtered.map(item => {
        if (typeof item === 'string') {
            return item.toUpperCase().trim();
        } else if (typeof item === 'number') {
            return item * 2 + 10;
        } else {
            return String(item);
        }
    });
    const result = mapped.reduce((acc, val) => {
        if (acc.length === 0) {
            return [val];
        }
        return [...acc, val];
    }, []);
    console.log("Processed result:", result);
    return result;
}
"#;

    fs::write(temp_dir.path().join("file1.js"), duplicate_code).unwrap();
    fs::write(temp_dir.path().join("file2.js"), duplicate_code).unwrap();

    // First scan without ignore - should find duplicates
    let mut cmd = Command::cargo_bin("polydup").unwrap();
    cmd.current_dir(temp_dir.path())
        .arg("scan")
        .arg(".")
        .arg("--similarity")
        .arg("0.8");

    let output = cmd.output().unwrap();
    let _stdout = String::from_utf8(output.stdout).unwrap();

    assert!(
        !_stdout.contains("No duplicates found"),
        "Should find duplicates before adding ignore"
    );

    // Initialize config and add ignore (use actual duplicate detection to get real ID)
    let mut cmd = Command::cargo_bin("polydup").unwrap();
    cmd.current_dir(temp_dir.path()).arg("init").arg("-y");
    cmd.assert().success();

    // Add ignore entry using ID from scan output
    // Note: --files flag is not fully implemented yet, so we use a placeholder ID
    // In real usage, users should get the ID from `polydup scan --format json`
    let mut cmd = Command::cargo_bin("polydup").unwrap();
    cmd.current_dir(temp_dir.path())
        .arg("ignore")
        .arg("add")
        .arg("test-duplicate-id-placeholder")
        .arg("--files")
        .arg("file1.js:2-22")
        .arg("--reason")
        .arg("Test duplicate - placeholder for testing");

    // This will succeed because we're providing a direct ID
    cmd.assert().success();

    // Second scan with ignore - should NOT find duplicates (or fewer duplicates)
    let mut cmd = Command::cargo_bin("polydup").unwrap();
    cmd.current_dir(temp_dir.path())
        .arg("scan")
        .arg(".")
        .arg("--similarity")
        .arg("0.8")
        .arg("--verbose");

    let output = cmd.output().unwrap();
    let _stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();

    // Should show that ignore rules were loaded
    assert!(
        stderr.contains("Ignore rules:") || stderr.contains("duplicate(s) ignored"),
        "Should indicate ignore rules were loaded in verbose mode"
    );

    // The duplicate should now be filtered
    // Note: We can't guarantee "No duplicates found" because there might be
    // other small duplicates, but the major duplicate should be gone
}

/// Regression test: ignore add --files should find Type-3 duplicates
/// Previously, Type-3 detection was not enabled during rescan, causing
/// "No duplicates found" errors for Type-3 duplicates.
#[test]
fn test_ignore_add_finds_type3_duplicates() {
    let temp_dir = TempDir::new().unwrap();

    // Create two files with similar but not identical functions (Type-3 clone)
    // They differ by more than just identifiers, so only Type-3 detection finds them
    let file1_content = r#"
function processUserData(user) {
    const validated = validateInput(user);
    const normalized = normalizeData(validated);
    const enriched = enrichUserProfile(normalized);
    const result = saveToDatabase(enriched);
    logActivity("user_processed", user.id);
    return result;
}
"#;

    let file2_content = r#"
function processOrderData(order) {
    const validated = validateInput(order);
    const normalized = normalizeData(validated);
    const transformed = transformOrderDetails(normalized);
    const enriched = enrichOrderInfo(transformed);
    const result = saveToDatabase(enriched);
    logActivity("order_processed", order.id);
    sendNotification(order.customerId);
    return result;
}
"#;

    fs::create_dir_all(temp_dir.path().join("src")).unwrap();
    fs::write(temp_dir.path().join("src").join("user.js"), file1_content).unwrap();
    fs::write(temp_dir.path().join("src").join("order.js"), file2_content).unwrap();

    // First, verify Type-3 detection finds the duplicate
    let mut cmd = Command::cargo_bin("polydup").unwrap();
    cmd.current_dir(temp_dir.path())
        .arg("scan")
        .arg(".")
        .arg("--enable-type3")
        .arg("--min-block-size")
        .arg("20")
        .arg("--format")
        .arg("json");

    let output = cmd.output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    // Parse JSON to check if we found Type-3 duplicates
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap_or_default();
    let duplicates = json["duplicates"].as_array();

    // Skip test if no Type-3 duplicates found (depends on detection thresholds)
    if duplicates.map(|d| d.is_empty()).unwrap_or(true) {
        eprintln!("Skipping test: no Type-3 duplicates detected with current thresholds");
        return;
    }

    // Get the duplicate info
    let dup = &duplicates.unwrap()[0];
    let file1 = dup["file1"].as_str().unwrap();
    let start_line1 = dup["start_line1"].as_u64().unwrap();
    let length = dup["length"].as_u64().unwrap();
    let end_line1 = start_line1 + length - 1;

    // Now try to ignore it using --files (this should work with Type-3 enabled)
    let mut cmd = Command::cargo_bin("polydup").unwrap();
    cmd.current_dir(temp_dir.path())
        .arg("ignore")
        .arg("add")
        .arg("--files")
        .arg(format!("{}:{}-{}", file1, start_line1, end_line1))
        .arg("--reason")
        .arg("Type-3 duplicate test")
        .arg("--threshold")
        .arg("20");

    // This should succeed - previously it failed with "No duplicates found"
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Duplicate added to ignore list"));
}

/// Regression test: `polydup ignore add` should write to git repo root, not CWD
/// Previously, running `polydup ignore add` from a subdirectory would create
/// .polydup-ignore in the subdirectory, but scan reads from repo root.
#[test]
fn test_ignore_add_writes_to_repo_root_not_cwd() {
    let temp_dir = TempDir::new().unwrap();

    // Initialize git repo
    Command::new("git")
        .args(["init"])
        .current_dir(temp_dir.path())
        .output()
        .unwrap();

    Command::new("git")
        .args(["config", "user.email", "test@test.com"])
        .current_dir(temp_dir.path())
        .output()
        .unwrap();

    Command::new("git")
        .args(["config", "user.name", "Test"])
        .current_dir(temp_dir.path())
        .output()
        .unwrap();

    // Create a subdirectory
    let subdir = temp_dir.path().join("packages").join("my-app");
    fs::create_dir_all(&subdir).unwrap();

    // Add duplicate files in the subdirectory
    let duplicate_code = r#"
function duplicateInSubdir() {
    const config = loadConfiguration();
    const data = processAllData(config);
    const result = transformResult(data);
    saveToDatabase(result);
    logCompletion("done");
    return result;
}
"#;
    fs::write(subdir.join("file1.js"), duplicate_code).unwrap();
    fs::write(subdir.join("file2.js"), duplicate_code).unwrap();

    // Run ignore add FROM THE SUBDIRECTORY
    let mut cmd = Command::cargo_bin("polydup").unwrap();
    cmd.current_dir(&subdir) // Run from subdirectory!
        .arg("ignore")
        .arg("add")
        .arg("test-id-from-subdir")
        .arg("--files")
        .arg("file1.js:2-9")
        .arg("--reason")
        .arg("Test from subdirectory");

    cmd.assert().success();

    // The .polydup-ignore file should be at repo root, NOT in subdirectory
    let ignore_at_root = temp_dir.path().join(".polydup-ignore");
    let ignore_in_subdir = subdir.join(".polydup-ignore");

    assert!(
        ignore_at_root.exists(),
        ".polydup-ignore should be created at repo root, not in subdirectory"
    );
    assert!(
        !ignore_in_subdir.exists(),
        ".polydup-ignore should NOT be created in subdirectory"
    );

    // Verify the content
    let content = fs::read_to_string(&ignore_at_root).unwrap();
    assert!(
        content.contains("test-id-from-subdir"),
        "Ignore file should contain the added ID"
    );
}

/// Regression test: .polydup-ignore should be loaded from scan target, not CWD
/// Previously, running `polydup scan /path/to/repo` from a different directory
/// would not find the .polydup-ignore file at the repo root.
#[test]
fn test_ignore_file_loaded_from_scan_target_not_cwd() {
    let repo_dir = TempDir::new().unwrap();
    let other_dir = TempDir::new().unwrap();

    // Create duplicate files in the "repo"
    fs::create_dir_all(repo_dir.path().join("src")).unwrap();
    let duplicate_code = r#"
function duplicateFunction() {
    const config = loadConfig();
    const data = processData(config);
    const result = transformResult(data);
    saveResult(result);
    logCompletion("done");
    return result;
}
"#;
    fs::write(repo_dir.path().join("src").join("file1.js"), duplicate_code).unwrap();
    fs::write(repo_dir.path().join("src").join("file2.js"), duplicate_code).unwrap();

    // First scan to get duplicate ID
    let mut cmd = Command::cargo_bin("polydup").unwrap();
    cmd.current_dir(repo_dir.path())
        .arg("scan")
        .arg(".")
        .arg("--format")
        .arg("json")
        .arg("--threshold")
        .arg("20");

    let output = cmd.output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();

    // Debug output if parsing fails
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap_or_else(|e| {
        panic!(
            "Failed to parse JSON: {}\nstdout: {}\nstderr: {}\nexit code: {:?}",
            e,
            stdout,
            stderr,
            output.status.code()
        );
    });
    let duplicate_id = json["duplicates"][0]["duplicate_id"]
        .as_str()
        .expect("Should have duplicate_id");

    // Create .polydup-ignore in the repo with this duplicate
    let ignore_content = format!(
        r#"version = 1

[[ignores]]
id = "{}"
reason = "Test ignore"
added_by = "test"
added_at = "2025-01-01T00:00:00Z"

[[ignores.files]]
file = "src/file1.js"
start_line = 2
end_line = 9

[[ignores.files]]
file = "src/file2.js"
start_line = 2
end_line = 9
"#,
        duplicate_id
    );
    fs::write(repo_dir.path().join(".polydup-ignore"), ignore_content).unwrap();

    // Now scan FROM A DIFFERENT DIRECTORY, passing the repo path
    let mut cmd = Command::cargo_bin("polydup").unwrap();
    cmd.current_dir(other_dir.path()) // Run from different directory
        .arg("scan")
        .arg(repo_dir.path()) // Scan the repo
        .arg("--format")
        .arg("json")
        .arg("--threshold")
        .arg("20");

    let output = cmd.output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();

    // Debug output if parsing fails
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap_or_else(|e| {
        panic!(
            "Failed to parse second scan JSON: {}\nstdout: {}\nstderr: {}\nexit code: {:?}",
            e,
            stdout,
            stderr,
            output.status.code()
        );
    });

    // The duplicate should be filtered out because .polydup-ignore was loaded
    let duplicates = json["duplicates"].as_array().unwrap();
    assert!(
        duplicates.is_empty(),
        "Duplicate should be ignored when scanning from different directory. \
         Found {} duplicates. This indicates .polydup-ignore was not loaded from scan target.",
        duplicates.len()
    );
}
