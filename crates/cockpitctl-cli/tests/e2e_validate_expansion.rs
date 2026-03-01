//! Expanded E2E tests for `cockpitctl validate`.
//!
//! Covers: valid sensor/cockpit reports, malformed JSON, missing fields,
//! nonexistent file, missing --input, empty file, and large valid receipt.

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

fn cmd() -> Command {
    let mut cmd = assert_cmd::cargo_bin_cmd!("cockpitctl");
    if let Ok(profile) = std::env::var("LLVM_PROFILE_FILE") {
        cmd.env("LLVM_PROFILE_FILE", profile);
    }
    cmd
}

fn valid_sensor_report() -> &'static str {
    r#"{
  "schema": "sensor.report.v1",
  "tool": { "name": "test-sensor", "version": "1.0.0" },
  "run": { "started_at": "2026-02-02T11:00:00Z" },
  "verdict": { "status": "pass", "counts": { "info": 0, "warn": 0, "error": 0 } },
  "findings": []
}"#
}

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

fn write_temp(name: &str, content: &str) -> (TempDir, String) {
    let dir = TempDir::new().expect("create temp dir");
    let path = dir.path().join(name);
    fs::write(&path, content).expect("write temp file");
    (dir, path.to_string_lossy().to_string())
}

// =============================================================================
// Valid sensor receipt → exit 0
// =============================================================================

#[test]
fn validate_valid_sensor_receipt_lax_ok() {
    let (_dir, path) = write_temp("sensor.json", valid_sensor_report());

    cmd()
        .args(["validate", "--input", &path, "--lax"])
        .assert()
        .success()
        .stderr(predicate::str::contains("ok"));
}

// =============================================================================
// Valid cockpit report → exit 0
// =============================================================================

#[test]
fn validate_valid_cockpit_report_lax_ok() {
    let (_dir, path) = write_temp("cockpit.json", valid_cockpit_report());

    cmd()
        .args(["validate", "--input", &path, "--lax"])
        .assert()
        .success()
        .stderr(predicate::str::contains("ok"));
}

// =============================================================================
// Malformed JSON → exit 1 with parse-related message
// =============================================================================

#[test]
fn validate_malformed_json_exits_1() {
    let (_dir, path) = write_temp("bad.json", "{ this is not json! }");

    cmd()
        .args(["validate", "--input", &path, "--lax"])
        .assert()
        .code(1)
        .stderr(predicate::str::is_empty().not());
}

// =============================================================================
// Missing required fields → exit 1 with field info
// =============================================================================

#[test]
fn validate_missing_required_fields_exits_1() {
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

// =============================================================================
// Nonexistent file → exit 1 with file reference
// =============================================================================

#[test]
fn validate_nonexistent_file_exits_1() {
    cmd()
        .args(["validate", "--input", "ghost_file_404.json", "--lax"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("ghost_file_404.json"));
}

// =============================================================================
// Missing --input → usage error
// =============================================================================

#[test]
fn validate_without_input_shows_usage() {
    cmd()
        .args(["validate"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--input"));
}

// =============================================================================
// Empty file → graceful error (not panic)
// =============================================================================

#[test]
fn validate_empty_file_exits_1() {
    let (_dir, path) = write_temp("empty.json", "");

    cmd()
        .args(["validate", "--input", &path, "--lax"])
        .assert()
        .code(1)
        .stderr(predicate::str::is_empty().not());
}

// =============================================================================
// Large valid receipt → succeeds
// =============================================================================

#[test]
fn validate_large_valid_receipt_succeeds() {
    // Build a receipt with many findings.
    let mut findings = Vec::new();
    for i in 0..200 {
        findings.push(format!(
            r#"{{ "severity": "info", "code": "test.finding.{i}", "message": "Finding number {i}" }}"#
        ));
    }
    let json = format!(
        r#"{{
  "schema": "sensor.report.v1",
  "tool": {{ "name": "big-sensor", "version": "1.0.0" }},
  "run": {{ "started_at": "2026-02-02T11:00:00Z" }},
  "verdict": {{ "status": "pass", "counts": {{ "info": 200, "warn": 0, "error": 0 }} }},
  "findings": [{}]
}}"#,
        findings.join(",\n")
    );

    let (_dir, path) = write_temp("large.json", &json);

    cmd()
        .args(["validate", "--input", &path, "--lax"])
        .assert()
        .success()
        .stderr(predicate::str::contains("ok"));
}
