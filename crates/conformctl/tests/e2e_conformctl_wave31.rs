//! Wave 31 E2E tests for the `conformctl` binary.
//!
//! Covers: unknown severity, binary/text files, exit codes, output format,
//! check-dir edge cases, individual check flags, and error handling.

use std::fs;

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

// Helpers

fn cmd() -> Command {
    let mut cmd = assert_cmd::cargo_bin_cmd!("conformctl");
    if let Ok(profile) = std::env::var("LLVM_PROFILE_FILE") {
        cmd.env("LLVM_PROFILE_FILE", profile);
    }
    cmd
}

fn valid_receipt() -> &'static str {
    r#"{
  "schema": "sensor.report.v1",
  "tool": { "name": "test-sensor", "version": "1.0.0" },
  "run": { "started_at": "2026-01-01T00:00:00Z" },
  "verdict": { "status": "pass", "counts": { "info": 0, "warn": 0, "error": 0 } },
  "findings": []
}"#
}

fn valid_cockpit_report() -> &'static str {
    r#"{
  "schema": "cockpit.report.v1",
  "tool": { "name": "cockpitctl", "version": "0.3.0" },
  "run": { "started_at": "2026-01-01T00:00:00Z" },
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

fn write_report(dir: &TempDir, content: &str) -> std::path::PathBuf {
    let path = dir.path().join("report.json");
    fs::write(&path, content).expect("write report");
    path
}

fn write_sensor(dir: &TempDir, sensor_id: &str, content: &str) {
    let sensor_dir = dir.path().join(sensor_id);
    fs::create_dir_all(&sensor_dir).expect("create sensor dir");
    fs::write(sensor_dir.join("report.json"), content).expect("write sensor report");
}

// 1. Invalid input handling

#[test]
fn check_unknown_severity_fails_schema() {
    let dir = TempDir::new().unwrap();
    let report = r#"{
  "schema": "sensor.report.v1",
  "tool": { "name": "test", "version": "1.0.0" },
  "run": { "started_at": "2026-01-01T00:00:00Z" },
  "verdict": { "status": "fail", "counts": { "info": 0, "warn": 0, "error": 1 } },
  "findings": [{
    "severity": "critical",
    "message": "something bad",
    "location": { "path": "src/main.rs" }
  }]
}"#;
    let path = write_report(&dir, report);

    cmd()
        .args(["check", "--report"])
        .arg(&path)
        .assert()
        .failure()
        .stderr(predicate::str::contains("FAIL"));
}

#[test]
fn check_binary_file_graceful_error() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("report.json");
    fs::write(&path, b"\x00\x01\x02\x03\xff\xfe").unwrap();

    cmd()
        .args(["check", "--report"])
        .arg(&path)
        .assert()
        .failure();
}

#[test]
fn check_plain_text_file_fails() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("report.json");
    fs::write(&path, "this is not JSON at all").unwrap();

    cmd()
        .args(["check", "--report"])
        .arg(&path)
        .assert()
        .failure();
}

