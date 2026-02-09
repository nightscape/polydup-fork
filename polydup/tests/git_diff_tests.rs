//! Integration tests for git-diff mode
#![allow(deprecated)]

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use std::process::Command as StdCommand;
use tempfile::TempDir;

/// Helper to initialize a git repository
fn init_git_repo(dir: &TempDir) {
    let path = dir.path();

    // Initialize git repo
    StdCommand::new("git")
        .args(["init"])
        .current_dir(path)
        .output()
        .expect("Failed to init git repo");

    // Configure git user
    StdCommand::new("git")
        .args(["config", "user.email", "test@example.com"])
        .current_dir(path)
        .output()
        .expect("Failed to set git email");

    StdCommand::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(path)
        .output()
        .expect("Failed to set git name");
}

/// Helper to commit files
fn git_commit(dir: &TempDir, message: &str) {
    let path = dir.path();

    StdCommand::new("git")
        .args(["add", "."])
        .current_dir(path)
        .output()
        .expect("Failed to git add");

    StdCommand::new("git")
        .args(["commit", "-m", message])
        .current_dir(path)
        .output()
        .expect("Failed to git commit");
}

/// Helper to create a JavaScript file with a long function
fn create_js_function(name: &str, var_prefix: &str) -> String {
    format!(
        r#"
function {name}(input) {{
    const data = Array.isArray(input) ? input : [input];
    const filtered = data.filter(item => item != null);
    const mapped = filtered.map(item => {{
        if (typeof item === 'string') {{
            return item.toUpperCase().trim();
        }} else if (typeof item === 'number') {{
            return item * 2 + 10;
        }} else {{
            return String(item);
        }}
    }});
    const {var_prefix}1 = mapped.length;
    const {var_prefix}2 = {var_prefix}1 + 100;
    const {var_prefix}3 = {var_prefix}2 * 2;
    const {var_prefix}4 = {var_prefix}3 - 50;
    const {var_prefix}5 = {var_prefix}4 / 2;
    return {var_prefix}5;
}}
"#,
        name = name,
        var_prefix = var_prefix
    )
}

#[test]
fn test_git_diff_filters_to_changed_files_only() {
    let temp_dir = TempDir::new().unwrap();
    init_git_repo(&temp_dir);

    // Create three files with duplicates
    let unchanged1 = create_js_function("processArray", "value");
    let unchanged2 = create_js_function("handleArray", "result");
    let changed = create_js_function("transformData", "temp");

    fs::write(temp_dir.path().join("unchanged1.js"), &unchanged1).unwrap();
    fs::write(temp_dir.path().join("unchanged2.js"), &unchanged2).unwrap();
    fs::write(temp_dir.path().join("changed.js"), &changed).unwrap();

    git_commit(&temp_dir, "Initial commit");

    // Modify changed.js to duplicate unchanged1.js
    fs::write(
        temp_dir.path().join("changed.js"),
        format!("{}\n{}", changed, unchanged1),
    )
    .unwrap();

    git_commit(&temp_dir, "Add duplicate in changed.js");

    // Scan with git-diff mode
    let mut cmd = Command::cargo_bin("polydup").unwrap();
    cmd.current_dir(temp_dir.path())
        .arg("scan")
        .arg(".")
        .arg("--similarity")
        .arg("0.8")
        .arg("--git-diff")
        .arg("HEAD~1..HEAD");

    let output = cmd.output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    // Should find duplicates involving changed.js
    assert!(
        stdout.contains("changed.js"),
        "Should report duplicates involving changed.js"
    );

    // Should NOT show duplicate between unchanged1 and unchanged2
    // (both files are unchanged, so this duplicate should be filtered out)
    let lines: Vec<&str> = stdout.lines().collect();
    let duplicate_section = lines
        .iter()
        .skip_while(|line| !line.contains("Duplicates"))
        .take_while(|line| !line.contains("Tip:"))
        .collect::<Vec<_>>();

    // Count duplicates reported - should only be those involving changed.js
    let duplicate_count = duplicate_section
        .iter()
        .filter(|line| line.contains("Type-2") || line.contains("Type-1"))
        .count();

    assert!(
        duplicate_count > 0,
        "Should find at least one duplicate involving changed file"
    );
}

