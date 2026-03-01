//! Schema validator adapter tests for cockpitctl-io-schema.
//!
//! Tests the `JsonSchemaValidator` against valid/invalid documents for both
//! sensor.report.v1 and cockpit.report.v1, plus edge cases.

use cockpitctl_ingest::{SchemaValidationResult, SchemaValidator};
use cockpitctl_io_schema::JsonSchemaValidator;

// ═══════════════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════════════

fn minimal_sensor_receipt() -> &'static str {
    r#"{
      "schema": "sensor.report.v1",
      "tool": { "name": "test-tool", "version": "1.0.0" },
      "run": { "started_at": "2026-01-01T00:00:00Z" },
      "verdict": { "status": "pass", "counts": { "info": 0, "warn": 0, "error": 0 } },
      "findings": []
    }"#
}

fn minimal_cockpit_report() -> &'static str {
    r#"{
      "schema": "cockpit.report.v1",
      "tool": { "name": "cockpitctl", "version": "0.1.0" },
      "run": { "started_at": "2026-01-01T00:00:00Z" },
      "verdict": { "status": "pass", "counts": { "info": 0, "warn": 0, "error": 0 } },
      "sensors": [],
      "highlights": [],
      "policy": {
        "warn_is_fail": false,
        "max_highlights": 5,
        "max_per_sensor_findings": 10,
        "section_order": [],
        "sensors": []
      }
    }"#
}

fn assert_valid(result: SchemaValidationResult) {
    match result {
        SchemaValidationResult::Valid => {}
        SchemaValidationResult::Invalid(errors) => {
            panic!("expected Valid, got Invalid: {}", errors.join(" | "));
        }
    }
}

fn assert_invalid_containing(result: SchemaValidationResult, needle: &str) {
    match result {
        SchemaValidationResult::Invalid(errors) => {
            let joined = errors.join(" | ");
            assert!(
                joined.to_lowercase().contains(&needle.to_lowercase()),
                "expected errors to mention '{needle}', got: {joined}"
            );
        }
        SchemaValidationResult::Valid => {
            panic!("expected Invalid containing '{needle}', got Valid");
        }
    }
}

fn assert_invalid(result: SchemaValidationResult) {
    assert!(
        matches!(result, SchemaValidationResult::Invalid(_)),
        "expected Invalid, got Valid"
    );
}

// ═══════════════════════════════════════════════════════════════════════
// 1. Valid sensor receipt passes
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn valid_sensor_receipt_passes_validation() {
    let v = JsonSchemaValidator::sensor_report_v1().unwrap();
    let result = v
        .validate_receipt(minimal_sensor_receipt().as_bytes())
        .unwrap();
    assert_valid(result);
}

// ═══════════════════════════════════════════════════════════════════════
// 2. Invalid receipt → specific error messages
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn invalid_sensor_receipt_reports_specific_errors() {
    let v = JsonSchemaValidator::sensor_report_v1().unwrap();
    let json = r#"{
      "schema": "sensor.report.v1",
      "tool": { "name": "t", "version": "1" },
      "run": { "started_at": "2026-01-01T00:00:00Z" },
      "verdict": { "status": "invalid_status", "counts": { "info": 0, "warn": 0, "error": 0 } },
      "findings": []
    }"#;
    let result = v.validate_receipt(json.as_bytes()).unwrap();
    assert_invalid(result);
}

// ═══════════════════════════════════════════════════════════════════════
// 3. Valid cockpit report passes
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn valid_cockpit_report_passes_validation() {
    let v = JsonSchemaValidator::cockpit_report_v1().unwrap();
    let result = v
        .validate_receipt(minimal_cockpit_report().as_bytes())
        .unwrap();
    assert_valid(result);
}

// ═══════════════════════════════════════════════════════════════════════
// 4. Invalid cockpit report → errors
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn invalid_cockpit_report_wrong_schema_const() {
    let v = JsonSchemaValidator::cockpit_report_v1().unwrap();
    let json = r#"{
      "schema": "wrong.schema",
      "tool": { "name": "cockpitctl", "version": "0.1.0" },
      "run": { "started_at": "2026-01-01T00:00:00Z" },
      "verdict": { "status": "pass", "counts": { "info": 0, "warn": 0, "error": 0 } },
      "sensors": [],
      "highlights": [],
      "policy": {
        "warn_is_fail": false,
        "max_highlights": 5,
        "max_per_sensor_findings": 10,
        "section_order": [],
        "sensors": []
      }
    }"#;
    let result = v.validate_receipt(json.as_bytes()).unwrap();
    assert_invalid(result);
}

