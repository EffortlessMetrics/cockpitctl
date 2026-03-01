//! Integration tests for cockpitctl-io-schema.
//!
//! Focuses on schema loading from embedded bytes and files, validation
//! of valid/invalid receipts, sequential validations, and edge cases.

use cockpitctl_ingest::{SchemaValidationResult, SchemaValidator};
use cockpitctl_io_schema::JsonSchemaValidator;
use std::fs;
use tempfile::tempdir;

// ── Helpers ────────────────────────────────────────────────────────────

fn minimal_sensor_report() -> &'static str {
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

// ── Schema loading from embedded bytes ─────────────────────────────────

#[test]
fn embedded_sensor_schema_creates_working_validator() {
    let v = JsonSchemaValidator::sensor_report_v1().unwrap();
    let result = v
        .validate_receipt(minimal_sensor_report().as_bytes())
        .unwrap();
    assert!(matches!(result, SchemaValidationResult::Valid));
}

#[test]
fn embedded_cockpit_schema_creates_working_validator() {
    let v = JsonSchemaValidator::cockpit_report_v1().unwrap();
    let result = v
        .validate_receipt(minimal_cockpit_report().as_bytes())
        .unwrap();
    assert!(matches!(result, SchemaValidationResult::Valid));
}

#[test]
fn sensor_validator_rejects_cockpit_report() {
    let v = JsonSchemaValidator::sensor_report_v1().unwrap();
    let result = v
        .validate_receipt(minimal_cockpit_report().as_bytes())
        .unwrap();
    // Cockpit report has "cockpit.report.v1" which doesn't match sensor schema const
    assert!(matches!(result, SchemaValidationResult::Invalid(_)));
}

// ── Schema validation of valid receipt ─────────────────────────────────

#[test]
fn valid_receipt_with_all_optional_fields() {
    let v = JsonSchemaValidator::sensor_report_v1().unwrap();
    let json = r#"{
      "schema": "sensor.report.v1",
      "tool": { "name": "clippy", "version": "0.2.0", "commit": "abc123" },
      "run": {
        "started_at": "2026-06-01T12:00:00Z",
        "ended_at": "2026-06-01T12:01:00Z",
        "duration_ms": 60000,
        "git": { "repo": "org/repo", "head_sha": "deadbeef" }
      },
      "verdict": {
        "status": "warn",
        "counts": { "info": 1, "warn": 2, "error": 0 },
        "reasons": ["found warnings"]
      },
      "findings": [{
        "severity": "warn",
        "code": "W001",
        "message": "unused variable",
        "location": { "path": "src/lib.rs", "line": 10 }
      }],
      "data": { "custom": "value" },
      "artifacts": [{ "id": "log", "path": "build.log", "mime": "text/plain" }]
    }"#;
    let result = v.validate_receipt(json.as_bytes()).unwrap();
    assert!(matches!(result, SchemaValidationResult::Valid));
}

// ── Schema validation of invalid receipt → errors ──────────────────────

#[test]
fn missing_multiple_required_fields_reports_all_errors() {
    let v = JsonSchemaValidator::sensor_report_v1().unwrap();
    // Only has "schema" — missing tool, run, verdict, findings
    let json = r#"{"schema": "sensor.report.v1"}"#;
    let result = v.validate_receipt(json.as_bytes()).unwrap();
    match result {
        SchemaValidationResult::Invalid(errors) => {
            assert!(
                errors.len() >= 3,
                "expected at least 3 errors for missing fields, got {}: {:?}",
                errors.len(),
                errors
            );
        }
        SchemaValidationResult::Valid => panic!("expected Invalid"),
    }
}

#[test]
fn wrong_verdict_status_enum_reports_error() {
    let v = JsonSchemaValidator::sensor_report_v1().unwrap();
    let json = r#"{
      "schema": "sensor.report.v1",
      "tool": { "name": "t", "version": "1" },
      "run": { "started_at": "2026-01-01T00:00:00Z" },
      "verdict": { "status": "CRITICAL", "counts": { "info": 0, "warn": 0, "error": 0 } },
      "findings": []
    }"#;
    let result = v.validate_receipt(json.as_bytes()).unwrap();
    assert!(matches!(result, SchemaValidationResult::Invalid(_)));
}

#[test]
fn non_json_input_returns_malformed_error() {
    let v = JsonSchemaValidator::sensor_report_v1().unwrap();
    let result = v.validate_receipt(b"<xml>not json</xml>").unwrap();
    match result {
        SchemaValidationResult::Invalid(errors) => {
            assert_eq!(errors.len(), 1);
            assert!(errors[0].contains("malformed JSON"));
        }
        SchemaValidationResult::Valid => panic!("expected Invalid"),
    }
}

// ── Missing schema → fallback behavior ─────────────────────────────────

#[test]
fn from_file_with_missing_path_returns_error() {
    let tmp = tempdir().unwrap();
    let missing = tmp.path().join("no_such_schema.json");
    let err = JsonSchemaValidator::from_file(&missing).unwrap_err();
    assert!(format!("{err:#}").contains("read schema file"));
}

