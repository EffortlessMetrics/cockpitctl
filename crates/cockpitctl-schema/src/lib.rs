use anyhow::{Context, Result};
use cockpitctl_ingest::{SchemaValidationResult, SchemaValidator};
use jsonschema::Validator;

/// JSON Schema validator for sensor reports and cockpit reports.
pub struct JsonSchemaValidator {
    validator: Validator,
}

impl JsonSchemaValidator {
    /// Create a new validator by loading the schema from a file path.
    pub fn from_file(schema_path: impl AsRef<std::path::Path>) -> Result<Self> {
        let schema_str = std::fs::read_to_string(schema_path.as_ref())
            .with_context(|| format!("read schema file {}", schema_path.as_ref().display()))?;
        let schema: serde_json::Value = serde_json::from_str(&schema_str).with_context(|| {
            format!("parse schema JSON from {}", schema_path.as_ref().display())
        })?;
        let validator =
            Validator::new(&schema).map_err(|e| anyhow::anyhow!("invalid JSON schema: {}", e))?;
        Ok(Self { validator })
    }

    /// Create a new validator from a JSON schema value.
    pub fn from_schema(schema: &serde_json::Value) -> Result<Self> {
        let validator =
            Validator::new(schema).map_err(|e| anyhow::anyhow!("invalid JSON schema: {}", e))?;
        Ok(Self { validator })
    }

    /// Create a new validator using the embedded sensor.report.v1 schema.
    pub fn sensor_report_v1() -> Result<Self> {
        const SCHEMA: &str = cockpitctl_types::SENSOR_REPORT_V1_SCHEMA_JSON;
        let schema: serde_json::Value =
            serde_json::from_str(SCHEMA).context("parse embedded sensor.report.v1 schema")?;
        Self::from_schema(&schema)
    }

    /// Create a new validator using the embedded cockpit.report.v1 schema.
    pub fn cockpit_report_v1() -> Result<Self> {
        const SCHEMA: &str = cockpitctl_types::COCKPIT_REPORT_V1_SCHEMA_JSON;
        let schema: serde_json::Value =
            serde_json::from_str(SCHEMA).context("parse embedded cockpit.report.v1 schema")?;
        Self::from_schema(&schema)
    }
}

impl SchemaValidator for JsonSchemaValidator {
    fn validate_receipt(&self, bytes: &[u8]) -> Result<SchemaValidationResult> {
        let value: serde_json::Value = match serde_json::from_slice(bytes) {
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
            let errors: Vec<String> = self
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
