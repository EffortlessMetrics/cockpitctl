//! End-to-end tests for the `cockpitctl ingest` CLI command.
//!
//! These tests exercise the binary through `assert_cmd`, verifying exit codes,
//! output files, and error messages for various scenarios.

use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use predicates::str::contains;
use tempfile::TempDir;

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Build a `cockpitctl` command with deterministic timestamp.
fn cmd() -> Command {
    let mut cmd = assert_cmd::cargo_bin_cmd!("cockpitctl");
    cmd.env("COCKPITCTL_STARTED_AT", "2026-02-02T12:00:00Z");
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

/// Minimal valid sensor report.
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

/// Sensor report with a failing verdict.
fn fail_sensor_report(sensor_name: &str) -> String {
    format!(
        r#"{{
  "schema": "{sensor_name}.report.v1",
  "tool": {{ "name": "{sensor_name}", "version": "1.0.0" }},
  "run": {{ "started_at": "2026-02-02T11:00:00Z" }},
  "verdict": {{ "status": "fail", "counts": {{ "info": 0, "warn": 0, "error": 1 }} }},
  "findings": [
    {{
      "severity": "error",
      "code": "{sensor_name}.hard_error",
      "message": "A blocking failure was detected"
    }}
  ]
}}"#
    )
}

/// Helper to set up a temp directory with artifacts and config.
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

    fn cockpit_report_path(&self) -> PathBuf {
        self.artifacts_dir.join("cockpit").join("report.json")
    }

    fn cockpit_comment_path(&self) -> PathBuf {
        self.artifacts_dir.join("cockpit").join("comment.md")
    }

    fn read_cockpit_report(&self) -> String {
        let path = self.cockpit_report_path();
        fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read cockpit report at {:?}: {}", path, e))
    }

    fn artifacts_arg(&self) -> String {
        self.artifacts_dir.to_string_lossy().to_string()
    }

    fn config_arg(&self) -> String {
        self.config_path.to_string_lossy().to_string()
    }
}

/// Copy a fixture's artifacts directory into a TestSetup (avoids mutating repo fixtures).
fn setup_from_fixture(fixture_name: &str) -> TestSetup {
    let setup = TestSetup::new();
    let fixture = fixture_path(fixture_name);

    // Copy cockpit.toml
    let config_src = fixture.join("cockpit.toml");
    if config_src.exists() {
        fs::copy(&config_src, &setup.config_path).expect("copy cockpit.toml");
    }

    // Recursively copy artifacts (except cockpit output dir)
    let src_artifacts = fixture.join("artifacts");
    if src_artifacts.exists() {
        copy_dir_recursive(&src_artifacts, &setup.artifacts_dir);
    }

    setup
}

