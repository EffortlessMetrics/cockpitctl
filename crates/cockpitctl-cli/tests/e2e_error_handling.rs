//! End-to-end tests for CLI error handling paths.
//!
//! Covers: missing artifacts directory, missing/invalid config files,
//! empty artifacts, and corrupt receipt files.

use std::fs;
use std::path::PathBuf;

use assert_cmd::Command;
use predicates::prelude::*;
use predicates::str::contains;
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

    fn artifacts_arg(&self) -> String {
        self.artifacts_dir.to_string_lossy().to_string()
    }

    fn config_arg(&self) -> String {
        self.config_path.to_string_lossy().to_string()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Missing artifacts directory
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn missing_artifacts_dir_with_blocking_sensor() {
    let temp_dir = TempDir::new().expect("create temp dir");
    let nonexistent = temp_dir.path().join("does_not_exist");
    let config = temp_dir.path().join("cockpit.toml");
    fs::write(
        &config,
        r#"[policy]

[sensors.required]
blocking = true
missing = "fail"
"#,
    )
    .unwrap();

    // Missing artifacts dir with a blocking sensor configured as missing = "fail"
    // triggers exit 2 (policy fail, not runtime error).
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

#[test]
fn missing_artifacts_dir_no_blocking_sensors() {
    let temp_dir = TempDir::new().expect("create temp dir");
    let nonexistent = temp_dir.path().join("does_not_exist");
    let config = temp_dir.path().join("cockpit.toml");
    fs::write(
        &config,
        r#"[policy]

[sensors.optional]
blocking = false
missing = "skip"
"#,
    )
    .unwrap();

    // Non-blocking sensor with skip policy should succeed even with missing dir
    cmd()
        .args([
            "ingest",
            "--artifacts",
            nonexistent.to_string_lossy().as_ref(),
            "--config",
            config.to_string_lossy().as_ref(),
        ])
        .assert()
        .success();
}

// ─────────────────────────────────────────────────────────────────────────────
// Missing config file
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn missing_config_falls_back_to_defaults() {
    let setup = TestSetup::new();
    setup.write_sensor_report("somesensor", &valid_sensor_report("somesensor"));

    let nonexistent_config = setup._temp_dir.path().join("nonexistent_cockpit.toml");

    // With no config, defaults are used — no declared sensors, all discovered pass through.
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

#[test]
fn missing_config_still_produces_outputs() {
    let setup = TestSetup::new();
    setup.write_sensor_report("somesensor", &valid_sensor_report("somesensor"));

    let nonexistent_config = setup._temp_dir.path().join("nonexistent_cockpit.toml");

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

    assert!(
        setup.cockpit_report_path().exists(),
        "report.json should be created even without config"
    );
    assert!(
        setup.cockpit_comment_path().exists(),
        "comment.md should be created even without config"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Invalid config file (malformed TOML)
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn malformed_toml_config_exits_with_error() {
    let setup = TestSetup::new();
    setup.write_config("{ this is not valid toml [[[");
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
        .code(1)
        .stderr(contains("config").or(contains("toml").or(contains("parse"))));
}

#[test]
fn empty_config_file_uses_defaults() {
    let setup = TestSetup::new();
    setup.write_config("");
    setup.write_sensor_report("sensor1", &valid_sensor_report("sensor1"));

    // An empty config is valid TOML (empty document) — defaults should apply.
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
// Empty artifacts directory
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn empty_artifacts_with_no_required_sensors() {
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

    assert!(
        setup.cockpit_report_path().exists(),
        "report.json should be created with empty artifacts"
    );
    assert!(
        setup.cockpit_comment_path().exists(),
        "comment.md should be created with empty artifacts"
    );
}

#[test]
fn empty_artifacts_with_required_sensors_exits_two() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]

[sensors.required]
blocking = true
missing = "fail"
"#,
    );

    // No sensor reports present → blocking sensor is missing → exit 2
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

#[test]
fn empty_artifacts_report_has_zero_sensors() {
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

    let report = fs::read_to_string(setup.cockpit_report_path()).expect("read cockpit report.json");
    let json: serde_json::Value = serde_json::from_str(&report).expect("parse report");

    let sensors = json["sensors"].as_array().expect("sensors array");
    assert_eq!(
        sensors.len(),
        0,
        "empty artifacts should produce zero sensors"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Corrupt receipt files
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn corrupt_receipt_json_is_handled_gracefully() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]
warn_is_fail = false

[sensors.broken]
blocking = true
missing = "fail"
"#,
    );
    setup.write_sensor_report("broken", "{ broken json !!!");

    // Corrupt receipt should not cause a crash — cockpitctl handles it as a finding
    let assert = cmd()
        .args([
            "ingest",
            "--artifacts",
            &setup.artifacts_arg(),
            "--config",
            &setup.config_arg(),
        ])
        .assert();

    // Should still produce outputs (exit 2 because the receipt is invalid)
    assert.code(2);

    assert!(
        setup.cockpit_report_path().exists(),
        "report.json should be created even with corrupt receipt"
    );
    assert!(
        setup.cockpit_comment_path().exists(),
        "comment.md should be created even with corrupt receipt"
    );
}

#[test]
fn corrupt_receipt_produces_invalid_receipt_finding() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]
warn_is_fail = false

[sensors.bad]
blocking = true
missing = "fail"
"#,
    );
    setup.write_sensor_report("bad", "not json at all");

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

    let report = fs::read_to_string(setup.cockpit_report_path()).expect("read cockpit report.json");
    let json: serde_json::Value = serde_json::from_str(&report).expect("parse report");

    // Should have a finding about the invalid receipt
    let highlights = json["highlights"].as_array().expect("highlights array");
    let has_invalid = highlights.iter().any(|h| {
        let code = h["finding"]["code"].as_str().unwrap_or("");
        code == "cockpit.invalid_receipt"
    });
    assert!(
        has_invalid,
        "corrupt receipt should produce cockpit.invalid_receipt finding"
    );
}

#[test]
fn valid_json_but_missing_required_fields_is_handled() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]

[sensors.incomplete]
blocking = true
missing = "fail"
"#,
    );
    // Valid JSON but missing required fields for sensor.report.v1
    setup.write_sensor_report("incomplete", r#"{"tool": {"name": "x", "version": "1.0"}}"#);

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

    assert!(
        setup.cockpit_report_path().exists(),
        "report.json should still be created"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Invalid CLI arguments
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn unknown_subcommand_shows_error() {
    cmd()
        .args(["nonexistent-subcommand"])
        .assert()
        .failure()
        .stderr(contains("error"));
}

#[test]
fn invalid_schema_validation_value_shows_error() {
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
            "--schema-validation",
            "invalid_value",
        ])
        .assert()
        .failure()
        .stderr(contains("error"));
}

// ─────────────────────────────────────────────────────────────────────────────
// Fixture-based: invalid_receipt fixture
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn invalid_receipt_fixture_handled_gracefully() {
    let setup = {
        let s = TestSetup::new();
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("fixtures")
            .join("invalid_receipt");

        let config_src = fixture.join("cockpit.toml");
        if config_src.exists() {
            fs::copy(&config_src, &s.config_path).expect("copy config");
        }

        let src_artifacts = fixture.join("artifacts");
        if src_artifacts.exists() {
            copy_dir_recursive(&src_artifacts, &s.artifacts_dir);
        }
        s
    };

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

    assert!(
        setup.cockpit_report_path().exists(),
        "report.json should be created even with invalid receipt fixture"
    );
}

fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) {
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
