//! End-to-end tests for `cockpitctl` CLI help and version output.
//!
//! Verifies that --help, --version, and subcommand help all produce
//! correct output and that unknown commands/flags produce errors.

use assert_cmd::Command;
use predicates::prelude::*;

/// Build a `cockpitctl` command, forwarding the LLVM coverage env if set.
fn cmd() -> Command {
    let mut cmd = assert_cmd::cargo_bin_cmd!("cockpitctl");
    if let Ok(profile) = std::env::var("LLVM_PROFILE_FILE") {
        cmd.env("LLVM_PROFILE_FILE", profile);
    }
    cmd
}

// =============================================================================
// --version
// =============================================================================

#[test]
fn version_flag_shows_version() {
    cmd()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("cockpitctl"));
}

// =============================================================================
// --help lists all subcommands
// =============================================================================

#[test]
fn help_flag_lists_all_subcommands() {
    cmd().arg("--help").assert().success().stdout(
        predicate::str::contains("ingest")
            .and(predicate::str::contains("init"))
            .and(predicate::str::contains("validate"))
            .and(predicate::str::contains("explain")),
    );
}

// =============================================================================
// Subcommand --help: ingest
// =============================================================================

#[test]
fn ingest_help_shows_options() {
    cmd().args(["ingest", "--help"]).assert().success().stdout(
        predicate::str::contains("--artifacts")
            .and(predicate::str::contains("--config"))
            .and(predicate::str::contains("--schema-validation"))
            .and(predicate::str::contains("--label")),
    );
}

// =============================================================================
// Subcommand --help: init
// =============================================================================

#[test]
fn init_help_shows_options() {
    cmd()
        .args(["init", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--path"));
}

// =============================================================================
// Subcommand --help: validate
// =============================================================================

#[test]
fn validate_help_shows_options() {
    cmd()
        .args(["validate", "--help"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("--input")
                .and(predicate::str::contains("--strict"))
                .and(predicate::str::contains("--lax")),
        );
}

// =============================================================================
// Subcommand --help: explain
// =============================================================================

#[test]
fn explain_help_shows_usage() {
    cmd()
        .args(["explain", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("code"));
}

// =============================================================================
// Unknown subcommand → error
// =============================================================================

#[test]
fn unknown_subcommand_exits_with_error() {
    cmd()
        .arg("unknown-command")
        .assert()
        .failure()
        .stderr(predicate::str::contains("error"));
}

// =============================================================================
// Unknown flag on ingest → error
// =============================================================================

#[test]
fn ingest_unknown_flag_exits_with_error() {
    cmd()
        .args(["ingest", "--unknown-flag"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("error"));
}

// =============================================================================
// help subcommand (alternative to --help)
// =============================================================================

#[test]
fn help_subcommand_shows_usage() {
    cmd().arg("help").assert().success().stdout(
        predicate::str::contains("ingest")
            .and(predicate::str::contains("init"))
            .and(predicate::str::contains("validate"))
            .and(predicate::str::contains("explain")),
    );
}

// =============================================================================
// Version output contains semver-like string
// =============================================================================

#[test]
fn version_output_contains_semver() {
    let output = cmd()
        .arg("--version")
        .output()
        .expect("execute cockpitctl --version");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    // Semver: major.minor.patch
    assert!(
        stdout.contains('.'),
        "version output should contain a semver string with dots, got: {stdout}"
    );
}
