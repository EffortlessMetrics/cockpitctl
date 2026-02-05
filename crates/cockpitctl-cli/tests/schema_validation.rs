//! Integration tests for schema validation feature.
//!
//! These tests verify the full pipeline with strict schema validation enabled,
//! ensuring that schema violations are properly surfaced as findings.

use assert_cmd::Command;
use std::fs;
use tempfile::TempDir;

/// Create the ingest command with common settings.
fn cmd() -> Command {
    let mut cmd = assert_cmd::cargo_bin_cmd!("cockpitctl");
    cmd.env("COCKPITCTL_STARTED_AT", "2026-02-02T12:00:00Z");
    cmd
}

/// Minimal valid sensor report that conforms to sensor.report.v1 schema.
fn valid_sensor_report() -> &'static str {
    r#"{
  "schema": "sensor.report.v1",
  "tool": { "name": "test-sensor", "version": "1.0.0" },
  "run": { "started_at": "2026-02-02T11:00:00Z" },
  "verdict": { "status": "pass", "counts": { "info": 0, "warn": 0, "error": 0 } },
  "findings": []
}"#
}

/// Invalid sensor report: missing required "schema" field.
fn invalid_report_missing_schema() -> &'static str {
    r#"{
  "tool": { "name": "bad-sensor", "version": "1.0.0" },
  "run": { "started_at": "2026-02-02T11:00:00Z" },
  "verdict": { "status": "pass", "counts": { "info": 0, "warn": 0, "error": 0 } },
  "findings": []
}"#
}

/// Invalid sensor report: invalid verdict status (not one of pass/warn/fail/skip).
fn invalid_report_bad_status() -> &'static str {
    r#"{
  "schema": "sensor.report.v1",
  "tool": { "name": "bad-sensor", "version": "1.0.0" },
  "run": { "started_at": "2026-02-02T11:00:00Z" },
  "verdict": { "status": "invalid_status", "counts": { "info": 0, "warn": 0, "error": 0 } },
  "findings": []
}"#
}

/// Invalid sensor report: has additional properties not allowed by schema.
fn invalid_report_extra_field() -> &'static str {
    r#"{
  "schema": "sensor.report.v1",
  "tool": { "name": "bad-sensor", "version": "1.0.0" },
  "run": { "started_at": "2026-02-02T11:00:00Z" },
  "verdict": { "status": "pass", "counts": { "info": 0, "warn": 0, "error": 0 } },
  "findings": [],
  "extra_not_allowed": "this violates additionalProperties: false"
}"#
}

/// Invalid sensor report: finding missing required "code" field.
fn invalid_report_finding_missing_code() -> &'static str {
    r#"{
  "schema": "sensor.report.v1",
  "tool": { "name": "bad-sensor", "version": "1.0.0" },
  "run": { "started_at": "2026-02-02T11:00:00Z" },
  "verdict": { "status": "warn", "counts": { "info": 0, "warn": 1, "error": 0 } },
  "findings": [
    {
      "severity": "warn",
      "message": "missing code field"
    }
  ]
}"#
}

/// Cockpit.toml with strict schema validation enabled.
fn strict_config_single_sensor(sensor_id: &str) -> String {
    format!(
        r#"[policy]
schema_validation = "strict"

[sensors.{sensor_id}]
blocking = true
missing = "fail"
"#
    )
}

/// Cockpit.toml with strict schema validation and multiple sensors.
fn strict_config_multi_sensor(sensors: &[&str]) -> String {
    let mut config = String::from(
        r#"[policy]
schema_validation = "strict"

"#,
    );
    for sensor_id in sensors {
        config.push_str(&format!(
            r#"[sensors.{sensor_id}]
blocking = true
missing = "fail"

"#
        ));
    }
    config
}

/// Cockpit.toml with lax schema validation (for comparison).
fn lax_config_single_sensor(sensor_id: &str) -> String {
    format!(
        r#"[policy]
schema_validation = "lax"

[sensors.{sensor_id}]
blocking = true
missing = "fail"
"#
    )
}

