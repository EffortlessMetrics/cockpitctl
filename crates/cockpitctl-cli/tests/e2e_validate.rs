//! End-to-end tests for the `cockpitctl validate` CLI subcommand.
//!
//! These tests exercise the binary via `assert_cmd`, covering:
//! - Valid sensor and cockpit reports
//! - Malformed JSON
//! - Missing required fields
//! - Wrong schema version
//! - Missing file error
//! - Useful output messages

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

/// Build a `cockpitctl` command, forwarding the LLVM coverage env if set.
fn cmd() -> Command {
    let mut cmd = assert_cmd::cargo_bin_cmd!("cockpitctl");
    if let Ok(profile) = std::env::var("LLVM_PROFILE_FILE") {
        cmd.env("LLVM_PROFILE_FILE", profile);
    }
    cmd
}

/// Minimal valid sensor report that conforms to sensor.report.v1.
fn valid_sensor_report() -> &'static str {
    r#"{
  "schema": "sensor.report.v1",
  "tool": { "name": "test-sensor", "version": "1.0.0" },
  "run": { "started_at": "2026-02-02T11:00:00Z" },
  "verdict": { "status": "pass", "counts": { "info": 0, "warn": 0, "error": 0 } },
  "findings": []
}"#
}

/// Minimal valid cockpit report that conforms to cockpit.report.v1.
fn valid_cockpit_report() -> &'static str {
    r#"{
  "schema": "cockpit.report.v1",
  "tool": { "name": "cockpitctl", "version": "0.2.0" },
  "run": { "started_at": "2026-02-01T00:00:00Z" },
  "verdict": { "status": "pass", "counts": { "info": 0, "warn": 0, "error": 0 }, "reasons": [] },
  "sensors": [],
  "highlights": [],
  "policy": {
    "warn_is_fail": false,
    "max_highlights": 10,
    "max_per_sensor_findings": 50,
    "section_order": [],
    "sensors": []
  }
}"#
}

/// Write a temporary file and return the directory (kept alive) and file path.
fn write_temp(name: &str, content: &str) -> (TempDir, String) {
    let dir = TempDir::new().expect("create temp dir");
    let path = dir.path().join(name);
    fs::write(&path, content).expect("write temp file");
    (dir, path.to_string_lossy().to_string())
}

// =============================================================================
// Valid receipt → exit 0
// =============================================================================

#[test]
fn validate_valid_sensor_report_exits_0() {
    let (_dir, path) = write_temp("sensor.json", valid_sensor_report());

    cmd()
        .args(["validate", "--input", &path, "--lax"])
        .assert()
        .success()
        .stderr(predicate::str::contains("ok"));
}

#[test]
fn validate_valid_sensor_report_strict_exits_0() {
    let (_dir, path) = write_temp("sensor.json", valid_sensor_report());

    cmd()
        .args(["validate", "--input", &path, "--strict"])
        .assert()
        .success()
        .stderr(predicate::str::contains("ok"));
}

// =============================================================================
// Invalid receipt: malformed JSON → exit 1
// =============================================================================

#[test]
fn validate_malformed_json_exits_1() {
    let (_dir, path) = write_temp("bad.json", "{ not valid json }");

    cmd()
        .args(["validate", "--input", &path, "--lax"])
        .assert()
        .code(1);
}

#[test]
fn validate_malformed_json_strict_exits_1() {
    let (_dir, path) = write_temp("bad.json", "{ not valid json }");

    cmd()
        .args(["validate", "--input", &path, "--strict"])
        .assert()
        .code(1);
}

// =============================================================================
// Missing required fields → exit 1
// =============================================================================

