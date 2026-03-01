//! Expanded conformance and schema-sync integration tests for `xtask`.
//!
//! Covers drift detection, flag combinations, invalid inputs, edge cases
//! for conform/conform-dir, and schema-sync round-trips.

use std::fs;
use std::path::Path;

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

fn cmd() -> Command {
    let mut cmd = assert_cmd::cargo_bin_cmd!("xtask");
    if let Ok(profile) = std::env::var("LLVM_PROFILE_FILE") {
        cmd.env("LLVM_PROFILE_FILE", profile);
    }
    cmd
}

fn write_file(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create dirs");
    }
    fs::write(path, content).expect("write file");
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

/// Sensor report with a specific verdict and findings.
fn sensor_report_with_verdict(sensor_name: &str, status: &str, findings: &str) -> String {
    format!(
        r#"{{
  "schema": "{sensor_name}.report.v1",
  "tool": {{ "name": "{sensor_name}", "version": "1.0.0" }},
  "run": {{ "started_at": "2026-02-02T11:00:00Z" }},
  "verdict": {{ "status": "{status}", "counts": {{ "info": 0, "warn": 0, "error": 1 }} }},
  "findings": [{findings}]
}}"#
    )
}

/// Minimal valid cockpit report for validate-cockpit tests.
fn valid_cockpit_report() -> String {
    r#"{
  "schema": "cockpit.report.v1",
  "tool": { "name": "cockpitctl", "version": "0.1.0" },
  "run": { "started_at": "2026-02-02T12:00:00Z" },
  "verdict": { "status": "pass", "counts": { "info": 0, "warn": 0, "error": 0 } },
  "sensors": [],
  "highlights": [],
  "policy": {
    "warn_is_fail": false,
    "max_highlights": 7,
    "max_per_sensor_findings": 20,
    "max_annotations": 25,
    "section_order": [],
    "sensors": []
  }
}"#
    .to_string()
}

// ─────────────────────────────────────────────────────────────────────────────
// Schema sync: round-trip fix → check
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn schema_sync_fix_then_check_roundtrip_all_schemas() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path();

    let schemas = [
        "sensor.report.v1.json",
        "cockpit.report.v1.json",
        "buildfix.plan.v1.json",
        "cockpit.promote.v1.json",
    ];

    // Write canonical versions in contracts/schemas and stale versions in types.
    for name in &schemas {
        let canonical = format!(r#"{{"$id":"{}","title":"{}"}}"#, name, name);
        write_file(&root.join("contracts/schemas").join(name), &canonical);
        write_file(
            &root.join("crates/cockpitctl-types/schemas").join(name),
            "stale",
        );
    }

    // Confirm check detects the drift.
    cmd()
        .current_dir(root)
        .arg("schema-sync-check")
        .assert()
        .failure()
        .stderr(predicate::str::contains("out of sync"));

    // Fix should bring them in sync.
    cmd()
        .current_dir(root)
        .arg("schema-sync-fix")
        .assert()
        .success()
        .stderr(predicate::str::contains("synced"));

    // Now check must pass.
    cmd()
        .current_dir(root)
        .arg("schema-sync-check")
        .assert()
        .success()
        .stderr(predicate::str::contains("in sync"));
}

#[test]
fn schema_sync_check_missing_contracts_dir_fails() {
    let temp = TempDir::new().expect("tempdir");
    // No contracts/schemas/ directory at all.
    cmd()
        .current_dir(temp.path())
        .arg("schema-sync-check")
        .assert()
        .failure();
}

// ─────────────────────────────────────────────────────────────────────────────
// Schema sync: selective drift
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn schema_sync_detects_single_file_drift() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path();

    let schemas = [
        "sensor.report.v1.json",
        "cockpit.report.v1.json",
        "buildfix.plan.v1.json",
        "cockpit.promote.v1.json",
    ];

    // Start all in sync.
    for name in &schemas {
        let content = format!(r#"{{"name":"{}"}}"#, name);
        write_file(&root.join("contracts/schemas").join(name), &content);
        write_file(
            &root.join("crates/cockpitctl-types/schemas").join(name),
            &content,
        );
    }

    // Tamper only the cockpit.report schema.
    write_file(
        &root.join("crates/cockpitctl-types/schemas/cockpit.report.v1.json"),
        r#"{"tampered":true}"#,
    );

    cmd()
        .current_dir(root)
        .arg("schema-sync-check")
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("MISMATCH")
                .and(predicate::str::contains("cockpit.report.v1.json")),
        );
}

