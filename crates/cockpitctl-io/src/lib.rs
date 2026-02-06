//! Filesystem adapters for cockpitctl (ports implementation).
//!
//! This crate is the boundary between IO and the ingest use case.

use anyhow::{Context, Result};
use cockpitctl_ingest::{
    CommentRead, DiscoveredSensors, OutputSink, PolicySource, ReceiptSource, ReportRead,
    SchemaValidationResult, SchemaValidator,
};
use cockpitctl_types::{CockpitConfig, is_valid_sensor_id};
use jsonschema::Validator;
use std::fs;
use std::path::{Path, PathBuf};

/// Default cap on number of receipts (sensors) to process.
/// Protects against DoS if someone creates thousands of sensor directories.
pub const DEFAULT_MAX_RECEIPTS: usize = 100;

#[derive(Clone)]
pub struct FsLayout {
    pub artifacts_dir: PathBuf,
    pub out_dir: PathBuf,
    pub config_path: PathBuf,
    pub max_receipt_bytes: usize,
    /// Maximum number of sensor receipts to process. Protects against DoS.
    pub max_receipts: usize,
}

impl FsLayout {
    pub fn new(artifacts_dir: impl Into<PathBuf>, config_path: impl Into<PathBuf>) -> Self {
        let artifacts_dir = artifacts_dir.into();
        let out_dir = artifacts_dir.join("cockpit");
        Self {
            artifacts_dir,
            out_dir,
            config_path: config_path.into(),
            max_receipt_bytes: 2 * 1024 * 1024, // 2MB default safety cap
            max_receipts: DEFAULT_MAX_RECEIPTS,
        }
    }

    /// Set a custom max_receipts limit. Returns self for chaining.
    pub fn with_max_receipts(mut self, max: usize) -> Self {
        self.max_receipts = max;
        self
    }

    pub fn sensor_dir(&self, sensor_id: &str) -> PathBuf {
        self.artifacts_dir.join(sensor_id)
    }

    pub fn report_file(&self, sensor_id: &str) -> PathBuf {
        self.sensor_dir(sensor_id).join("report.json")
    }

    pub fn comment_file(&self, sensor_id: &str) -> PathBuf {
        self.sensor_dir(sensor_id).join("comment.md")
    }

    pub fn cockpit_report_file(&self) -> PathBuf {
        self.out_dir.join("report.json")
    }

    pub fn cockpit_comment_file(&self) -> PathBuf {
        self.out_dir.join("comment.md")
    }
}

#[derive(Clone)]
pub struct FsReceiptSource {
    layout: FsLayout,
    artifacts_root: PathBuf,
}

impl FsReceiptSource {
    pub fn new(layout: FsLayout) -> Self {
        let artifacts_root = canonicalize_root(&layout.artifacts_dir);
        Self {
            layout,
            artifacts_root,
        }
    }

    fn is_safe_path(&self, path: &Path) -> bool {
        let canonical = match fs::canonicalize(path) {
            Ok(p) => p,
            Err(_) => return false,
        };
        canonical.starts_with(&self.artifacts_root)
    }
}

fn absolute_path(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    }
}

fn canonicalize_root(path: &Path) -> PathBuf {
    if path.exists() {
        fs::canonicalize(path).unwrap_or_else(|_| absolute_path(path))
    } else {
        absolute_path(path)
    }
}

impl ReceiptSource for FsReceiptSource {
    fn discovered_sensors(&self) -> Result<DiscoveredSensors> {
        let mut out = Vec::new();
        let mut invalid = Vec::new();
        if !self.layout.artifacts_dir.exists() {
            // No artifacts dir: valid for local runs. Treat as empty.
            return Ok(DiscoveredSensors {
                sensors: out,
                truncated: false,
                total_found: 0,
                invalid_sensor_ids: invalid,
            });
        }

        // Each direct child directory of artifacts/ is a sensor candidate.
        for entry in fs::read_dir(&self.layout.artifacts_dir).with_context(|| {
            format!("read artifacts dir {}", self.layout.artifacts_dir.display())
        })? {
            let entry = entry?;
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if name == "cockpit" {
                continue;
            }
            if !is_valid_sensor_id(&name) {
                invalid.push(name);
                continue;
            }
            if self.layout.report_file(&name).exists() {
                out.push(name);
            }
        }

        out.sort();
        invalid.sort();

        let total_found = out.len();
        let truncated = total_found > self.layout.max_receipts;
        if truncated {
            out.truncate(self.layout.max_receipts);
        }

        Ok(DiscoveredSensors {
            sensors: out,
            truncated,
            total_found,
            invalid_sensor_ids: invalid,
        })
    }

