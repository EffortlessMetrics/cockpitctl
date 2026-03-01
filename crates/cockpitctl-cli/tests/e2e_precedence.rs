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
