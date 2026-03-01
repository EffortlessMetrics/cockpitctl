//! Integration tests for the schema validation adapter.
//!
//! Exercises the public API (`JsonSchemaValidator`) verifying valid/invalid
//! receipt handling, empty/non-JSON input, and embedded schema correctness.

use cockpitctl_ingest::{SchemaValidationResult, SchemaValidator};
use cockpitctl_io_schema::JsonSchemaValidator;

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

// ── Valid receipt → pass ───────────────────────────────────────────────

#[test]
fn valid_sensor_report_passes() {
    let v = JsonSchemaValidator::sensor_report_v1().unwrap();
    let result = v
        .validate_receipt(minimal_sensor_report().as_bytes())
        .unwrap();
    assert!(matches!(result, SchemaValidationResult::Valid));
}

#[test]
fn valid_cockpit_report_passes() {
    let v = JsonSchemaValidator::cockpit_report_v1().unwrap();
    let result = v
        .validate_receipt(minimal_cockpit_report().as_bytes())
        .unwrap();
    assert!(matches!(result, SchemaValidationResult::Valid));
}

#[test]
fn valid_sensor_report_with_findings() {
    let v = JsonSchemaValidator::sensor_report_v1().unwrap();
    let json = r#"{
      "schema": "sensor.report.v1",
      "tool": { "name": "clippy", "version": "0.1.0" },
      "run": { "started_at": "2026-06-01T12:00:00Z" },
      "verdict": { "status": "warn", "counts": { "info": 0, "warn": 1, "error": 0 } },
      "findings": [{
        "severity": "warn",
        "code": "unused_variable",
        "message": "unused variable `x`",
        "location": { "path": "src/main.rs", "line": 42 }
      }]
    }"#;
    assert!(matches!(
        v.validate_receipt(json.as_bytes()).unwrap(),
        SchemaValidationResult::Valid
    ));
}

// ── Invalid receipt (missing required fields) → fail with details ──────

#[test]
fn missing_required_field_returns_invalid_with_details() {
    let v = JsonSchemaValidator::sensor_report_v1().unwrap();
    // Missing "tool" field
    let json = r#"{
      "schema": "sensor.report.v1",
      "run": { "started_at": "2026-01-01T00:00:00Z" },
      "verdict": { "status": "pass", "counts": { "info": 0, "warn": 0, "error": 0 } },
      "findings": []
    }"#;
    let result = v.validate_receipt(json.as_bytes()).unwrap();
    match result {
        SchemaValidationResult::Invalid(errors) => {
            assert!(!errors.is_empty());
            let joined = errors.join(" | ");
            assert!(
                joined.contains("tool") || joined.contains("required"),
                "expected mention of 'tool': {joined}"
            );
        }
        SchemaValidationResult::Valid => panic!("expected Invalid"),
    }
}

#[test]
fn multiple_missing_fields_reported() {
    let v = JsonSchemaValidator::sensor_report_v1().unwrap();
    let json = r#"{ "schema": "sensor.report.v1" }"#;
    let result = v.validate_receipt(json.as_bytes()).unwrap();
    match result {
        SchemaValidationResult::Invalid(errors) => {
            assert!(
                errors.len() >= 2,
                "expected multiple errors, got {}: {:?}",
                errors.len(),
                errors
            );
        }
        SchemaValidationResult::Valid => panic!("expected Invalid"),
    }
}

#[test]
fn invalid_receipt_snapshot() {
    let v = JsonSchemaValidator::sensor_report_v1().unwrap();
    let json = r#"{ "schema": "sensor.report.v1" }"#;
    let result = v.validate_receipt(json.as_bytes()).unwrap();
    match result {
        SchemaValidationResult::Invalid(errors) => {
            // Snapshot the error list for regression detection
            insta::assert_json_snapshot!("missing_fields_errors", errors);
        }
        SchemaValidationResult::Valid => panic!("expected Invalid"),
    }
}

// ── Empty JSON → fail ──────────────────────────────────────────────────