// ═══════════════════════════════════════════════════════════════════════
// 5. Missing required fields → descriptive errors
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn missing_required_fields_yields_descriptive_errors() {
    let v = JsonSchemaValidator::sensor_report_v1().unwrap();
    // Missing tool, run, verdict, findings
    let json = r#"{ "schema": "sensor.report.v1" }"#;
    let result = v.validate_receipt(json.as_bytes()).unwrap();
    match result {
        SchemaValidationResult::Invalid(errors) => {
            assert!(
                errors.len() >= 2,
                "expected multiple missing-field errors, got {}: {:?}",
                errors.len(),
                errors
            );
            let joined = errors.join(" | ");
            assert!(
                joined.contains("required"),
                "errors should mention 'required': {joined}"
            );
        }
        SchemaValidationResult::Valid => panic!("expected Invalid"),
    }
}

#[test]
fn cockpit_report_missing_sensors_field() {
    let v = JsonSchemaValidator::cockpit_report_v1().unwrap();
    let json = r#"{
      "schema": "cockpit.report.v1",
      "tool": { "name": "cockpitctl", "version": "0.1.0" },
      "run": { "started_at": "2026-01-01T00:00:00Z" },
      "verdict": { "status": "pass", "counts": { "info": 0, "warn": 0, "error": 0 } },
      "highlights": [],
      "policy": {
        "warn_is_fail": false, "max_highlights": 5,
        "max_per_sensor_findings": 10, "section_order": [], "sensors": []
      }
    }"#;
    let result = v.validate_receipt(json.as_bytes()).unwrap();
    assert_invalid_containing(result, "required");
}

// ═══════════════════════════════════════════════════════════════════════
// 6. Extra fields → pass (sensor uses additionalProperties: false, so reject)
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn extra_fields_rejected_by_sensor_schema() {
    let v = JsonSchemaValidator::sensor_report_v1().unwrap();
    let json = r#"{
      "schema": "sensor.report.v1",
      "tool": { "name": "t", "version": "1" },
      "run": { "started_at": "2026-01-01T00:00:00Z" },
      "verdict": { "status": "pass", "counts": { "info": 0, "warn": 0, "error": 0 } },
      "findings": [],
      "unexpected": true
    }"#;
    let result = v.validate_receipt(json.as_bytes()).unwrap();
    assert_invalid(result);
}

// ═══════════════════════════════════════════════════════════════════════
// 7. Wrong types → type errors
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn wrong_type_findings_string_instead_of_array() {
    let v = JsonSchemaValidator::sensor_report_v1().unwrap();
    let json = r#"{
      "schema": "sensor.report.v1",
      "tool": { "name": "t", "version": "1" },
      "run": { "started_at": "2026-01-01T00:00:00Z" },
      "verdict": { "status": "pass", "counts": { "info": 0, "warn": 0, "error": 0 } },
      "findings": "not-an-array"
    }"#;
    let result = v.validate_receipt(json.as_bytes()).unwrap();
    assert_invalid(result);
}

#[test]
fn wrong_type_verdict_count_string_instead_of_integer() {
    let v = JsonSchemaValidator::sensor_report_v1().unwrap();
    let json = r#"{
      "schema": "sensor.report.v1",
      "tool": { "name": "t", "version": "1" },
      "run": { "started_at": "2026-01-01T00:00:00Z" },
      "verdict": { "status": "pass", "counts": { "info": "zero", "warn": 0, "error": 0 } },
      "findings": []
    }"#;
    let result = v.validate_receipt(json.as_bytes()).unwrap();
    assert_invalid(result);
}

// ═══════════════════════════════════════════════════════════════════════
// 8. Schema not found → appropriate error
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn from_file_nonexistent_path_returns_error() {
    let err = JsonSchemaValidator::from_file("nonexistent/schema.json").unwrap_err();
    let msg = format!("{:#}", err);
    assert!(
        msg.contains("read schema file"),
        "expected 'read schema file' error, got: {msg}"
    );
}

// ═══════════════════════════════════════════════════════════════════════
// 9. Empty document → validation errors
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn empty_bytes_returns_malformed_json_error() {
    let v = JsonSchemaValidator::sensor_report_v1().unwrap();
    let result = v.validate_receipt(b"").unwrap();
    assert_invalid_containing(result, "malformed JSON");
}

#[test]
fn empty_object_returns_validation_errors() {
    let v = JsonSchemaValidator::sensor_report_v1().unwrap();
    let result = v.validate_receipt(b"{}").unwrap();
    assert_invalid(result);
}

// ═══════════════════════════════════════════════════════════════════════
// 10. Null document → validation errors
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn null_json_returns_invalid() {
    let v = JsonSchemaValidator::sensor_report_v1().unwrap();
    let result = v.validate_receipt(b"null").unwrap();
    assert_invalid(result);
}

#[test]
fn null_cockpit_json_returns_invalid() {
    let v = JsonSchemaValidator::cockpit_report_v1().unwrap();
    let result = v.validate_receipt(b"null").unwrap();
    assert_invalid(result);
}