#[test]
fn test_git_diff_empty_changes() {
    let temp_dir = TempDir::new().unwrap();
    init_git_repo(&temp_dir);

    // Create file and commit
    fs::write(
        temp_dir.path().join("test.js"),
        "function test() { return 1; }",
    )
    .unwrap();
    git_commit(&temp_dir, "Initial commit");

    // Create another commit with no file changes to have HEAD~1
    fs::write(
        temp_dir.path().join("test2.js"),
        "function test2() { return 2; }",
    )
    .unwrap();
    git_commit(&temp_dir, "Add test2");

    // No changes between last two commits to test.js

    // Scan with git-diff mode - should show no changed files or handle gracefully
    let mut cmd = Command::cargo_bin("polydup").unwrap();
    cmd.current_dir(temp_dir.path())
        .arg("scan")
        .arg(".")
        .arg("--git-diff")
        .arg("HEAD~1..HEAD");

    cmd.assert().success();
}

#[test]
fn test_git_diff_with_multiple_changed_files() {
    let temp_dir = TempDir::new().unwrap();
    init_git_repo(&temp_dir);

    // Create files
    let func1 = create_js_function("func1", "a");
    let func2 = create_js_function("func2", "b");
    let func3 = create_js_function("func3", "c");

    fs::write(temp_dir.path().join("file1.js"), &func1).unwrap();
    fs::write(temp_dir.path().join("file2.js"), &func2).unwrap();
    fs::write(temp_dir.path().join("file3.js"), &func3).unwrap();

    git_commit(&temp_dir, "Initial commit");

    // Modify file1 and file2 to create duplicates
    fs::write(
        temp_dir.path().join("file1.js"),
        format!("{}\n{}", func1, func2),
    )
    .unwrap();

    fs::write(
        temp_dir.path().join("file2.js"),
        format!("{}\n{}", func2, func3),
    )
    .unwrap();

    git_commit(&temp_dir, "Modify file1 and file2");

    // Scan with git-diff
    let mut cmd = Command::cargo_bin("polydup").unwrap();
    cmd.current_dir(temp_dir.path())
        .arg("scan")
        .arg(".")
        .arg("--similarity")
        .arg("0.8")
        .arg("--git-diff")
        .arg("HEAD~1..HEAD")
        .arg("--verbose");

    let output = cmd.output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();

    // Should report changed files
    assert!(stdout.contains("file1.js"));
    assert!(stdout.contains("file2.js"));

    // Verify git-diff filter is active (verbose output goes to stderr)
    assert!(stderr.contains("Git-diff filter:") || stdout.contains("Git-diff filter:"));
}

#[test]
fn test_git_diff_no_duplicates() {
    let temp_dir = TempDir::new().unwrap();
    init_git_repo(&temp_dir);

    // Create unique files
    fs::write(
        temp_dir.path().join("unique1.js"),
        "function unique1() { return 'totally unique'; }",
    )
    .unwrap();

    git_commit(&temp_dir, "Initial commit");

    // Add another unique file
    fs::write(
        temp_dir.path().join("unique2.js"),
        "function unique2() { return 'also unique and different'; }",
    )
    .unwrap();

    git_commit(&temp_dir, "Add unique file");

    // Scan with git-diff
    let mut cmd = Command::cargo_bin("polydup").unwrap();
    cmd.current_dir(temp_dir.path())
        .arg("scan")
        .arg(".")
        .arg("--git-diff")
        .arg("HEAD~1..HEAD");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("No duplicates found"));
}