#[test]
fn empty_json_object_returns_invalid() {
    let v = JsonSchemaValidator::sensor_report_v1().unwrap();
    let result = v.validate_receipt(b"{}").unwrap();
    assert!(matches!(result, SchemaValidationResult::Invalid(_)));
}

#[test]
fn empty_bytes_returns_malformed_json() {
    let v = JsonSchemaValidator::sensor_report_v1().unwrap();
    let result = v.validate_receipt(b"").unwrap();
    match result {
        SchemaValidationResult::Invalid(errors) => {
            assert_eq!(errors.len(), 1);
            assert!(errors[0].contains("malformed JSON"));
        }
        SchemaValidationResult::Valid => panic!("expected Invalid"),
    }
}

// ── Non-JSON → fail ────────────────────────────────────────────────────

#[test]
fn non_json_returns_malformed() {
    let v = JsonSchemaValidator::sensor_report_v1().unwrap();
    let result = v.validate_receipt(b"this is not json at all").unwrap();
    match result {
        SchemaValidationResult::Invalid(errors) => {
            assert_eq!(errors.len(), 1);
            assert!(errors[0].contains("malformed JSON"));
        }
        SchemaValidationResult::Valid => panic!("expected Invalid"),
    }
}

#[test]
fn json_array_returns_invalid() {
    let v = JsonSchemaValidator::sensor_report_v1().unwrap();
    let result = v.validate_receipt(b"[]").unwrap();
    assert!(matches!(result, SchemaValidationResult::Invalid(_)));
}

#[test]
fn null_json_returns_invalid() {
    let v = JsonSchemaValidator::sensor_report_v1().unwrap();
    let result = v.validate_receipt(b"null").unwrap();
    assert!(matches!(result, SchemaValidationResult::Invalid(_)));
}

// ── Embedded schemas match contracts/schemas/ ──────────────────────────

#[test]
fn embedded_sensor_schema_matches_contracts() {
    let contracts_schema = std::fs::read_to_string("../../contracts/schemas/sensor.report.v1.json")
        .expect("read contracts schema");
    let embedded = cockpitctl_types::SENSOR_REPORT_V1_SCHEMA_JSON;
    // Normalize by parsing and re-serializing
    let contracts_val: serde_json::Value =
        serde_json::from_str(&contracts_schema).expect("parse contracts");
    let embedded_val: serde_json::Value = serde_json::from_str(embedded).expect("parse embedded");
    assert_eq!(
        contracts_val, embedded_val,
        "embedded sensor.report.v1 schema drifted from contracts/schemas/"
    );
}

#[test]
fn embedded_cockpit_schema_matches_contracts() {
    let contracts_schema =
        std::fs::read_to_string("../../contracts/schemas/cockpit.report.v1.json")
            .expect("read contracts schema");
    let embedded = cockpitctl_types::COCKPIT_REPORT_V1_SCHEMA_JSON;
    let contracts_val: serde_json::Value =
        serde_json::from_str(&contracts_schema).expect("parse contracts");
    let embedded_val: serde_json::Value = serde_json::from_str(embedded).expect("parse embedded");
    assert_eq!(
        contracts_val, embedded_val,
        "embedded cockpit.report.v1 schema drifted from contracts/schemas/"
    );
}

// ── From-file constructor ──────────────────────────────────────────────

#[test]
fn from_file_with_valid_schema_validates() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("schema.json");
    std::fs::write(&path, cockpitctl_types::SENSOR_REPORT_V1_SCHEMA_JSON).unwrap();
    let v = JsonSchemaValidator::from_file(&path).unwrap();
    let result = v
        .validate_receipt(minimal_sensor_report().as_bytes())
        .unwrap();
    assert!(matches!(result, SchemaValidationResult::Valid));
}

#[test]
fn from_file_missing_returns_error() {
    let err = JsonSchemaValidator::from_file("nonexistent.json").unwrap_err();
    assert!(format!("{err:#}").contains("read schema file"));
}

#[test]
fn from_file_invalid_json_returns_error() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("bad.json");
    std::fs::write(&path, "not-json!!!").unwrap();
    let err = JsonSchemaValidator::from_file(&path).unwrap_err();
    assert!(format!("{err:#}").contains("parse schema JSON"));
}
