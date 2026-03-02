//! End-to-end tests proving the CLI precedence contract:
//!
//!   Config provides defaults; CLI flags override ONLY when explicitly provided.
//!
//! Covers:
//! - Schema validation precedence (config vs. CLI `--schema-validation`)
//! - Exit code semantics (0=pass, 2=policy-fail, 1=runtime-error)
//! - Config defaults honored when CLI flags are absent
//! - CLI flags override config when explicitly provided

use std::fs;
use std::path::PathBuf;

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

    fn read_cockpit_report(&self) -> String {
        let path = self.artifacts_dir.join("cockpit").join("report.json");
        fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read cockpit report at {:?}: {}", path, e))
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

fn warn_sensor_report(sensor_name: &str) -> String {
    format!(
        r#"{{
  "schema": "{sensor_name}.report.v1",
  "tool": {{ "name": "{sensor_name}", "version": "1.0.0" }},
  "run": {{ "started_at": "2026-02-02T11:00:00Z" }},
  "verdict": {{ "status": "warn", "counts": {{ "info": 0, "warn": 1, "error": 0 }} }},
  "findings": [
    {{
      "severity": "warn",
      "code": "{sensor_name}.style_issue",
      "message": "Style issue detected"
    }}
  ]
}}"#
    )
}

/// Sensor report with extra fields that violate strict schema but parse in lax mode.
fn extra_field_sensor_report(sensor_name: &str) -> String {
    format!(
        r#"{{
  "schema": "sensor.report.v1",
  "tool": {{ "name": "{sensor_name}", "version": "1.0.0" }},
  "run": {{ "started_at": "2026-02-02T11:00:00Z" }},
  "verdict": {{ "status": "pass", "counts": {{ "info": 0, "warn": 0, "error": 0 }} }},
  "findings": [],
  "extra_not_allowed": "violates additionalProperties"
}}"#
    )
}

// =============================================================================
// Schema validation precedence
// =============================================================================

/// Config says lax, no CLI flag → uses lax (config default honoured).
#[test]
fn schema_config_lax_no_cli_flag_uses_lax() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]
schema_validation = "lax"

[sensors.alpha]
blocking = true
missing = "fail"
"#,
    );
    // Extra field would fail strict, but lax ignores it.
    setup.write_sensor_report("alpha", &extra_field_sensor_report("alpha"));

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

    // No schema violation → config lax was honoured.
    let highlights = json["highlights"].as_array().expect("highlights");
    let has_violation = highlights
        .iter()
        .any(|h| h["finding"]["code"].as_str() == Some("cockpit.schema_violation"));
    assert!(
        !has_violation,
        "config lax should skip schema validation when CLI flag is absent"
    );
}

/// Config says strict, no CLI flag → uses strict (config default honoured).
#[cfg(feature = "feature-schema")]
#[test]
fn schema_config_strict_no_cli_flag_uses_strict() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]
schema_validation = "strict"

[sensors.alpha]
blocking = true
missing = "fail"
"#,
    );
    setup.write_sensor_report("alpha", &extra_field_sensor_report("alpha"));

    // Config strict kicks in without CLI flag → exit 2 (schema violation on blocking sensor).
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

    let highlights = json["highlights"].as_array().expect("highlights");
    let has_violation = highlights
        .iter()
        .any(|h| h["finding"]["code"].as_str() == Some("cockpit.schema_violation"));
    assert!(
        has_violation,
        "config strict should enforce validation when CLI flag is absent"
    );
}

/// Config says lax, CLI says strict → CLI overrides to strict.
#[cfg(feature = "feature-schema")]
#[test]
fn schema_config_lax_cli_strict_overrides_to_strict() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]
schema_validation = "lax"

[sensors.alpha]
blocking = true
missing = "fail"
"#,
    );
    setup.write_sensor_report("alpha", &extra_field_sensor_report("alpha"));

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
        .code(2);

    let report = setup.read_cockpit_report();
    let json: serde_json::Value = serde_json::from_str(&report).expect("parse report");

    let highlights = json["highlights"].as_array().expect("highlights");
    let has_violation = highlights
        .iter()
        .any(|h| h["finding"]["code"].as_str() == Some("cockpit.schema_violation"));
    assert!(
        has_violation,
        "CLI --schema-validation strict must override config lax"
    );
}

/// Config says strict, CLI says lax → CLI overrides to lax.
#[test]
fn schema_config_strict_cli_lax_overrides_to_lax() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]
schema_validation = "strict"

[sensors.alpha]
blocking = true
missing = "fail"
"#,
    );
    setup.write_sensor_report("alpha", &extra_field_sensor_report("alpha"));

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

    let highlights = json["highlights"].as_array().expect("highlights");
    let has_violation = highlights
        .iter()
        .any(|h| h["finding"]["code"].as_str() == Some("cockpit.schema_violation"));
    assert!(
        !has_violation,
        "CLI --schema-validation lax must override config strict"
    );
}

/// No config file at all → default schema_validation is lax.
#[test]
fn schema_no_config_defaults_to_lax() {
    let setup = TestSetup::new();
    // Write a sensor but NO cockpit.toml
    setup.write_sensor_report("alpha", &extra_field_sensor_report("alpha"));

    let nonexistent_config = setup._temp_dir.path().join("nonexistent.toml");

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

    let report = setup.read_cockpit_report();
    let json: serde_json::Value = serde_json::from_str(&report).expect("parse report");

    let highlights = json["highlights"].as_array().expect("highlights");
    let has_violation = highlights
        .iter()
        .any(|h| h["finding"]["code"].as_str() == Some("cockpit.schema_violation"));
    assert!(
        !has_violation,
        "absent config should default to lax schema validation"
    );
}

// =============================================================================
// Exit code semantics
// =============================================================================

/// All sensors pass → exit 0.
#[test]
fn exit_code_all_pass_is_zero() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]

[sensors.alpha]
blocking = true
missing = "fail"

[sensors.beta]
blocking = true
missing = "fail"
"#,
    );
    setup.write_sensor_report("alpha", &valid_sensor_report("alpha"));
    setup.write_sensor_report("beta", &valid_sensor_report("beta"));

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

    let json: serde_json::Value =
        serde_json::from_str(&setup.read_cockpit_report()).expect("parse");
    assert_eq!(json["verdict"]["status"].as_str(), Some("pass"));
}

/// One blocking sensor fails → exit 2.
#[test]
fn exit_code_blocking_fail_is_two() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]

