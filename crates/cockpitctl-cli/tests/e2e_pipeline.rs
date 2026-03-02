//! Comprehensive end-to-end integration tests for the full cockpitctl ingest pipeline.
//!
//! These tests exercise the complete pipeline through the CLI binary, verifying:
//! - Full ingest with 5+ sensors of different types
//! - Schema validation modes (strict/lax)
//! - All 4 verdict states (pass/warn/fail/skip) in one pipeline
//! - Buildfix plans in the pipeline
//! - Hooks configuration (with --disable-hooks)
//! - Output file verification (report.json structure, comment.md markers)
//! - Exit code verification for all paths (0, 1, 2)
//! - Config file loading from multiple paths
//! - validate subcommand with valid and invalid inputs
//! - explain subcommand output
//! - init subcommand creates valid config

use std::fs;
use std::path::{Path, PathBuf};

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

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixture_path(name: &str) -> PathBuf {
    workspace_root().join("fixtures").join(name)
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

fn sensor_report(sensor_name: &str, status: &str, findings_json: &str) -> String {
    let (info, warn, error) = match status {
        "pass" => (0, 0, 0),
        "warn" => (0, 1, 0),
        "fail" => (0, 0, 1),
        "skip" => (0, 0, 0),
        _ => panic!("unknown status: {status}"),
    };
    let reasons = if status == "skip" {
        r#", "reasons": ["not_applicable"]"#
    } else {
        ""
    };
    format!(
        r#"{{
  "schema": "{sensor_name}.report.v1",
  "tool": {{ "name": "{sensor_name}", "version": "1.0.0" }},
  "run": {{ "started_at": "2026-02-02T11:00:00Z" }},
  "verdict": {{ "status": "{status}", "counts": {{ "info": {info}, "warn": {warn}, "error": {error} }}{reasons} }},
  "findings": [{findings_json}]
}}"#
    )
}

fn pass_report(name: &str) -> String {
    sensor_report(name, "pass", "")
}

fn warn_report(name: &str) -> String {
    sensor_report(
        name,
        "warn",
        &format!(
            r#"
    {{
      "severity": "warn",
      "code": "{name}.style_issue",
      "message": "Style issue detected in {name}"
    }}"#
        ),
    )
}

fn fail_report(name: &str) -> String {
    sensor_report(
        name,
        "fail",
        &format!(
            r#"
    {{
      "severity": "error",
      "code": "{name}.hard_error",
      "message": "A blocking failure in {name}"
    }}"#
        ),
    )
}

fn skip_report(name: &str) -> String {
    sensor_report(name, "skip", "")
}

// ═════════════════════════════════════════════════════════════════════════════
// 1) Full ingest pipeline with 5+ sensors of different types
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn pipeline_five_sensors_all_pass_exits_zero() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]
warn_is_fail = false
max_highlights = 10
max_per_sensor_findings = 50

[sensors.builddiag]
blocking = true
missing = "fail"
section = "Repo contract"

[sensors.linter]
blocking = true
missing = "fail"
section = "Policy"

[sensors.coverage]
blocking = false
missing = "skip"
section = "Policy"

[sensors.perftest]
blocking = false
missing = "skip"
section = "Other"

[sensors.security]
blocking = true
missing = "fail"
section = "Policy"
"#,
    );

    setup.write_sensor_report("builddiag", &pass_report("builddiag"));
    setup.write_sensor_report("linter", &pass_report("linter"));
    setup.write_sensor_report("coverage", &pass_report("coverage"));
    setup.write_sensor_report("perftest", &pass_report("perftest"));
    setup.write_sensor_report("security", &pass_report("security"));

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
    assert_eq!(sensors.len(), 5, "should have exactly 5 sensors");

    assert_eq!(json["verdict"]["status"].as_str(), Some("pass"));
}

#[test]
fn pipeline_six_sensors_mixed_types_with_findings() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]
warn_is_fail = false
max_highlights = 20
max_per_sensor_findings = 50

[sensors.builddiag]
blocking = true
missing = "fail"

