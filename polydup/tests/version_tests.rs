//! Tests for --version output format
//!
//! Verifies that the version output matches Cargo.toml and includes
//! installation source indicator.

use std::process::Command;

/// Get the expected version from Cargo.toml
fn expected_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[test]
fn version_output_includes_cargo_indicator() {
    let output = Command::new(env!("CARGO_BIN_EXE_polydup"))
        .arg("--version")
        .output()
        .expect("Failed to execute polydup");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let version_line = stdout.trim();

    // Should include (cargo) indicator
    assert!(
        version_line.contains("(cargo)"),
        "Version output should include '(cargo)' indicator. Got: {}",
        version_line
    );
}

#[test]
fn version_output_matches_cargo_toml() {
    let output = Command::new(env!("CARGO_BIN_EXE_polydup"))
        .arg("--version")
        .output()
        .expect("Failed to execute polydup");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let version_line = stdout.trim();

    let expected = expected_version();
    assert!(
        version_line.contains(&expected),
        "Version output should contain '{}'. Got: {}",
        expected,
        version_line
    );
}

#[test]
fn version_output_format_is_consistent() {
    let output = Command::new(env!("CARGO_BIN_EXE_polydup"))
        .arg("--version")
        .output()
        .expect("Failed to execute polydup");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let version_line = stdout.trim();

    // Expected format: "polydup X.Y.Z (cargo)"
    let expected_format = format!("polydup {} (cargo)", expected_version());
    assert_eq!(
        version_line, expected_format,
        "Version output format mismatch. Expected: '{}', Got: '{}'",
        expected_format, version_line
    );
}

#[test]
fn version_flag_exits_successfully() {
    let output = Command::new(env!("CARGO_BIN_EXE_polydup"))
        .arg("--version")
        .output()
        .expect("Failed to execute polydup");

    assert!(
        output.status.success(),
        "--version should exit with success code"
    );
}