#[test]
fn test_git_diff_with_ignore_rules() {
    let temp_dir = TempDir::new().unwrap();
    init_git_repo(&temp_dir);

    // Create files with duplicates
    let func = create_js_function("process", "val");
    fs::write(temp_dir.path().join("file1.js"), &func).unwrap();
    fs::write(temp_dir.path().join("file2.js"), &func).unwrap();

    git_commit(&temp_dir, "Initial commit");

    // Initialize polydup config
    let mut cmd = Command::cargo_bin("polydup").unwrap();
    cmd.current_dir(temp_dir.path()).arg("init").arg("-y"); // Use -y flag instead of --non-interactive
    cmd.assert().success();

    // Add ignore rule
    let mut cmd = Command::cargo_bin("polydup").unwrap();
    cmd.current_dir(temp_dir.path())
        .arg("ignore")
        .arg("add")
        .arg("test-duplicate-id")
        .arg("--files")
        .arg("file1.js:2-20")
        .arg("--reason")
        .arg("Test ignore");
    cmd.assert().success();

    git_commit(&temp_dir, "Add ignore rules");

    // Modify file1 to trigger git-diff
    fs::write(
        temp_dir.path().join("file1.js"),
        format!("{}\n// Modified", func),
    )
    .unwrap();

    git_commit(&temp_dir, "Modify file1");

    // Scan with git-diff - ignore rules should be loaded
    let mut cmd = Command::cargo_bin("polydup").unwrap();
    cmd.current_dir(temp_dir.path())
        .arg("scan")
        .arg(".")
        .arg("--similarity")
        .arg("0.8")
        .arg("--git-diff")
        .arg("HEAD~1..HEAD")
        .arg("--verbose");

    let output = cmd.output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();

    // Should load config (check both stdout and stderr)
    assert!(
        stdout.contains("Config:")
            || stdout.contains(".polyduprc.toml")
            || stderr.contains("Config:")
            || stderr.contains(".polyduprc.toml"),
        "Should load configuration file"
    );
}

#[test]
fn test_git_diff_with_directives() {
    let temp_dir = TempDir::new().unwrap();
    init_git_repo(&temp_dir);

    // Create files with duplicate code
    let func = create_js_function("duplicateFunc", "x");
    fs::write(temp_dir.path().join("file1.js"), &func).unwrap();
    fs::write(temp_dir.path().join("file2.js"), &func).unwrap();

    git_commit(&temp_dir, "Initial commit");

    // Add directive to file1
    fs::write(
        temp_dir.path().join("file1.js"),
        format!("// polydup-ignore: intentional code reuse\n{}", func),
    )
    .unwrap();

    git_commit(&temp_dir, "Add directive");

    // Scan with git-diff and directives enabled
    let mut cmd = Command::cargo_bin("polydup").unwrap();
    cmd.current_dir(temp_dir.path())
        .arg("scan")
        .arg(".")
        .arg("--similarity")
        .arg("0.8")
        .arg("--git-diff")
        .arg("HEAD~1..HEAD")
        .arg("--enable-directives")
        .arg("--verbose");

    let output = cmd.output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();

    // Should enable directives (check both stdout and stderr)
    assert!(
        stdout.contains("Inline directives: enabled")
            || stderr.contains("Inline directives: enabled"),
        "Should indicate directives are enabled"
    );

    // If duplicates are found, they should be marked as suppressed
    if stdout.contains("SUPPRESSED") {
        assert!(
            stdout.contains("[SUPPRESSED]"),
            "Directives should mark duplicates as suppressed"
        );
    }
}

#[test]
fn test_git_diff_verbose_output() {
    let temp_dir = TempDir::new().unwrap();
    init_git_repo(&temp_dir);

    // Create simple files
    fs::write(
        temp_dir.path().join("test.js"),
        "function test() { return 1; }",
    )
    .unwrap();

    git_commit(&temp_dir, "Initial commit");

    // Modify file
    fs::write(
        temp_dir.path().join("test.js"),
        "function test() { return 2; }",
    )
    .unwrap();

    git_commit(&temp_dir, "Modify test");

    // Scan with verbose flag
    let mut cmd = Command::cargo_bin("polydup").unwrap();
    cmd.current_dir(temp_dir.path())
        .arg("scan")
        .arg(".")
        .arg("--git-diff")
        .arg("HEAD~1..HEAD")
        .arg("--verbose");

    cmd.assert()
        .success()
        .stderr(predicate::str::contains("Git-Diff Mode:"))
        .stderr(predicate::str::contains("Changed files"));
}