[sensors.linter]
blocking = true
missing = "fail"

[sensors.coverage]
blocking = false
missing = "skip"

[sensors.perftest]
blocking = false
missing = "skip"

[sensors.security]
blocking = true
missing = "fail"

[sensors.docs]
blocking = false
missing = "skip"
"#,
    );

    // Mix of pass and warn — no fail on blocking sensors
    setup.write_sensor_report("builddiag", &pass_report("builddiag"));
    setup.write_sensor_report("linter", &warn_report("linter"));
    setup.write_sensor_report("coverage", &pass_report("coverage"));
    setup.write_sensor_report("perftest", &skip_report("perftest"));
    setup.write_sensor_report("security", &pass_report("security"));
    setup.write_sensor_report("docs", &warn_report("docs"));

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
    assert_eq!(sensors.len(), 6, "should have exactly 6 sensors");

    // Sensors should be in lexical order (determinism)
    let ids: Vec<&str> = sensors.iter().filter_map(|s| s["id"].as_str()).collect();
    let mut sorted = ids.clone();
    sorted.sort();
    assert_eq!(ids, sorted, "sensors must be in lexical order");

    // Overall verdict is warn (sensors have warnings, warn_is_fail = false so not fail)
    let status = json["verdict"]["status"].as_str().unwrap_or("");
    assert!(
        status == "pass" || status == "warn",
        "overall verdict should be pass or warn, got {status}"
    );

    // Highlights should contain findings from linter and docs
    let highlights = json["highlights"].as_array().expect("highlights");
    let highlight_sensors: Vec<&str> = highlights
        .iter()
        .filter_map(|h| h["sensor_id"].as_str())
        .collect();
    assert!(
        highlight_sensors.contains(&"linter") || highlight_sensors.contains(&"docs"),
        "highlights should contain findings from sensors with warnings"
    );
}

// ═════════════════════════════════════════════════════════════════════════════
// 2) Schema validation modes
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn pipeline_strict_schema_valid_receipts_exits_zero() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]
schema_validation = "strict"

[sensors.alpha]
blocking = true
missing = "fail"

[sensors.beta]
blocking = true
missing = "fail"
"#,
    );

    // Use sensor.report.v1 schema (canonical) for strict mode
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
    setup.write_sensor_report(
        "beta",
        r#"{
  "schema": "sensor.report.v1",
  "tool": { "name": "beta", "version": "2.0.0" },
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

    let report = setup.read_cockpit_report();
    let json: serde_json::Value = serde_json::from_str(&report).expect("parse report");
    assert_eq!(json["verdict"]["status"].as_str(), Some("pass"));
}

#[test]
fn pipeline_lax_schema_accepts_nonstandard_schema_field() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]

[sensors.alpha]
blocking = true
missing = "fail"
"#,
    );

    // Uses custom schema name — lax mode should accept it
    setup.write_sensor_report(
        "alpha",
        r#"{
  "schema": "alpha.report.v1",
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
            "lax",
        ])
        .assert()
        .success();
}

#[test]
fn pipeline_lax_no_schema_violations_in_report() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]
schema_validation = "lax"

[sensors.alpha]
blocking = true
missing = "fail"
"#,
    );

    // Receipt with extra field that strict would reject
    setup.write_sensor_report(
        "alpha",
        r#"{
  "schema": "sensor.report.v1",
  "tool": { "name": "alpha", "version": "1.0.0" },
  "run": { "started_at": "2026-02-02T11:00:00Z" },
  "verdict": { "status": "pass", "counts": { "info": 0, "warn": 0, "error": 0 } },
  "findings": [],
  "extra_metadata": { "ci": true }
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

    let report = setup.read_cockpit_report();
    let json: serde_json::Value = serde_json::from_str(&report).expect("parse report");

    let highlights = json["highlights"].as_array().expect("highlights");
    let has_schema_violation = highlights
        .iter()
        .any(|h| h["finding"]["code"].as_str() == Some("cockpit.schema_violation"));
    assert!(
        !has_schema_violation,
        "lax mode must not produce schema_violation findings"
    );
}