/// Set up a test directory with artifacts and config.
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

    fn read_cockpit_report(&self) -> String {
        let path = self.artifacts_dir.join("cockpit").join("report.json");
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

// =============================================================================
// Test: Valid receipt passes in strict mode
// =============================================================================

#[test]
fn valid_receipt_passes_in_strict_mode() {
    let setup = TestSetup::new();
    setup.write_config(&strict_config_single_sensor("validsensor"));
    setup.write_sensor_report("validsensor", valid_sensor_report());

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
    let json: serde_json::Value = serde_json::from_str(&report).expect("parse cockpit report");

    // Verify overall verdict is pass.
    assert_eq!(
        json["verdict"]["status"].as_str(),
        Some("pass"),
        "valid receipt should result in pass verdict"
    );

    // Verify no schema violation errors.
    let sensors = json["sensors"].as_array().expect("sensors array");
    assert_eq!(sensors.len(), 1);
    let sensor = &sensors[0];
    assert!(
        sensor["errors"]
            .as_array()
            .map(|a| a.is_empty())
            .unwrap_or(true),
        "valid receipt should have no errors"
    );

    // Verify no SCHEMA_VIOLATION in highlights.
    let highlights = json["highlights"].as_array().expect("highlights array");
    for h in highlights {
        let code = h["finding"]["code"].as_str().unwrap_or("");
        assert_ne!(
            code, "cockpit.schema_violation",
            "valid receipt should not have SCHEMA_VIOLATION highlight"
        );
    }
}

// =============================================================================
// Test: Invalid receipt (missing schema field) produces SCHEMA_VIOLATION
// =============================================================================

#[test]
fn invalid_receipt_missing_schema_produces_schema_violation() {
    let setup = TestSetup::new();
    setup.write_config(&strict_config_single_sensor("badsensor"));
    setup.write_sensor_report("badsensor", invalid_report_missing_schema());

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
        .code(2); // Policy fail exit code

    let report = setup.read_cockpit_report();
    let json: serde_json::Value = serde_json::from_str(&report).expect("parse cockpit report");

    // Verify overall verdict is fail (blocking sensor with schema violation).
    assert_eq!(
        json["verdict"]["status"].as_str(),
        Some("fail"),
        "schema violation on blocking sensor should result in fail verdict"
    );

    // Verify the sensor summary has schema_violation in reasons.
    let sensors = json["sensors"].as_array().expect("sensors array");
    let sensor = sensors
        .iter()
        .find(|s| s["id"].as_str() == Some("badsensor"))
        .expect("badsensor in sensors");

    assert_eq!(
        sensor["verdict"]["status"].as_str(),
        Some("fail"),
        "sensor verdict should be fail"
    );

    let reasons = sensor["verdict"]["reasons"]
        .as_array()
        .expect("reasons array");
    assert!(
        reasons
            .iter()
            .any(|r| r.as_str() == Some("schema_violation")),
        "sensor reasons should include 'schema_violation': {:?}",
        reasons
    );

    // Verify SCHEMA_VIOLATION finding in highlights.
    let highlights = json["highlights"].as_array().expect("highlights array");
    let schema_violation = highlights
        .iter()
        .find(|h| h["finding"]["code"].as_str() == Some("cockpit.schema_violation"));

    assert!(
        schema_violation.is_some(),
        "should have SCHEMA_VIOLATION highlight"
    );

    let violation = schema_violation.unwrap();
    assert_eq!(
        violation["sensor_id"].as_str(),
        Some("badsensor"),
        "violation should be for badsensor"
    );

    // Verify the message mentions the missing field.
    let message = violation["finding"]["message"].as_str().expect("message");
    assert!(
        message.contains("schema") || message.contains("badsensor"),
        "violation message should mention schema or sensor: {}",
        message
    );
}

// =============================================================================
// Test: Invalid receipt (bad verdict status) produces SCHEMA_VIOLATION
// =============================================================================

#[test]
fn invalid_receipt_bad_status_produces_schema_violation() {
    let setup = TestSetup::new();
    setup.write_config(&strict_config_single_sensor("badstatus"));
    setup.write_sensor_report("badstatus", invalid_report_bad_status());

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
    let json: serde_json::Value = serde_json::from_str(&report).expect("parse cockpit report");

    // Verify schema_violation finding exists.
    let highlights = json["highlights"].as_array().expect("highlights array");
    let has_violation = highlights
        .iter()
        .any(|h| h["finding"]["code"].as_str() == Some("cockpit.schema_violation"));

    assert!(
        has_violation,
        "invalid verdict status should produce SCHEMA_VIOLATION"
    );
}

// =============================================================================
// Test: Invalid receipt (extra field) produces SCHEMA_VIOLATION
// =============================================================================

#[test]
fn invalid_receipt_extra_field_produces_schema_violation() {
    let setup = TestSetup::new();
    setup.write_config(&strict_config_single_sensor("extrafield"));
    setup.write_sensor_report("extrafield", invalid_report_extra_field());

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
    let json: serde_json::Value = serde_json::from_str(&report).expect("parse cockpit report");

    // Verify schema_violation finding exists.
    let highlights = json["highlights"].as_array().expect("highlights array");
    let has_violation = highlights
        .iter()
        .any(|h| h["finding"]["code"].as_str() == Some("cockpit.schema_violation"));

    assert!(
        has_violation,
        "extra field (additionalProperties violation) should produce SCHEMA_VIOLATION"
    );
}

// =============================================================================
// Test: Invalid receipt (finding missing code) produces SCHEMA_VIOLATION
// =============================================================================

#[test]
fn invalid_receipt_finding_missing_code_produces_schema_violation() {
    let setup = TestSetup::new();
    setup.write_config(&strict_config_single_sensor("badfindings"));
    setup.write_sensor_report("badfindings", invalid_report_finding_missing_code());

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
    let json: serde_json::Value = serde_json::from_str(&report).expect("parse cockpit report");

    // Verify schema_violation finding exists.
    let highlights = json["highlights"].as_array().expect("highlights array");
    let has_violation = highlights
        .iter()
        .any(|h| h["finding"]["code"].as_str() == Some("cockpit.schema_violation"));

    assert!(
        has_violation,
        "finding missing required code field should produce SCHEMA_VIOLATION"
    );
}

// =============================================================================
// Test: Lax mode does not validate schema (invalid receipt passes parsing)
// =============================================================================

#[test]
fn lax_mode_skips_schema_validation() {
    let setup = TestSetup::new();
    setup.write_config(&lax_config_single_sensor("laxsensor"));
    // Use a receipt that violates schema (extra field) but can still parse as SensorReport
    // Note: This will fail parsing because serde doesn't allow unknown fields by default.
    // Let's use a valid-looking receipt that would fail strict schema but parses as JSON.
    // Actually, the serde deserializer is more lenient than the JSON schema, so we need
    // something that serde accepts but the schema rejects.
    // The "extra_not_allowed" field will be ignored by serde (no deny_unknown_fields).
    setup.write_sensor_report("laxsensor", invalid_report_extra_field());

    // With lax CLI flag, schema validation is skipped entirely.
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
    let json: serde_json::Value = serde_json::from_str(&report).expect("parse cockpit report");

    // Verify no SCHEMA_VIOLATION in highlights (lax mode skips validation).
    let highlights = json["highlights"].as_array().expect("highlights array");
    let has_violation = highlights
        .iter()
        .any(|h| h["finding"]["code"].as_str() == Some("cockpit.schema_violation"));

    assert!(
        !has_violation,
        "lax mode should not produce SCHEMA_VIOLATION findings"
    );
}

// =============================================================================
// Test: Mixed valid and invalid receipts
// =============================================================================

#[test]
fn mixed_valid_and_invalid_receipts_in_strict_mode() {
    let setup = TestSetup::new();
    setup.write_config(&strict_config_multi_sensor(&["goodsensor", "badsensor"]));
    setup.write_sensor_report("goodsensor", valid_sensor_report());
    setup.write_sensor_report("badsensor", invalid_report_missing_schema());

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
        .code(2); // Fail due to badsensor

    let report = setup.read_cockpit_report();
    let json: serde_json::Value = serde_json::from_str(&report).expect("parse cockpit report");

    let sensors = json["sensors"].as_array().expect("sensors array");

    // Find goodsensor - should be pass.
    let goodsensor = sensors
        .iter()
        .find(|s| s["id"].as_str() == Some("goodsensor"))
        .expect("goodsensor in sensors");
    assert_eq!(
        goodsensor["verdict"]["status"].as_str(),
        Some("pass"),
        "goodsensor should pass"
    );

    // Find badsensor - should be fail with schema_violation.
    let badsensor = sensors
        .iter()
        .find(|s| s["id"].as_str() == Some("badsensor"))
        .expect("badsensor in sensors");
    assert_eq!(
        badsensor["verdict"]["status"].as_str(),
        Some("fail"),
        "badsensor should fail"
    );

    let reasons = badsensor["verdict"]["reasons"].as_array().expect("reasons");
    assert!(
        reasons
            .iter()
            .any(|r| r.as_str() == Some("schema_violation")),
        "badsensor should have schema_violation reason"
    );
}

// =============================================================================
// Test: Non-blocking sensor with schema violation does not fail overall
// =============================================================================

#[test]
fn non_blocking_sensor_schema_violation_does_not_fail_overall() {
    let setup = TestSetup::new();

    // Config with non-blocking sensor.
    let config = r#"[policy]
schema_validation = "strict"

[sensors.nonblocking]
blocking = false
missing = "warn"
"#;
    setup.write_config(config);
    setup.write_sensor_report("nonblocking", invalid_report_missing_schema());

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
        .success(); // Exit 0 because sensor is non-blocking

    let report = setup.read_cockpit_report();
    let json: serde_json::Value = serde_json::from_str(&report).expect("parse cockpit report");

    // Overall verdict should be pass (non-blocking sensor doesn't affect it).
    assert_eq!(
        json["verdict"]["status"].as_str(),
        Some("pass"),
        "non-blocking sensor failure should not affect overall verdict"
    );

    // But the sensor itself should still show fail.
    let sensors = json["sensors"].as_array().expect("sensors array");
    let sensor = &sensors[0];
    assert_eq!(
        sensor["verdict"]["status"].as_str(),
        Some("fail"),
        "sensor itself should still show fail"
    );

    // And there should still be a SCHEMA_VIOLATION highlight.
    let highlights = json["highlights"].as_array().expect("highlights array");
    let has_violation = highlights
        .iter()
        .any(|h| h["finding"]["code"].as_str() == Some("cockpit.schema_violation"));
    assert!(
        has_violation,
        "should still have SCHEMA_VIOLATION highlight for non-blocking sensor"
    );
}

// =============================================================================
// Test: Config says strict but CLI says lax = no validation
// =============================================================================

#[test]
fn config_strict_cli_lax_skips_validation() {
    let setup = TestSetup::new();
    // Config says strict.
    setup.write_config(&strict_config_single_sensor("testsensor"));
    // Report violates schema.
    setup.write_sensor_report("testsensor", invalid_report_extra_field());

    // CLI says lax - this should skip validation entirely (NoOpSchemaValidator).
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
    let json: serde_json::Value = serde_json::from_str(&report).expect("parse cockpit report");

    // No schema violation because CLI flag takes precedence (NoOpSchemaValidator used).
    let highlights = json["highlights"].as_array().expect("highlights array");
    let has_violation = highlights
        .iter()
        .any(|h| h["finding"]["code"].as_str() == Some("cockpit.schema_violation"));
    assert!(
        !has_violation,
        "CLI lax flag should skip validation even if config says strict"
    );
}

// =============================================================================
// Test: Config says lax but CLI says strict = strict validation (CLI override)
// =============================================================================

#[test]
fn config_lax_cli_strict_overrides_config() {
    let setup = TestSetup::new();
    // Config says lax.
    setup.write_config(&lax_config_single_sensor("testsensor"));
    // Report violates schema.
    setup.write_sensor_report("testsensor", invalid_report_extra_field());

    // CLI says strict - this should enforce validation.
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
    let json: serde_json::Value = serde_json::from_str(&report).expect("parse cockpit report");

    let highlights = json["highlights"].as_array().expect("highlights array");
    let has_violation = highlights
        .iter()
        .any(|h| h["finding"]["code"].as_str() == Some("cockpit.schema_violation"));
    assert!(
        has_violation,
        "CLI strict should override config lax and produce SCHEMA_VIOLATION"
    );
}

// =============================================================================
// Test: Verify schema violation errors array contains details
// =============================================================================

#[test]
fn schema_violation_errors_contain_details() {
    let setup = TestSetup::new();
    setup.write_config(&strict_config_single_sensor("detailed"));
    setup.write_sensor_report("detailed", invalid_report_missing_schema());

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
    let json: serde_json::Value = serde_json::from_str(&report).expect("parse cockpit report");

    let sensors = json["sensors"].as_array().expect("sensors array");
    let sensor = &sensors[0];

    // The errors array should contain the validation error details.
    let errors = sensor["errors"].as_array().expect("errors array");
    assert!(
        !errors.is_empty(),
        "sensor errors should contain schema validation details"
    );

    // At least one error should mention "schema" or "required".
    let error_text = errors
        .iter()
        .filter_map(|e| e.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    assert!(
        error_text.contains("schema")
            || error_text.contains("required")
            || error_text.contains("\"schema\""),
        "error details should mention the validation issue: {}",
        error_text
    );
}