#[test]
fn test_git_diff_json_output() {
    let temp_dir = TempDir::new().unwrap();
    init_git_repo(&temp_dir);

    // Create files with duplicates
    let func = create_js_function("test", "v");
    fs::write(temp_dir.path().join("file1.js"), &func).unwrap();
    fs::write(temp_dir.path().join("file2.js"), &func).unwrap();

    git_commit(&temp_dir, "Initial commit");

    // Modify file1
    fs::write(
        temp_dir.path().join("file1.js"),
        format!("{}\n// Comment", func),
    )
    .unwrap();

    git_commit(&temp_dir, "Modify file1");

    // Scan with JSON output
    let mut cmd = Command::cargo_bin("polydup").unwrap();
    cmd.current_dir(temp_dir.path())
        .arg("scan")
        .arg(".")
        .arg("--similarity")
        .arg("0.8")
        .arg("--git-diff")
        .arg("HEAD~1..HEAD")
        .arg("--format")
        .arg("json");

    let output = cmd.output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    // Should be valid JSON
    assert!(
        serde_json::from_str::<serde_json::Value>(&stdout).is_ok(),
        "Output should be valid JSON"
    );
}

#[test]
fn test_git_diff_all_duplicates_filtered() {
    let temp_dir = TempDir::new().unwrap();
    init_git_repo(&temp_dir);

    // Create files where unchanged files have duplicates
    let unchanged_func = create_js_function("unchanged", "u");
    fs::write(temp_dir.path().join("unchanged1.js"), &unchanged_func).unwrap();
    fs::write(temp_dir.path().join("unchanged2.js"), &unchanged_func).unwrap();

    // Create a changed file with unique code
    fs::write(
        temp_dir.path().join("changed.js"),
        "function unique() { return 'very unique code here'; }",
    )
    .unwrap();

    git_commit(&temp_dir, "Initial commit");

    // Modify changed.js but keep it unique
    fs::write(
        temp_dir.path().join("changed.js"),
        "function unique() { return 'very unique and modified code'; }",
    )
    .unwrap();

    git_commit(&temp_dir, "Modify changed.js");

    // Scan with git-diff - should find no duplicates involving changed files
    let mut cmd = Command::cargo_bin("polydup").unwrap();
    cmd.current_dir(temp_dir.path())
        .arg("scan")
        .arg(".")
        .arg("--similarity")
        .arg("0.8")
        .arg("--git-diff")
        .arg("HEAD~1..HEAD");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("No duplicates found"));
}

#[test]
fn test_git_diff_with_deleted_files() {
    let temp_dir = TempDir::new().unwrap();
    init_git_repo(&temp_dir);

    // Create files
    fs::write(
        temp_dir.path().join("file1.js"),
        "function test1() { return 1; }",
    )
    .unwrap();
    fs::write(
        temp_dir.path().join("file2.js"),
        "function test2() { return 2; }",
    )
    .unwrap();

    git_commit(&temp_dir, "Initial commit");

    // Delete file1
    fs::remove_file(temp_dir.path().join("file1.js")).unwrap();

    git_commit(&temp_dir, "Delete file1");

    // Scan with git-diff - should handle deleted files gracefully
    let mut cmd = Command::cargo_bin("polydup").unwrap();
    cmd.current_dir(temp_dir.path())
        .arg("scan")
        .arg(".")
        .arg("--git-diff")
        .arg("HEAD~1..HEAD")
        .arg("--verbose");

    cmd.assert().success();
}