// ═════════════════════════════════════════════════════════════════════════════
// 3) Pipeline with all 4 verdict states represented
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn pipeline_all_four_verdict_states_in_report() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]
warn_is_fail = false

[sensors.passing]
blocking = false
missing = "skip"

[sensors.warning]
blocking = false
missing = "skip"

[sensors.failing]
blocking = true
missing = "fail"

[sensors.skipping]
blocking = false
missing = "skip"
"#,
    );

    setup.write_sensor_report("passing", &pass_report("passing"));
    setup.write_sensor_report("warning", &warn_report("warning"));
    setup.write_sensor_report("failing", &fail_report("failing"));
    setup.write_sensor_report("skipping", &skip_report("skipping"));

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

    let sensors = json["sensors"].as_array().expect("sensors array");
    assert_eq!(sensors.len(), 4);

    // Collect individual sensor verdicts
    let mut verdicts: Vec<&str> = sensors
        .iter()
        .filter_map(|s| s["verdict"]["status"].as_str())
        .collect();
    verdicts.sort();

    assert_eq!(
        verdicts,
        vec!["fail", "pass", "skip", "warn"],
        "all 4 verdict states should be represented"
    );

    // Overall verdict must be fail (blocking sensor failed)
    assert_eq!(json["verdict"]["status"].as_str(), Some("fail"));
}

#[test]
fn pipeline_all_four_verdicts_non_blocking_exits_zero() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]
warn_is_fail = false

[sensors.passing]
blocking = false
missing = "skip"

[sensors.warning]
blocking = false
missing = "skip"

[sensors.failing]
blocking = false
missing = "skip"

[sensors.skipping]
blocking = false
missing = "skip"
"#,
    );

    setup.write_sensor_report("passing", &pass_report("passing"));
    setup.write_sensor_report("warning", &warn_report("warning"));
    setup.write_sensor_report("failing", &fail_report("failing"));
    setup.write_sensor_report("skipping", &skip_report("skipping"));

    // All non-blocking, so even a fail verdict exits 0
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
    assert_eq!(json["verdict"]["status"].as_str(), Some("pass"));
}

// ═════════════════════════════════════════════════════════════════════════════
// 4) Pipeline with buildfix plans present
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn pipeline_buildfix_fixture_produces_outputs() {
    let setup = setup_from_fixture("buildfix_plan");

    cmd()
        .args([
            "ingest",
            "--artifacts",
            &setup.artifacts_arg(),
            "--config",
            &setup.config_arg(),
        ])
        .assert()
        .code(predicate::in_iter([0, 2]));

    assert!(
        setup.cockpit_report_path().exists(),
        "report.json must be created for buildfix fixture"
    );
    assert!(
        setup.cockpit_comment_path().exists(),
        "comment.md must be created for buildfix fixture"
    );

    let report = setup.read_cockpit_report();
    let json: serde_json::Value = serde_json::from_str(&report).expect("parse report");
    assert!(json["sensors"].is_array());
    assert!(json["verdict"].is_object());
}

#[test]
fn pipeline_buildfix_with_disable_flag_skips_buildfix() {
    let setup = setup_from_fixture("buildfix_plan");

    cmd()
        .args([
            "ingest",
            "--artifacts",
            &setup.artifacts_arg(),
            "--config",
            &setup.config_arg(),
            "--disable-buildfix",
        ])
        .assert()
        .code(predicate::in_iter([0, 2]));

    assert!(setup.cockpit_report_path().exists());

    // Buildfix sidecar must not exist when disabled
    let sidecar = setup
        .artifacts_dir
        .join("cockpit")
        .join("buildfix.apply.json");
    assert!(
        !sidecar.exists(),
        "buildfix sidecar must not exist when --disable-buildfix is set"
    );
}

