//! Snapshot tests for `conformctl` CLI output format.
//!
//! Uses insta to snapshot stderr output for common scenarios, ensuring
//! output format consistency across changes.

use std::fs;

use assert_cmd::Command;
use tempfile::TempDir;

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

fn cmd() -> Command {
    let mut cmd = assert_cmd::cargo_bin_cmd!("conformctl");
    if let Ok(profile) = std::env::var("LLVM_PROFILE_FILE") {
        cmd.env("LLVM_PROFILE_FILE", profile);
    }
    cmd
}

fn valid_sensor_report(sensor_name: &str) -> String {
    format!(
        r#"{{
  "schema": "{sensor_name}.report.v1",
  "tool": {{ "name": "{sensor_name}", "version": "1.0.0" }},
  "run": {{ "started_at": "2026-02-02T11:00:00Z" }},
  "verdict": {{ "status": "pass", "counts": {{ "info": 0, "warn": 0, "error": 0 }} }},
  "findings": []
}}"#
    )
}

/// Normalize paths in output so snapshots are platform-independent.
fn normalize_output(s: &str) -> String {
    // Replace temp dir paths with a stable placeholder
    let mut result = String::new();
    for line in s.lines() {
        let normalized =
            if line.starts_with("conformance check:") || line.starts_with("conform-dir:") {
                // Replace the path after the colon
                if let Some(prefix_end) = line.find(": ") {
                    format!("{}: <PATH>", &line[..prefix_end])
                } else {
                    line.to_string()
                }
            } else {
                line.to_string()
            };
        result.push_str(&normalized);
        result.push('\n');
    }
    result
}

// ─────────────────────────────────────────────────────────────────────────────
// Snapshot: check valid receipt with --all
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn snapshot_check_valid_receipt_all_checks() {
    let tmp = TempDir::new().expect("tempdir");
    let report = tmp.path().join("report.json");
    fs::write(&report, valid_sensor_report("testsensor")).expect("write");

    let output = cmd()
        .args([
            "check",
            "--report",
            report.to_string_lossy().as_ref(),
            "--sensor-id",
            "testsensor",
            "--all",
        ])
        .output()
        .expect("run");

    let stderr = String::from_utf8_lossy(&output.stderr);
    insta::assert_snapshot!("check_valid_all", normalize_output(&stderr));
}

// ─────────────────────────────────────────────────────────────────────────────
// Snapshot: check schema validation failure
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn snapshot_check_schema_failure() {
    let tmp = TempDir::new().expect("tempdir");
    let report = tmp.path().join("report.json");
    // Missing required fields
    fs::write(
        &report,
        r#"{"schema": "sensor.report.v1", "tool": {"name": "x", "version": "1.0"}}"#,
    )
    .expect("write");

    let output = cmd()
        .args([
            "check",
            "--report",
            report.to_string_lossy().as_ref(),
            "--sensor-id",
            "x",
            "--all",
        ])
        .output()
        .expect("run");

    let stderr = String::from_utf8_lossy(&output.stderr);
    insta::assert_snapshot!("check_schema_failure", normalize_output(&stderr));
}

// ─────────────────────────────────────────────────────────────────────────────
// Snapshot: check malformed JSON error
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn snapshot_check_malformed_json() {
    let tmp = TempDir::new().expect("tempdir");
    let report = tmp.path().join("report.json");
    fs::write(&report, "{ definitely not valid JSON !!!").expect("write");

    let output = cmd()
        .args([
            "check",
            "--report",
            report.to_string_lossy().as_ref(),
            "--all",
        ])
        .output()
        .expect("run");

    let stderr = String::from_utf8_lossy(&output.stderr);
    insta::assert_snapshot!("check_malformed_json", normalize_output(&stderr));
}

// ─────────────────────────────────────────────────────────────────────────────
// Snapshot: check missing file error
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn snapshot_check_missing_file() {
    let output = cmd()
        .args(["check", "--report", "nonexistent_file.json", "--all"])
        .output()
        .expect("run");

    let stderr = String::from_utf8_lossy(&output.stderr);
    insta::assert_snapshot!("check_missing_file", normalize_output(&stderr));
}

// ─────────────────────────────────────────────────────────────────────────────
// Snapshot: check ordering violation
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn snapshot_check_ordering_violation() {
    let tmp = TempDir::new().expect("tempdir");
    let content = r#"{
  "schema": "sensor.report.v1",
  "tool": { "name": "linter", "version": "1.0.0" },
  "run": { "started_at": "2026-02-01T00:00:00Z" },
  "verdict": { "status": "warn", "counts": { "info": 1, "warn": 1, "error": 0 } },
  "findings": [
    { "severity": "info", "code": "I1", "message": "info first" },
    { "severity": "warn", "code": "W1", "message": "warn second" }
  ]
}"#;
    let report = tmp.path().join("report.json");
    fs::write(&report, content).expect("write");

    let output = cmd()
        .args([
            "check",
            "--report",
            report.to_string_lossy().as_ref(),
            "--sensor-id",
            "linter",
            "--ordering",
        ])
        .output()
        .expect("run");

    let stderr = String::from_utf8_lossy(&output.stderr);
    insta::assert_snapshot!("check_ordering_violation", normalize_output(&stderr));
}