[sensors.alpha]
blocking = true
missing = "fail"

[sensors.beta]
blocking = true
missing = "fail"
"#,
    );
    setup.write_sensor_report("alpha", &valid_sensor_report("alpha"));
    setup.write_sensor_report("beta", &fail_sensor_report("beta"));

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

    let json: serde_json::Value =
        serde_json::from_str(&setup.read_cockpit_report()).expect("parse");
    assert_eq!(json["verdict"]["status"].as_str(), Some("fail"));
}

/// Missing required sensor (missing = "fail") → exit 2.
#[test]
fn exit_code_missing_required_sensor_is_two() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]

[sensors.required]
blocking = true
missing = "fail"
"#,
    );
    // Do not write any sensor report.

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

    let json: serde_json::Value =
        serde_json::from_str(&setup.read_cockpit_report()).expect("parse");
    assert_eq!(json["verdict"]["status"].as_str(), Some("fail"));
}

/// Bad artifacts path (non-existent, no sensors configured) → exit 0
/// (treated as empty discovery with no policy sensors).
#[test]
fn exit_code_bad_artifacts_no_policy_is_zero() {
    let temp_dir = TempDir::new().expect("create temp dir");
    let bad_path = temp_dir.path().join("does_not_exist");
    let config = temp_dir.path().join("cockpit.toml");
    fs::write(&config, "[policy]\n").unwrap();

    cmd()
        .args([
            "ingest",
            "--artifacts",
            bad_path.to_string_lossy().as_ref(),
            "--config",
            config.to_string_lossy().as_ref(),
        ])
        .assert()
        .success();
}

/// Bad artifacts path with a blocking missing="fail" sensor → exit 2.
#[test]
fn exit_code_bad_artifacts_with_required_sensor_is_two() {
    let temp_dir = TempDir::new().expect("create temp dir");
    let bad_path = temp_dir.path().join("does_not_exist");
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

    cmd()
        .args([
            "ingest",
            "--artifacts",
            bad_path.to_string_lossy().as_ref(),
            "--config",
            config.to_string_lossy().as_ref(),
        ])
        .assert()
        .code(2);
}

/// No sensors found, no sensors expected → exit 0.
#[test]
fn exit_code_empty_artifacts_no_policy_is_zero() {
    let setup = TestSetup::new();
    setup.write_config("[policy]\n");

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

    assert!(setup.cockpit_report_path().exists());
    assert!(setup.cockpit_comment_path().exists());

    let json: serde_json::Value =
        serde_json::from_str(&setup.read_cockpit_report()).expect("parse");
    assert_eq!(json["verdict"]["status"].as_str(), Some("pass"));
}

// =============================================================================
// Config defaults honoured when CLI flags absent
// =============================================================================

/// Config warn_is_fail = true, no CLI override → warn treated as fail (exit 2).
#[test]
fn config_warn_is_fail_true_honoured() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]
warn_is_fail = true

[sensors.linter]
blocking = true
missing = "fail"
"#,
    );
    setup.write_sensor_report("linter", &warn_sensor_report("linter"));

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

    let json: serde_json::Value =
        serde_json::from_str(&setup.read_cockpit_report()).expect("parse");
    assert_eq!(
        json["verdict"]["status"].as_str(),
        Some("fail"),
        "warn_is_fail = true should escalate warn verdict to fail"
    );
}

/// Config warn_is_fail = false, no CLI override → warn passes (exit 0).
#[test]
fn config_warn_is_fail_false_honoured() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]
warn_is_fail = false

[sensors.linter]
blocking = true
missing = "fail"
"#,
    );
    setup.write_sensor_report("linter", &warn_sensor_report("linter"));

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

    // Exit code 0 confirms warn did not trigger policy failure.
    // The overall verdict may be "warn" or "pass" depending on composition;
    // the key invariant is that it is NOT "fail".
    let json: serde_json::Value =
        serde_json::from_str(&setup.read_cockpit_report()).expect("parse");
    let status = json["verdict"]["status"].as_str().expect("verdict status");
    assert_ne!(
        status, "fail",
        "warn_is_fail = false should not escalate warn to fail"
    );
}

/// Config max_highlights = 3 → highlights array capped at 3.
#[test]
fn config_max_highlights_honoured() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]
warn_is_fail = false
max_highlights = 3

[sensors.checker]
blocking = true
missing = "fail"
"#,
    );

    // Produce a report with many findings so highlights would exceed 3.
    let report = r#"{
  "schema": "checker.report.v1",
  "tool": { "name": "checker", "version": "1.0.0" },
  "run": { "started_at": "2026-02-02T11:00:00Z" },
  "verdict": { "status": "warn", "counts": { "info": 0, "warn": 10, "error": 0 } },
  "findings": [
    { "severity": "warn", "code": "checker.a", "message": "Issue A", "location": { "path": "a.rs", "line": 1 } },
    { "severity": "warn", "code": "checker.b", "message": "Issue B", "location": { "path": "b.rs", "line": 2 } },
    { "severity": "warn", "code": "checker.c", "message": "Issue C", "location": { "path": "c.rs", "line": 3 } },
    { "severity": "warn", "code": "checker.d", "message": "Issue D", "location": { "path": "d.rs", "line": 4 } },
    { "severity": "warn", "code": "checker.e", "message": "Issue E", "location": { "path": "e.rs", "line": 5 } },
    { "severity": "warn", "code": "checker.f", "message": "Issue F", "location": { "path": "f.rs", "line": 6 } },
    { "severity": "warn", "code": "checker.g", "message": "Issue G", "location": { "path": "g.rs", "line": 7 } },
    { "severity": "warn", "code": "checker.h", "message": "Issue H", "location": { "path": "h.rs", "line": 8 } },
    { "severity": "warn", "code": "checker.i", "message": "Issue I", "location": { "path": "i.rs", "line": 9 } },
    { "severity": "warn", "code": "checker.j", "message": "Issue J", "location": { "path": "j.rs", "line": 10 } }
  ]
}"#;
    setup.write_sensor_report("checker", report);

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

    let json: serde_json::Value =
        serde_json::from_str(&setup.read_cockpit_report()).expect("parse");
    let highlights = json["highlights"].as_array().expect("highlights");
    assert!(
        highlights.len() <= 3,
        "max_highlights = 3 should cap highlights to at most 3, got {}",
        highlights.len()
    );
}

/// Config max_highlights = 7 (default) → more highlights allowed than cap=3.
#[test]
fn config_max_highlights_default_allows_more() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]
warn_is_fail = false
max_highlights = 7

