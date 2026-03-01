//! End-to-end regression tests for `cockpitctl` CLI error messages and
//! exit code semantics.
//!
//! Covers: no-args behavior, missing required flags, existing-file init,
//! output format consistency, and exit code correctness.

use std::fs;

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

fn cmd() -> Command {
    let mut cmd = assert_cmd::cargo_bin_cmd!("cockpitctl");
    cmd.env("COCKPITCTL_STARTED_AT", "2026-02-02T12:00:00Z");
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

struct TestSetup {
    _temp_dir: TempDir,
    artifacts_dir: std::path::PathBuf,
    config_path: std::path::PathBuf,
}

impl TestSetup {
    fn new() -> Self {
        let temp_dir = TempDir::new().expect("create temp dir");
        let artifacts_dir = temp_dir.path().join("artifacts");
        let config_path = temp_dir.path().join("cockpit.toml");
        fs::create_dir_all(&artifacts_dir).expect("create artifacts dir");
        Self {
            _temp_dir: temp_dir,
            artifacts_dir,
            config_path,
        }
    }

    fn write_config(&self, content: &str) {
        fs::write(&self.config_path, content).expect("write config");
    }

    fn write_sensor_report(&self, sensor_id: &str, content: &str) {
        let sensor_dir = self.artifacts_dir.join(sensor_id);
        fs::create_dir_all(&sensor_dir).expect("create sensor dir");
        fs::write(sensor_dir.join("report.json"), content).expect("write report");
    }

    fn artifacts_arg(&self) -> String {
        self.artifacts_dir.to_string_lossy().to_string()
    }

    fn config_arg(&self) -> String {
        self.config_path.to_string_lossy().to_string()
    }

    fn cockpit_report_path(&self) -> std::path::PathBuf {
        self.artifacts_dir.join("cockpit").join("report.json")
    }

    fn cockpit_comment_path(&self) -> std::path::PathBuf {
        self.artifacts_dir.join("cockpit").join("comment.md")
    }
}

// =============================================================================
// No args → helpful error or help text
// =============================================================================

#[test]
fn no_args_shows_usage() {
    cmd().assert().failure().stderr(
        predicate::str::contains("Usage")
            .or(predicate::str::contains("COMMAND"))
            .or(predicate::str::contains("help")),
    );
}

// =============================================================================
// validate with no --input → clap error mentioning --input
// =============================================================================

#[test]
fn validate_missing_input_shows_error() {
    cmd()
        .args(["validate"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--input"));
}

// =============================================================================
// init with existing file → exit 2 and "refusing to overwrite"
// =============================================================================

#[test]
fn init_existing_file_exits_code_two() {
    let tmp = TempDir::new().expect("tempdir");
    let path = tmp.path().join("cockpit.toml");
    fs::write(&path, "# existing\n").unwrap();

    cmd()
        .args(["init", "--path", &path.to_string_lossy()])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("refusing to overwrite"));
}

// =============================================================================
// Output format: ingest produces JSON report and markdown comment
// =============================================================================

#[test]
fn ingest_produces_json_report_and_markdown_comment() {
    let setup = TestSetup::new();
    setup.write_config("[policy]\n");
    setup.write_sensor_report("sensor1", &valid_sensor_report("sensor1"));

    cmd()
        .args([
            "ingest",
            "--artifacts",
            &setup.artifacts_arg(),
            "--config",
            &setup.config_arg(),
        ])
        .assert()
        .success();

    // report.json must be valid JSON
    let report = fs::read_to_string(setup.cockpit_report_path()).expect("read report");
    let json: serde_json::Value = serde_json::from_str(&report).expect("parse report as JSON");
    assert_eq!(json["schema"], "cockpit.report.v1");

    // comment.md must exist and contain markdown
    let comment = fs::read_to_string(setup.cockpit_comment_path()).expect("read comment");
    assert!(!comment.is_empty(), "comment.md should not be empty");
}

// =============================================================================
// Exit code 0 for passing policy
// =============================================================================

#[test]
fn exit_code_zero_for_pass() {
    let setup = TestSetup::new();
    setup.write_config("[policy]\n");
    setup.write_sensor_report("s", &valid_sensor_report("s"));

    cmd()
        .args([
            "ingest",
            "--artifacts",
            &setup.artifacts_arg(),
            "--config",
            &setup.config_arg(),
        ])
        .assert()
        .code(0);
}

// =============================================================================
// Exit code 2 for policy fail (missing blocking sensor)
// =============================================================================

#[test]
fn exit_code_two_for_policy_fail() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]

[sensors.required]
blocking = true
missing = "fail"
"#,
    );
    // No sensor present → missing required sensor → policy fail

    cmd()
        .args([
            "ingest",
            "--artifacts",
            &setup.artifacts_arg(),
            "--config",
            &setup.config_arg(),
        ])
        .assert()
        .code(2);
}

// =============================================================================
// Exit code 1 for runtime error (malformed config)
// =============================================================================

#[test]
fn exit_code_one_for_runtime_error() {
    let setup = TestSetup::new();
    setup.write_config("{{{{ not valid toml }}}}");

    cmd()
        .args([
            "ingest",
            "--artifacts",
            &setup.artifacts_arg(),
            "--config",
            &setup.config_arg(),
        ])
        .assert()
        .code(1);
}

// =============================================================================
// validate exit code 1 for missing file
// =============================================================================

#[test]
fn validate_missing_file_exits_one() {
    cmd()
        .args(["validate", "--input", "nonexistent.json", "--lax"])
        .assert()
        .code(1);
}

// =============================================================================
// explain unknown code exits 1
// =============================================================================

#[test]
fn explain_unknown_code_exits_one() {
    cmd()
        .args(["explain", "totally.fake.code"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("unknown code"));
}

// =============================================================================
// explain known code exits 0
// =============================================================================

#[test]
fn explain_known_code_exits_zero() {
    cmd()
        .args(["explain", "cockpit.invalid_receipt"])
        .assert()
        .code(0);
}

// =============================================================================
// ingest report contains sensors array
// =============================================================================

#[test]
fn ingest_report_contains_sensors_array() {
    let setup = TestSetup::new();
    setup.write_config("[policy]\n");
    setup.write_sensor_report("alpha", &valid_sensor_report("alpha"));

    cmd()
        .args([
            "ingest",
            "--artifacts",
            &setup.artifacts_arg(),
            "--config",
            &setup.config_arg(),
        ])
        .assert()
        .success();

    let report = fs::read_to_string(setup.cockpit_report_path()).expect("read report");
    let json: serde_json::Value = serde_json::from_str(&report).expect("parse JSON");
    let sensors = json["sensors"].as_array().expect("sensors array");
    assert_eq!(sensors.len(), 1, "should discover exactly one sensor");
}

// =============================================================================
// Conflicting validate flags → clap error
// =============================================================================

#[test]
fn validate_conflicting_strict_and_lax_flags() {
    let tmp = TempDir::new().expect("tempdir");
    let path = tmp.path().join("dummy.json");
    fs::write(&path, "{}").unwrap();

    cmd()
        .args([
            "validate",
            "--input",
            &path.to_string_lossy(),
            "--strict",
            "--lax",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("cannot be used with"));
}
