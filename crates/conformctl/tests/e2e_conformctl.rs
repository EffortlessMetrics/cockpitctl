//! End-to-end tests for the `conformctl` binary.
//!
//! Covers: check valid/invalid reports, check-dir with valid/invalid artifacts.

use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use predicates::prelude::*;
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

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixture_path(name: &str) -> PathBuf {
    workspace_root().join("fixtures").join(name)
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

fn write_temp_report(dir: &TempDir, content: &str) -> PathBuf {
    let path = dir.path().join("report.json");
    fs::write(&path, content).expect("write report");
    path
}

fn copy_dir_recursive(src: &Path, dst: &Path) {
    fs::create_dir_all(dst).expect("create dest dir");
    for entry in fs::read_dir(src).expect("read source dir") {
        let entry = entry.expect("dir entry");
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if src_path.is_dir() {
            if entry.file_name() == "cockpit" {
                continue;
            }
            copy_dir_recursive(&src_path, &dst_path);
        } else {
            fs::copy(&src_path, &dst_path).expect("copy file");
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// conformctl check: valid reports
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn check_valid_report_exits_zero() {
    let tmp = TempDir::new().expect("tempdir");
    let report = write_temp_report(&tmp, &valid_sensor_report("builddiag"));

    cmd()
        .args([
            "check",
            "--report",
            report.to_string_lossy().as_ref(),
            "--sensor-id",
            "builddiag",
            "--all",
        ])
        .assert()
        .success();
}

#[test]
fn check_valid_report_path_hygiene() {
    let tmp = TempDir::new().expect("tempdir");
    let content = r#"{
  "schema": "sensor.report.v1",
  "tool": { "name": "checker", "version": "1.0.0" },
  "run": { "started_at": "2026-02-02T11:00:00Z" },
  "verdict": { "status": "warn", "counts": { "info": 0, "warn": 1, "error": 0 } },
  "findings": [
    {
      "severity": "warn",
      "code": "checker.style",
      "message": "Style issue",
      "location": { "path": "src/main.rs", "line": 10 }
    }
  ]
}"#;
    let report = write_temp_report(&tmp, content);

    cmd()
        .args([
            "check",
            "--report",
            report.to_string_lossy().as_ref(),
            "--path-hygiene",
        ])
        .assert()
        .success();
}

#[test]
fn check_valid_report_ordering() {
    let tmp = TempDir::new().expect("tempdir");
    let content = r#"{
  "schema": "sensor.report.v1",
  "tool": { "name": "linter", "version": "1.0.0" },
  "run": { "started_at": "2026-02-02T11:00:00Z" },
  "verdict": { "status": "warn", "counts": { "info": 0, "warn": 2, "error": 0 } },
  "findings": [
    {
      "severity": "warn",
      "code": "linter.a_first",
      "message": "First finding"
    },
    {
      "severity": "warn",
      "code": "linter.b_second",
      "message": "Second finding"
    }
  ]
}"#;
    let report = write_temp_report(&tmp, content);

    cmd()
        .args([
            "check",
            "--report",
            report.to_string_lossy().as_ref(),
            "--sensor-id",
            "linter",
            "--ordering",
        ])
        .assert()
        .success();
}

// ─────────────────────────────────────────────────────────────────────────────
// conformctl check: invalid reports
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn check_malformed_json_exits_nonzero() {
    let tmp = TempDir::new().expect("tempdir");
    let report = write_temp_report(&tmp, "{ broken json !!!");

    cmd()
        .args([
            "check",
            "--report",
            report.to_string_lossy().as_ref(),
            "--all",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("error"));
}

#[test]
fn check_missing_report_file_exits_nonzero() {
    cmd()
        .args(["check", "--report", "nonexistent_report.json", "--all"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("error"));
}

#[test]
fn check_missing_required_fields_exits_nonzero() {
    let tmp = TempDir::new().expect("tempdir");
    let report = write_temp_report(&tmp, r#"{"tool": {"name": "x", "version": "1.0"}}"#);

    cmd()
        .args([
            "check",
            "--report",
            report.to_string_lossy().as_ref(),
            "--all",
        ])
        .assert()
        .failure();
}

// ─────────────────────────────────────────────────────────────────────────────
// conformctl check: survivability check
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn check_survivability_pass_report() {
    let tmp = TempDir::new().expect("tempdir");
    let report = write_temp_report(&tmp, &valid_sensor_report("alpha"));

    cmd()
        .args([
            "check",
            "--report",
            report.to_string_lossy().as_ref(),
            "--survivability",
        ])
        .assert()
        .success();
}

// ─────────────────────────────────────────────────────────────────────────────
// conformctl check: fixture-based reports
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn check_happy_path_builddiag_report() {
    let report = fixture_path("happy_path")
        .join("artifacts")
        .join("builddiag")
        .join("report.json");

    cmd()
        .args([
            "check",
            "--report",
            report.to_string_lossy().as_ref(),
            "--sensor-id",
            "builddiag",
            "--all",
        ])
        .assert()
        .success();
}

// ─────────────────────────────────────────────────────────────────────────────
// conformctl check-dir: valid artifacts
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn check_dir_happy_path_exits_zero() {
    let tmp = TempDir::new().expect("tempdir");
    let artifacts = tmp.path().join("artifacts");

    let fixture_artifacts = fixture_path("happy_path").join("artifacts");
    copy_dir_recursive(&fixture_artifacts, &artifacts);

    cmd()
        .args([
            "check-dir",
            "--dir",
            artifacts.to_string_lossy().as_ref(),
            "--all",
        ])
        .assert()
        .success();
}

#[test]
fn check_dir_with_synthetic_sensors() {
    let tmp = TempDir::new().expect("tempdir");
    let artifacts = tmp.path().join("artifacts");

    let alpha_dir = artifacts.join("alpha");
    fs::create_dir_all(&alpha_dir).expect("create alpha dir");
    fs::write(alpha_dir.join("report.json"), valid_sensor_report("alpha"))
        .expect("write alpha report");

    let beta_dir = artifacts.join("beta");
    fs::create_dir_all(&beta_dir).expect("create beta dir");
    fs::write(beta_dir.join("report.json"), valid_sensor_report("beta"))
        .expect("write beta report");

    cmd()
        .args([
            "check-dir",
            "--dir",
            artifacts.to_string_lossy().as_ref(),
            "--all",
        ])
        .assert()
        .success();
}

// ─────────────────────────────────────────────────────────────────────────────
// conformctl check-dir: invalid artifacts
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn check_dir_with_corrupt_receipt_exits_nonzero() {
    let tmp = TempDir::new().expect("tempdir");
    let artifacts = tmp.path().join("artifacts");

    let sensor_dir = artifacts.join("bad");
    fs::create_dir_all(&sensor_dir).expect("create sensor dir");
    fs::write(sensor_dir.join("report.json"), "{ broken json").expect("write corrupt report");

    cmd()
        .args([
            "check-dir",
            "--dir",
            artifacts.to_string_lossy().as_ref(),
            "--all",
        ])
        .assert()
        .failure();
}

#[test]
fn check_dir_nonexistent_directory() {
    cmd()
        .args(["check-dir", "--dir", "nonexistent_artifacts_dir", "--all"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("error"));
}

#[test]
fn check_dir_empty_directory_exits_zero() {
    let tmp = TempDir::new().expect("tempdir");
    let artifacts = tmp.path().join("artifacts");
    fs::create_dir_all(&artifacts).expect("create empty artifacts dir");

    cmd()
        .args([
            "check-dir",
            "--dir",
            artifacts.to_string_lossy().as_ref(),
            "--all",
        ])
        .assert()
        .success();
}

// ─────────────────────────────────────────────────────────────────────────────
// conformctl check-dir: --validate-cockpit flag
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn check_dir_validate_cockpit_with_valid_report() {
    let tmp = TempDir::new().expect("tempdir");
    let artifacts = tmp.path().join("artifacts");

    // Create a valid sensor directory
    let alpha_dir = artifacts.join("alpha");
    fs::create_dir_all(&alpha_dir).expect("create alpha dir");
    fs::write(alpha_dir.join("report.json"), valid_sensor_report("alpha"))
        .expect("write alpha report");

    // Write a synthetic cockpit report directly
    let cockpit_dir = artifacts.join("cockpit");
    fs::create_dir_all(&cockpit_dir).expect("create cockpit dir");
    fs::write(
        cockpit_dir.join("report.json"),
        r#"{
  "schema": "cockpit.report.v1",
  "tool": { "name": "cockpitctl", "version": "0.3.0" },
  "run": { "started_at": "2026-02-02T12:00:00Z" },
  "verdict": { "status": "pass", "counts": { "info": 0, "warn": 0, "error": 0 }, "reasons": [] },
  "sensors": [
    {
      "id": "alpha",
      "blocking": true,
      "missing": "fail",
      "presence": "present",
      "report_path": "artifacts/alpha/report.json",
      "verdict": { "status": "pass", "counts": { "info": 0, "warn": 0, "error": 0, "suppressed": 0 }, "reasons": [] }
    }
  ],
  "highlights": [],
  "policy": {
    "warn_is_fail": false,
    "max_highlights": 10,
    "max_per_sensor_findings": 50,
    "max_annotations": 25,
    "section_order": [],
    "sensors": []
  }
}"#,
    )
    .expect("write cockpit report");

    // Now run conformctl check-dir with --validate-cockpit
    cmd()
        .args([
            "check-dir",
            "--dir",
            artifacts.to_string_lossy().as_ref(),
            "--all",
            "--validate-cockpit",
        ])
        .assert()
        .success();
}

// ─────────────────────────────────────────────────────────────────────────────
// conformctl: no subcommand shows help
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn no_subcommand_shows_usage() {
    cmd()
        .assert()
        .failure()
        .stderr(predicate::str::contains("Usage"));
}

#[test]
fn check_no_args_shows_usage() {
    cmd()
        .args(["check"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--report"));
}
