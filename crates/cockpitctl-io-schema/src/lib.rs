//! Schema validation adapter extracted from `cockpitctl-io`.

use anyhow::{Context, Result};
use cockpitctl_ingest::{SchemaValidationResult, SchemaValidator};
use jsonschema::Validator;
use serde_json::Value;
use std::fs;

/// JSON Schema validator for sensor and cockpit reports.
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

    #[test]
    fn json_schema_validator_from_file_validates() {
        use cockpitctl_types::SENSOR_REPORT_V1_SCHEMA_JSON;
        let tmp = tempdir().unwrap();
        let schema_path = tmp.path().join("sensor.schema.json");
        fs::write(schema_path.clone(), SENSOR_REPORT_V1_SCHEMA_JSON).unwrap();
        let validator = JsonSchemaValidator::from_file(&schema_path).unwrap();
        let report = r#"{
          "schema":"sensor.report.v1",
          "tool":{"name":"test-tool","version":"1.0.0"},
          "run":{"started_at":"2026-01-01T00:00:00Z"},
          "verdict":{"status":"pass","counts":{"info":0,"warn":0,"error":0}},
          "findings":[]
        }"#;
        assert!(matches!(
            validator.validate_receipt(report.as_bytes()).unwrap(),
            SchemaValidationResult::Valid
        ));
    }

    #[test]
    fn json_schema_validator_from_file_invalid_schema_errors() {
        let tmp = tempdir().unwrap();
        let schema_path = tmp.path().join("bad.schema.json");
        fs::write(&schema_path, r#"{ "type": 123 }"#).unwrap();
        let err = JsonSchemaValidator::from_file(&schema_path).expect_err("expected error");
        assert!(format!("{:#}", err).contains("invalid JSON schema"));
    }

    #[test]
    fn json_schema_validator_from_schema_invalid_schema_errors() {
        let bad = serde_json::json!({ "type": 123 });
        let err = JsonSchemaValidator::from_schema(&bad).expect_err("expected error");
        assert!(format!("{:#}", err).contains("invalid JSON schema"));
    }
}
