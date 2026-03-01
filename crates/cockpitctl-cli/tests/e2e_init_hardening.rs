//! Hardening tests for `cockpitctl init`.
//!
//! Covers: custom nested paths, overwrite protection, idempotency,
//! generated-config parseability, invalid path handling, and output messages.

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

fn cmd() -> Command {
    let mut cmd = assert_cmd::cargo_bin_cmd!("cockpitctl");
    if let Ok(profile) = std::env::var("LLVM_PROFILE_FILE") {
        cmd.env("LLVM_PROFILE_FILE", profile);
    }
    cmd
}

// =============================================================================
// Custom nested path creates intermediate directories' parent must exist
// =============================================================================

#[test]
fn init_custom_nested_path_creates_file() {
    let tmp = TempDir::new().expect("tempdir");
    let nested = tmp.path().join("deep").join("nested");
    fs::create_dir_all(&nested).expect("create parent dirs");
    let target = nested.join("cockpit.toml");

    cmd()
        .args(["init", "--path", &target.to_string_lossy()])
        .assert()
        .success()
        .stderr(predicate::str::contains("wrote"));

    assert!(target.exists(), "file should be created at nested path");
    let content = fs::read_to_string(&target).expect("read");
    let _parsed: toml::Value = toml::from_str(&content).expect("valid TOML");
}

// =============================================================================
// Overwrite protection: existing file must NOT be modified
// =============================================================================

#[test]
fn init_does_not_overwrite_existing_file() {
    let tmp = TempDir::new().expect("tempdir");
    let path = tmp.path().join("cockpit.toml");
    let original = "# my precious config\nkey = \"value\"\n";
    fs::write(&path, original).expect("seed file");

    cmd()
        .args(["init", "--path", &path.to_string_lossy()])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("refusing to overwrite"));

    let after = fs::read_to_string(&path).expect("read");
    assert_eq!(after, original, "content must be preserved exactly");
}

// =============================================================================
// Idempotency: init twice — second run detects existing
// =============================================================================

#[test]
fn init_twice_second_run_refuses() {
    let tmp = TempDir::new().expect("tempdir");
    let path = tmp.path().join("cockpit.toml");

    // First run succeeds.
    cmd()
        .args(["init", "--path", &path.to_string_lossy()])
        .assert()
        .success();

    let first = fs::read_to_string(&path).expect("read first");

    // Second run refuses.
    cmd()
        .args(["init", "--path", &path.to_string_lossy()])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("refusing to overwrite"));

    let second = fs::read_to_string(&path).expect("read second");
    assert_eq!(first, second, "file must not be mutated by second init");
}

// =============================================================================
// Generated config is valid TOML and parseable as CockpitConfig
// =============================================================================

#[test]
fn init_output_is_parseable_as_cockpit_config() {
    let tmp = TempDir::new().expect("tempdir");
    let path = tmp.path().join("cockpit.toml");

    cmd()
        .args(["init", "--path", &path.to_string_lossy()])
        .assert()
        .success();

    let content = fs::read_to_string(&path).expect("read");

    // Must be valid TOML.
    let _val: toml::Value = toml::from_str(&content).expect("valid TOML");

    // Must deserialize into the CockpitConfig domain type.
    let _cfg: cockpitctl::CockpitConfig =
        toml::from_str(&content).expect("parseable as CockpitConfig");
}

// =============================================================================
// Invalid path → graceful error (not a panic)
// =============================================================================

#[test]
fn init_invalid_path_returns_error() {
    // Attempt to write to a path whose parent does not exist.
    let tmp = TempDir::new().expect("tempdir");
    let bad = tmp.path().join("no_such_dir").join("cockpit.toml");

    cmd()
        .args(["init", "--path", &bad.to_string_lossy()])
        .assert()
        .failure()
        .stderr(predicate::str::is_empty().not());
}

// =============================================================================
// Output message includes the path that was written
// =============================================================================

#[test]
fn init_output_message_includes_path() {
    let tmp = TempDir::new().expect("tempdir");
    let path = tmp.path().join("my_config.toml");

    cmd()
        .args(["init", "--path", &path.to_string_lossy()])
        .assert()
        .success()
        .stderr(predicate::str::contains("my_config.toml"));
}

// =============================================================================
// Default init (no --path) writes cockpit.toml in cwd
// =============================================================================

#[test]
fn init_default_path_writes_in_cwd() {
    let tmp = TempDir::new().expect("tempdir");

    cmd()
        .current_dir(tmp.path())
        .args(["init"])
        .assert()
        .success()
        .stderr(predicate::str::contains("wrote"));

    let path = tmp.path().join("cockpit.toml");
    assert!(path.exists(), "cockpit.toml should be created in cwd");
}