// ─────────────────────────────────────────────────────────────────────────────
// Conform: --all flag enables all checks
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn conform_all_flag_runs_all_checks() {
    let temp = TempDir::new().expect("tempdir");
    let report = temp.path().join("report.json");
    write_file(&report, &valid_sensor_report("allcheck"));

    cmd()
        .args(["conform", "--report"])
        .arg(&report)
        .args(["--sensor-id", "allcheck", "--all"])
        .assert()
        .success()
        .stderr(
            predicate::str::contains("PASS")
                .and(predicate::str::contains("schema validation passed")),
        );
}

// ─────────────────────────────────────────────────────────────────────────────
// Conform: sensor-id specific validation
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn conform_with_specific_sensor_id_validates() {
    let temp = TempDir::new().expect("tempdir");
    let report = temp.path().join("report.json");
    write_file(&report, &valid_sensor_report("my_sensor"));

    cmd()
        .args(["conform", "--report"])
        .arg(&report)
        .args(["--sensor-id", "my_sensor", "--sensor-id-format"])
        .assert()
        .success()
        .stderr(predicate::str::contains("sensor-id-format passed"));
}

// ─────────────────────────────────────────────────────────────────────────────
// Conform: survivability check — fail verdict without findings
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn conform_survivability_fails_on_fail_verdict_without_findings() {
    let temp = TempDir::new().expect("tempdir");
    let report = temp.path().join("report.json");

    // fail verdict, zero findings, no reasons → survivability violation
    let content = r#"{
  "schema": "badsensor.report.v1",
  "tool": { "name": "badsensor", "version": "1.0.0" },
  "run": { "started_at": "2026-02-02T11:00:00Z" },
  "verdict": { "status": "fail", "counts": { "info": 0, "warn": 0, "error": 0 } },
  "findings": []
}"#;
    write_file(&report, content);

    cmd()
        .args(["conform", "--report"])
        .arg(&report)
        .args(["--sensor-id", "badsensor", "--survivability"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("survivability"));
}

// ─────────────────────────────────────────────────────────────────────────────
// Conform: path hygiene detects traversal
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn conform_path_hygiene_detects_traversal() {
    let temp = TempDir::new().expect("tempdir");
    let report = temp.path().join("report.json");

    let finding = r#"{
      "severity": "error",
      "code": "E001",
      "message": "bad path",
      "location": { "path": "../../../etc/passwd", "line": 1 }
    }"#;
    write_file(
        &report,
        &sensor_report_with_verdict("pathsensor", "fail", finding),
    );

    cmd()
        .args(["conform", "--report"])
        .arg(&report)
        .args(["--sensor-id", "pathsensor", "--path-hygiene"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("path-hygiene"));
}

// ─────────────────────────────────────────────────────────────────────────────
// Conform: reason lint rejects invalid tokens
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn conform_reason_lint_rejects_invalid_tokens() {
    let temp = TempDir::new().expect("tempdir");
    let report = temp.path().join("report.json");

    // Verdict with invalid reason token (uppercase, spaces).
    let content = r#"{
  "schema": "badsensor.report.v1",
  "tool": { "name": "badsensor", "version": "1.0.0" },
  "run": { "started_at": "2026-02-02T11:00:00Z" },
  "verdict": {
    "status": "warn",
    "counts": { "info": 0, "warn": 1, "error": 0 },
    "reasons": ["INVALID TOKEN!"]
  },
  "findings": []
}"#;
    write_file(&report, content);

    cmd()
        .args(["conform", "--report"])
        .arg(&report)
        .args(["--sensor-id", "badsensor", "--reason-lint"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("reason"));
}

// ─────────────────────────────────────────────────────────────────────────────
// Conform: handles nonexistent report file gracefully
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn conform_handles_missing_report_file() {
    cmd()
        .args([
            "conform",
            "--report",
            "nonexistent_dir/report.json",
            "--sensor-id",
            "ghost",
        ])
        .assert()
        .failure();
}