    fn read_report_bytes(&self, sensor_id: &str) -> Result<ReportRead> {
        if !is_valid_sensor_id(sensor_id) {
            return Ok(ReportRead::UnsafePath);
        }
        let p = self.layout.report_file(sensor_id);
        if !p.exists() {
            return Ok(ReportRead::Missing);
        }
        if !self.is_safe_path(&p) {
            return Ok(ReportRead::UnsafePath);
        }
        let meta = fs::metadata(&p)?;
        if meta.len() as usize > self.layout.max_receipt_bytes {
            return Ok(ReportRead::Oversized {
                size: meta.len(),
                cap: self.layout.max_receipt_bytes,
            });
        }
        let bytes = fs::read(&p).with_context(|| format!("read receipt {}", p.display()))?;
        Ok(ReportRead::Bytes(bytes))
    }

    fn report_path(&self, sensor_id: &str) -> String {
        format!("artifacts/{}/report.json", sensor_id)
    }

    fn comment_path_if_present(&self, sensor_id: &str) -> Result<CommentRead> {
        if !is_valid_sensor_id(sensor_id) {
            return Ok(CommentRead::UnsafePath);
        }
        let p = self.layout.comment_file(sensor_id);
        if p.exists() {
            if !self.is_safe_path(&p) {
                return Ok(CommentRead::UnsafePath);
            }
            Ok(CommentRead::Present(format!(
                "artifacts/{}/comment.md",
                sensor_id
            )))
        } else {
            Ok(CommentRead::Missing)
        }
    }
}

#[derive(Clone)]
pub struct FsPolicySource {
    layout: FsLayout,
}

impl FsPolicySource {
    pub fn new(layout: FsLayout) -> Self {
        Self { layout }
    }
}

impl PolicySource for FsPolicySource {
    fn load_config(&self) -> Result<Option<CockpitConfig>> {
        let p = &self.layout.config_path;
        if !p.exists() {
            return Ok(None);
        }
        let txt = fs::read_to_string(p).with_context(|| format!("read config {}", p.display()))?;
        let cfg: CockpitConfig =
            toml::from_str(&txt).with_context(|| format!("parse TOML {}", p.display()))?;
        Ok(Some(cfg))
    }
}

#[derive(Clone)]
pub struct FsOutputSink {
    layout: FsLayout,
}

impl FsOutputSink {
    pub fn new(layout: FsLayout) -> Self {
        Self { layout }
    }
}

impl OutputSink for FsOutputSink {
    fn write_cockpit_report(&self, json: &str) -> Result<()> {
        fs::create_dir_all(&self.layout.out_dir)
            .with_context(|| format!("create out dir {}", self.layout.out_dir.display()))?;
        let p = self.layout.cockpit_report_file();
        fs::write(&p, json).with_context(|| format!("write {}", p.display()))?;
        Ok(())
    }

    fn write_cockpit_comment(&self, md: &str) -> Result<()> {
        fs::create_dir_all(&self.layout.out_dir)
            .with_context(|| format!("create out dir {}", self.layout.out_dir.display()))?;
        let p = self.layout.cockpit_comment_file();
        fs::write(&p, md).with_context(|| format!("write {}", p.display()))?;
        Ok(())
    }
}

/// JSON Schema validator for sensor reports.
///
/// Validates receipts against the `sensor.report.v1` JSON schema.
pub struct JsonSchemaValidator {
    validator: Validator,
}