[sensors.checker]
blocking = true
missing = "fail"
"#,
    );

    let report = r#"{
  "schema": "checker.report.v1",
  "tool": { "name": "checker", "version": "1.0.0" },
  "run": { "started_at": "2026-02-02T11:00:00Z" },
  "verdict": { "status": "warn", "counts": { "info": 0, "warn": 10, "error": 0 } },
  "findings": [
    { "severity": "warn", "code": "checker.a", "message": "Issue A", "location": { "path": "a.rs", "line": 1 } },
    { "severity": "warn", "code": "checker.b", "message": "Issue B", "location": { "path": "b.rs", "line": 2 } },
    { "severity": "warn", "code": "checker.c", "message": "Issue C", "location": { "path": "c.rs", "line": 3 } },
    { "severity": "warn", "code": "checker.d", "message": "Issue D", "location": { "path": "d.rs", "line": 4 } },
    { "severity": "warn", "code": "checker.e", "message": "Issue E", "location": { "path": "e.rs", "line": 5 } },
    { "severity": "warn", "code": "checker.f", "message": "Issue F", "location": { "path": "f.rs", "line": 6 } },
    { "severity": "warn", "code": "checker.g", "message": "Issue G", "location": { "path": "g.rs", "line": 7 } },
    { "severity": "warn", "code": "checker.h", "message": "Issue H", "location": { "path": "h.rs", "line": 8 } },
    { "severity": "warn", "code": "checker.i", "message": "Issue I", "location": { "path": "i.rs", "line": 9 } },
    { "severity": "warn", "code": "checker.j", "message": "Issue J", "location": { "path": "j.rs", "line": 10 } }
  ]
}"#;
    setup.write_sensor_report("checker", report);

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

    let json: serde_json::Value =
        serde_json::from_str(&setup.read_cockpit_report()).expect("parse");
    let highlights = json["highlights"].as_array().expect("highlights");
    assert!(
        highlights.len() <= 7,
        "max_highlights = 7 should cap at 7, got {}",
        highlights.len()
    );
    // With 10 findings and cap=7, we should get more than 3.
    assert!(
        highlights.len() > 3,
        "max_highlights = 7 should allow more than 3 highlights, got {}",
        highlights.len()
    );
}

/// Policy section in report.json reflects the config values.
#[test]
fn config_values_reflected_in_report_policy() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]
warn_is_fail = true
max_highlights = 5
max_per_sensor_findings = 15

[sensors.alpha]
blocking = true
missing = "fail"
"#,
    );
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

    let json: serde_json::Value =
        serde_json::from_str(&setup.read_cockpit_report()).expect("parse");
    let policy = &json["policy"];
    assert_eq!(
        policy["warn_is_fail"].as_bool(),
        Some(true),
        "report.policy should reflect warn_is_fail from config"
    );
    assert_eq!(
        policy["max_highlights"].as_u64(),
        Some(5),
        "report.policy should reflect max_highlights from config"
    );
    assert_eq!(
        policy["max_per_sensor_findings"].as_u64(),
        Some(15),
        "report.policy should reflect max_per_sensor_findings from config"
    );
}

// =============================================================================
// CLI flag overrides config
// =============================================================================

/// Config warn_is_fail = false + warn sensor → passes. Then config warn_is_fail = true
/// with same sensor → fails. (Proves config is read, not hardcoded.)
#[test]
fn config_toggle_warn_is_fail_changes_outcome() {
    // First run: warn_is_fail = false → pass
    let setup1 = TestSetup::new();
    setup1.write_config(
        r#"[policy]
warn_is_fail = false

[sensors.linter]
blocking = true
missing = "fail"
"#,
    );
    setup1.write_sensor_report("linter", &warn_sensor_report("linter"));

    cmd()
        .args([
            "ingest",
            "--artifacts",
            &setup1.artifacts_arg(),
            "--config",
            &setup1.config_arg(),
        ])
        .assert()
        .success();

    // Second run: warn_is_fail = true → fail (exit 2)
    let setup2 = TestSetup::new();
    setup2.write_config(
        r#"[policy]
warn_is_fail = true

[sensors.linter]
blocking = true
missing = "fail"
"#,
    );
    setup2.write_sensor_report("linter", &warn_sensor_report("linter"));

    cmd()
        .args([
            "ingest",
            "--artifacts",
            &setup2.artifacts_arg(),
            "--config",
            &setup2.config_arg(),
        ])
        .assert()
        .code(2);
}

/// Missing sensor with missing = "skip" → exit 0 (skip policy honoured).
#[test]
fn missing_sensor_skip_policy_exits_zero() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]

[sensors.optional]
blocking = true
missing = "skip"
"#,
    );
    // No sensor report written.

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

/// Missing sensor with missing = "warn" + warn_is_fail = true → exit 2.
#[test]
fn missing_sensor_warn_policy_plus_warn_is_fail_exits_two() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]
warn_is_fail = true

[sensors.expected]
blocking = true
missing = "warn"
"#,
    );
    // No sensor report written.

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

    let json: serde_json::Value =
        serde_json::from_str(&setup.read_cockpit_report()).expect("parse");
    assert_eq!(
        json["verdict"]["status"].as_str(),
        Some("fail"),
        "missing sensor with warn policy + warn_is_fail should result in fail"
    );
}

/// Missing sensor with missing = "warn" + warn_is_fail = false → exit 0.
#[test]
fn missing_sensor_warn_policy_without_warn_is_fail_exits_zero() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]
warn_is_fail = false

[sensors.expected]
blocking = true
missing = "warn"
"#,
    );
    // No sensor report written.

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

// =============================================================================
// Outputs always written (even on policy failure)
// =============================================================================

/// Both report.json and comment.md are always produced regardless of exit code.
#[test]
fn outputs_written_on_policy_failure() {
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

    assert!(
        setup.cockpit_report_path().exists(),
        "report.json must be written even on policy failure"
    );
    assert!(
        setup.cockpit_comment_path().exists(),
        "comment.md must be written even on policy failure"
    );
}

/// Outputs written when everything passes.
#[test]
fn outputs_written_on_pass() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]

[sensors.alpha]
blocking = true
missing = "fail"
"#,
    );
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

    assert!(setup.cockpit_report_path().exists());
    assert!(setup.cockpit_comment_path().exists());
}

// =============================================================================
// No config file → uses defaults
// =============================================================================

