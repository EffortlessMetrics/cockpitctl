use anyhow::{Context, Result};
use cockpitctl_ingest::{SchemaValidationResult, SchemaValidator};
use cockpitctl_io::JsonSchemaValidator;
use cockpitctl_types::SchemaValidation;

/// Validate a JSON file as either sensor.report.v1 or cockpit.report.v1.
pub fn validate_input_file(input: &str, mode: SchemaValidation) -> Result<()> {
    let bytes = std::fs::read(input).with_context(|| format!("read {}", input))?;
    validate_input_bytes(&bytes, mode)
}

/// Validate raw JSON bytes as either sensor.report.v1 or cockpit.report.v1.
pub fn validate_input_bytes(bytes: &[u8], mode: SchemaValidation) -> Result<()> {
    match mode {
        SchemaValidation::Lax => validate_lax(bytes),
        SchemaValidation::Strict => validate_strict(bytes),
    }
}

fn validate_lax(bytes: &[u8]) -> Result<()> {
    if serde_json::from_slice::<cockpitctl_types::SensorReport>(bytes).is_ok() {
        return Ok(());
    }
    if serde_json::from_slice::<cockpitctl_types::CockpitReport>(bytes).is_ok() {
        return Ok(());
    }

    anyhow::bail!("input did not parse as SensorReport or CockpitReport")
}

fn validate_strict(bytes: &[u8]) -> Result<()> {
    let value: serde_json::Value = serde_json::from_slice(bytes).context("parse JSON input")?;
    let schema_hint = value.get("schema").and_then(|s| s.as_str());

    let mut candidates = Vec::new();
    if schema_hint == Some("cockpit.report.v1") {
        candidates.push((
            "cockpit.report.v1",
            JsonSchemaValidator::cockpit_report_v1()
                .context("load cockpit.report.v1 JSON schema")?,
        ));
    } else if schema_hint.is_some() {
        candidates.push((
            "sensor.report.v1",
            JsonSchemaValidator::sensor_report_v1().context("load sensor.report.v1 JSON schema")?,
        ));
    } else {
        candidates.push((
            "sensor.report.v1",
            JsonSchemaValidator::sensor_report_v1().context("load sensor.report.v1 JSON schema")?,
        ));
        candidates.push((
            "cockpit.report.v1",
            JsonSchemaValidator::cockpit_report_v1()
                .context("load cockpit.report.v1 JSON schema")?,
        ));
    }

    let mut errors = Vec::new();
    for (label, validator) in candidates {
        match validator.validate_receipt(bytes)? {
            SchemaValidationResult::Valid => return Ok(()),
            SchemaValidationResult::Invalid(errs) => {
                errors.push(format_schema_errors(label, &errs))
            }
        }
    }

    anyhow::bail!("strict validation failed:\n{}", errors.join("\n"))
}

fn format_schema_errors(label: &str, errs: &[String]) -> String {
    let detail = if errs.is_empty() {
        "schema validation failed".to_string()
    } else {
        errs.join("; ")
    };
    format!("{}: {}", label, detail)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn minimal_sensor_report_json() -> String {
        let report = cockpitctl_types::SensorReport {
            schema: "sensor.report.v1".to_string(),
            tool: cockpitctl_types::ToolInfo {
                name: "test".to_string(),
                version: "1.0.0".to_string(),
                commit: None,
            },
            run: cockpitctl_types::RunInfo {
                started_at: "2026-02-01T00:00:00Z".to_string(),
                ended_at: None,
                duration_ms: None,
                host: None,
                git: None,
                ci: None,
                capabilities: BTreeMap::new(),
            },
            verdict: cockpitctl_types::Verdict {
                status: cockpitctl_types::VerdictStatus::Pass,
                counts: cockpitctl_types::VerdictCounts::default(),
                reasons: vec![],
            },
            findings: vec![],
            artifacts: vec![],
            data: None,
        };
        serde_json::to_string(&report).expect("serialize sensor report")
    }

    #[test]
    fn strict_accepts_minimal_sensor_report() {
        let json = minimal_sensor_report_json();
        validate_input_bytes(json.as_bytes(), SchemaValidation::Strict)
            .expect("strict validation should pass");
    }
}
