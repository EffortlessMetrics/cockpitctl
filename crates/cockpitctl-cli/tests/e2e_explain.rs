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

// =============================================================================
// Explain includes version information via --version flag
// =============================================================================

#[test]
fn explain_binary_includes_version_info() {
    cmd()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("cockpitctl"));
}

// =============================================================================
// Explain output includes schema-related code documentation
// =============================================================================

#[test]
fn explain_schema_violation_includes_schema_reference() {
    let output = cmd()
        .args(["explain", "cockpit.schema_violation"])
        .output()
        .expect("run explain cockpit.schema_violation");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);

    // The description/fix should reference schemas or validation
    assert!(
        stdout.contains("schema") || stdout.contains("Schema"),
        "schema_violation explanation should reference schemas, got:\n{stdout}"
    );
}

// =============================================================================
// Explain output is human-readable: no JSON, structured text
// =============================================================================

#[test]
fn explain_output_is_human_readable_not_json() {
    let output = cmd()
        .args(["explain", "cockpit.missing_receipt"])
        .output()
        .expect("run explain");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should not be JSON
    assert!(
        !stdout.trim_start().starts_with('{'),
        "explain output should be human-readable text, not JSON"
    );

    // Should have labeled fields
    assert!(stdout.contains("Title:"), "missing Title: label");
    assert!(stdout.contains("Fix:"), "missing Fix: label");

    // Each field value should be non-empty (text after the label on same line)
    for label in ["Title:", "Description:", "Cause:", "Fix:"] {
        let line = stdout
            .lines()
            .find(|l| l.contains(label))
            .unwrap_or_else(|| panic!("missing {label} line"));
        let value = line.split(label).nth(1).unwrap_or("").trim();
        assert!(
            !value.is_empty(),
            "{label} field should have a non-empty value, got: {line}"
        );
    }
}

// =============================================================================
// Explain `all` output is deterministic (snapshot)
// =============================================================================

#[test]
fn explain_all_snapshot_stability() {
    let output1 = cmd()
        .args(["explain", "all"])
        .output()
        .expect("run explain all (1)");
    let output2 = cmd()
        .args(["explain", "all"])
        .output()
        .expect("run explain all (2)");

    assert!(output1.status.success());
    assert!(output2.status.success());

    let stdout1 = String::from_utf8_lossy(&output1.stdout);
    let stdout2 = String::from_utf8_lossy(&output2.stdout);

    assert_eq!(
        stdout1, stdout2,
        "explain all output should be deterministic across runs"
    );
}

// =============================================================================
// Explain single-code output is deterministic (snapshot)
// =============================================================================

#[test]
fn explain_single_code_snapshot_stability() {
    let output1 = cmd()
        .args(["explain", "cockpit.invalid_receipt"])
        .output()
        .expect("run explain (1)");
    let output2 = cmd()
        .args(["explain", "cockpit.invalid_receipt"])
        .output()
        .expect("run explain (2)");

    assert!(output1.status.success());
    assert!(output2.status.success());

    let stdout1 = String::from_utf8_lossy(&output1.stdout);
    let stdout2 = String::from_utf8_lossy(&output2.stdout);

    assert_eq!(
        stdout1, stdout2,
        "explain single-code output should be deterministic across runs"
    );
}

// =============================================================================
// Explain all codes are documented with cause and fix
// =============================================================================

#[test]
fn explain_all_codes_have_cause_and_fix_content() {
    let all_output = cmd()
        .args(["explain", "all"])
        .output()
        .expect("run explain all");

    assert!(all_output.status.success());
    let stdout = String::from_utf8_lossy(&all_output.stdout);

    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let code = trimmed.split_whitespace().next().unwrap();

        let detail = cmd()
            .args(["explain", code])
            .output()
            .unwrap_or_else(|_| panic!("explain {code}"));

        assert!(detail.status.success());
        let detail_out = String::from_utf8_lossy(&detail.stdout);

        // Each code must have non-trivial Cause and Fix fields
        let cause_line = detail_out.lines().find(|l| l.contains("Cause:"));
        let fix_line = detail_out.lines().find(|l| l.contains("Fix:"));

        assert!(cause_line.is_some(), "{code} is missing a Cause: field");
        assert!(fix_line.is_some(), "{code} is missing a Fix: field");

        let cause_text = cause_line
            .unwrap()
            .split("Cause:")
            .nth(1)
            .unwrap_or("")
            .trim();
        let fix_text = fix_line.unwrap().split("Fix:").nth(1).unwrap_or("").trim();

        assert!(
            cause_text.len() > 10,
            "{code} Cause: should be a meaningful description, got: '{cause_text}'"
        );
        assert!(
            fix_text.len() > 10,
            "{code} Fix: should be a meaningful description, got: '{fix_text}'"
        );
    }
}

// =============================================================================
// Explain produces no stderr on success
// =============================================================================

#[test]
fn explain_success_produces_no_stderr() {
    let output = cmd()
        .args(["explain", "cockpit.path_traversal"])
        .output()
        .expect("run explain");

    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.is_empty(),
        "successful explain should produce no stderr, got: {stderr}"
    );
}