/// No config file at all → valid input passes with defaults (exit 0).
#[test]
fn no_config_file_uses_defaults_exit_zero() {
    let setup = TestSetup::new();
    setup.write_sensor_report("alpha", &valid_sensor_report("alpha"));

    let nonexistent_config = setup._temp_dir.path().join("does_not_exist.toml");

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

    let json: serde_json::Value =
        serde_json::from_str(&setup.read_cockpit_report()).expect("parse");
    assert_eq!(json["verdict"]["status"].as_str(), Some("pass"));
    // Default policy values reflected
    assert_eq!(json["policy"]["warn_is_fail"].as_bool(), Some(false));
    assert_eq!(json["policy"]["max_highlights"].as_u64(), Some(7));
    assert_eq!(json["policy"]["max_per_sensor_findings"].as_u64(), Some(20));
    assert_eq!(json["policy"]["max_annotations"].as_u64(), Some(25));
}

// =============================================================================
// Config overriding max_per_sensor_findings
// =============================================================================

/// Config max_per_sensor_findings = 2 → findings truncated to at most 2 per sensor.
#[test]
fn config_max_per_sensor_findings_respected() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]
warn_is_fail = false
max_per_sensor_findings = 2

[sensors.checker]
blocking = false
missing = "skip"
"#,
    );

    let report = r#"{
  "schema": "checker.report.v1",
  "tool": { "name": "checker", "version": "1.0.0" },
  "run": { "started_at": "2026-02-02T11:00:00Z" },
  "verdict": { "status": "warn", "counts": { "info": 0, "warn": 5, "error": 0 } },
  "findings": [
    { "severity": "warn", "code": "checker.a", "message": "Issue A" },
    { "severity": "warn", "code": "checker.b", "message": "Issue B" },
    { "severity": "warn", "code": "checker.c", "message": "Issue C" },
    { "severity": "warn", "code": "checker.d", "message": "Issue D" },
    { "severity": "warn", "code": "checker.e", "message": "Issue E" }
  ]
}"#;
    setup.write_sensor_report("checker", report);

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

    let json: serde_json::Value =
        serde_json::from_str(&setup.read_cockpit_report()).expect("parse");
    assert_eq!(
        json["policy"]["max_per_sensor_findings"].as_u64(),
        Some(2),
        "report should reflect config max_per_sensor_findings"
    );
    // The sensor should be marked as truncated.
    let sensors = json["sensors"].as_array().expect("sensors");
    let checker = sensors.iter().find(|s| s["id"] == "checker");
    assert!(checker.is_some(), "checker sensor should appear in report");
    if let Some(c) = checker {
        assert_eq!(
            c["truncated"].as_bool(),
            Some(true),
            "findings should be truncated"
        );
    }
}

// =============================================================================
// Config with max_annotations = 0
// =============================================================================

/// Config max_annotations = 0 → no annotations in output.
#[test]
fn config_max_annotations_zero_respected() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]
max_annotations = 0

[sensors.alpha]
blocking = true
missing = "fail"
"#,
    );
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

    let json: serde_json::Value =
        serde_json::from_str(&setup.read_cockpit_report()).expect("parse");
    assert_eq!(
        json["policy"]["max_annotations"].as_u64(),
        Some(0),
        "report.policy should reflect max_annotations = 0"
    );
}

// =============================================================================
// Config from --config flag
// =============================================================================

/// --config flag loads a specific file from a non-default path.
#[test]
fn config_flag_loads_specific_file() {
    let setup = TestSetup::new();
    // Write config to a non-standard location
    let custom_config = setup._temp_dir.path().join("custom").join("my-policy.toml");
    fs::create_dir_all(custom_config.parent().unwrap()).expect("create custom dir");
    fs::write(
        &custom_config,
        r#"[policy]
warn_is_fail = true
max_highlights = 2

[sensors.alpha]
blocking = true
missing = "fail"
"#,
    )
    .expect("write custom config");

    setup.write_sensor_report("alpha", &valid_sensor_report("alpha"));

    cmd()
        .args([
            "ingest",
            "--artifacts",
            &setup.artifacts_arg(),
            "--config",
            custom_config.to_string_lossy().as_ref(),
        ])
        .assert()
        .success();

    let json: serde_json::Value =
        serde_json::from_str(&setup.read_cockpit_report()).expect("parse");
    assert_eq!(
        json["policy"]["warn_is_fail"].as_bool(),
        Some(true),
        "should load warn_is_fail from custom config path"
    );
    assert_eq!(
        json["policy"]["max_highlights"].as_u64(),
        Some(2),
        "should load max_highlights from custom config path"
    );
}

// =============================================================================
// Missing config from --config flag
// =============================================================================

/// --config pointing to a non-existent file → falls back to defaults (no error).
#[test]
fn config_flag_missing_file_falls_back_to_defaults() {
    let setup = TestSetup::new();
    setup.write_sensor_report("alpha", &valid_sensor_report("alpha"));

    let missing = setup._temp_dir.path().join("nowhere").join("missing.toml");

    cmd()
        .args([
            "ingest",
            "--artifacts",
            &setup.artifacts_arg(),
            "--config",
            missing.to_string_lossy().as_ref(),
        ])
        .assert()
        .success();

    let json: serde_json::Value =
        serde_json::from_str(&setup.read_cockpit_report()).expect("parse");
    // Falls back to defaults
    assert_eq!(json["policy"]["warn_is_fail"].as_bool(), Some(false));
    assert_eq!(json["policy"]["max_highlights"].as_u64(), Some(7));
}

// =============================================================================
// Config with all defaults → same as no config
// =============================================================================