#[test]
fn validate_missing_required_fields_lax_exits_1() {
    // Valid JSON but missing all receipt-required fields.
    let (_dir, path) = write_temp(
        "incomplete.json",
        r#"{"tool": {"name": "x", "version": "1.0"}}"#,
    );

    cmd()
        .args(["validate", "--input", &path, "--lax"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("did not parse"));
}

#[test]
fn validate_missing_required_fields_strict_exits_1() {
    let (_dir, path) = write_temp(
        "incomplete.json",
        r#"{"tool": {"name": "x", "version": "1.0"}}"#,
    );

    cmd()
        .args(["validate", "--input", &path, "--strict"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("strict validation failed"));
}

// =============================================================================
// Wrong schema version → exit 1 in strict mode
// =============================================================================

#[test]
fn validate_wrong_schema_version_strict_exits_1() {
    // Claims to be cockpit.report.v1 but has a sensor-like body (missing
    // required policy/sensors/highlights fields). The strict validator selects
    // the cockpit schema based on the hint and rejects it.
    let bad = r#"{
  "schema": "cockpit.report.v1",
  "tool": { "name": "test-sensor", "version": "1.0.0" },
  "run": { "started_at": "2026-02-02T11:00:00Z" },
  "verdict": { "status": "pass", "counts": { "info": 0, "warn": 0, "error": 0 } },
  "findings": []
}"#;
    let (_dir, path) = write_temp("wrong_version.json", bad);

    cmd()
        .args(["validate", "--input", &path, "--strict"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("strict validation failed"));
}

// =============================================================================
// Cockpit report validation
// =============================================================================

#[test]
fn validate_cockpit_report_lax_exits_0() {
    let (_dir, path) = write_temp("cockpit.json", valid_cockpit_report());

    cmd()
        .args(["validate", "--input", &path, "--lax"])
        .assert()
        .success()
        .stderr(predicate::str::contains("ok: parsed as"));
}

#[test]
fn validate_cockpit_report_strict_exits_0() {
    let (_dir, path) = write_temp("cockpit.json", valid_cockpit_report());

    cmd()
        .args(["validate", "--input", &path, "--strict"])
        .assert()
        .success()
        .stderr(predicate::str::contains("cockpit.report.v1"));
}

// =============================================================================
// Missing file → exit 1 with useful error
// =============================================================================

#[test]
fn validate_missing_file_exits_1_with_message() {
    cmd()
        .args(["validate", "--input", "does-not-exist.json", "--strict"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("does-not-exist.json"));
}

// =============================================================================
// Output format: messages are useful
// =============================================================================

#[test]
fn validate_strict_success_prints_ok() {
    let (_dir, path) = write_temp("sensor.json", valid_sensor_report());

    cmd()
        .args(["validate", "--input", &path, "--strict"])
        .assert()
        .success()
        .stderr(predicate::str::contains("ok: validated as"));
}

#[test]
fn validate_lax_success_prints_ok() {
    let (_dir, path) = write_temp("sensor.json", valid_sensor_report());

    cmd()
        .args(["validate", "--input", &path, "--lax"])
        .assert()
        .success()
        .stderr(predicate::str::contains("ok: parsed as"));
}

#[test]
fn validate_strict_failure_shows_schema_errors() {
    // Missing "schema" field entirely — strict mode should show what failed.
    let bad = r#"{
  "tool": { "name": "test-sensor", "version": "1.0.0" },
  "run": { "started_at": "2026-02-02T11:00:00Z" },
  "verdict": { "status": "pass", "counts": { "info": 0, "warn": 0, "error": 0 } },
  "findings": []
}"#;
    let (_dir, path) = write_temp("no_schema.json", bad);

    cmd()
        .args(["validate", "--input", &path, "--strict"])
        .assert()
        .code(1)
        .stderr(
            predicate::str::contains("strict validation failed")
                .and(predicate::str::contains("sensor.report.v1")),
        );
}

#[test]
fn validate_default_mode_is_strict() {
    // Without --strict or --lax, default should be strict validation.
    let (_dir, path) = write_temp("sensor.json", valid_sensor_report());

    cmd()
        .args(["validate", "--input", &path])
        .assert()
        .success()
        .stderr(predicate::str::contains("ok: validated as"));
}
