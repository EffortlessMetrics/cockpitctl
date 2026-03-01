//! Edge-case tests for the schema validation adapter.
//!
//! Covers lax-mode behavior via permissive custom schemas, unicode field
//! values, deeply nested validation error paths, and boolean schema edges.

use cockpitctl_ingest::{SchemaValidationResult, SchemaValidator};
use cockpitctl_io_schema::JsonSchemaValidator;

// ── Lax mode: permissive schema allows additional properties ───────────

#[test]
fn lax_schema_allows_additional_properties() {
    // A schema without additionalProperties: false acts as "lax"
    let schema: serde_json::Value = serde_json::json!({
        "type": "object",
        "required": ["name"],
        "properties": {
            "name": { "type": "string" }
        }
    });
    let v = JsonSchemaValidator::from_schema(&schema).unwrap();
    // Extra fields are allowed in lax mode
    let result = v
        .validate_receipt(br#"{"name": "test", "extra": true, "another": 42}"#)
        .unwrap();
    assert!(
        matches!(result, SchemaValidationResult::Valid),
        "lax schema should allow additional properties"
    );
}

// ── Strict mode: schema rejects additional properties ──────────────────

#[test]
fn strict_schema_rejects_additional_properties() {
    let schema: serde_json::Value = serde_json::json!({
        "type": "object",
        "required": ["name"],
        "properties": {
            "name": { "type": "string" }
        },
        "additionalProperties": false
    });
    let v = JsonSchemaValidator::from_schema(&schema).unwrap();
    let result = v
        .validate_receipt(br#"{"name": "test", "extra": true}"#)
        .unwrap();
    assert!(
        matches!(result, SchemaValidationResult::Invalid(_)),
        "strict schema should reject additional properties"
    );
}

// ── Unicode field values validated correctly ────────────────────────────

#[test]
fn unicode_field_values_pass_validation() {
    let v = JsonSchemaValidator::sensor_report_v1().unwrap();
    let json = r#"{
      "schema": "sensor.report.v1",
      "tool": { "name": "日本語ツール", "version": "1.0.0" },
      "run": { "started_at": "2026-01-01T00:00:00Z" },
      "verdict": { "status": "pass", "counts": { "info": 0, "warn": 0, "error": 0 } },
      "findings": []
    }"#;
    let result = v.validate_receipt(json.as_bytes()).unwrap();
    assert!(matches!(result, SchemaValidationResult::Valid));
}

// ── Deeply nested validation errors include path info ──────────────────

#[test]
fn deeply_nested_validation_errors_include_path() {
    let v = JsonSchemaValidator::sensor_report_v1().unwrap();
    // Finding with wrong type for location.line (string instead of integer)
    let json = r#"{
      "schema": "sensor.report.v1",
      "tool": { "name": "t", "version": "1" },
      "run": { "started_at": "2026-01-01T00:00:00Z" },
      "verdict": { "status": "pass", "counts": { "info": 0, "warn": 0, "error": 0 } },
      "findings": [{
        "severity": "info",
        "code": "E001",
        "message": "test",
        "location": { "path": "file.rs", "line": "not-a-number" }
      }]
    }"#;
    let result = v.validate_receipt(json.as_bytes()).unwrap();
    match result {
        SchemaValidationResult::Invalid(errors) => {
            let joined = errors.join("\n");
            // Error should reference the nested path (findings/location/line)
            assert!(
                joined.contains("findings") || joined.contains("line") || joined.contains("type"),
                "expected path info in error, got: {joined}"
            );
        }
        SchemaValidationResult::Valid => panic!("expected Invalid for wrong line type"),
    }
}

// ── Boolean schema true accepts everything ─────────────────────────────

#[test]
fn boolean_true_schema_accepts_everything() {
    let schema: serde_json::Value = serde_json::json!(true);
    let v = JsonSchemaValidator::from_schema(&schema).unwrap();

    let result = v.validate_receipt(br#"{"anything": "goes"}"#).unwrap();
    assert!(matches!(result, SchemaValidationResult::Valid));

    let result = v.validate_receipt(b"42").unwrap();
    assert!(matches!(result, SchemaValidationResult::Valid));

    let result = v.validate_receipt(b"null").unwrap();
    assert!(matches!(result, SchemaValidationResult::Valid));
}

// ── Boolean schema false rejects everything ────────────────────────────

#[test]
fn boolean_false_schema_rejects_everything() {
    let schema: serde_json::Value = serde_json::json!(false);
    let v = JsonSchemaValidator::from_schema(&schema).unwrap();

    let result = v.validate_receipt(br#"{"name": "test"}"#).unwrap();
    assert!(matches!(result, SchemaValidationResult::Invalid(_)));

    let result = v.validate_receipt(b"null").unwrap();
    assert!(matches!(result, SchemaValidationResult::Invalid(_)));
}

// ── Truncated JSON returns malformed error ──────────────────────────────

#[test]
fn truncated_json_returns_malformed_error() {
    let v = JsonSchemaValidator::sensor_report_v1().unwrap();
    let result = v
        .validate_receipt(br#"{"schema": "sensor.report.v1", "tool":"#)
        .unwrap();
    match result {
        SchemaValidationResult::Invalid(errors) => {
            assert_eq!(errors.len(), 1);
            assert!(errors[0].contains("malformed JSON"), "got: {}", errors[0]);
        }
        SchemaValidationResult::Valid => panic!("expected Invalid for truncated JSON"),
    }
}

// ── Validator reuse: alternating valid/invalid doesn't corrupt state ───

#[test]
fn validator_alternating_valid_invalid_no_corruption() {
    let v = JsonSchemaValidator::sensor_report_v1().unwrap();
    let valid = r#"{
      "schema": "sensor.report.v1",
      "tool": { "name": "t", "version": "1.0.0" },
      "run": { "started_at": "2026-01-01T00:00:00Z" },
      "verdict": { "status": "pass", "counts": { "info": 0, "warn": 0, "error": 0 } },
      "findings": []
    }"#;

    for _ in 0..5 {
        // Valid
        assert!(matches!(
            v.validate_receipt(valid.as_bytes()).unwrap(),
            SchemaValidationResult::Valid
        ));
        // Invalid
        assert!(matches!(
            v.validate_receipt(b"not json").unwrap(),
            SchemaValidationResult::Invalid(_)
        ));
        // Empty
        assert!(matches!(
            v.validate_receipt(b"{}").unwrap(),
            SchemaValidationResult::Invalid(_)
        ));
    }
    // Final valid check — validator must not be corrupted
    assert!(matches!(
        v.validate_receipt(valid.as_bytes()).unwrap(),
        SchemaValidationResult::Valid
    ));
}