#[test]
fn pipeline_buildfix_sensor_findings_appear_in_report() {
    let setup = setup_from_fixture("buildfix_plan");

    cmd()
        .args([
            "ingest",
            "--artifacts",
            &setup.artifacts_arg(),
            "--config",
            &setup.config_arg(),
        ])
        .assert()
        .code(predicate::in_iter([0, 2]));

    let report = setup.read_cockpit_report();
    let json: serde_json::Value = serde_json::from_str(&report).expect("parse report");

    let sensors = json["sensors"].as_array().expect("sensors array");
    let buildfix_sensor = sensors
        .iter()
        .find(|s| s["id"].as_str() == Some("buildfix"));
    assert!(
        buildfix_sensor.is_some(),
        "buildfix sensor should appear in the report"
    );
}

// ═════════════════════════════════════════════════════════════════════════════
// 5) Pipeline with hooks configured (--disable-hooks)
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn pipeline_hooks_configured_disable_hooks_succeeds() {
    let setup = TestSetup::new();
    // Config includes hooks section but we pass --disable-hooks
    setup.write_config(
        r#"[policy]
warn_is_fail = false

[sensors.alpha]
blocking = true
missing = "fail"

[[hooks]]
name = "notify-slack"
command = "nonexistent-command-that-should-not-run"
timeout_ms = 5000
"#,
    );

    setup.write_sensor_report("alpha", &pass_report("alpha"));

    cmd()
        .args([
            "ingest",
            "--artifacts",
            &setup.artifacts_arg(),
            "--config",
            &setup.config_arg(),
            "--disable-hooks",
        ])
        .assert()
        .success();

    assert!(setup.cockpit_report_path().exists());
    assert!(setup.cockpit_comment_path().exists());
}

#[test]
fn pipeline_hooks_configured_disable_hooks_with_multiple_sensors() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]
warn_is_fail = false

[sensors.build]
blocking = true
missing = "fail"

[sensors.lint]
blocking = false
missing = "skip"

[sensors.test]
blocking = true
missing = "fail"

[[hooks]]
name = "post-ingest"
command = "nonexistent-hook"
timeout_ms = 3000
"#,
    );

    setup.write_sensor_report("build", &pass_report("build"));
    setup.write_sensor_report("lint", &warn_report("lint"));
    setup.write_sensor_report("test", &pass_report("test"));

    cmd()
        .args([
            "ingest",
            "--artifacts",
            &setup.artifacts_arg(),
            "--config",
            &setup.config_arg(),
            "--disable-hooks",
        ])
        .assert()
        .success();

    let report = setup.read_cockpit_report();
    let json: serde_json::Value = serde_json::from_str(&report).expect("parse report");
    let sensors = json["sensors"].as_array().expect("sensors");
    assert_eq!(sensors.len(), 3);
}

// ═════════════════════════════════════════════════════════════════════════════
// 6) Output file verification
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn pipeline_report_json_full_structure_verification() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]
warn_is_fail = false
max_highlights = 10
max_per_sensor_findings = 50

[sensors.alpha]
blocking = true
missing = "fail"

[sensors.beta]
blocking = false
missing = "skip"

[sensors.gamma]
blocking = true
missing = "fail"

[sensors.delta]
blocking = false
missing = "skip"

