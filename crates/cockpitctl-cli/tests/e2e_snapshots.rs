//! Snapshot tests for `cockpitctl` CLI help output stability.
//!
//! Uses insta to snapshot help output so unintended CLI interface changes
//! are caught during review.

use assert_cmd::Command;

fn cmd() -> Command {
    let mut cmd = assert_cmd::cargo_bin_cmd!("cockpitctl");
    if let Ok(profile) = std::env::var("LLVM_PROFILE_FILE") {
        cmd.env("LLVM_PROFILE_FILE", profile);
    }
    cmd
}

/// Strip Windows .exe suffixes so snapshots match on all platforms.
fn normalize(s: &str) -> String {
    s.replace("cockpitctl.exe", "cockpitctl")
}

// ─────────────────────────────────────────────────────────────────────────────
// Snapshot: cockpitctl --help
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn snapshot_cockpitctl_help() {
    let output = cmd().arg("--help").output().expect("run");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    insta::assert_snapshot!("cockpitctl_help", normalize(&stdout));
}

// ─────────────────────────────────────────────────────────────────────────────
// Snapshot: cockpitctl ingest --help
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn snapshot_cockpitctl_ingest_help() {
    let output = cmd().args(["ingest", "--help"]).output().expect("run");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    insta::assert_snapshot!("cockpitctl_ingest_help", normalize(&stdout));
}

// ─────────────────────────────────────────────────────────────────────────────
// Snapshot: cockpitctl init --help
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn snapshot_cockpitctl_init_help() {
    let output = cmd().args(["init", "--help"]).output().expect("run");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    insta::assert_snapshot!("cockpitctl_init_help", normalize(&stdout));
}

// ─────────────────────────────────────────────────────────────────────────────
// Snapshot: cockpitctl validate --help
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn snapshot_cockpitctl_validate_help() {
    let output = cmd().args(["validate", "--help"]).output().expect("run");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    insta::assert_snapshot!("cockpitctl_validate_help", normalize(&stdout));
}

// ─────────────────────────────────────────────────────────────────────────────
// Snapshot: cockpitctl explain --help
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn snapshot_cockpitctl_explain_help() {
    let output = cmd().args(["explain", "--help"]).output().expect("run");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    insta::assert_snapshot!("cockpitctl_explain_help", normalize(&stdout));
}

// ─────────────────────────────────────────────────────────────────────────────
// Snapshot: cockpitctl no-subcommand (error)
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn snapshot_cockpitctl_no_subcommand() {
    let output = cmd().output().expect("run");
    let stderr = String::from_utf8_lossy(&output.stderr);
    insta::assert_snapshot!("cockpitctl_no_subcommand", normalize(&stderr));
}
