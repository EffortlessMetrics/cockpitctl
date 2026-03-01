//! End-to-end tests for `cockpitctl ingest` with tokmd sensor receipts.
//!
//! These tests verify that cockpitctl correctly ingests receipts produced by
//! the tokmd token-counting sensor. cockpitctl does not depend on tokmd as a
//! Rust crate — it only reads the JSON receipt that tokmd produces.

use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
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

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixture_path(name: &str) -> PathBuf {
    workspace_root().join("fixtures").join(name)
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

    fn read_cockpit_comment(&self) -> String {
        let path = self.cockpit_comment_path();
        fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read cockpit comment at {:?}: {}", path, e))
    }

    fn artifacts_arg(&self) -> String {
        self.artifacts_dir.to_string_lossy().to_string()
    }

    fn config_arg(&self) -> String {
        self.config_path.to_string_lossy().to_string()
    }
}

fn setup_from_fixture(fixture_name: &str) -> TestSetup {
    let setup = TestSetup::new();
    let fixture = fixture_path(fixture_name);

    let config_src = fixture.join("cockpit.toml");
    if config_src.exists() {
        fs::copy(&config_src, &setup.config_path).expect("copy cockpit.toml");
    }

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
// tokmd receipt fixture: pass verdict → exit 0
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn tokmd_receipt_pass_exits_zero() {
    let setup = setup_from_fixture("tokmd_receipt");

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
// tokmd receipt: cockpit report includes tokmd sensor summary
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn tokmd_receipt_appears_in_cockpit_report() {
    let setup = setup_from_fixture("tokmd_receipt");

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
    let json: serde_json::Value = serde_json::from_str(&report).expect("parse cockpit report");

    let sensors = json["sensors"].as_array().expect("sensors array");
    let tokmd_sensor = sensors.iter().find(|s| s["id"].as_str() == Some("tokmd"));
    assert!(
        tokmd_sensor.is_some(),
        "cockpit report should include tokmd sensor"
    );

    let tokmd = tokmd_sensor.unwrap();
    assert_eq!(
        tokmd["verdict"]["status"].as_str(),
        Some("pass"),
        "tokmd sensor verdict should be pass"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// tokmd receipt: comment.md mentions tokmd
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn tokmd_receipt_mentioned_in_comment() {
    let setup = setup_from_fixture("tokmd_receipt");

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

    let comment = setup.read_cockpit_comment();
    assert!(
        comment.contains("tokmd"),
        "comment.md should mention tokmd sensor"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// tokmd with blocking = true: pass → exit 0
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn tokmd_blocking_pass_exits_zero() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]
warn_is_fail = false

[sensors.tokmd]
blocking = true
missing = "fail"
"#,
    );
    setup.write_sensor_report(
        "tokmd",
        r#"{
  "schema": "sensor.report.v1",
  "tool": { "name": "tokmd", "version": "0.1.0" },
  "run": { "started_at": "2026-02-02T11:55:00Z" },
  "verdict": { "status": "pass", "counts": { "info": 1, "warn": 0, "error": 0 } },
  "findings": [
    {
      "severity": "info",
      "code": "token-count",
      "message": "File contains 800 tokens (budget: 5000)",
      "location": { "path": "src/main.rs", "line": 1 }
    }
  ]
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
        .success();
}

// ─────────────────────────────────────────────────────────────────────────────
// tokmd with blocking = true: fail verdict → exit 2
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn tokmd_blocking_fail_exits_two() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]
warn_is_fail = false

[sensors.tokmd]
blocking = true
missing = "fail"
"#,
    );
    setup.write_sensor_report(
        "tokmd",
        r#"{
  "schema": "sensor.report.v1",
  "tool": { "name": "tokmd", "version": "0.1.0" },
  "run": { "started_at": "2026-02-02T11:55:00Z" },
  "verdict": { "status": "fail", "counts": { "info": 0, "warn": 0, "error": 1 } },
  "findings": [
    {
      "severity": "error",
      "code": "token-budget-exceeded",
      "message": "File contains 8500 tokens (budget: 5000)",
      "location": { "path": "src/main.rs", "line": 1 }
    }
  ]
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
        .code(2);

    let report = setup.read_cockpit_report();
    let json: serde_json::Value = serde_json::from_str(&report).expect("parse report");
    assert_eq!(
        json["verdict"]["status"].as_str(),
        Some("fail"),
        "blocking tokmd fail should cause overall fail"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// tokmd with blocking = false: fail verdict → exit 0 (non-blocking)
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn tokmd_nonblocking_fail_exits_zero() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]
warn_is_fail = false

[sensors.tokmd]
blocking = false
missing = "skip"
"#,
    );
    setup.write_sensor_report(
        "tokmd",
        r#"{
  "schema": "sensor.report.v1",
  "tool": { "name": "tokmd", "version": "0.1.0" },
  "run": { "started_at": "2026-02-02T11:55:00Z" },
  "verdict": { "status": "fail", "counts": { "info": 0, "warn": 0, "error": 1 } },
  "findings": [
    {
      "severity": "error",
      "code": "token-budget-exceeded",
      "message": "File contains 8500 tokens (budget: 5000)",
      "location": { "path": "src/main.rs", "line": 1 }
    }
  ]
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
        .success();
}

// ─────────────────────────────────────────────────────────────────────────────
// tokmd with warnings: warn receipt + warn_is_fail = true → exit 2
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn tokmd_warn_with_warn_is_fail_exits_two() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]
warn_is_fail = true

[sensors.tokmd]
blocking = true
missing = "fail"
"#,
    );
    setup.write_sensor_report(
        "tokmd",
        r#"{
  "schema": "sensor.report.v1",
  "tool": { "name": "tokmd", "version": "0.1.0" },
  "run": { "started_at": "2026-02-02T11:55:00Z" },
  "verdict": { "status": "warn", "counts": { "info": 1, "warn": 1, "error": 0 } },
  "findings": [
    {
      "severity": "info",
      "code": "token-count",
      "message": "File contains 800 tokens (budget: 5000)",
      "location": { "path": "src/lib.rs", "line": 1 }
    },
    {
      "severity": "warn",
      "code": "token-budget-near",
      "message": "File contains 4800 tokens (budget: 5000, threshold: 90%)",
      "location": { "path": "src/main.rs", "line": 1 }
    }
  ]
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
        .code(2);

    let report = setup.read_cockpit_report();
    let json: serde_json::Value = serde_json::from_str(&report).expect("parse report");
    assert_eq!(
        json["verdict"]["status"].as_str(),
        Some("fail"),
        "warn_is_fail should escalate tokmd warn to fail"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// tokmd receipt: findings appear as highlights in cockpit report
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn tokmd_findings_included_in_cockpit_highlights() {
    let setup = setup_from_fixture("tokmd_receipt");

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
    let json: serde_json::Value = serde_json::from_str(&report).expect("parse cockpit report");

    let highlights = json["highlights"].as_array().expect("highlights array");
    let tokmd_highlights: Vec<&serde_json::Value> = highlights
        .iter()
        .filter(|h| h["sensor_id"].as_str() == Some("tokmd"))
        .collect();
    assert_eq!(
        tokmd_highlights.len(),
        2,
        "tokmd fixture has 2 findings that should appear as highlights"
    );

    let codes: Vec<&str> = tokmd_highlights
        .iter()
        .filter_map(|h| h["finding"]["code"].as_str())
        .collect();
    assert!(
        codes.iter().all(|c| *c == "token-count"),
        "all tokmd highlights should have code 'token-count'"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// tokmd receipt: strict schema validation passes
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn tokmd_receipt_passes_strict_validation() {
    let setup = setup_from_fixture("tokmd_receipt");

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