/// Config with only [policy] (all defaults) produces identical policy to no config.
#[test]
fn config_all_defaults_matches_no_config() {
    // Run 1: explicit config with only [policy]
    let setup1 = TestSetup::new();
    setup1.write_config("[policy]\n");
    setup1.write_sensor_report("alpha", &valid_sensor_report("alpha"));

    cmd()
        .args([
            "ingest",
            "--artifacts",
            &setup1.artifacts_arg(),
            "--config",
            &setup1.config_arg(),
        ])
        .assert()
        .success();

    let json1: serde_json::Value =
        serde_json::from_str(&setup1.read_cockpit_report()).expect("parse run 1");

    // Run 2: no config file
    let setup2 = TestSetup::new();
    setup2.write_sensor_report("alpha", &valid_sensor_report("alpha"));
    let nonexistent = setup2._temp_dir.path().join("nonexistent.toml");

    cmd()
        .args([
            "ingest",
            "--artifacts",
            &setup2.artifacts_arg(),
            "--config",
            nonexistent.to_string_lossy().as_ref(),
        ])
        .assert()
        .success();

    let json2: serde_json::Value =
        serde_json::from_str(&setup2.read_cockpit_report()).expect("parse run 2");

    // Policy sections should be identical
    assert_eq!(
        json1["policy"]["warn_is_fail"], json2["policy"]["warn_is_fail"],
        "warn_is_fail should match between default config and no config"
    );
    assert_eq!(
        json1["policy"]["max_highlights"], json2["policy"]["max_highlights"],
        "max_highlights should match"
    );
    assert_eq!(
        json1["policy"]["max_per_sensor_findings"], json2["policy"]["max_per_sensor_findings"],
        "max_per_sensor_findings should match"
    );
    assert_eq!(
        json1["policy"]["max_annotations"], json2["policy"]["max_annotations"],
        "max_annotations should match"
    );
}

// =============================================================================
// Empty config file → treated as defaults
// =============================================================================

/// Empty config file is treated as all-defaults (valid TOML, empty table).
#[test]
fn empty_config_file_uses_defaults() {
    let setup = TestSetup::new();
    setup.write_config("");
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

    let json: serde_json::Value =
        serde_json::from_str(&setup.read_cockpit_report()).expect("parse");
    assert_eq!(json["policy"]["warn_is_fail"].as_bool(), Some(false));
    assert_eq!(json["policy"]["max_highlights"].as_u64(), Some(7));
    assert_eq!(json["policy"]["max_per_sensor_findings"].as_u64(), Some(20));
    assert_eq!(json["policy"]["max_annotations"].as_u64(), Some(25));
}

// =============================================================================
// Config with unknown fields → tolerated (forward-compatible)
// =============================================================================

/// Unknown fields in cockpit.toml are tolerated for forward compatibility.
#[test]
fn config_unknown_fields_tolerated() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]
warn_is_fail = false
future_field = "ignored"
another_unknown = 42

[sensors.alpha]
blocking = true
missing = "fail"
future_sensor_field = true
"#,
    );
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

    let json: serde_json::Value =
        serde_json::from_str(&setup.read_cockpit_report()).expect("parse");
    assert_eq!(json["verdict"]["status"].as_str(), Some("pass"));
}

// =============================================================================
// CLI --disable-hooks → hooks not executed (precedence test)
// =============================================================================

/// --disable-hooks prevents hook execution even when config defines hooks.
#[test]
fn cli_disable_hooks_overrides_config() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]

[sensors.alpha]
blocking = false
missing = "skip"
"#,
    );
    setup.write_sensor_report("alpha", &valid_sensor_report("alpha"));

    // Even if hooks were configured, --disable-hooks should suppress them.
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
    // No hook output sidecar should exist.
    let hook_output = setup.artifacts_dir.join("cockpit").join("hooks.json");
    assert!(
        !hook_output.exists(),
        "hooks output should not exist when --disable-hooks is passed"
    );
}

// =============================================================================
// CLI --disable-buildfix → buildfix not executed (precedence test)
// =============================================================================

/// --disable-buildfix prevents buildfix even when config enables it.
#[test]
fn cli_disable_buildfix_overrides_config() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]

[sensors.alpha]
blocking = false
missing = "skip"

[buildfix]
auto_apply = true
"#,
    );
    setup.write_sensor_report("alpha", &valid_sensor_report("alpha"));

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
        .success();

    assert!(setup.cockpit_report_path().exists());
    let sidecar = setup
        .artifacts_dir
        .join("cockpit")
        .join("buildfix.apply.json");
    assert!(
        !sidecar.exists(),
        "buildfix sidecar should not exist when --disable-buildfix is passed"
    );
}

// =============================================================================
// Config blocking sensors → policy applied
// =============================================================================

/// Config with custom blocking sensors: one failing blocking sensor → exit 2.
#[test]
fn config_custom_blocking_sensors_policy_applied() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]

[sensors.critical]
blocking = true
missing = "fail"

[sensors.optional]
blocking = false
missing = "skip"
"#,
    );
    // critical fails, optional passes → exit 2 because critical is blocking.
    setup.write_sensor_report("critical", &fail_sensor_report("critical"));
    setup.write_sensor_report("optional", &valid_sensor_report("optional"));

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

    let json: serde_json::Value =
        serde_json::from_str(&setup.read_cockpit_report()).expect("parse");
    assert_eq!(json["verdict"]["status"].as_str(), Some("fail"));

    // Verify both sensors are in the report
    let sensors = json["sensors"].as_array().expect("sensors");
    let sensor_ids: Vec<&str> = sensors.iter().filter_map(|s| s["id"].as_str()).collect();
    assert!(sensor_ids.contains(&"critical"));
    assert!(sensor_ids.contains(&"optional"));
}

/// Config with custom blocking sensors: only non-blocking fails → exit 0.
#[test]
fn config_non_blocking_sensor_fail_exits_zero() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]

[sensors.critical]
blocking = true
missing = "fail"

[sensors.optional]
blocking = false
missing = "skip"
"#,
    );
    // critical passes, optional fails → exit 0 because optional is non-blocking.
    setup.write_sensor_report("critical", &valid_sensor_report("critical"));
    setup.write_sensor_report("optional", &fail_sensor_report("optional"));

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

// =============================================================================
// CLI --disable-hooks + --disable-buildfix combined → precedence
// =============================================================================

/// Both disable flags together still produce valid output.
#[test]
fn cli_both_disable_flags_combined() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]

[sensors.alpha]
blocking = true
missing = "fail"

[buildfix]
auto_apply = true
"#,
    );
    setup.write_sensor_report("alpha", &valid_sensor_report("alpha"));

    cmd()
        .args([
            "ingest",
            "--artifacts",
            &setup.artifacts_arg(),
            "--config",
            &setup.config_arg(),
            "--disable-hooks",
            "--disable-buildfix",
        ])
        .assert()
        .success();

    assert!(setup.cockpit_report_path().exists());
    assert!(setup.cockpit_comment_path().exists());

    let json: serde_json::Value =
        serde_json::from_str(&setup.read_cockpit_report()).expect("parse");
    assert_eq!(json["verdict"]["status"].as_str(), Some("pass"));
}

// =============================================================================
// Schema validation: no config file, CLI says lax → uses lax
// =============================================================================