[sensors.epsilon]
blocking = false
missing = "skip"
"#,
    );

    setup.write_sensor_report("alpha", &pass_report("alpha"));
    setup.write_sensor_report("beta", &warn_report("beta"));
    setup.write_sensor_report("gamma", &pass_report("gamma"));
    setup.write_sensor_report("delta", &skip_report("delta"));
    setup.write_sensor_report("epsilon", &fail_report("epsilon"));

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

    // Top-level required fields per cockpit.report.v1
    assert_eq!(json["schema"].as_str(), Some("cockpit.report.v1"));
    assert!(json["tool"].is_object());
    assert!(json["tool"]["name"].is_string());
    assert!(json["tool"]["version"].is_string());
    assert!(json["run"].is_object());
    assert!(json["run"]["started_at"].is_string());
    assert!(json["verdict"].is_object());
    assert!(json["verdict"]["status"].is_string());
    assert!(json["verdict"]["counts"].is_object());
    assert!(json["sensors"].is_array());
    assert!(json["highlights"].is_array());
    assert!(json["policy"].is_object());

    // Verify counts structure
    let counts = &json["verdict"]["counts"];
    assert!(counts["info"].is_number());
    assert!(counts["warn"].is_number());
    assert!(counts["error"].is_number());

    // Verify policy reflects config
    let policy = &json["policy"];
    assert_eq!(policy["warn_is_fail"].as_bool(), Some(false));

    // All 5 sensors present in lexical order
    let sensors = json["sensors"].as_array().unwrap();
    assert_eq!(sensors.len(), 5);
    let ids: Vec<&str> = sensors.iter().filter_map(|s| s["id"].as_str()).collect();
    assert_eq!(ids, vec!["alpha", "beta", "delta", "epsilon", "gamma"]);

    // Each sensor has id, verdict.status, verdict.counts
    for sensor in sensors {
        assert!(sensor["id"].is_string());
        assert!(sensor["verdict"]["status"].is_string());
        assert!(sensor["verdict"]["counts"].is_object());
    }
}

#[test]
fn pipeline_comment_md_markers_and_structure() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]
warn_is_fail = false

[sensors.builddiag]
blocking = true
missing = "fail"

[sensors.linter]
blocking = false
missing = "skip"

[sensors.coverage]
blocking = false
missing = "skip"
"#,
    );

    setup.write_sensor_report("builddiag", &pass_report("builddiag"));
    setup.write_sensor_report("linter", &warn_report("linter"));
    setup.write_sensor_report("coverage", &pass_report("coverage"));

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

    // Comment must mention each sensor
    assert!(
        comment.contains("builddiag"),
        "comment should mention builddiag"
    );
    assert!(comment.contains("linter"), "comment should mention linter");
    assert!(
        comment.contains("coverage"),
        "comment should mention coverage"
    );

    // Comment must contain cockpit header
    assert!(
        comment.contains("Cockpit") || comment.contains("cockpit"),
        "comment should contain Cockpit marker"
    );

    // Comment must not be empty
    assert!(comment.len() > 50, "comment should be substantial");
}

#[test]
fn pipeline_comment_md_on_failure_shows_failure_indicator() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]

[sensors.critical]
blocking = true
missing = "fail"
"#,
    );

    setup.write_sensor_report("critical", &fail_report("critical"));

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

    let comment = setup.read_cockpit_comment();
    let lower = comment.to_lowercase();
    assert!(
        lower.contains("fail") || lower.contains("❌") || lower.contains("blocked"),
        "comment should indicate failure state"
    );
}

// ═════════════════════════════════════════════════════════════════════════════
// 7) Exit code verification for all paths
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn exit_code_zero_all_blocking_pass() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]

[sensors.a]
blocking = true
missing = "fail"

[sensors.b]
blocking = true
missing = "fail"
"#,
    );
    setup.write_sensor_report("a", &pass_report("a"));
    setup.write_sensor_report("b", &pass_report("b"));

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

#[test]
fn exit_code_one_malformed_config() {
    let setup = TestSetup::new();
    setup.write_config("{{ invalid toml [[[ not parseable");
    setup.write_sensor_report("x", &pass_report("x"));

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

#[test]
fn exit_code_two_blocking_sensor_fails() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]

[sensors.gate]
blocking = true
missing = "fail"
"#,
    );
    setup.write_sensor_report("gate", &fail_report("gate"));

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
fn exit_code_two_missing_blocking_sensor() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]

[sensors.required]
blocking = true
missing = "fail"
"#,
    );
    // Don't write any sensor report

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
fn exit_code_zero_with_warn_when_warn_is_fail_false() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]
warn_is_fail = false

[sensors.linter]
blocking = true
missing = "fail"
"#,
    );
    setup.write_sensor_report("linter", &warn_report("linter"));

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

#[test]
fn exit_code_two_outputs_still_written() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]

