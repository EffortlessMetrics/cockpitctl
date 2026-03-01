//! Expanded E2E tests for the `cockpitctl explain` subcommand.
//!
//! Complements `e2e_explain.rs` with additional coverage:
//! - All remaining known codes individually verified
//! - Consistency: every code from `explain all` is individually explainable
//! - Multiple unknown codes produce correct error messages
//! - Output structure includes all required fields for every code

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
// Known code: cockpit.receipt_inconsistent (not in e2e_explain.rs)
// =============================================================================

#[test]
fn explain_receipt_inconsistent_exits_0_with_all_fields() {
    cmd()
        .args(["explain", "cockpit.receipt_inconsistent"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("cockpit.receipt_inconsistent")
                .and(predicate::str::contains("Title:"))
                .and(predicate::str::contains("Description:"))
                .and(predicate::str::contains("Cause:"))
                .and(predicate::str::contains("Fix:")),
        );
}

// =============================================================================
// Known code: cockpit.sensors_truncated (not in e2e_explain.rs)
// =============================================================================

#[test]
fn explain_sensors_truncated_exits_0_with_all_fields() {
    cmd()
        .args(["explain", "cockpit.sensors_truncated"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("cockpit.sensors_truncated")
                .and(predicate::str::contains("Title:"))
                .and(predicate::str::contains("Description:"))
                .and(predicate::str::contains("Cause:"))
                .and(predicate::str::contains("Fix:")),
        );
}

// =============================================================================
// Each code from `explain all` is individually explainable
// =============================================================================

#[test]
fn every_code_in_explain_all_is_individually_explainable() {
    let all_output = cmd()
        .args(["explain", "all"])
        .output()
        .expect("execute cockpitctl explain all");

    assert!(all_output.status.success());
    let stdout = String::from_utf8_lossy(&all_output.stdout);

    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let code = trimmed.split_whitespace().next().unwrap();
        assert!(
            code.starts_with("cockpit."),
            "expected code prefix, got: {code}"
        );

        // Each code should be individually explainable with exit 0
        cmd()
            .args(["explain", code])
            .assert()
            .success()
            .stdout(predicate::str::contains(code));
    }
}

// =============================================================================
// Unknown codes with various patterns → exit 1
// =============================================================================

#[test]
fn explain_empty_looking_code_exits_1() {
    cmd()
        .args(["explain", "cockpit.does_not_exist"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("unknown code"));
}

#[test]
fn explain_partial_prefix_exits_1() {
    cmd()
        .args(["explain", "cockpit."])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("unknown code"));
}

#[test]
fn explain_random_string_exits_1() {
    cmd()
        .args(["explain", "not_a_real_code_at_all"])
        .assert()
        .code(1)
        .stderr(
            predicate::str::contains("unknown code")
                .and(predicate::str::contains("cockpitctl explain all")),
        );
}

// =============================================================================
// Output structure: all codes produce consistent field layout
// =============================================================================

#[test]
fn all_known_codes_have_consistent_output_fields() {
    let known_codes = [
        "cockpit.missing_receipt",
        "cockpit.invalid_receipt",
        "cockpit.schema_violation",
        "cockpit.receipt_inconsistent",
        "cockpit.sensors_truncated",
        "cockpit.path_traversal",
        "cockpit.receipt_oversized",
    ];

    for code in &known_codes {
        let output = cmd()
            .args(["explain", code])
            .output()
            .unwrap_or_else(|_| panic!("failed to run explain {code}"));

        assert!(output.status.success(), "explain {code} should succeed");
        let stdout = String::from_utf8_lossy(&output.stdout);

        assert!(
            stdout.contains("Title:"),
            "{code} output missing Title field"
        );
        assert!(
            stdout.contains("Description:"),
            "{code} output missing Description field"
        );
        assert!(
            stdout.contains("Cause:"),
            "{code} output missing Cause field"
        );
        assert!(stdout.contains("Fix:"), "{code} output missing Fix field");
    }
}

// =============================================================================
// `explain all` line count matches known code count
// =============================================================================

#[test]
fn explain_all_line_count_equals_known_code_count() {
    let output = cmd()
        .args(["explain", "all"])
        .output()
        .expect("execute cockpitctl explain all");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let code_lines: Vec<&str> = stdout.lines().filter(|l| !l.trim().is_empty()).collect();

    // There are exactly 7 known codes
    assert_eq!(
        code_lines.len(),
        7,
        "explain all should list exactly 7 codes, got: {code_lines:?}"
    );
}