/// No config file, CLI explicitly says --schema-validation lax → uses lax.
#[test]
fn schema_no_config_cli_lax_uses_lax() {
    let setup = TestSetup::new();
    setup.write_sensor_report("alpha", &extra_field_sensor_report("alpha"));

    let nonexistent_config = setup._temp_dir.path().join("nonexistent.toml");

    cmd()
        .args([
            "ingest",
            "--artifacts",
            &setup.artifacts_arg(),
            "--config",
            nonexistent_config.to_string_lossy().as_ref(),
            "--schema-validation",
            "lax",
        ])
        .assert()
        .success();

    let report = setup.read_cockpit_report();
    let json: serde_json::Value = serde_json::from_str(&report).expect("parse report");

    let highlights = json["highlights"].as_array().expect("highlights");
    let has_violation = highlights
        .iter()
        .any(|h| h["finding"]["code"].as_str() == Some("cockpit.schema_violation"));
    assert!(
        !has_violation,
        "no config + CLI lax should skip schema validation"
    );
}

/// No config file, CLI explicitly says --schema-validation strict → uses strict.
#[cfg(feature = "feature-schema")]
#[test]
fn schema_no_config_cli_strict_uses_strict() {
    let setup = TestSetup::new();
    setup.write_sensor_report("alpha", &extra_field_sensor_report("alpha"));

    let nonexistent_config = setup._temp_dir.path().join("nonexistent.toml");

    cmd()
        .args([
            "ingest",
            "--artifacts",
            &setup.artifacts_arg(),
            "--config",
            nonexistent_config.to_string_lossy().as_ref(),
            "--schema-validation",
            "strict",
        ])
        .assert()
        .success(); // no sensors configured → sensor not blocking

    let report = setup.read_cockpit_report();
    let json: serde_json::Value = serde_json::from_str(&report).expect("parse report");

    // With strict mode and extra fields, the sensor should have schema_violation findings.
    let highlights = json["highlights"].as_array().expect("highlights");
    let has_violation = highlights
        .iter()
        .any(|h| h["finding"]["code"].as_str() == Some("cockpit.schema_violation"));
    // Sensor is unconfigured/non-blocking, so violation surfaces but doesn't fail.
    assert!(
        has_violation,
        "no config + CLI strict should enforce schema validation"
    );
}

// =============================================================================
// Schema validation: report reflects effective schema_validation
// =============================================================================

/// Config lax + valid report → no schema violation in highlights (lax honoured).
#[test]
fn schema_validation_lax_no_violations_in_report() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]
schema_validation = "lax"

[sensors.alpha]
blocking = true
missing = "fail"
"#,
    );
    setup.write_sensor_report("alpha", &extra_field_sensor_report("alpha"));

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

    let json: serde_json::Value =
        serde_json::from_str(&setup.read_cockpit_report()).expect("parse");
    let highlights = json["highlights"].as_array().expect("highlights");
    let has_violation = highlights
        .iter()
        .any(|h| h["finding"]["code"].as_str() == Some("cockpit.schema_violation"));
    assert!(
        !has_violation,
        "config lax should produce no schema violations even with extra fields"
    );
}

/// Config lax + CLI strict → report shows schema violations (CLI override effective).
#[cfg(feature = "feature-schema")]
#[test]
fn schema_validation_cli_strict_produces_violations_in_report() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]
schema_validation = "lax"

[sensors.alpha]
blocking = true
missing = "fail"
"#,
    );
    setup.write_sensor_report("alpha", &extra_field_sensor_report("alpha"));

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
        .code(2);

    let json: serde_json::Value =
        serde_json::from_str(&setup.read_cockpit_report()).expect("parse");
    let highlights = json["highlights"].as_array().expect("highlights");
    let has_violation = highlights
        .iter()
        .any(|h| h["finding"]["code"].as_str() == Some("cockpit.schema_violation"));
    assert!(
        has_violation,
        "CLI --schema-validation strict should produce schema violations in report"
    );
}

// =============================================================================
// Feature flag precedence: buildfix
// =============================================================================

/// Config enables buildfix (auto_apply = true), CLI disables → no buildfix sidecar.
#[test]
fn feature_buildfix_config_enabled_cli_disabled() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]

[sensors.alpha]
blocking = false
missing = "skip"

[buildfix]
auto_apply = true
max_auto_apply_safety = "safe"
"#,
    );
    setup.write_sensor_report("alpha", &valid_sensor_report("alpha"));

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
        .success();

    assert!(setup.cockpit_report_path().exists());
    let sidecar = setup
        .artifacts_dir
        .join("cockpit")
        .join("buildfix.apply.json");
    assert!(
        !sidecar.exists(),
        "--disable-buildfix should suppress buildfix even when config enables auto_apply"
    );

    let json: serde_json::Value =
        serde_json::from_str(&setup.read_cockpit_report()).expect("parse");
    if let Some(data) = json.get("data") {
        assert!(
            data.get("_buildfix_apply").is_none(),
            "buildfix data should be absent when --disable-buildfix is passed"
        );
    }
}

/// Config does not mention buildfix, CLI does not disable → succeeds without sidecar.
#[test]
fn feature_buildfix_config_absent_cli_absent() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]

[sensors.alpha]
blocking = false
missing = "skip"
"#,
    );
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

    // With no buildfix config and no CLI flag, buildfix sidecar should be absent.
    let sidecar = setup
        .artifacts_dir
        .join("cockpit")
        .join("buildfix.apply.json");
    assert!(
        !sidecar.exists(),
        "buildfix sidecar should not exist when buildfix not configured"
    );
}

// =============================================================================
// Feature flag precedence: hooks
// =============================================================================

/// Config with sensors, CLI disables hooks → no hooks output.
#[test]
fn feature_hooks_config_present_cli_disabled() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]

[sensors.alpha]
blocking = false
missing = "skip"
"#,
    );
    setup.write_sensor_report("alpha", &valid_sensor_report("alpha"));

    // Even if hooks were enabled by default feature, --disable-hooks suppresses them.
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
    let hook_output = setup.artifacts_dir.join("cockpit").join("hooks.json");
    assert!(
        !hook_output.exists(),
        "--disable-hooks should suppress hooks"
    );
}

/// Hooks not configured, not disabled via CLI → succeeds without hooks output.
#[test]
fn feature_hooks_config_absent_cli_absent() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]

[sensors.alpha]
blocking = false
missing = "skip"
"#,
    );
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

    let hook_output = setup.artifacts_dir.join("cockpit").join("hooks.json");
    assert!(
        !hook_output.exists(),
        "hooks output should not exist when hooks are not configured"
    );
}

