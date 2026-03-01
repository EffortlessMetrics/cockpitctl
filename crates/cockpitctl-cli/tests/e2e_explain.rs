//! End-to-end tests for the `cockpitctl explain` CLI subcommand.
//!
//! These tests exercise the binary via `assert_cmd`, covering:
//! - Known finding codes (single-code lookup)
//! - Unknown finding codes (graceful error)
//! - The `all` pseudo-code (list every known code)
//! - Missing argument (usage error from clap)
//! - Output format checks (human-readable fields)

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
// Known code: cockpit.invalid_receipt
// =============================================================================

#[test]
fn explain_invalid_receipt_exits_0_with_description() {
    cmd()
        .args(["explain", "cockpit.invalid_receipt"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("cockpit.invalid_receipt")
                .and(predicate::str::contains("Title:"))
                .and(predicate::str::contains("Description:"))
                .and(predicate::str::contains("Cause:"))
                .and(predicate::str::contains("Fix:")),
        );
}

// =============================================================================
// Known code: cockpit.schema_violation
// =============================================================================

#[test]
fn explain_schema_violation_exits_0() {
    cmd()
        .args(["explain", "cockpit.schema_violation"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("cockpit.schema_violation")
                .and(predicate::str::contains("Schema Violation")),
        );
}

// =============================================================================
// Known code: cockpit.missing_receipt
// =============================================================================

#[test]
fn explain_missing_receipt_exits_0() {
    cmd()
        .args(["explain", "cockpit.missing_receipt"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("cockpit.missing_receipt")
                .and(predicate::str::contains("Missing Receipt")),
        );
}

// =============================================================================
// Known code: cockpit.path_traversal
// =============================================================================

#[test]
fn explain_path_traversal_exits_0() {
    cmd()
        .args(["explain", "cockpit.path_traversal"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("cockpit.path_traversal")
                .and(predicate::str::contains("Path Traversal")),
        );
}

// =============================================================================
// Known code: cockpit.receipt_oversized
// =============================================================================

#[test]
fn explain_receipt_oversized_exits_0() {
    cmd()
        .args(["explain", "cockpit.receipt_oversized"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("cockpit.receipt_oversized")
                .and(predicate::str::contains("Receipt Oversized")),
        );
}

// =============================================================================
// Unknown code → exit 1 with helpful message
// =============================================================================

#[test]
fn explain_unknown_code_exits_1() {
    cmd()
        .args(["explain", "nonexistent_code"])
        .assert()
        .code(1)
        .stderr(
            predicate::str::contains("unknown code: nonexistent_code")
                .and(predicate::str::contains("cockpitctl explain all")),
        );
}

// =============================================================================
// No argument → clap usage error (exit 2)
// =============================================================================

#[test]
fn explain_no_argument_exits_with_usage_error() {
    cmd()
        .args(["explain"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Usage"));
}

// =============================================================================
// List all codes via `explain all`
// =============================================================================

#[test]
fn explain_all_lists_every_known_code() {
    cmd().args(["explain", "all"]).assert().success().stdout(
        predicate::str::contains("cockpit.missing_receipt")
            .and(predicate::str::contains("cockpit.invalid_receipt"))
            .and(predicate::str::contains("cockpit.schema_violation"))
            .and(predicate::str::contains("cockpit.receipt_inconsistent"))
            .and(predicate::str::contains("cockpit.sensors_truncated"))
            .and(predicate::str::contains("cockpit.path_traversal"))
            .and(predicate::str::contains("cockpit.receipt_oversized")),
    );
}

// =============================================================================
// Output format: human-readable fields present
// =============================================================================

#[test]
fn explain_output_has_all_fields() {
    cmd()
        .args(["explain", "cockpit.receipt_inconsistent"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Title:")
                .and(predicate::str::contains("Description:"))
                .and(predicate::str::contains("Cause:"))
                .and(predicate::str::contains("Fix:")),
        );
}

#[test]
fn explain_all_output_is_tabular() {
    let output = cmd()
        .args(["explain", "all"])
        .output()
        .expect("execute cockpitctl explain all");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Each line should contain a code and a title separated by whitespace.
    for line in stdout.lines() {
        if line.trim().is_empty() {
            continue;
        }
        assert!(
            line.starts_with("cockpit."),
            "each line in 'explain all' should start with a code prefix, got: {line}"
        );
    }
}
