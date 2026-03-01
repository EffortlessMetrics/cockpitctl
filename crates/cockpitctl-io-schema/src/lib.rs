//! Schema validation adapter extracted from `cockpitctl-io`.
//!
//! Provides JSON Schema validation for sensor and cockpit reports using
//! either embedded schema bytes or a user-supplied schema file path.

use anyhow::{Context, Result};
use cockpitctl_ingest::{SchemaValidationResult, SchemaValidator};
use jsonschema::Validator;
use serde_json::Value;
use std::fs;

/// JSON Schema validator for sensor and cockpit reports.
///
/// # Examples
///
/// ```
/// use cockpitctl_io_schema::JsonSchemaValidator;
/// use cockpitctl_ingest::{SchemaValidator, SchemaValidationResult};
///
/// let validator = JsonSchemaValidator::sensor_report_v1().unwrap();
///
/// let valid = r#"{
///   "schema": "sensor.report.v1",
///   "tool": { "name": "test", "version": "1.0.0" },
///   "run": { "started_at": "2026-01-01T00:00:00Z" },
///   "verdict": { "status": "pass", "counts": { "info": 0, "warn": 0, "error": 0 } },
///   "findings": []
/// }"#;
///
/// let result = validator.validate_receipt(valid.as_bytes()).unwrap();
/// assert!(matches!(result, SchemaValidationResult::Valid));
/// ```
#[derive(Debug)]
pub struct JsonSchemaValidator {
    validator: Validator,
}

impl JsonSchemaValidator {
    /// Create a new validator by loading the schema from a file path.
    pub fn from_file(schema_path: impl AsRef<std::path::Path>) -> Result<Self> {
        let schema_str = fs::read_to_string(schema_path.as_ref())
            .with_context(|| format!("read schema file {}", schema_path.as_ref().display()))?;
        let schema: Value = serde_json::from_str(&schema_str).with_context(|| {
            format!("parse schema JSON from {}", schema_path.as_ref().display())
        })?;
        let validator =
            Validator::new(&schema).map_err(|e| anyhow::anyhow!("invalid JSON schema: {}", e))?;
        Ok(Self { validator })
    }

    /// Create a new validator from a JSON schema value.
    pub fn from_schema(schema: &Value) -> Result<Self> {
        let validator =
            Validator::new(schema).map_err(|e| anyhow::anyhow!("invalid JSON schema: {}", e))?;
        Ok(Self { validator })
    }

    /// Create a new validator using the embedded sensor.report.v1 schema.
    pub fn sensor_report_v1() -> Result<Self> {
        let schema: Value = serde_json::from_str(cockpitctl_types::SENSOR_REPORT_V1_SCHEMA_JSON)
            .context("parse embedded sensor.report.v1 schema")?;
        Self::from_schema(&schema)
    }

    /// Create a new validator using the embedded cockpit.report.v1 schema.
    pub fn cockpit_report_v1() -> Result<Self> {
        let schema: Value = serde_json::from_str(cockpitctl_types::COCKPIT_REPORT_V1_SCHEMA_JSON)
            .context("parse embedded cockpit.report.v1 schema")?;
        Self::from_schema(&schema)
    }
}

impl SchemaValidator for JsonSchemaValidator {
    fn validate_receipt(&self, bytes: &[u8]) -> Result<SchemaValidationResult> {
        let value: Value = match serde_json::from_slice(bytes) {
            Ok(v) => v,
            Err(e) => {
                return Ok(SchemaValidationResult::Invalid(vec![format!(
                    "malformed JSON: {}",
                    e
                )]));
            }
        };

        let result = self.validator.validate(&value);
        if result.is_ok() {
            Ok(SchemaValidationResult::Valid)
        } else {
            let errors = self
                .validator
                .iter_errors(&value)
                .map(|e| {
                    let path = e.instance_path().to_string();
                    if path.is_empty() {
                        e.to_string()
                    } else {
                        format!("{}: {}", path, e)
                    }
                })
                .collect();
            Ok(SchemaValidationResult::Invalid(errors))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    // ── Helper: minimal valid sensor report ──────────────────────────
    fn minimal_sensor_report() -> &'static str {
        r#"{
          "schema": "sensor.report.v1",
          "tool": { "name": "test-tool", "version": "1.0.0" },
          "run": { "started_at": "2026-01-01T00:00:00Z" },
          "verdict": { "status": "pass", "counts": { "info": 0, "warn": 0, "error": 0 } },
          "findings": []
        }"#
    }

