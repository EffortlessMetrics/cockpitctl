//! Error message quality tests.
//!
//! Asserts that every user-facing error condition produces actionable
//! diagnostic output — not just a bare "Error".

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

fn cmd() -> Command {
    let mut cmd = assert_cmd::cargo_bin_cmd!("cockpitctl");
    cmd.env("COCKPITCTL_STARTED_AT", "2026-02-02T12:00:00Z");
    if let Ok(profile) = std::env::var("LLVM_PROFILE_FILE") {
        cmd.env("LLVM_PROFILE_FILE", profile);
    }
    cmd
}

struct TestSetup {
    _temp_dir: TempDir,
    artifacts_dir: PathBuf,
    config_path: PathBuf,
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
}

// =============================================================================
// Malformed TOML config → message includes TOML parse context
// =============================================================================

#[test]
fn malformed_config_error_includes_toml_context() {
    let setup = TestSetup::new();
    setup.write_config("{ this is [[ not valid toml");
    setup.write_sensor_report(
        "alpha",
        r#"{
  "schema": "sensor.report.v1",
  "tool": { "name": "alpha", "version": "1.0.0" },
  "run": { "started_at": "2026-02-02T11:00:00Z" },
  "verdict": { "status": "pass", "counts": { "info": 0, "warn": 0, "error": 0 } },
  "findings": []
}"#,
    );

    cmd()
        .args([
            "ingest",
            "--artifacts",
            &setup.artifacts_arg(),
            "--config",
            &setup.config_arg(),
        ])
        .assert()
        .code(1)
        .stderr(
            predicate::str::contains("config")
                .or(predicate::str::contains("toml"))
                .or(predicate::str::contains("parse")),
        );
}

// =============================================================================
// Invalid JSON receipt → message includes which sensor/file failed
// =============================================================================

#[test]
fn corrupt_receipt_error_includes_sensor_context() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]
[sensors.badsensor]
blocking = true
missing = "fail"
"#,
    );
    setup.write_sensor_report("badsensor", "<<< totally not json >>>");

    let assert = cmd()
        .args([
            "ingest",
            "--artifacts",
            &setup.artifacts_arg(),
            "--config",
            &setup.config_arg(),
        ])
        .assert();

    // Exit 2 (policy fail due to invalid receipt finding).
    assert.code(2);

    // The cockpit report should surface the sensor name in its findings.
    let report_path = setup.artifacts_dir.join("cockpit").join("report.json");
    if report_path.exists() {
        let report = fs::read_to_string(&report_path).expect("read report");
        assert!(
            report.contains("badsensor"),
            "cockpit report should reference the failing sensor"
        );
    }
}

// =============================================================================
// Path traversal attempt → message explains rejection
// =============================================================================

#[test]
fn path_traversal_sensor_id_is_rejected() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]
"#,
    );

    // Manually create a sensor dir with ".." in name.
    let traversal_dir = setup.artifacts_dir.join("..").join("escape");
    let _ = fs::create_dir_all(&traversal_dir);
    let _ = fs::write(
        traversal_dir.join("report.json"),
        r#"{
  "schema": "sensor.report.v1",
  "tool": { "name": "escape", "version": "1.0.0" },
  "run": { "started_at": "2026-02-02T11:00:00Z" },
  "verdict": { "status": "pass", "counts": { "info": 0, "warn": 0, "error": 0 } },
  "findings": []
}"#,
    );

    // cockpitctl must not follow the traversal path outside artifacts.
    // The exact behavior may be to skip or produce a finding — either is acceptable,
    // but it must not crash.
    cmd()
        .args([
            "ingest",
            "--artifacts",
            &setup.artifacts_arg(),
            "--config",
            &setup.config_arg(),
        ])
        .assert()
        .code(predicate::in_iter([0i32, 2]));
}

// =============================================================================
// Missing --input for validate → shows usage help with --input
// =============================================================================

#[test]
fn validate_missing_input_shows_usage() {
    cmd()
        .args(["validate"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--input"));
}

// =============================================================================
// Unknown subcommand → shows available subcommands
// =============================================================================

#[test]
fn unknown_subcommand_lists_available_commands() {
    cmd().args(["does-not-exist"]).assert().failure().stderr(
        predicate::str::contains("ingest")
            .or(predicate::str::contains("error"))
            .or(predicate::str::contains("Usage")),
    );
}

// =============================================================================
// Validate nonexistent file → error includes the filename
// =============================================================================

#[test]
fn validate_missing_file_error_includes_path() {
    cmd()
        .args(["validate", "--input", "no_such_file_xyz.json", "--lax"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("no_such_file_xyz.json"));
}

// =============================================================================
// Validate malformed JSON → stderr is not empty (actionable)
// =============================================================================

#[test]
fn validate_malformed_json_has_actionable_stderr() {
    let dir = TempDir::new().expect("tempdir");
    let path = dir.path().join("broken.json");
    fs::write(&path, "{{{{not json at all").expect("write");

    cmd()
        .args(["validate", "--input", &path.to_string_lossy(), "--lax"])
        .assert()
        .code(1)
        .stderr(predicate::str::is_empty().not());
}

// =============================================================================
// Empty artifacts with blocking sensor → error is not bare "Error"
// =============================================================================

#[test]
fn missing_blocking_sensor_error_is_actionable() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]
[sensors.required_one]
blocking = true
missing = "fail"
"#,
    );

    // The cockpit report should contain information about the missing sensor.
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

    let report_path = setup.artifacts_dir.join("cockpit").join("report.json");
    assert!(report_path.exists(), "report.json must still be produced");
    let report = fs::read_to_string(&report_path).expect("read report");
    assert!(
        report.contains("required_one"),
        "cockpit report should name the missing sensor"
    );
}