// ─────────────────────────────────────────────────────────────────────────────
// Snapshot: check path hygiene violation
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn snapshot_check_path_hygiene_violation() {
    let tmp = TempDir::new().expect("tempdir");
    let content = r#"{
  "schema": "sensor.report.v1",
  "tool": { "name": "scanner", "version": "1.0.0" },
  "run": { "started_at": "2026-02-01T00:00:00Z" },
  "verdict": { "status": "warn", "counts": { "info": 0, "warn": 1, "error": 0 } },
  "findings": [
    { "severity": "warn", "code": "W1", "message": "traversal", "location": { "path": "../../../etc/passwd" } }
  ]
}"#;
    let report = tmp.path().join("report.json");
    fs::write(&report, content).expect("write");

    let output = cmd()
        .args([
            "check",
            "--report",
            report.to_string_lossy().as_ref(),
            "--path-hygiene",
        ])
        .output()
        .expect("run");

    let stderr = String::from_utf8_lossy(&output.stderr);
    insta::assert_snapshot!("check_path_hygiene_violation", normalize_output(&stderr));
}

// ─────────────────────────────────────────────────────────────────────────────
// Snapshot: check-dir summary table
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn snapshot_check_dir_summary_table() {
    let tmp = TempDir::new().expect("tempdir");
    let artifacts = tmp.path().join("artifacts");

    for sensor in &["alpha", "beta"] {
        let dir = artifacts.join(sensor);
        fs::create_dir_all(&dir).expect("create dir");
        fs::write(dir.join("report.json"), valid_sensor_report(sensor)).expect("write");
    }

    let output = cmd()
        .args([
            "check-dir",
            "--dir",
            artifacts.to_string_lossy().as_ref(),
            "--all",
        ])
        .output()
        .expect("run");

    let stderr = String::from_utf8_lossy(&output.stderr);
    // Normalize paths in the conform-dir scanning line
    let normalized = normalize_output(&stderr);
    insta::assert_snapshot!("check_dir_summary_table", normalized);
}

// ─────────────────────────────────────────────────────────────────────────────
// Snapshot: check-dir mixed valid/invalid
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn snapshot_check_dir_mixed_results() {
    let tmp = TempDir::new().expect("tempdir");
    let artifacts = tmp.path().join("artifacts");

    // Valid sensor
    let good_dir = artifacts.join("good");
    fs::create_dir_all(&good_dir).expect("create good dir");
    fs::write(good_dir.join("report.json"), valid_sensor_report("good")).expect("write good");

    // Invalid sensor
    let bad_dir = artifacts.join("bad");
    fs::create_dir_all(&bad_dir).expect("create bad dir");
    fs::write(bad_dir.join("report.json"), "{ broken json").expect("write bad");

    let output = cmd()
        .args([
            "check-dir",
            "--dir",
            artifacts.to_string_lossy().as_ref(),
            "--all",
        ])
        .output()
        .expect("run");

    let stderr = String::from_utf8_lossy(&output.stderr);
    insta::assert_snapshot!("check_dir_mixed_results", normalize_output(&stderr));
}

// ─────────────────────────────────────────────────────────────────────────────
// Snapshot: check-dir empty directory
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn snapshot_check_dir_empty() {
    let tmp = TempDir::new().expect("tempdir");
    let artifacts = tmp.path().join("artifacts");
    fs::create_dir_all(&artifacts).expect("create empty artifacts dir");

    let output = cmd()
        .args([
            "check-dir",
            "--dir",
            artifacts.to_string_lossy().as_ref(),
            "--all",
        ])
        .output()
        .expect("run");

    let stderr = String::from_utf8_lossy(&output.stderr);
    insta::assert_snapshot!("check_dir_empty", normalize_output(&stderr));
}

// ─────────────────────────────────────────────────────────────────────────────
// Snapshot: check reason-lint failure
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn snapshot_check_reason_lint_failure() {
    let tmp = TempDir::new().expect("tempdir");
    let content = r#"{
  "schema": "sensor.report.v1",
  "tool": { "name": "linter", "version": "1.0.0" },
  "run": { "started_at": "2026-02-01T00:00:00Z" },
  "verdict": { "status": "fail", "counts": { "info": 0, "warn": 0, "error": 1 }, "reasons": ["BAD-TOKEN!!"] },
  "findings": [
    { "severity": "error", "code": "E1", "message": "crash" }
  ]
}"#;
    let report = tmp.path().join("report.json");
    fs::write(&report, content).expect("write");

    let output = cmd()
        .args([
            "check",
            "--report",
            report.to_string_lossy().as_ref(),
            "--reason-lint",
        ])
        .output()
        .expect("run");

    let stderr = String::from_utf8_lossy(&output.stderr);
    insta::assert_snapshot!("check_reason_lint_failure", normalize_output(&stderr));
}

// ─────────────────────────────────────────────────────────────────────────────
// Snapshot: no subcommand error
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn snapshot_no_subcommand() {
    let output = cmd().output().expect("run");

    let stderr = String::from_utf8_lossy(&output.stderr);
    insta::assert_snapshot!("no_subcommand", normalize_output(&stderr));
}