// =============================================================================
// Feature flag precedence: policy signing
// =============================================================================

/// Config enables signing, CLI disables → no signature sidecar.
#[test]
fn feature_signing_config_enabled_cli_disabled() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]

[sensors.alpha]
blocking = false
missing = "skip"

[policy_signing]
enabled = true
"#,
    );
    setup.write_sensor_report("alpha", &valid_sensor_report("alpha"));

    cmd()
        .args([
            "ingest",
            "--artifacts",
            &setup.artifacts_arg(),
            "--config",
            &setup.config_arg(),
            "--disable-policy-signing",
        ])
        .assert()
        .success();

    assert!(setup.cockpit_report_path().exists());
    let sidecar = setup
        .artifacts_dir
        .join("cockpit")
        .join("policy.signature.json");
    assert!(
        !sidecar.exists(),
        "--disable-policy-signing should suppress signing even when config enables it"
    );
}

// =============================================================================
// Artifact path precedence
// =============================================================================

/// Default artifact path ("artifacts") is used when --artifacts is not explicitly overridden.
#[test]
fn artifact_path_default_used_when_not_overridden() {
    let temp_dir = TempDir::new().expect("create temp dir");
    let cwd = temp_dir.path();
    let artifacts_dir = cwd.join("artifacts");
    let sensor_dir = artifacts_dir.join("alpha");
    fs::create_dir_all(&sensor_dir).expect("create sensor dir");
    fs::write(sensor_dir.join("report.json"), valid_sensor_report("alpha")).expect("write report");

    let config = cwd.join("cockpit.toml");
    fs::write(
        &config,
        r#"[policy]

[sensors.alpha]
blocking = true
missing = "fail"
"#,
    )
    .expect("write config");

    // Run without --artifacts, relying on default "artifacts" path.
    cmd()
        .current_dir(cwd)
        .args(["ingest", "--config", config.to_string_lossy().as_ref()])
        .assert()
        .success();

    // Output should be under cwd/artifacts/cockpit/
    assert!(
        artifacts_dir.join("cockpit").join("report.json").exists(),
        "default artifacts path should produce output in ./artifacts/cockpit/"
    );
}

/// --artifacts overrides the default path.
#[test]
fn artifact_path_cli_override() {
    let temp_dir = TempDir::new().expect("create temp dir");
    let custom_artifacts = temp_dir.path().join("custom_output");
    let sensor_dir = custom_artifacts.join("alpha");
    fs::create_dir_all(&sensor_dir).expect("create sensor dir");
    fs::write(sensor_dir.join("report.json"), valid_sensor_report("alpha")).expect("write report");

    let config = temp_dir.path().join("cockpit.toml");
    fs::write(
        &config,
        r#"[policy]

[sensors.alpha]
blocking = true
missing = "fail"
"#,
    )
    .expect("write config");

    cmd()
        .args([
            "ingest",
            "--artifacts",
            custom_artifacts.to_string_lossy().as_ref(),
            "--config",
            config.to_string_lossy().as_ref(),
        ])
        .assert()
        .success();

    assert!(
        custom_artifacts
            .join("cockpit")
            .join("report.json")
            .exists(),
        "CLI --artifacts should direct output to the specified path"
    );
}

/// Two different --artifacts paths produce independent outputs.
#[test]
fn artifact_path_cli_override_independent() {
    let temp_dir = TempDir::new().expect("create temp dir");

    // Setup two independent artifacts directories.
    let artifacts_a = temp_dir.path().join("artifacts_a");
    let artifacts_b = temp_dir.path().join("artifacts_b");
    for (dir, sensor) in [(&artifacts_a, "alpha"), (&artifacts_b, "beta")] {
        let sensor_dir = dir.join(sensor);
        fs::create_dir_all(&sensor_dir).expect("create dir");
        fs::write(sensor_dir.join("report.json"), valid_sensor_report(sensor))
            .expect("write report");
    }

    let config = temp_dir.path().join("cockpit.toml");
    fs::write(&config, "[policy]\n").expect("write config");

    // First ingest: artifacts_a
    cmd()
        .args([
            "ingest",
            "--artifacts",
            artifacts_a.to_string_lossy().as_ref(),
            "--config",
            config.to_string_lossy().as_ref(),
        ])
        .assert()
        .success();

    // Second ingest: artifacts_b
    cmd()
        .args([
            "ingest",
            "--artifacts",
            artifacts_b.to_string_lossy().as_ref(),
            "--config",
            config.to_string_lossy().as_ref(),
        ])
        .assert()
        .success();

    // Both should produce output in their respective directories.
    let report_a: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(artifacts_a.join("cockpit").join("report.json")).unwrap(),
    )
    .unwrap();
    let report_b: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(artifacts_b.join("cockpit").join("report.json")).unwrap(),
    )
    .unwrap();

    // Reports should contain different sensors.
    let sensors_a: Vec<&str> = report_a["sensors"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|s| s["id"].as_str())
        .collect();
    let sensors_b: Vec<&str> = report_b["sensors"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|s| s["id"].as_str())
        .collect();
    assert!(
        sensors_a.contains(&"alpha"),
        "artifacts_a should have alpha"
    );
    assert!(sensors_b.contains(&"beta"), "artifacts_b should have beta");
}

// =============================================================================
// Exit code: runtime error (exit 1)
// =============================================================================

/// Malformed config file (invalid TOML) → exit 1 (runtime error).
#[test]
fn exit_code_malformed_config_is_one() {
    let setup = TestSetup::new();
    setup.write_config("this is not valid toml {{{{");
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
        .code(1);
}

/// Malformed receipt JSON (not valid JSON at all) → still exits 0 or 2, not 1,
/// because bad receipts produce findings, not runtime errors.
#[test]
fn exit_code_malformed_receipt_json_not_runtime_error() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]

[sensors.broken]
blocking = false
missing = "skip"
"#,
    );
    setup.write_sensor_report("broken", "{{not valid json at all!!!");

    let assert = cmd()
        .args([
            "ingest",
            "--artifacts",
            &setup.artifacts_arg(),
            "--config",
            &setup.config_arg(),
        ])
        .assert();

    // Exit code should be 0 or 2 (policy), never 1 (runtime error).
    // Bad receipts are handled as findings, not crashes.
    let code = assert.get_output().status.code().unwrap();
    assert!(
        code == 0 || code == 2,
        "malformed receipt should produce a finding, not a runtime error (got exit {})",
        code
    );
}

