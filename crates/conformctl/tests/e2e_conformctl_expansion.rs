//! Expanded E2E tests for the `conformctl` binary.
//!
//! Covers: check with valid/invalid receipts, check-dir with --validate-cockpit,
//! missing file handling, malformed JSON, and ordering violations.

use std::fs;

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

fn valid_receipt_json() -> &'static str {
    r#"{
  "schema": "sensor.report.v1",
  "tool": { "name": "test", "version": "1.0.0" },
  "run": { "started_at": "2026-02-01T00:00:00Z" },
  "verdict": { "status": "pass", "counts": { "info": 0, "warn": 0, "error": 0 } },
  "findings": []
}"#
}

fn valid_cockpit_report_json() -> &'static str {
    r#"{
  "schema": "cockpit.report.v1",
  "tool": { "name": "cockpitctl", "version": "0.3.0" },
  "run": { "started_at": "2026-02-01T00:00:00Z" },
  "verdict": { "status": "pass", "counts": { "info": 0, "warn": 0, "error": 0 }, "reasons": [] },
  "sensors": [],
  "highlights": [],
  "policy": {
    "warn_is_fail": false,
    "max_highlights": 10,
    "max_per_sensor_findings": 50,
    "max_annotations": 25,
    "section_order": [],
    "sensors": []
  }
}"#
}

// ─────────────────────────────────────────────────────────────────────────────
// conformctl check: valid receipt with --all
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn check_valid_receipt_all_checks_exit_zero() {
    let tmp = TempDir::new().expect("tempdir");
    let report = tmp.path().join("report.json");
    fs::write(&report, valid_receipt_json()).expect("write");

    cmd()
        .args([
            "check",
            "--report",
            report.to_string_lossy().as_ref(),
            "--sensor-id",
            "test",
            "--all",
        ])
        .assert()
        .success()
        .stderr(predicate::str::contains("PASS"));
}

// ─────────────────────────────────────────────────────────────────────────────
// conformctl check: ordering violation reported
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn check_ordering_violation_reported() {
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

    // info before warn is out of order (canonical: higher severity first)
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
        .failure()
        .stderr(predicate::str::contains("ordering"));
}

// ─────────────────────────────────────────────────────────────────────────────
// conformctl check-dir: validates all receipts
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn check_dir_multiple_sensors_all_valid() {
    let tmp = TempDir::new().expect("tempdir");
    let artifacts = tmp.path().join("artifacts");

    for sensor in &["alpha", "beta", "gamma"] {
        let dir = artifacts.join(sensor);
        fs::create_dir_all(&dir).expect("create dir");
        fs::write(dir.join("report.json"), valid_receipt_json()).expect("write");
    }

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
// conformctl check-dir: --validate-cockpit validates cockpit report
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn check_dir_validate_cockpit_valid() {
    let tmp = TempDir::new().expect("tempdir");
    let artifacts = tmp.path().join("artifacts");

    // Sensor receipt
    let sensor_dir = artifacts.join("test_sensor");
    fs::create_dir_all(&sensor_dir).expect("create sensor dir");
    fs::write(sensor_dir.join("report.json"), valid_receipt_json()).expect("write sensor");

    // Cockpit report
    let cockpit_dir = artifacts.join("cockpit");
    fs::create_dir_all(&cockpit_dir).expect("create cockpit dir");
    fs::write(cockpit_dir.join("report.json"), valid_cockpit_report_json()).expect("write cockpit");

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

#[test]
fn check_dir_validate_cockpit_invalid_schema() {
    let tmp = TempDir::new().expect("tempdir");
    let artifacts = tmp.path().join("artifacts");

    // Cockpit report with invalid content
    let cockpit_dir = artifacts.join("cockpit");
    fs::create_dir_all(&cockpit_dir).expect("create cockpit dir");
    fs::write(
        cockpit_dir.join("report.json"),
        r#"{"schema": "cockpit.report.v1"}"#,
    )
    .expect("write invalid cockpit");

    cmd()
        .args([
            "check-dir",
            "--dir",
            artifacts.to_string_lossy().as_ref(),
            "--all",
            "--validate-cockpit",
        ])
        .assert()
        .failure();
}

// ─────────────────────────────────────────────────────────────────────────────
// conformctl check: missing file → exit 1 with error
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn check_missing_file_exits_one_with_error() {
    cmd()
        .args([
            "check",
            "--report",
            "this_file_does_not_exist.json",
            "--all",
        ])
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("error"));
}

// ─────────────────────────────────────────────────────────────────────────────
// conformctl check: malformed JSON → graceful error
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn check_malformed_json_graceful_error() {
    let tmp = TempDir::new().expect("tempdir");
    let report = tmp.path().join("report.json");
    fs::write(&report, "{ this is not valid JSON !!!").expect("write");

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

// ─────────────────────────────────────────────────────────────────────────────
// conformctl check-dir: one bad receipt fails the whole batch
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn check_dir_one_bad_receipt_fails_batch() {
    let tmp = TempDir::new().expect("tempdir");
    let artifacts = tmp.path().join("artifacts");

    // Valid sensor
    let good_dir = artifacts.join("good");
    fs::create_dir_all(&good_dir).expect("create good dir");
    fs::write(good_dir.join("report.json"), valid_receipt_json()).expect("write good");

    // Invalid sensor (malformed JSON)
    let bad_dir = artifacts.join("bad");
    fs::create_dir_all(&bad_dir).expect("create bad dir");
    fs::write(bad_dir.join("report.json"), "not json").expect("write bad");

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

// ─────────────────────────────────────────────────────────────────────────────
// conformctl check: path hygiene violation via CLI
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn check_path_hygiene_violation_reported() {
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

    cmd()
        .args([
            "check",
            "--report",
            report.to_string_lossy().as_ref(),
            "--path-hygiene",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("path-hygiene"));
}