impl JsonSchemaValidator {
    /// Create a new validator by loading the schema from a file path.
    pub fn from_file(schema_path: impl AsRef<std::path::Path>) -> Result<Self> {
        let schema_str = fs::read_to_string(schema_path.as_ref())
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
        // First, parse the JSON.
        let value: serde_json::Value =
            serde_json::from_slice(bytes).context("receipt is not valid JSON")?;

        // Validate against the schema.
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Minimal valid sensor report for testing.
    fn minimal_report() -> &'static str {
        r#"{
            "schema": "sensor.report.v1",
            "tool": { "name": "test", "version": "1.0" },
            "run": { "started_at": "2026-01-01T00:00:00Z" },
            "verdict": { "status": "pass", "counts": { "info": 0, "warn": 0, "error": 0 } }
        }"#
    }

    #[test]
    fn discovered_sensors_respects_max_receipts_cap() {
        let tmp = TempDir::new().unwrap();
        let artifacts = tmp.path().join("artifacts");

        // Create 5 sensor directories with reports.
        for i in 0..5 {
            let sensor_dir = artifacts.join(format!("sensor_{:02}", i));
            fs::create_dir_all(&sensor_dir).unwrap();
            fs::write(sensor_dir.join("report.json"), minimal_report()).unwrap();
        }

        // Use a cap of 3.
        let layout =
            FsLayout::new(&artifacts, tmp.path().join("cockpit.toml")).with_max_receipts(3);
        let source = FsReceiptSource::new(layout);
        let result = source.discovered_sensors().unwrap();

        // Should return exactly 3 sensors, truncated flag true, total 5.
        assert_eq!(result.sensors.len(), 3);
        assert!(result.truncated);
        assert_eq!(result.total_found, 5);
        assert!(result.invalid_sensor_ids.is_empty());

        // Should be the first 3 in lexical order.
        assert_eq!(result.sensors, vec!["sensor_00", "sensor_01", "sensor_02"]);
    }

    #[test]
    fn discovered_sensors_no_truncation_when_under_cap() {
        let tmp = TempDir::new().unwrap();
        let artifacts = tmp.path().join("artifacts");

        // Create 2 sensor directories.
        for i in 0..2 {
            let sensor_dir = artifacts.join(format!("sensor_{}", i));
            fs::create_dir_all(&sensor_dir).unwrap();
            fs::write(sensor_dir.join("report.json"), minimal_report()).unwrap();
        }

        // Use default cap (100).
        let layout = FsLayout::new(&artifacts, tmp.path().join("cockpit.toml"));
        let source = FsReceiptSource::new(layout);
        let result = source.discovered_sensors().unwrap();

        assert_eq!(result.sensors.len(), 2);
        assert!(!result.truncated);
        assert_eq!(result.total_found, 2);
        assert!(result.invalid_sensor_ids.is_empty());
    }

    #[test]
    fn discovered_sensors_cap_at_exact_limit_is_not_truncated() {
        let tmp = TempDir::new().unwrap();
        let artifacts = tmp.path().join("artifacts");

        // Create exactly 3 sensors with cap of 3.
        for i in 0..3 {
            let sensor_dir = artifacts.join(format!("sensor_{}", i));
            fs::create_dir_all(&sensor_dir).unwrap();
            fs::write(sensor_dir.join("report.json"), minimal_report()).unwrap();
        }

        let layout =
            FsLayout::new(&artifacts, tmp.path().join("cockpit.toml")).with_max_receipts(3);
        let source = FsReceiptSource::new(layout);
        let result = source.discovered_sensors().unwrap();

        assert_eq!(result.sensors.len(), 3);
        assert!(!result.truncated);
        assert_eq!(result.total_found, 3);
        assert!(result.invalid_sensor_ids.is_empty());
    }

    #[test]
    fn default_max_receipts_is_100() {
        let layout = FsLayout::new("/tmp/artifacts", "/tmp/cockpit.toml");
        assert_eq!(layout.max_receipts, 100);
        assert_eq!(DEFAULT_MAX_RECEIPTS, 100);
    }

    // -------------------------------------------------------------------------
    // JsonSchemaValidator tests
    // -------------------------------------------------------------------------

    /// Minimal valid sensor report with all required fields and the findings array.
    fn valid_sensor_report() -> &'static str {
        r#"{
            "schema": "sensor.report.v1",
            "tool": { "name": "test-tool", "version": "1.0.0" },
            "run": { "started_at": "2026-01-15T10:30:00Z" },
            "verdict": { "status": "pass", "counts": { "info": 0, "warn": 0, "error": 0 } },
            "findings": []
        }"#
    }

    #[test]
    fn json_schema_validator_accepts_valid_receipt() {
        let validator = JsonSchemaValidator::sensor_report_v1().unwrap();
        let result = validator
            .validate_receipt(valid_sensor_report().as_bytes())
            .unwrap();
        assert!(matches!(result, SchemaValidationResult::Valid));
    }

    #[test]
    fn json_schema_validator_accepts_valid_receipt_with_findings() {
        let report = r#"{
            "schema": "sensor.report.v1",
            "tool": { "name": "clippy", "version": "0.1.0" },
            "run": { "started_at": "2026-01-15T10:30:00Z" },
            "verdict": { "status": "warn", "counts": { "info": 0, "warn": 1, "error": 0 } },
            "findings": [
                {
                    "severity": "warn",
                    "code": "clippy::unwrap_used",
                    "message": "used `unwrap()` on a `Result` value"
                }
            ]
        }"#;
        let validator = JsonSchemaValidator::sensor_report_v1().unwrap();
        let result = validator.validate_receipt(report.as_bytes()).unwrap();
        assert!(matches!(result, SchemaValidationResult::Valid));
    }

    #[test]
    fn json_schema_validator_accepts_valid_receipt_with_full_finding() {
        let report = r#"{
            "schema": "sensor.report.v1",
            "tool": { "name": "clippy", "version": "0.1.0", "commit": "abc123" },
            "run": {
                "started_at": "2026-01-15T10:30:00Z",
                "ended_at": "2026-01-15T10:31:00Z",
                "duration_ms": 60000
            },
            "verdict": { "status": "warn", "counts": { "info": 0, "warn": 1, "error": 0 } },
            "findings": [
                {
                    "severity": "warn",
                    "code": "clippy::unwrap_used",
                    "message": "used `unwrap()` on a `Result` value",
                    "location": { "path": "src/main.rs", "line": 42, "col": 10 },
                    "help": "Consider using `expect()` or `?` instead",
                    "url": "https://rust-lang.github.io/rust-clippy/",
                    "fingerprint": "abc123def456",
                    "check_id": "unwrap-check"
                }
            ],
            "data": { "custom": "payload" }
        }"#;
        let validator = JsonSchemaValidator::sensor_report_v1().unwrap();
        let result = validator.validate_receipt(report.as_bytes()).unwrap();
        assert!(matches!(result, SchemaValidationResult::Valid));
    }

    #[test]
    fn json_schema_validator_rejects_missing_required_fields() {
        // Missing "schema" field
        let report = r#"{
            "tool": { "name": "test", "version": "1.0" },
            "run": { "started_at": "2026-01-15T10:30:00Z" },
            "verdict": { "status": "pass", "counts": { "info": 0, "warn": 0, "error": 0 } },
            "findings": []
        }"#;
        let validator = JsonSchemaValidator::sensor_report_v1().unwrap();
        let result = validator.validate_receipt(report.as_bytes()).unwrap();
        match result {
            SchemaValidationResult::Invalid(errors) => {
                assert!(!errors.is_empty(), "should have at least one error");
                let joined = errors.join(" ");
                assert!(
                    joined.contains("schema") || joined.contains("required"),
                    "error should mention missing 'schema' field: {:?}",
                    errors
                );
            }
            SchemaValidationResult::Valid => panic!("expected Invalid result for missing 'schema'"),
        }
    }

    #[test]
    fn json_schema_validator_rejects_invalid_verdict_status() {
        let report = r#"{
            "schema": "sensor.report.v1",
            "tool": { "name": "test", "version": "1.0" },
            "run": { "started_at": "2026-01-15T10:30:00Z" },
            "verdict": { "status": "invalid_status", "counts": { "info": 0, "warn": 0, "error": 0 } },
            "findings": []
        }"#;
        let validator = JsonSchemaValidator::sensor_report_v1().unwrap();
        let result = validator.validate_receipt(report.as_bytes()).unwrap();
        match result {
            SchemaValidationResult::Invalid(errors) => {
                assert!(!errors.is_empty(), "should have at least one error");
                // Should mention the invalid enum value
                let joined = errors.join(" ");
                assert!(
                    joined.contains("invalid_status")
                        || joined.contains("status")
                        || joined.contains("enum"),
                    "error should mention invalid verdict status: {:?}",
                    errors
                );
            }
            SchemaValidationResult::Valid => {
                panic!("expected Invalid result for bad verdict status")
            }
        }
    }

    #[test]
    fn json_schema_validator_rejects_invalid_finding_severity() {
        let report = r#"{
            "schema": "sensor.report.v1",
            "tool": { "name": "test", "version": "1.0" },
            "run": { "started_at": "2026-01-15T10:30:00Z" },
            "verdict": { "status": "pass", "counts": { "info": 0, "warn": 0, "error": 0 } },
            "findings": [
                {
                    "severity": "critical",
                    "code": "test-001",
                    "message": "test message"
                }
            ]
        }"#;
        let validator = JsonSchemaValidator::sensor_report_v1().unwrap();
        let result = validator.validate_receipt(report.as_bytes()).unwrap();
        match result {
            SchemaValidationResult::Invalid(errors) => {
                assert!(!errors.is_empty(), "should have at least one error");
            }
            SchemaValidationResult::Valid => panic!("expected Invalid result for bad severity"),
        }
    }

    #[test]
    fn json_schema_validator_rejects_finding_missing_required_fields() {
        // Finding missing "code" field
        let report = r#"{
            "schema": "sensor.report.v1",
            "tool": { "name": "test", "version": "1.0" },
            "run": { "started_at": "2026-01-15T10:30:00Z" },
            "verdict": { "status": "pass", "counts": { "info": 0, "warn": 0, "error": 0 } },
            "findings": [
                {
                    "severity": "warn",
                    "message": "test message"
                }
            ]
        }"#;
        let validator = JsonSchemaValidator::sensor_report_v1().unwrap();
        let result = validator.validate_receipt(report.as_bytes()).unwrap();
        match result {
            SchemaValidationResult::Invalid(errors) => {
                assert!(!errors.is_empty(), "should have at least one error");
                let joined = errors.join(" ");
                assert!(
                    joined.contains("code") || joined.contains("required"),
                    "error should mention missing 'code' field: {:?}",
                    errors
                );
            }
            SchemaValidationResult::Valid => {
                panic!("expected Invalid result for missing finding code")
            }
        }
    }

    #[test]
    fn json_schema_validator_rejects_additional_properties_at_root() {
        let report = r#"{
            "schema": "sensor.report.v1",
            "tool": { "name": "test", "version": "1.0" },
            "run": { "started_at": "2026-01-15T10:30:00Z" },
            "verdict": { "status": "pass", "counts": { "info": 0, "warn": 0, "error": 0 } },
            "findings": [],
            "extra_field": "not allowed"
        }"#;
        let validator = JsonSchemaValidator::sensor_report_v1().unwrap();
        let result = validator.validate_receipt(report.as_bytes()).unwrap();
        match result {
            SchemaValidationResult::Invalid(errors) => {
                assert!(
                    !errors.is_empty(),
                    "should have at least one error for extra field"
                );
            }
            SchemaValidationResult::Valid => {
                panic!("expected Invalid result for additional properties at root")
            }
        }
    }

    #[test]
    fn json_schema_validator_returns_error_for_invalid_json() {
        let invalid_json = b"{ not valid json }";
        let validator = JsonSchemaValidator::sensor_report_v1().unwrap();
        let result = validator.validate_receipt(invalid_json);
        assert!(result.is_err(), "should return Err for invalid JSON");
        let err_msg = format!("{:?}", result.err().unwrap());
        assert!(
            err_msg.contains("JSON") || err_msg.contains("json"),
            "error should mention JSON parsing: {}",
            err_msg
        );
    }

    #[test]
    fn json_schema_validator_rejects_empty_code_in_finding() {
        // code has minLength: 1
        let report = r#"{
            "schema": "sensor.report.v1",
            "tool": { "name": "test", "version": "1.0" },
            "run": { "started_at": "2026-01-15T10:30:00Z" },
            "verdict": { "status": "pass", "counts": { "info": 0, "warn": 0, "error": 0 } },
            "findings": [
                {
                    "severity": "warn",
                    "code": "",
                    "message": "test message"
                }
            ]
        }"#;
        let validator = JsonSchemaValidator::sensor_report_v1().unwrap();
        let result = validator.validate_receipt(report.as_bytes()).unwrap();
        match result {
            SchemaValidationResult::Invalid(errors) => {
                assert!(
                    !errors.is_empty(),
                    "should have at least one error for empty code"
                );
            }
            SchemaValidationResult::Valid => panic!("expected Invalid result for empty code"),
        }
    }

    #[test]
    fn json_schema_validator_rejects_negative_line_number() {
        // line has minimum: 1
        let report = r#"{
            "schema": "sensor.report.v1",
            "tool": { "name": "test", "version": "1.0" },
            "run": { "started_at": "2026-01-15T10:30:00Z" },
            "verdict": { "status": "pass", "counts": { "info": 0, "warn": 0, "error": 0 } },
            "findings": [
                {
                    "severity": "warn",
                    "code": "test",
                    "message": "test",
                    "location": { "path": "src/main.rs", "line": 0 }
                }
            ]
        }"#;
        let validator = JsonSchemaValidator::sensor_report_v1().unwrap();
        let result = validator.validate_receipt(report.as_bytes()).unwrap();
        match result {
            SchemaValidationResult::Invalid(errors) => {
                assert!(
                    !errors.is_empty(),
                    "should have at least one error for line=0"
                );
            }
            SchemaValidationResult::Valid => panic!("expected Invalid result for line=0"),
        }
    }
}