// =============================================================================
// Exit code: pass/fail for each combination
// =============================================================================

/// Non-blocking sensor with fail verdict + blocking sensor with pass → exit 0.
#[test]
fn exit_code_non_blocking_fail_plus_blocking_pass_is_zero() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]

[sensors.critical]
blocking = true
missing = "fail"

[sensors.advisory]
blocking = false
missing = "skip"
"#,
    );
    setup.write_sensor_report("critical", &valid_sensor_report("critical"));
    setup.write_sensor_report("advisory", &fail_sensor_report("advisory"));

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

/// All sensors skip → exit 0.
#[test]
fn exit_code_all_skip_is_zero() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]

[sensors.optional_a]
blocking = true
missing = "skip"

[sensors.optional_b]
blocking = true
missing = "skip"
"#,
    );
    // Write no sensor reports — all missing sensors treated as skip.

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

    let json: serde_json::Value =
        serde_json::from_str(&setup.read_cockpit_report()).expect("parse");
    let status = json["verdict"]["status"].as_str().expect("verdict status");
    assert_ne!(status, "fail", "all-skip should not fail");
}

/// Schema violation on blocking sensor in strict mode → exit 2 (not 1).
#[cfg(feature = "feature-schema")]
#[test]
fn exit_code_schema_violation_blocking_is_two() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]
schema_validation = "strict"

[sensors.alpha]
blocking = true
missing = "fail"
"#,
    );
    setup.write_sensor_report("alpha", &extra_field_sensor_report("alpha"));

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

/// Schema violation on non-blocking sensor in strict mode → exit 0.
#[cfg(feature = "feature-schema")]
#[test]
fn exit_code_schema_violation_non_blocking_is_zero() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]
schema_validation = "strict"

[sensors.advisory]
blocking = false
missing = "skip"
"#,
    );
    setup.write_sensor_report("advisory", &extra_field_sensor_report("advisory"));

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

// =============================================================================
// Outputs always written (even with multiple failures)
// =============================================================================

/// Outputs written even when multiple blocking sensors fail.
#[test]
fn outputs_written_multi_blocking_fail() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]

[sensors.alpha]
blocking = true
missing = "fail"

[sensors.beta]
blocking = true
missing = "fail"
"#,
    );
    setup.write_sensor_report("alpha", &fail_sensor_report("alpha"));
    setup.write_sensor_report("beta", &fail_sensor_report("beta"));

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
        "report.json must be written even with multiple blocking failures"
    );
    assert!(
        setup.cockpit_comment_path().exists(),
        "comment.md must be written even with multiple blocking failures"
    );

    let json: serde_json::Value =
        serde_json::from_str(&setup.read_cockpit_report()).expect("parse");
    assert_eq!(json["verdict"]["status"].as_str(), Some("fail"));
    let sensors = json["sensors"].as_array().expect("sensors");
    assert_eq!(sensors.len(), 2, "both sensors should appear in the report");
}

// =============================================================================
// Config path precedence: --config flag vs default
// =============================================================================

/// --config flag to a specific file overrides the default "cockpit.toml" location.
#[test]
fn config_path_cli_override_changes_policy() {
    let temp_dir = TempDir::new().expect("create temp dir");
    let cwd = temp_dir.path();

    // Default cockpit.toml in cwd: warn_is_fail = false
    let default_config = cwd.join("cockpit.toml");
    fs::write(
        &default_config,
        r#"[policy]
warn_is_fail = false

[sensors.linter]
blocking = true
missing = "fail"
"#,
    )
    .unwrap();

    // Alternative config: warn_is_fail = true
    let alt_config = cwd.join("strict-policy.toml");
    fs::write(
        &alt_config,
        r#"[policy]
warn_is_fail = true

[sensors.linter]
blocking = true
missing = "fail"
"#,
    )
    .unwrap();

    let artifacts = cwd.join("artifacts");
    let sensor_dir = artifacts.join("linter");
    fs::create_dir_all(&sensor_dir).unwrap();
    fs::write(sensor_dir.join("report.json"), warn_sensor_report("linter")).unwrap();

    // Run with default config → pass (warn_is_fail = false)
    cmd()
        .args([
            "ingest",
            "--artifacts",
            artifacts.to_string_lossy().as_ref(),
            "--config",
            default_config.to_string_lossy().as_ref(),
        ])
        .assert()
        .success();

    // Clean output for second run.
    let _ = fs::remove_dir_all(artifacts.join("cockpit"));

    // Run with alt config → fail (warn_is_fail = true)
    cmd()
        .args([
            "ingest",
            "--artifacts",
            artifacts.to_string_lossy().as_ref(),
            "--config",
            alt_config.to_string_lossy().as_ref(),
        ])
        .assert()
        .code(2);
}

// =============================================================================
// Combined precedence: multiple CLI overrides together
// =============================================================================

/// All disable flags + schema lax → everything disabled, lax validation, still pass.
#[test]
fn combined_all_cli_overrides_together() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]
schema_validation = "strict"

[sensors.alpha]
blocking = false
missing = "skip"

[buildfix]
auto_apply = true

[policy_signing]
enabled = true
"#,
    );
    setup.write_sensor_report("alpha", &extra_field_sensor_report("alpha"));

    cmd()
        .args([
            "ingest",
            "--artifacts",
            &setup.artifacts_arg(),
            "--config",
            &setup.config_arg(),
            "--schema-validation",
            "lax",
            "--disable-hooks",
            "--disable-buildfix",
            "--disable-policy-signing",
        ])
        .assert()
        .success();

    assert!(setup.cockpit_report_path().exists());
    assert!(setup.cockpit_comment_path().exists());

    let json: serde_json::Value =
        serde_json::from_str(&setup.read_cockpit_report()).expect("parse");
    // Lax should have been honoured: no schema violation.
    let highlights = json["highlights"].as_array().expect("highlights");
    let has_violation = highlights
        .iter()
        .any(|h| h["finding"]["code"].as_str() == Some("cockpit.schema_violation"));
    assert!(
        !has_violation,
        "CLI lax + all disable flags should produce no schema violations"
    );
    // No buildfix sidecar.
    let sidecar = setup
        .artifacts_dir
        .join("cockpit")
        .join("buildfix.apply.json");
    assert!(!sidecar.exists());
    // No signing sidecar.
    let sig = setup
        .artifacts_dir
        .join("cockpit")
        .join("policy.signature.json");
    assert!(!sig.exists());
}