#[test]
fn from_file_with_non_json_returns_error() {
    let tmp = tempdir().unwrap();
    let path = tmp.path().join("garbage.json");
    fs::write(&path, "{{{{not valid json").unwrap();
    let err = JsonSchemaValidator::from_file(&path).unwrap_err();
    assert!(format!("{err:#}").contains("parse schema JSON"));
}

#[test]
fn from_file_with_invalid_schema_type_returns_error() {
    let tmp = tempdir().unwrap();
    let path = tmp.path().join("bad_type.json");
    fs::write(&path, r#"{"type": 42}"#).unwrap();
    let err = JsonSchemaValidator::from_file(&path).unwrap_err();
    assert!(format!("{err:#}").contains("invalid JSON schema"));
}

// ── Multiple schema validations in sequence ────────────────────────────

#[test]
fn same_validator_validates_multiple_receipts_sequentially() {
    let v = JsonSchemaValidator::sensor_report_v1().unwrap();

    // Valid receipt
    let result = v
        .validate_receipt(minimal_sensor_report().as_bytes())
        .unwrap();
    assert!(matches!(result, SchemaValidationResult::Valid));

    // Invalid receipt (empty object)
    let result = v.validate_receipt(b"{}").unwrap();
    assert!(matches!(result, SchemaValidationResult::Invalid(_)));

    // Valid receipt again (validator not corrupted by previous invalid input)
    let result = v
        .validate_receipt(minimal_sensor_report().as_bytes())
        .unwrap();
    assert!(matches!(result, SchemaValidationResult::Valid));

    // Malformed JSON
    let result = v.validate_receipt(b"not json").unwrap();
    assert!(matches!(result, SchemaValidationResult::Invalid(_)));

    // Valid again
    let result = v
        .validate_receipt(minimal_sensor_report().as_bytes())
        .unwrap();
    assert!(matches!(result, SchemaValidationResult::Valid));
}

#[test]
fn different_validators_operate_independently() {
    let sensor_v = JsonSchemaValidator::sensor_report_v1().unwrap();
    let cockpit_v = JsonSchemaValidator::cockpit_report_v1().unwrap();

    // Sensor validator accepts sensor report
    assert!(matches!(
        sensor_v
            .validate_receipt(minimal_sensor_report().as_bytes())
            .unwrap(),
        SchemaValidationResult::Valid
    ));

    // Cockpit validator accepts cockpit report
    assert!(matches!(
        cockpit_v
            .validate_receipt(minimal_cockpit_report().as_bytes())
            .unwrap(),
        SchemaValidationResult::Valid
    ));

    // Cross-validation: sensor validator rejects cockpit report
    assert!(matches!(
        sensor_v
            .validate_receipt(minimal_cockpit_report().as_bytes())
            .unwrap(),
        SchemaValidationResult::Invalid(_)
    ));
}

// ── Schema loaded from file validates correctly ────────────────────────

#[test]
fn from_file_schema_produces_same_results_as_embedded() {
    let tmp = tempdir().unwrap();
    let schema_path = tmp.path().join("sensor.schema.json");
    fs::write(&schema_path, cockpitctl_types::SENSOR_REPORT_V1_SCHEMA_JSON).unwrap();

    let file_v = JsonSchemaValidator::from_file(&schema_path).unwrap();
    let embedded_v = JsonSchemaValidator::sensor_report_v1().unwrap();

    let test_inputs: Vec<&[u8]> = vec![
        minimal_sensor_report().as_bytes(),
        b"{}",
        b"null",
        b"[]",
        b"",
    ];

    for input in test_inputs {
        let file_result = file_v.validate_receipt(input).unwrap();
        let embedded_result = embedded_v.validate_receipt(input).unwrap();
        match (&file_result, &embedded_result) {
            (SchemaValidationResult::Valid, SchemaValidationResult::Valid) => {}
            (SchemaValidationResult::Invalid(_), SchemaValidationResult::Invalid(_)) => {}
            _ => panic!(
                "file and embedded validators disagree on input: {:?}",
                String::from_utf8_lossy(input)
            ),
        }
    }
}

// ── Custom schema from_schema ──────────────────────────────────────────

#[test]
fn custom_schema_validates_domain_objects() {
    let schema: serde_json::Value = serde_json::json!({
        "type": "object",
        "required": ["id", "value"],
        "properties": {
            "id": { "type": "string", "minLength": 1 },
            "value": { "type": "integer", "minimum": 0 }
        },
        "additionalProperties": false
    });
    let v = JsonSchemaValidator::from_schema(&schema).unwrap();

    // Valid
    let result = v
        .validate_receipt(br#"{"id": "abc", "value": 42}"#)
        .unwrap();
    assert!(matches!(result, SchemaValidationResult::Valid));

    // Invalid: empty id
    let result = v.validate_receipt(br#"{"id": "", "value": 42}"#).unwrap();
    assert!(matches!(result, SchemaValidationResult::Invalid(_)));

    // Invalid: negative value
    let result = v
        .validate_receipt(br#"{"id": "abc", "value": -1}"#)
        .unwrap();
    assert!(matches!(result, SchemaValidationResult::Invalid(_)));

    // Invalid: extra field
    let result = v
        .validate_receipt(br#"{"id": "abc", "value": 1, "extra": true}"#)
        .unwrap();
    assert!(matches!(result, SchemaValidationResult::Invalid(_)));
}