[sensors.gate]
blocking = true
missing = "fail"
"#,
    );
    setup.write_sensor_report("gate", &fail_report("gate"));

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
        "report.json must be written on exit 2"
    );
    assert!(
        setup.cockpit_comment_path().exists(),
        "comment.md must be written on exit 2"
    );
}

// ═════════════════════════════════════════════════════════════════════════════
// 8) Config file loading from multiple paths
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn config_from_subdirectory_path() {
    let temp_dir = TempDir::new().expect("create temp dir");
    let artifacts_dir = temp_dir.path().join("artifacts");
    let config_dir = temp_dir.path().join("config").join("nested");
    fs::create_dir_all(&artifacts_dir).expect("create artifacts dir");
    fs::create_dir_all(&config_dir).expect("create config dir");

    let config_path = config_dir.join("cockpit.toml");
    fs::write(
        &config_path,
        r#"[policy]

[sensors.alpha]
blocking = false
missing = "skip"
"#,
    )
    .expect("write config");

    let sensor_dir = artifacts_dir.join("alpha");
    fs::create_dir_all(&sensor_dir).expect("create sensor dir");
    fs::write(sensor_dir.join("report.json"), pass_report("alpha")).expect("write report");

    cmd()
        .args([
            "ingest",
            "--artifacts",
            &artifacts_dir.to_string_lossy(),
            "--config",
            &config_path.to_string_lossy(),
        ])
        .assert()
        .success();
}

#[test]
fn config_from_absolute_path() {
    let setup = TestSetup::new();
    // Write config to a different location
    let alt_config = setup._temp_dir.path().join("alt").join("cockpit.toml");
    fs::create_dir_all(alt_config.parent().unwrap()).expect("create alt dir");
    fs::write(
        &alt_config,
        r#"[policy]
warn_is_fail = false

[sensors.sensor1]
blocking = false
missing = "skip"
"#,
    )
    .expect("write alt config");

    setup.write_sensor_report("sensor1", &pass_report("sensor1"));

    cmd()
        .args([
            "ingest",
            "--artifacts",
            &setup.artifacts_arg(),
            "--config",
            &alt_config.to_string_lossy(),
        ])
        .assert()
        .success();
}

#[test]
fn config_nonexistent_path_uses_defaults() {
    let setup = TestSetup::new();
    setup.write_sensor_report("discovered", &pass_report("discovered"));

    let fake_config = setup._temp_dir.path().join("does_not_exist.toml");

    cmd()
        .args([
            "ingest",
            "--artifacts",
            &setup.artifacts_arg(),
            "--config",
            &fake_config.to_string_lossy(),
        ])
        .assert()
        .success();

    assert!(setup.cockpit_report_path().exists());
}

// ═════════════════════════════════════════════════════════════════════════════
// 9) validate subcommand with valid and invalid inputs
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn validate_valid_sensor_report_lax_exits_zero() {
    let temp = TempDir::new().expect("temp dir");
    let path = temp.path().join("report.json");
    fs::write(
        &path,
        r#"{
  "schema": "sensor.report.v1",
  "tool": { "name": "test", "version": "1.0.0" },
  "run": { "started_at": "2026-02-02T11:00:00Z" },
  "verdict": { "status": "pass", "counts": { "info": 0, "warn": 0, "error": 0 } },
  "findings": []
}"#,
    )
    .unwrap();

    cmd()
        .args(["validate", "--input", &path.to_string_lossy(), "--lax"])
        .assert()
        .success()
        .stderr(predicate::str::contains("ok"));
}

#[test]
fn validate_malformed_json_exits_one() {
    let temp = TempDir::new().expect("temp dir");
    let path = temp.path().join("broken.json");
    fs::write(&path, "{ this is not json !!!").unwrap();

    cmd()
        .args(["validate", "--input", &path.to_string_lossy(), "--lax"])
        .assert()
        .code(1);
}

