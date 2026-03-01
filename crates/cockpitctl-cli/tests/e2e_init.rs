//! End-to-end tests for the `cockpitctl init` subcommand.

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

/// Build the `cockpitctl` binary command.
fn cmd() -> Command {
    let mut cmd = assert_cmd::cargo_bin_cmd!("cockpitctl");
    if let Ok(profile) = std::env::var("LLVM_PROFILE_FILE") {
        cmd.env("LLVM_PROFILE_FILE", profile);
    }
    cmd
}

// =============================================================================
// Test: Default init creates cockpit.toml with valid TOML
// =============================================================================

#[test]
fn init_default_creates_cockpit_toml() {
    let tmp = TempDir::new().expect("tempdir");

    cmd()
        .current_dir(tmp.path())
        .args(["init"])
        .assert()
        .success()
        .stderr(predicate::str::contains("wrote"));

    let path = tmp.path().join("cockpit.toml");
    assert!(path.exists(), "cockpit.toml should be created");

    let content = fs::read_to_string(&path).expect("read cockpit.toml");
    let _parsed: toml::Value = toml::from_str(&content).expect("output should be valid TOML");
}

// =============================================================================
// Test: Custom path via --path flag
// =============================================================================

#[test]
fn init_custom_path() {
    let tmp = TempDir::new().expect("tempdir");
    let custom = tmp.path().join("custom.toml");

    cmd()
        .args(["init", "--path", &custom.to_string_lossy()])
        .assert()
        .success()
        .stderr(predicate::str::contains("wrote"));

    assert!(custom.exists(), "file at custom path should be created");

    let content = fs::read_to_string(&custom).expect("read custom.toml");
    let _parsed: toml::Value = toml::from_str(&content).expect("output should be valid TOML");
}

// =============================================================================
// Test: Refuses to overwrite existing file (exit code 2)
// =============================================================================

#[test]
fn init_refuses_overwrite_existing() {
    let tmp = TempDir::new().expect("tempdir");
    let path = tmp.path().join("cockpit.toml");
    fs::write(&path, "# existing config\n").expect("write seed file");

    cmd()
        .args(["init", "--path", &path.to_string_lossy()])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("refusing to overwrite"));

    // Original content must be preserved.
    let content = fs::read_to_string(&path).expect("read file");
    assert_eq!(
        content, "# existing config\n",
        "existing file must not be modified"
    );
}

// =============================================================================
// Test: Output contains expected TOML sections
// =============================================================================

#[test]
fn init_output_contains_expected_sections() {
    let tmp = TempDir::new().expect("tempdir");
    let path = tmp.path().join("cockpit.toml");

    cmd()
        .args(["init", "--path", &path.to_string_lossy()])
        .assert()
        .success();

    let content = fs::read_to_string(&path).expect("read cockpit.toml");
    let parsed: toml::Value = toml::from_str(&content).expect("valid TOML");

    // Must have a [policy] table.
    assert!(
        parsed.get("policy").is_some(),
        "generated config should contain [policy] section"
    );

    // Must have at least one sensor defined under [sensors.*].
    let sensors = parsed.get("sensors").and_then(|v| v.as_table());
    assert!(
        sensors.is_some_and(|t| !t.is_empty()),
        "generated config should contain at least one sensor"
    );
}

// =============================================================================
// Test: Init in a fresh temp directory (no pre-existing files)
// =============================================================================

#[test]
fn init_in_fresh_temp_directory() {
    let tmp = TempDir::new().expect("tempdir");
    let subdir = tmp.path().join("project");
    fs::create_dir_all(&subdir).expect("create subdir");

    let path = subdir.join("cockpit.toml");

    cmd()
        .args(["init", "--path", &path.to_string_lossy()])
        .assert()
        .success();

    assert!(
        path.exists(),
        "cockpit.toml should be created in subdirectory"
    );

    let content = fs::read_to_string(&path).expect("read cockpit.toml");
    assert!(!content.is_empty(), "generated file should not be empty");
    let _parsed: toml::Value = toml::from_str(&content).expect("output should be valid TOML");
}

// =============================================================================
// Test: Init twice — second call should fail without corrupting the first
// =============================================================================

#[test]
fn init_idempotent_second_call_fails() {
    let tmp = TempDir::new().expect("tempdir");
    let path = tmp.path().join("cockpit.toml");

    // First init succeeds.
    cmd()
        .args(["init", "--path", &path.to_string_lossy()])
        .assert()
        .success();

    let first_content = fs::read_to_string(&path).expect("read after first init");

    // Second init refuses.
    cmd()
        .args(["init", "--path", &path.to_string_lossy()])
        .assert()
        .code(2);

    let second_content = fs::read_to_string(&path).expect("read after second init");
    assert_eq!(
        first_content, second_content,
        "file must not be modified by second init"
    );
}