    // ── Helper: minimal valid cockpit report ─────────────────────────
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

    // ═══════════════════════════════════════════════════════════════════
    // Constructor tests
    // ═══════════════════════════════════════════════════════════════════

    #[test]
    fn from_file_loads_and_validates() {
        let tmp = tempdir().unwrap();
        let schema_path = tmp.path().join("sensor.schema.json");
        fs::write(&schema_path, cockpitctl_types::SENSOR_REPORT_V1_SCHEMA_JSON).unwrap();
        let validator = JsonSchemaValidator::from_file(&schema_path).unwrap();
        let result = validator
            .validate_receipt(minimal_sensor_report().as_bytes())
            .unwrap();
        assert!(matches!(result, SchemaValidationResult::Valid));
    }

    #[test]
    fn from_file_missing_file_returns_error() {
        let err = JsonSchemaValidator::from_file("nonexistent.json").unwrap_err();
        let msg = format!("{:#}", err);
        assert!(msg.contains("read schema file"), "got: {msg}");
    }

    #[test]
    fn from_file_invalid_json_returns_error() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("bad.json");
        fs::write(&path, "not-json!!!").unwrap();
        let err = JsonSchemaValidator::from_file(&path).unwrap_err();
        let msg = format!("{:#}", err);
        assert!(msg.contains("parse schema JSON"), "got: {msg}");
    }

    #[test]
    fn from_file_invalid_schema_returns_error() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("bad.schema.json");
        fs::write(&path, r#"{ "type": 123 }"#).unwrap();
        let err = JsonSchemaValidator::from_file(&path).unwrap_err();
        assert!(format!("{:#}", err).contains("invalid JSON schema"));
    }

    #[test]
    fn from_schema_with_valid_schema_succeeds() {
        let schema: Value = serde_json::json!({
            "type": "object",
            "required": ["name"],
            "properties": { "name": { "type": "string" } }
        });
        assert!(JsonSchemaValidator::from_schema(&schema).is_ok());
    }

    #[test]
    fn from_schema_with_invalid_schema_returns_error() {
        let bad = serde_json::json!({ "type": 123 });
        let err = JsonSchemaValidator::from_schema(&bad).unwrap_err();
        assert!(format!("{:#}", err).contains("invalid JSON schema"));
    }

    // ═══════════════════════════════════════════════════════════════════
    // Embedded schema constructors
    // ═══════════════════════════════════════════════════════════════════

    #[test]
    fn sensor_report_v1_constructor_succeeds() {
        let v = JsonSchemaValidator::sensor_report_v1().unwrap();
        let result = v
            .validate_receipt(minimal_sensor_report().as_bytes())
            .unwrap();
        assert!(matches!(result, SchemaValidationResult::Valid));
    }

    #[test]
    fn cockpit_report_v1_constructor_succeeds() {
        let v = JsonSchemaValidator::cockpit_report_v1().unwrap();
        let result = v
            .validate_receipt(minimal_cockpit_report().as_bytes())
            .unwrap();
        assert!(matches!(result, SchemaValidationResult::Valid));
    }

    // ═══════════════════════════════════════════════════════════════════
    // Validation of valid JSON
    // ═══════════════════════════════════════════════════════════════════

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

    #[test]
    fn valid_sensor_report_with_optional_fields() {
        let v = JsonSchemaValidator::sensor_report_v1().unwrap();
        let json = r#"{
          "schema": "sensor.report.v1",
          "tool": { "name": "t", "version": "1.0.0", "commit": "abc123" },
          "run": {
            "started_at": "2026-01-01T00:00:00Z",
            "ended_at": "2026-01-01T00:01:00Z",
            "duration_ms": 60000,
            "git": { "repo": "org/repo", "head_sha": "deadbeef" }
          },
          "verdict": {
            "status": "pass",
            "counts": { "info": 0, "warn": 0, "error": 0 },
            "reasons": ["all clear"]
          },
          "findings": [],
          "data": { "custom_key": "value" },
          "artifacts": [{ "id": "log", "path": "build.log", "mime": "text/plain" }]
        }"#;
        assert!(matches!(
            v.validate_receipt(json.as_bytes()).unwrap(),
            SchemaValidationResult::Valid
        ));
    }

    // ═══════════════════════════════════════════════════════════════════
    // Validation of invalid JSON — missing required fields
    // ═══════════════════════════════════════════════════════════════════

    #[test]
    fn missing_required_field_schema() {
        let v = JsonSchemaValidator::sensor_report_v1().unwrap();
        let json = r#"{
          "tool": { "name": "t", "version": "1" },
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
                    joined.contains("schema") || joined.contains("required"),
                    "expected mention of missing 'schema': {joined}"
                );
            }
            SchemaValidationResult::Valid => panic!("expected Invalid for missing 'schema' field"),
        }
    }

    #[test]
    fn missing_required_field_tool() {
        let v = JsonSchemaValidator::sensor_report_v1().unwrap();
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
                    "expected mention of missing 'tool': {joined}"
                );
            }
            SchemaValidationResult::Valid => panic!("expected Invalid for missing 'tool' field"),
        }
    }

    #[test]
    fn missing_nested_required_field() {
        let v = JsonSchemaValidator::sensor_report_v1().unwrap();
        // tool.name is required but missing
        let json = r#"{
          "schema": "sensor.report.v1",
          "tool": { "version": "1.0.0" },
          "run": { "started_at": "2026-01-01T00:00:00Z" },
          "verdict": { "status": "pass", "counts": { "info": 0, "warn": 0, "error": 0 } },
          "findings": []
        }"#;
        let result = v.validate_receipt(json.as_bytes()).unwrap();
        match result {
            SchemaValidationResult::Invalid(errors) => {
                let joined = errors.join(" | ");
                assert!(
                    joined.contains("name") || joined.contains("required"),
                    "expected mention of missing 'name': {joined}"
                );
            }
            SchemaValidationResult::Valid => panic!("expected Invalid"),
        }
    }

    // ═══════════════════════════════════════════════════════════════════
    // Validation of invalid JSON — wrong types
    // ═══════════════════════════════════════════════════════════════════

    #[test]
    fn wrong_type_for_findings() {
        let v = JsonSchemaValidator::sensor_report_v1().unwrap();
        let json = r#"{
          "schema": "sensor.report.v1",
          "tool": { "name": "t", "version": "1" },
          "run": { "started_at": "2026-01-01T00:00:00Z" },
          "verdict": { "status": "pass", "counts": { "info": 0, "warn": 0, "error": 0 } },
          "findings": "not-an-array"
        }"#;
        let result = v.validate_receipt(json.as_bytes()).unwrap();
        match result {
            SchemaValidationResult::Invalid(errors) => {
                assert!(!errors.is_empty());
                let joined = errors.join(" | ");
                assert!(
                    joined.contains("findings") || joined.contains("type"),
                    "expected type error about findings: {joined}"
                );
            }
            SchemaValidationResult::Valid => panic!("expected Invalid for wrong type"),
        }
    }

    #[test]
    fn wrong_type_for_verdict_counts() {
        let v = JsonSchemaValidator::sensor_report_v1().unwrap();
        let json = r#"{
          "schema": "sensor.report.v1",
          "tool": { "name": "t", "version": "1" },
          "run": { "started_at": "2026-01-01T00:00:00Z" },
          "verdict": { "status": "pass", "counts": { "info": "zero", "warn": 0, "error": 0 } },
          "findings": []
        }"#;
        let result = v.validate_receipt(json.as_bytes()).unwrap();
        assert!(matches!(result, SchemaValidationResult::Invalid(_)));
    }

    #[test]
    fn invalid_enum_value_for_verdict_status() {
        let v = JsonSchemaValidator::sensor_report_v1().unwrap();
        let json = r#"{
          "schema": "sensor.report.v1",
          "tool": { "name": "t", "version": "1" },
          "run": { "started_at": "2026-01-01T00:00:00Z" },
          "verdict": { "status": "invalid_status", "counts": { "info": 0, "warn": 0, "error": 0 } },
          "findings": []
        }"#;
        let result = v.validate_receipt(json.as_bytes()).unwrap();
        assert!(matches!(result, SchemaValidationResult::Invalid(_)));
    }

    #[test]
    fn invalid_enum_value_for_finding_severity() {
        let v = JsonSchemaValidator::sensor_report_v1().unwrap();
        let json = r#"{
          "schema": "sensor.report.v1",
          "tool": { "name": "t", "version": "1" },
          "run": { "started_at": "2026-01-01T00:00:00Z" },
          "verdict": { "status": "pass", "counts": { "info": 0, "warn": 0, "error": 0 } },
          "findings": [{ "severity": "critical", "code": "x", "message": "m" }]
        }"#;
        let result = v.validate_receipt(json.as_bytes()).unwrap();
        assert!(matches!(result, SchemaValidationResult::Invalid(_)));
    }

    // ═══════════════════════════════════════════════════════════════════
    // Error reporting quality
    // ═══════════════════════════════════════════════════════════════════

    #[test]
    fn errors_include_json_path_for_nested_violations() {
        let v = JsonSchemaValidator::sensor_report_v1().unwrap();
        // findings[0] is missing required "code" and "message"
        let json = r#"{
          "schema": "sensor.report.v1",
          "tool": { "name": "t", "version": "1" },
          "run": { "started_at": "2026-01-01T00:00:00Z" },
          "verdict": { "status": "pass", "counts": { "info": 0, "warn": 0, "error": 0 } },
          "findings": [{ "severity": "info" }]
        }"#;
        let result = v.validate_receipt(json.as_bytes()).unwrap();
        match result {
            SchemaValidationResult::Invalid(errors) => {
                let joined = errors.join("\n");
                // Errors for nested paths should include path info
                assert!(
                    joined.contains("code") && joined.contains("message"),
                    "expected errors mentioning missing 'code' and 'message': {joined}"
                );
            }
            SchemaValidationResult::Valid => panic!("expected Invalid"),
        }
    }

    #[test]
    fn multiple_errors_collected() {
        let v = JsonSchemaValidator::sensor_report_v1().unwrap();
        // Missing all required fields except schema
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

    // ═══════════════════════════════════════════════════════════════════
    // Edge cases
    // ═══════════════════════════════════════════════════════════════════

    #[test]
    fn empty_bytes_returns_malformed_json() {
        let v = JsonSchemaValidator::sensor_report_v1().unwrap();
        let result = v.validate_receipt(b"").unwrap();
        match result {
            SchemaValidationResult::Invalid(errors) => {
                assert_eq!(errors.len(), 1);
                assert!(
                    errors[0].contains("malformed JSON"),
                    "expected 'malformed JSON', got: {}",
                    errors[0]
                );
            }
            SchemaValidationResult::Valid => panic!("expected Invalid"),
        }
    }

    #[test]
    fn malformed_json_returns_parse_error() {
        let v = JsonSchemaValidator::sensor_report_v1().unwrap();
        let result = v.validate_receipt(b"{not valid json}").unwrap();
        match result {
            SchemaValidationResult::Invalid(errors) => {
                assert_eq!(errors.len(), 1);
                assert!(errors[0].contains("malformed JSON"));
            }
            SchemaValidationResult::Valid => panic!("expected Invalid"),
        }
    }

    #[test]
    fn null_json_returns_invalid() {
        let v = JsonSchemaValidator::sensor_report_v1().unwrap();
        let result = v.validate_receipt(b"null").unwrap();
        assert!(matches!(result, SchemaValidationResult::Invalid(_)));
    }

    #[test]
    fn json_array_instead_of_object_returns_invalid() {
        let v = JsonSchemaValidator::sensor_report_v1().unwrap();
        let result = v.validate_receipt(b"[]").unwrap();
        assert!(matches!(result, SchemaValidationResult::Invalid(_)));
    }

    #[test]
    fn empty_json_object_returns_invalid() {
        let v = JsonSchemaValidator::sensor_report_v1().unwrap();
        let result = v.validate_receipt(b"{}").unwrap();
        assert!(matches!(result, SchemaValidationResult::Invalid(_)));
    }

    #[test]
    fn extra_fields_rejected_by_additional_properties() {
        let v = JsonSchemaValidator::sensor_report_v1().unwrap();
        // The sensor schema has additionalProperties: false
        let json = r#"{
          "schema": "sensor.report.v1",
          "tool": { "name": "t", "version": "1" },
          "run": { "started_at": "2026-01-01T00:00:00Z" },
          "verdict": { "status": "pass", "counts": { "info": 0, "warn": 0, "error": 0 } },
          "findings": [],
          "unexpected_field": true
        }"#;
        let result = v.validate_receipt(json.as_bytes()).unwrap();
        assert!(
            matches!(result, SchemaValidationResult::Invalid(_)),
            "extra fields should be rejected by additionalProperties: false"
        );
    }

    #[test]
    fn negative_count_rejected() {
        let v = JsonSchemaValidator::sensor_report_v1().unwrap();
        let json = r#"{
          "schema": "sensor.report.v1",
          "tool": { "name": "t", "version": "1" },
          "run": { "started_at": "2026-01-01T00:00:00Z" },
          "verdict": { "status": "pass", "counts": { "info": -1, "warn": 0, "error": 0 } },
          "findings": []
        }"#;
        let result = v.validate_receipt(json.as_bytes()).unwrap();
        assert!(matches!(result, SchemaValidationResult::Invalid(_)));
    }

    #[test]
    fn empty_string_tool_name_rejected() {
        let v = JsonSchemaValidator::sensor_report_v1().unwrap();
        // tool.name has minLength: 1
        let json = r#"{
          "schema": "sensor.report.v1",
          "tool": { "name": "", "version": "1" },
          "run": { "started_at": "2026-01-01T00:00:00Z" },
          "verdict": { "status": "pass", "counts": { "info": 0, "warn": 0, "error": 0 } },
          "findings": []
        }"#;
        let result = v.validate_receipt(json.as_bytes()).unwrap();
        assert!(matches!(result, SchemaValidationResult::Invalid(_)));
    }

    // ═══════════════════════════════════════════════════════════════════
    // Cockpit report validation
    // ═══════════════════════════════════════════════════════════════════

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
        match result {
            SchemaValidationResult::Invalid(errors) => {
                let joined = errors.join(" | ");
                assert!(
                    joined.contains("sensors") || joined.contains("required"),
                    "expected mention of missing 'sensors': {joined}"
                );
            }
            SchemaValidationResult::Valid => panic!("expected Invalid"),
        }
    }

    #[test]
    fn cockpit_report_wrong_schema_const() {
        let v = JsonSchemaValidator::cockpit_report_v1().unwrap();
        // "schema" must be exactly "cockpit.report.v1"
        let json = r#"{
          "schema": "wrong.value",
          "tool": { "name": "cockpitctl", "version": "0.1.0" },
          "run": { "started_at": "2026-01-01T00:00:00Z" },
          "verdict": { "status": "pass", "counts": { "info": 0, "warn": 0, "error": 0 } },
          "sensors": [],
          "highlights": [],
          "policy": {
            "warn_is_fail": false, "max_highlights": 5,
            "max_per_sensor_findings": 10, "section_order": [], "sensors": []
          }
        }"#;
        let result = v.validate_receipt(json.as_bytes()).unwrap();
        assert!(matches!(result, SchemaValidationResult::Invalid(_)));
    }

    // ═══════════════════════════════════════════════════════════════════
    // from_schema with custom schema
    // ═══════════════════════════════════════════════════════════════════

    #[test]
    fn from_schema_validates_custom_schema() {
        let schema: Value = serde_json::json!({
            "type": "object",
            "required": ["name"],
            "properties": { "name": { "type": "string" } },
            "additionalProperties": false
        });
        let v = JsonSchemaValidator::from_schema(&schema).unwrap();

        // Valid
        let result = v.validate_receipt(br#"{"name": "hello"}"#).unwrap();
        assert!(matches!(result, SchemaValidationResult::Valid));

        // Invalid — missing required field
        let result = v.validate_receipt(b"{}").unwrap();
        assert!(matches!(result, SchemaValidationResult::Invalid(_)));

        // Invalid — extra field
        let result = v
            .validate_receipt(br#"{"name": "hello", "extra": true}"#)
            .unwrap();
        assert!(matches!(result, SchemaValidationResult::Invalid(_)));
    }

    // ═══════════════════════════════════════════════════════════════════
    // Debug trait on JsonSchemaValidator
    // ═══════════════════════════════════════════════════════════════════

    #[test]
    fn json_schema_validator_implements_debug() {
        let v = JsonSchemaValidator::sensor_report_v1().unwrap();
        let debug = format!("{:?}", v);
        assert!(
            debug.contains("JsonSchemaValidator"),
            "Debug output should contain type name: {debug}"
        );
    }
}