// ─────────────────────────────────────────────────────────────────────────────
// Conform: sensor-id-format rejects invalid IDs
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn conform_sensor_id_format_rejects_invalid_chars() {
    let temp = TempDir::new().expect("tempdir");
    let report = temp.path().join("report.json");
    write_file(&report, &valid_sensor_report("bad/sensor/../id"));

    cmd()
        .args(["conform", "--report"])
        .arg(&report)
        .args(["--sensor-id", "bad/sensor/../id", "--sensor-id-format"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("sensor-id-format"));
}

// ─────────────────────────────────────────────────────────────────────────────
// Conform-dir: empty directory with no sensor subdirectories
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn conform_dir_empty_directory_passes_with_zero_sensors() {
    let temp = TempDir::new().expect("tempdir");
    // Empty dir with no subdirectories at all.
    cmd()
        .args(["conform-dir", "--dir"])
        .arg(temp.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("0 sensor(s) checked"));
}

// ─────────────────────────────────────────────────────────────────────────────
// Conform-dir: validate-cockpit with valid cockpit report
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn conform_dir_validate_cockpit_with_valid_report_passes() {
    let temp = TempDir::new().expect("tempdir");
    let artifacts = temp.path();

    write_file(
        &artifacts.join("sensor_a/report.json"),
        &valid_sensor_report("sensor_a"),
    );
    write_file(
        &artifacts.join("cockpit/report.json"),
        &valid_cockpit_report(),
    );

    cmd()
        .args(["conform-dir", "--dir"])
        .arg(artifacts)
        .arg("--validate-cockpit")
        .assert()
        .success()
        .stderr(
            predicate::str::contains("cockpit report schema validation passed")
                .and(predicate::str::contains("PASS")),
        );
}

// ─────────────────────────────────────────────────────────────────────────────
// Conform-dir: multiple failures are all reported
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn conform_dir_multiple_failures_reports_all() {
    let temp = TempDir::new().expect("tempdir");
    let artifacts = temp.path();

    // Two sensors with broken JSON.
    write_file(&artifacts.join("bad_a/report.json"), "not json");
    write_file(&artifacts.join("bad_b/report.json"), "{");

    cmd()
        .args(["conform-dir", "--dir"])
        .arg(artifacts)
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("bad_a")
                .and(predicate::str::contains("bad_b"))
                .and(predicate::str::contains("FAIL")),
        );
}

// ─────────────────────────────────────────────────────────────────────────────
// Conform-dir: --all flag exercises all per-report checks
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn conform_dir_all_flag_exercises_all_checks() {
    let temp = TempDir::new().expect("tempdir");
    let artifacts = temp.path();

    write_file(
        &artifacts.join("sensor_x/report.json"),
        &valid_sensor_report("sensor_x"),
    );

    cmd()
        .args(["conform-dir", "--dir"])
        .arg(artifacts)
        .arg("--all")
        .assert()
        .success()
        .stderr(
            predicate::str::contains("PASS").and(predicate::str::contains("1 sensor(s) checked")),
        );
}

// ─────────────────────────────────────────────────────────────────────────────
// Fixtures-help: expected output content
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn fixtures_help_contains_expected_instructions() {
    cmd().arg("fixtures-help").assert().success().stderr(
        predicate::str::contains("Golden fixtures")
            .and(predicate::str::contains("cargo run -p cockpitctl"))
            .and(predicate::str::contains("report.json"))
            .and(predicate::str::contains("comment.md")),
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Conform-dir: mix of passing and failing sensors in summary
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn conform_dir_mixed_pass_and_fail_in_summary() {
    let temp = TempDir::new().expect("tempdir");
    let artifacts = temp.path();

    write_file(
        &artifacts.join("good_sensor/report.json"),
        &valid_sensor_report("good_sensor"),
    );
    write_file(
        &artifacts.join("broken_sensor/report.json"),
        "not valid json",
    );

    cmd()
        .args(["conform-dir", "--dir"])
        .arg(artifacts)
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("Summary")
                .and(predicate::str::contains("PASS"))
                .and(predicate::str::contains("FAIL")),
        );
}