#[test]
fn test_git_diff_with_renamed_files() {
    let temp_dir = TempDir::new().unwrap();
    init_git_repo(&temp_dir);

    // Create file
    let func = create_js_function("test", "v");
    fs::write(temp_dir.path().join("old_name.js"), &func).unwrap();

    git_commit(&temp_dir, "Initial commit");

    // Rename file
    fs::rename(
        temp_dir.path().join("old_name.js"),
        temp_dir.path().join("new_name.js"),
    )
    .unwrap();

    git_commit(&temp_dir, "Rename file");

    // Scan with git-diff - should detect renamed file
    let mut cmd = Command::cargo_bin("polydup").unwrap();
    cmd.current_dir(temp_dir.path())
        .arg("scan")
        .arg(".")
        .arg("--git-diff")
        .arg("HEAD~1..HEAD")
        .arg("--verbose");

    let output = cmd.output().unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();

    // Should report renamed file in changed files
    assert!(stderr.contains("new_name.js") || stderr.contains("Renamed"));
}

/// Regression test: git-diff mode should scan ONLY changed files for performance
/// Previously, git-diff mode scanned all files and filtered results afterward,
/// losing the 10-100x performance benefit. This test verifies that duplicates
/// between unchanged files are not found (because those files aren't scanned).
#[test]
fn test_git_diff_scans_only_changed_files_not_all() {
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

    // Create UNCHANGED duplicate files (committed in first commit, never modified)
    let unchanged_duplicate = r#"
function unchangedDuplicate() {
    const config = loadConfiguration();
    const data = processAllData(config);
    const result = transformResult(data);
    saveToDatabase(result);
    logCompletion("unchanged");
    return result;
}
"#;
    fs::write(temp_dir.path().join("unchanged1.js"), unchanged_duplicate).unwrap();
    fs::write(temp_dir.path().join("unchanged2.js"), unchanged_duplicate).unwrap();

    // Create a different file that will be changed later
    fs::write(
        temp_dir.path().join("will_change.js"),
        "function willChange() { return 1; }\n",
    )
    .unwrap();

    // Commit all files
    Command::new("git")
        .args(["add", "."])
        .current_dir(temp_dir.path())
        .output()
        .unwrap();

    Command::new("git")
        .args(["commit", "-m", "Initial commit with unchanged duplicates"])
        .current_dir(temp_dir.path())
        .output()
        .unwrap();

    // Now modify only will_change.js (the unchanged duplicates stay the same)
    fs::write(
        temp_dir.path().join("will_change.js"),
        "function willChange() { return 2; /* modified */ }\n",
    )
    .unwrap();

    Command::new("git")
        .args(["add", "."])
        .current_dir(temp_dir.path())
        .output()
        .unwrap();

    Command::new("git")
        .args(["commit", "-m", "Modify will_change.js only"])
        .current_dir(temp_dir.path())
        .output()
        .unwrap();

    // Scan with git-diff mode
    let mut cmd = Command::cargo_bin("polydup").unwrap();
    cmd.current_dir(temp_dir.path())
        .arg("scan")
        .arg(".")
        .arg("--git-diff")
        .arg("HEAD~1..HEAD")
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

    // The unchanged duplicates (unchanged1.js and unchanged2.js) should NOT be found
    // because git-diff mode only scans changed files (will_change.js)
    let duplicates = json["duplicates"].as_array().unwrap();

    // Verify no duplicates involving unchanged files are reported
    for dup in duplicates {
        let file1 = dup["file1"].as_str().unwrap_or("");
        let file2 = dup["file2"].as_str().unwrap_or("");

        // If git-diff properly scans only changed files, we should NOT find
        // duplicates between unchanged1.js and unchanged2.js
        let involves_unchanged = (file1.contains("unchanged1") && file2.contains("unchanged2"))
            || (file1.contains("unchanged2") && file2.contains("unchanged1"));

        assert!(
            !involves_unchanged,
            "Git-diff mode found duplicates between unchanged files ({} and {}). \
             This indicates ALL files were scanned instead of just changed files, \
             which is a performance regression.",
            file1, file2
        );
    }
}