fn copy_dir_recursive(src: &Path, dst: &Path) {
    fs::create_dir_all(dst).expect("create dest dir");
    for entry in fs::read_dir(src).expect("read source dir") {
        let entry = entry.expect("dir entry");
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if src_path.is_dir() {
            // Skip the cockpit output dir from previous runs
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
// Happy path: valid fixture → exit 0
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn happy_path_exits_zero() {
    let setup = setup_from_fixture("happy_path");

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
}

// ─────────────────────────────────────────────────────────────────────────────
// Policy fail: blocking sensor fails → exit 2
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn policy_fail_exits_two() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]
warn_is_fail = false

[sensors.blocker]
blocking = true
missing = "fail"
"#,
    );
    setup.write_sensor_report("blocker", &fail_sensor_report("blocker"));

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

// ─────────────────────────────────────────────────────────────────────────────
// Missing artifacts directory → exit 1 with error message
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn missing_artifacts_dir_exits_with_error() {
    let temp_dir = TempDir::new().expect("create temp dir");
    let nonexistent = temp_dir.path().join("does_not_exist");
    let config = temp_dir.path().join("cockpit.toml");
    fs::write(
        &config,
        r#"[policy]
[sensors.alpha]
blocking = true
missing = "fail"
"#,
    )
    .unwrap();

    // When artifacts dir doesn't exist, cockpitctl treats it as empty discovery.
    // With a blocking sensor configured as missing = "fail", this triggers exit 2.
    cmd()
        .args([
            "ingest",
            "--artifacts",
            nonexistent.to_string_lossy().as_ref(),
            "--config",
            config.to_string_lossy().as_ref(),
        ])
        .assert()
        .code(2);
}

// ─────────────────────────────────────────────────────────────────────────────
// Missing config: no cockpit.toml → uses defaults
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn missing_config_uses_defaults() {
    let setup = TestSetup::new();
    // Write a sensor report but no cockpit.toml
    setup.write_sensor_report("somesensor", &valid_sensor_report("somesensor"));

    let nonexistent_config = setup._temp_dir.path().join("nonexistent_cockpit.toml");

    // Default config has no sensors declared, so all discovered sensors are accepted.
    cmd()
        .args([
            "ingest",
            "--artifacts",
            &setup.artifacts_arg(),
            "--config",
            nonexistent_config.to_string_lossy().as_ref(),
        ])
        .assert()
        .success();
}

// ─────────────────────────────────────────────────────────────────────────────
// Empty artifacts: no sensors → completes
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn empty_artifacts_dir_completes() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]
"#,
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
        .success();

    // Cockpit output should still be written
    assert!(
        setup.cockpit_report_path().exists(),
        "report.json should be created even with empty artifacts"
    );
    assert!(
        setup.cockpit_comment_path().exists(),
        "comment.md should be created even with empty artifacts"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Multiple sensors: mixed_verdicts fixture
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn multiple_sensors_mixed_verdicts() {
    let setup = setup_from_fixture("mixed_verdicts");

    cmd()
        .args([
            "ingest",
            "--artifacts",
            &setup.artifacts_arg(),
            "--config",
            &setup.config_arg(),
        ])
        .assert()
        .code(2); // Contains a failing blocking sensor

    let report = setup.read_cockpit_report();
    let json: serde_json::Value = serde_json::from_str(&report).expect("parse cockpit report");

    // Verify multiple sensors are present in the output
    let sensors = json["sensors"].as_array().expect("sensors array");
    assert!(
        sensors.len() >= 3,
        "mixed_verdicts should have at least 3 sensors, got {}",
        sensors.len()
    );

    // Overall verdict should be fail (has a blocking sensor that fails)
    assert_eq!(
        json["verdict"]["status"].as_str(),
        Some("fail"),
        "overall verdict should be fail"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Output file verification: report.json and comment.md are created
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn output_files_are_created() {
    let setup = setup_from_fixture("happy_path");

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

    assert!(
        setup.cockpit_report_path().exists(),
        "artifacts/cockpit/report.json should be created"
    );
    assert!(
        setup.cockpit_comment_path().exists(),
        "artifacts/cockpit/comment.md should be created"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Output format: report.json is valid JSON with expected top-level fields
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn report_json_is_valid_and_has_required_fields() {
    let setup = setup_from_fixture("happy_path");

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

    let report = setup.read_cockpit_report();
    let json: serde_json::Value =
        serde_json::from_str(&report).expect("report.json must be valid JSON");

    // Check required top-level fields per cockpit.report.v1
    assert_eq!(
        json["schema"].as_str(),
        Some("cockpit.report.v1"),
        "schema field must be cockpit.report.v1"
    );
    assert!(json["tool"].is_object(), "tool must be present");
    assert!(json["run"].is_object(), "run must be present");
    assert!(json["verdict"].is_object(), "verdict must be present");
    assert!(json["sensors"].is_array(), "sensors must be an array");
    assert!(json["highlights"].is_array(), "highlights must be an array");
    assert!(json["policy"].is_object(), "policy must be present");

    // Verdict must have status and counts
    assert!(
        json["verdict"]["status"].is_string(),
        "verdict.status must be a string"
    );
    assert!(
        json["verdict"]["counts"].is_object(),
        "verdict.counts must be present"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Schema validation modes: --schema-validation strict
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn schema_validation_strict_with_valid_receipt() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]

[sensors.alpha]
blocking = true
missing = "fail"
"#,
    );
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
            "--schema-validation",
            "strict",
        ])
        .assert()
        .success();
}

// ─────────────────────────────────────────────────────────────────────────────
// Schema validation modes: --schema-validation lax skips validation
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn schema_validation_lax_accepts_extra_fields() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]

[sensors.alpha]
blocking = true
missing = "fail"
"#,
    );
    // Extra field that would fail strict but serde allows it
    setup.write_sensor_report(
        "alpha",
        r#"{
  "schema": "sensor.report.v1",
  "tool": { "name": "alpha", "version": "1.0.0" },
  "run": { "started_at": "2026-02-02T11:00:00Z" },
  "verdict": { "status": "pass", "counts": { "info": 0, "warn": 0, "error": 0 } },
  "findings": [],
  "extra_not_allowed": "this violates additionalProperties"
}"#,
    );

    cmd()
        .args([
            "ingest",
            "--artifacts",
            &setup.artifacts_arg(),
            "--config",
            &setup.config_arg(),
            "--schema-validation",
            "lax",
        ])
        .assert()
        .success();

    let report = setup.read_cockpit_report();
    let json: serde_json::Value = serde_json::from_str(&report).expect("parse report");

    // No schema violations in lax mode
    let highlights = json["highlights"].as_array().expect("highlights array");
    let has_violation = highlights
        .iter()
        .any(|h| h["finding"]["code"].as_str() == Some("cockpit.schema_violation"));
    assert!(
        !has_violation,
        "lax mode should not produce SCHEMA_VIOLATION findings"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Warn-as-fail: warn_is_fail = true + warn receipt → exit 2
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn warn_as_fail_exits_two() {
    let setup = setup_from_fixture("warn_as_fail");

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

    let report = setup.read_cockpit_report();
    let json: serde_json::Value = serde_json::from_str(&report).expect("parse report");

    assert_eq!(
        json["verdict"]["status"].as_str(),
        Some("fail"),
        "warn_is_fail should escalate warn to fail"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Policy fail outputs: report.json and comment.md are created even on exit 2
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn output_files_created_on_policy_fail() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]

[sensors.blocker]
blocking = true
missing = "fail"
"#,
    );
    setup.write_sensor_report("blocker", &fail_sensor_report("blocker"));

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

    // Outputs are always written, even on policy failure
    assert!(
        setup.cockpit_report_path().exists(),
        "report.json should be created on policy fail"
    );
    assert!(
        setup.cockpit_comment_path().exists(),
        "comment.md should be created on policy fail"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Comment.md contains expected markers
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn comment_md_contains_cockpit_header() {
    let setup = setup_from_fixture("happy_path");

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

    let comment = fs::read_to_string(setup.cockpit_comment_path()).expect("read comment.md");
    assert!(
        comment.contains("Cockpit"),
        "comment.md should contain 'Cockpit' header"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Multiple sensors: synthetic multi-sensor with mixed verdicts
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn multi_sensor_synthetic_reports_all_sensors() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]
warn_is_fail = false

[sensors.alpha]
blocking = true
missing = "fail"

[sensors.beta]
blocking = false
missing = "skip"

[sensors.gamma]
blocking = true
missing = "fail"
"#,
    );

    setup.write_sensor_report("alpha", &valid_sensor_report("alpha"));
    setup.write_sensor_report("beta", &valid_sensor_report("beta"));
    setup.write_sensor_report("gamma", &valid_sensor_report("gamma"));

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

    let report = setup.read_cockpit_report();
    let json: serde_json::Value = serde_json::from_str(&report).expect("parse report");

    let sensors = json["sensors"].as_array().expect("sensors array");
    let sensor_ids: Vec<&str> = sensors.iter().filter_map(|s| s["id"].as_str()).collect();

    assert!(sensor_ids.contains(&"alpha"), "should contain alpha");
    assert!(sensor_ids.contains(&"beta"), "should contain beta");
    assert!(sensor_ids.contains(&"gamma"), "should contain gamma");
}

// ─────────────────────────────────────────────────────────────────────────────
// Stderr output on runtime error (invalid CLI args)
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn invalid_subcommand_shows_usage() {
    let mut c = cmd();
    c.arg("nonexistent-subcommand");
    c.assert().failure().stderr(contains("error"));
}