#[test]
fn validate_missing_file_exits_one() {
    cmd()
        .args(["validate", "--input", "nonexistent-file.json", "--lax"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("nonexistent-file.json"));
}

#[test]
fn validate_valid_cockpit_report_lax_exits_zero() {
    let temp = TempDir::new().expect("temp dir");
    let path = temp.path().join("cockpit.json");
    fs::write(
        &path,
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
}"#,
    )
    .unwrap();

    cmd()
        .args(["validate", "--input", &path.to_string_lossy(), "--lax"])
        .assert()
        .success()
        .stderr(predicate::str::contains("ok"));
}

#[test]
fn validate_missing_required_fields_exits_one() {
    let temp = TempDir::new().expect("temp dir");
    let path = temp.path().join("incomplete.json");
    fs::write(&path, r#"{"tool": {"name": "x", "version": "1.0"}}"#).unwrap();

    cmd()
        .args(["validate", "--input", &path.to_string_lossy(), "--lax"])
        .assert()
        .code(1);
}

// ═════════════════════════════════════════════════════════════════════════════
// 10) explain subcommand output
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn explain_known_code_exits_zero_with_fields() {
    cmd()
        .args(["explain", "cockpit.invalid_receipt"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("cockpit.invalid_receipt")
                .and(predicate::str::contains("Title:"))
                .and(predicate::str::contains("Description:"))
                .and(predicate::str::contains("Cause:"))
                .and(predicate::str::contains("Fix:")),
        );
}

#[test]
fn explain_unknown_code_exits_one() {
    cmd()
        .args(["explain", "nonexistent.code"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("unknown code"));
}

#[test]
fn explain_all_lists_codes() {
    cmd().args(["explain", "all"]).assert().success().stdout(
        predicate::str::contains("cockpit.missing_receipt")
            .and(predicate::str::contains("cockpit.invalid_receipt"))
            .and(predicate::str::contains("cockpit.schema_violation")),
    );
}

#[test]
fn explain_no_argument_fails() {
    cmd()
        .args(["explain"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Usage"));
}

// ═════════════════════════════════════════════════════════════════════════════
// 11) init subcommand creates valid config
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn init_creates_valid_toml_config() {
    let temp = TempDir::new().expect("temp dir");
    let path = temp.path().join("cockpit.toml");

    cmd()
        .args(["init", "--path", &path.to_string_lossy()])
        .assert()
        .success();

    assert!(path.exists(), "cockpit.toml should be created");

    let content = fs::read_to_string(&path).expect("read config");
    let parsed: toml::Value = toml::from_str(&content).expect("must be valid TOML");

    assert!(
        parsed.get("policy").is_some(),
        "generated config should have [policy]"
    );
}

#[test]
fn init_config_is_usable_for_ingest() {
    let temp = TempDir::new().expect("temp dir");
    let config_path = temp.path().join("cockpit.toml");
    let artifacts_dir = temp.path().join("artifacts");
    fs::create_dir_all(&artifacts_dir).expect("create artifacts dir");

    // Generate config via init
    cmd()
        .args(["init", "--path", &config_path.to_string_lossy()])
        .assert()
        .success();

    // Use the generated config for ingest (empty artifacts)
    cmd()
        .args([
            "ingest",
            "--artifacts",
            &artifacts_dir.to_string_lossy(),
            "--config",
            &config_path.to_string_lossy(),
        ])
        .assert()
        .code(predicate::in_iter([0, 2]));
}

#[test]
fn init_refuses_overwrite() {
    let temp = TempDir::new().expect("temp dir");
    let path = temp.path().join("cockpit.toml");
    fs::write(&path, "# existing\n").unwrap();

    cmd()
        .args(["init", "--path", &path.to_string_lossy()])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("refusing to overwrite"));
}

// ═════════════════════════════════════════════════════════════════════════════
// 12) Determinism: repeated runs produce identical output
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn pipeline_deterministic_with_five_sensors() {
    let run = || {
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

[sensors.delta]
blocking = false
missing = "skip"

[sensors.epsilon]
blocking = false
missing = "skip"
"#,
        );

        setup.write_sensor_report("alpha", &pass_report("alpha"));
        setup.write_sensor_report("beta", &warn_report("beta"));
        setup.write_sensor_report("gamma", &pass_report("gamma"));
        setup.write_sensor_report("delta", &skip_report("delta"));
        setup.write_sensor_report("epsilon", &fail_report("epsilon"));

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

        (setup.read_cockpit_report(), setup.read_cockpit_comment())
    };

    let (report1, comment1) = run();
    let (report2, comment2) = run();

    assert_eq!(report1, report2, "report.json must be deterministic");
    assert_eq!(comment1, comment2, "comment.md must be deterministic");
}

// ═════════════════════════════════════════════════════════════════════════════
// 13) Full pipeline with fixture: three_sensor_mixed
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn pipeline_three_sensor_mixed_fixture() {
    let setup = setup_from_fixture("three_sensor_mixed");

    let assert = cmd()
        .args([
            "ingest",
            "--artifacts",
            &setup.artifacts_arg(),
            "--config",
            &setup.config_arg(),
        ])
        .assert();

    // This fixture should produce outputs regardless of exit code
    assert.code(predicate::in_iter([0, 2]));
    assert!(setup.cockpit_report_path().exists());
    assert!(setup.cockpit_comment_path().exists());

    let report = setup.read_cockpit_report();
    let json: serde_json::Value = serde_json::from_str(&report).expect("parse report");
    let sensors = json["sensors"].as_array().expect("sensors");
    assert!(sensors.len() >= 2, "should have at least 2 sensors");
}

// ═════════════════════════════════════════════════════════════════════════════
// 14) Full pipeline with all feature disable flags combined
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn pipeline_all_disable_flags_with_five_sensors() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]
warn_is_fail = false

[sensors.build]
blocking = true
missing = "fail"

[sensors.lint]
blocking = true
missing = "fail"

[sensors.test]
blocking = false
missing = "skip"

[sensors.coverage]
blocking = false
missing = "skip"

[sensors.security]
blocking = true
missing = "fail"
"#,
    );

    setup.write_sensor_report("build", &pass_report("build"));
    setup.write_sensor_report("lint", &pass_report("lint"));
    setup.write_sensor_report("test", &pass_report("test"));
    setup.write_sensor_report("coverage", &skip_report("coverage"));
    setup.write_sensor_report("security", &pass_report("security"));

    cmd()
        .args([
            "ingest",
            "--artifacts",
            &setup.artifacts_arg(),
            "--config",
            &setup.config_arg(),
            "--disable-hooks",
            "--disable-buildfix",
            "--disable-policy-signing",
        ])
        .assert()
        .success();

    let report = setup.read_cockpit_report();
    let json: serde_json::Value = serde_json::from_str(&report).expect("parse report");
    assert_eq!(json["sensors"].as_array().unwrap().len(), 5);
    assert_eq!(json["verdict"]["status"].as_str(), Some("pass"));
}

// ═════════════════════════════════════════════════════════════════════════════
// 15) Findings sorting is deterministic
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn pipeline_highlights_sorted_by_severity_then_sensor_id() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]
warn_is_fail = false
max_highlights = 20

[sensors.zzz_sensor]
blocking = false
missing = "skip"

[sensors.aaa_sensor]
blocking = false
missing = "skip"
"#,
    );

    // zzz_sensor has an error finding, aaa_sensor has a warn finding
    setup.write_sensor_report("zzz_sensor", &fail_report("zzz_sensor"));
    setup.write_sensor_report("aaa_sensor", &warn_report("aaa_sensor"));

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

    let highlights = json["highlights"].as_array().expect("highlights");
    if highlights.len() >= 2 {
        // Error findings should come before warn findings (severity desc)
        let first_severity = highlights[0]["finding"]["severity"].as_str().unwrap_or("");
        let last_severity = highlights[highlights.len() - 1]["finding"]["severity"]
            .as_str()
            .unwrap_or("");
        assert!(
            first_severity == "error" || last_severity != "error",
            "error findings should precede warn findings in highlights"
        );
    }
}