#[test]
fn check_sensor_id_with_path_traversal_fails() {
    let dir = TempDir::new().unwrap();
    let path = write_report(&dir, valid_receipt());

    cmd()
        .args(["check", "--report"])
        .arg(&path)
        .args(["--sensor-id", "../escape", "--sensor-id-format"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("FAIL"));
}

#[test]
fn check_sensor_id_with_dotdot_component_fails() {
    let dir = TempDir::new().unwrap();
    let path = write_report(&dir, valid_receipt());

    cmd()
        .args(["check", "--report"])
        .arg(&path)
        .args(["--sensor-id", "foo/../bar", "--sensor-id-format"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("FAIL"));
}

#[test]
fn check_json_array_instead_of_object_fails() {
    let dir = TempDir::new().unwrap();
    let path = write_report(&dir, r#"[{"not": "a receipt"}]"#);

    cmd()
        .args(["check", "--report"])
        .arg(&path)
        .assert()
        .failure();
}

#[test]
fn check_empty_json_object_schema_failure() {
    let dir = TempDir::new().unwrap();
    let path = write_report(&dir, "{}");

    cmd()
        .args(["check", "--report"])
        .arg(&path)
        .args(["--sensor-id", "empty"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("FAIL"));
}

// 2. Exit code verification

#[test]
fn check_valid_receipt_exit_code_zero() {
    let dir = TempDir::new().unwrap();
    let path = write_report(&dir, valid_receipt());

    let assert = cmd()
        .args(["check", "--report"])
        .arg(&path)
        .assert()
        .success();

    assert.code(0);
}

#[test]
fn check_missing_file_exit_code_one() {
    let assert = cmd()
        .args(["check", "--report", "nonexistent_file.json"])
        .assert()
        .failure();

    assert.code(1);
}

#[test]
fn check_schema_violation_exit_code_one() {
    let dir = TempDir::new().unwrap();
    let report = r#"{
  "schema": "sensor.report.v1",
  "tool": { "name": "test", "version": "1.0.0" },
  "run": { "started_at": "2026-01-01T00:00:00Z" },
  "verdict": { "status": "invalid_status", "counts": { "info": 0, "warn": 0, "error": 0 } },
  "findings": []
}"#;
    let path = write_report(&dir, report);

    let assert = cmd()
        .args(["check", "--report"])
        .arg(&path)
        .assert()
        .failure();

    assert.code(1);
}

// 3. Output format assertions

#[test]
fn check_output_shows_conformance_check_header() {
    let dir = TempDir::new().unwrap();
    let path = write_report(&dir, valid_receipt());

    cmd()
        .args(["check", "--report"])
        .arg(&path)
        .assert()
        .success()
        .stderr(predicate::str::contains("conformance check:"));
}

#[test]
fn check_all_flag_outputs_pass_for_each_check() {
    let dir = TempDir::new().unwrap();
    let path = write_report(&dir, valid_receipt());

    cmd()
        .args(["check", "--report"])
        .arg(&path)
        .args(["--all", "--sensor-id", "test-sensor"])
        .assert()
        .success()
        .stderr(predicate::str::contains("schema validation passed"))
        .stderr(predicate::str::contains("PASS"));
}

#[test]
fn check_sensor_id_flag_used_for_ordering() {
    let dir = TempDir::new().unwrap();
    let path = write_report(&dir, valid_receipt());

    cmd()
        .args(["check", "--report"])
        .arg(&path)
        .args(["--ordering", "--sensor-id", "test-sensor"])
        .assert()
        .success()
        .stderr(predicate::str::contains("ordering passed"));
}

#[test]
fn check_dir_output_contains_sensor_headers() {
    let dir = TempDir::new().unwrap();
    write_sensor(&dir, "alpha", valid_receipt());
    write_sensor(&dir, "beta", valid_receipt());

    cmd()
        .args(["check-dir", "--dir"])
        .arg(dir.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("--- sensor: alpha ---"))
        .stderr(predicate::str::contains("--- sensor: beta ---"));
}

#[test]
fn check_empty_findings_passes_all_checks() {
    let dir = TempDir::new().unwrap();
    let path = write_report(&dir, valid_receipt());

    cmd()
        .args(["check", "--report"])
        .arg(&path)
        .args(["--all", "--sensor-id", "test-sensor"])
        .assert()
        .success()
        .stderr(predicate::str::contains("PASS"));
}

// 4. check-dir coverage

#[test]
fn check_dir_nonexistent_exit_code_one() {
    let assert = cmd()
        .args(["check-dir", "--dir", "totally_nonexistent_dir_xyz"])
        .assert()
        .failure();

    assert.code(1);
}

#[test]
fn check_dir_empty_exits_zero_with_pass_message() {
    let dir = TempDir::new().unwrap();

    cmd()
        .args(["check-dir", "--dir"])
        .arg(dir.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("PASS"));
}

#[test]
fn check_dir_mixed_reports_pass_and_fail_in_summary() {
    let dir = TempDir::new().unwrap();
    write_sensor(&dir, "good-sensor", valid_receipt());

    let bad_report = r#"{
  "schema": "sensor.report.v1",
  "tool": { "name": "bad", "version": "1.0.0" },
  "run": { "started_at": "2026-01-01T00:00:00Z" },
  "verdict": { "status": "invalid_verdict", "counts": { "info": 0, "warn": 0, "error": 0 } },
  "findings": []
}"#;
    write_sensor(&dir, "bad-sensor", bad_report);

    cmd()
        .args(["check-dir", "--dir"])
        .arg(dir.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("Summary:"))
        .stderr(predicate::str::contains("PASS"))
        .stderr(predicate::str::contains("FAIL"));
}

#[test]
fn check_dir_validate_cockpit_missing_skips() {
    let dir = TempDir::new().unwrap();
    write_sensor(&dir, "sensor-a", valid_receipt());

    cmd()
        .args(["check-dir", "--dir"])
        .arg(dir.path())
        .args(["--validate-cockpit"])
        .assert()
        .success()
        .stderr(predicate::str::contains(
            "skip: no cockpit/report.json found",
        ));
}

#[test]
fn check_dir_validate_cockpit_valid_exit_zero() {
    let dir = TempDir::new().unwrap();
    write_sensor(&dir, "sensor-a", valid_receipt());

    let cockpit_dir = dir.path().join("cockpit");
    fs::create_dir_all(&cockpit_dir).unwrap();
    fs::write(cockpit_dir.join("report.json"), valid_cockpit_report()).unwrap();

    cmd()
        .args(["check-dir", "--dir"])
        .arg(dir.path())
        .args(["--validate-cockpit"])
        .assert()
        .success()
        .stderr(predicate::str::contains(
            "cockpit report schema validation passed",
        ));
}

#[test]
fn check_dir_validate_cockpit_invalid_exit_nonzero() {
    let dir = TempDir::new().unwrap();
    write_sensor(&dir, "sensor-a", valid_receipt());

    let cockpit_dir = dir.path().join("cockpit");
    fs::create_dir_all(&cockpit_dir).unwrap();
    fs::write(cockpit_dir.join("report.json"), r#"{"not": "valid"}"#).unwrap();

    cmd()
        .args(["check-dir", "--dir"])
        .arg(dir.path())
        .args(["--validate-cockpit"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("FAIL"));
}

#[test]
fn check_dir_skips_cockpit_in_sensor_enumeration() {
    let dir = TempDir::new().unwrap();
    write_sensor(&dir, "cockpit", valid_receipt());
    write_sensor(&dir, "real-sensor", valid_receipt());

    cmd()
        .args(["check-dir", "--dir"])
        .arg(dir.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("--- sensor: real-sensor ---"))
        .stderr(predicate::str::contains("1 sensor(s) checked"));
}

#[test]
fn check_dir_only_files_no_sensors() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("stray-file.txt"), "not a sensor").unwrap();
    fs::write(dir.path().join("another.json"), "{}").unwrap();

    cmd()
        .args(["check-dir", "--dir"])
        .arg(dir.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("PASS"))
        .stderr(predicate::str::contains("0 sensor(s) checked"));
}

#[test]
fn check_dir_single_sensor_reports_one_checked() {
    let dir = TempDir::new().unwrap();
    write_sensor(&dir, "only-sensor", valid_receipt());

    cmd()
        .args(["check-dir", "--dir"])
        .arg(dir.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("1 sensor(s) checked"));
}

#[test]
fn check_dir_many_sensors_one_schema_violation_fails() {
    let dir = TempDir::new().unwrap();
    write_sensor(&dir, "alpha", valid_receipt());
    write_sensor(&dir, "beta", valid_receipt());

    let bad = r#"{"schema": "sensor.report.v1", "tool": {"name": "gamma"}, "findings": []}"#;
    write_sensor(&dir, "gamma", bad);

    cmd()
        .args(["check-dir", "--dir"])
        .arg(dir.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("FAIL"));
}

// 5. Individual check flags

#[test]
fn check_ordering_multiple_out_of_order_findings_fails() {
    let dir = TempDir::new().unwrap();
    let report = r#"{
  "schema": "sensor.report.v1",
  "tool": { "name": "test", "version": "1.0.0" },
  "run": { "started_at": "2026-01-01T00:00:00Z" },
  "verdict": { "status": "fail", "counts": { "info": 0, "warn": 1, "error": 1 } },
  "findings": [
    { "severity": "warn", "message": "low prio", "location": { "path": "a.rs" } },
    { "severity": "error", "message": "high prio", "location": { "path": "b.rs" } }
  ]
}"#;
    let path = write_report(&dir, report);

    cmd()
        .args(["check", "--report"])
        .arg(&path)
        .args(["--ordering", "--sensor-id", "test"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("FAIL"));
}

#[test]
fn check_reason_lint_valid_snake_case_passes() {
    let dir = TempDir::new().unwrap();
    let report = r#"{
  "schema": "sensor.report.v1",
  "tool": { "name": "test", "version": "1.0.0" },
  "run": { "started_at": "2026-01-01T00:00:00Z" },
  "verdict": { "status": "fail", "counts": { "info": 0, "warn": 0, "error": 1 }, "reasons": ["build_failed"] },
  "findings": [
    { "severity": "error", "message": "compile error", "location": { "path": "src/main.rs" }, "code": "E0001" }
  ]
}"#;
    let path = write_report(&dir, report);

    cmd()
        .args(["check", "--report"])
        .arg(&path)
        .args(["--reason-lint"])
        .assert()
        .success()
        .stderr(predicate::str::contains("reason-lint passed"));
}

#[test]
fn check_survivability_fail_no_findings_reported() {
    let dir = TempDir::new().unwrap();
    let report = r#"{
  "schema": "sensor.report.v1",
  "tool": { "name": "test", "version": "1.0.0" },
  "run": { "started_at": "2026-01-01T00:00:00Z" },
  "verdict": { "status": "fail", "counts": { "info": 0, "warn": 0, "error": 0 } },
  "findings": []
}"#;
    let path = write_report(&dir, report);

    cmd()
        .args(["check", "--report"])
        .arg(&path)
        .args(["--survivability"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("FAIL"));
}

// 6. Missing required flags show help

#[test]
fn check_missing_report_flag_shows_help() {
    cmd()
        .args(["check"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--report"));
}

#[test]
fn check_dir_missing_dir_flag_shows_help() {
    cmd()
        .args(["check-dir"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--dir"));
}
