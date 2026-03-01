//! Port contracts shared by ingest orchestration and adapters.

use anyhow::Result;
use cockpitctl_types::{
    BuildfixSummary, CockpitConfig, CockpitReport, RunInfo, SchemaValidation, ToolInfo,
};

/// Result of sensor discovery, including any truncation info.
pub struct DiscoveredSensors {
    /// Discovered sensor IDs (sorted lexically).
    pub sensors: Vec<String>,
    /// True if the number of sensors was capped due to max_receipts limit.
    pub truncated: bool,
    /// Total number of sensors found before truncation (if truncated).
    pub total_found: usize,
    /// Sensor IDs that were rejected due to invalid path traversal.
    pub invalid_sensor_ids: Vec<String>,
}

/// Result of reading a sensor report.
pub enum ReportRead {
    Missing,
    Bytes(Vec<u8>),
    Oversized { size: u64, cap: usize },
    UnsafePath,
}

/// Result of checking for a sensor comment.
pub enum CommentRead {
    Missing,
    Present(String),
    UnsafePath,
}

/// Result of reading a buildfix plan.
pub enum PlanRead {
    Missing,
    Bytes(Vec<u8>),
    Oversized { size: u64, cap: usize },
}

/// Ports: where receipts come from.
pub trait ReceiptSource {
    /// Return a stable list of discovered sensor IDs that have a receipt file present.
    /// May be truncated if the number of sensors exceeds a safety limit.
    fn discovered_sensors(&self) -> Result<DiscoveredSensors>;

    /// Read the report.json bytes for a sensor if present.
    fn read_report_bytes(&self, sensor_id: &str) -> Result<ReportRead>;

    /// Return canonical relative path to the sensor's report.json.
    fn report_path(&self, sensor_id: &str) -> String;

    /// Return canonical relative path to the sensor's comment.md if present.
    fn comment_path_if_present(&self, sensor_id: &str) -> Result<CommentRead>;

    /// Read the plan.json bytes for a sensor if present (buildfix integration).
    fn read_plan_bytes(&self, _sensor_id: &str) -> Result<PlanRead> {
        Ok(PlanRead::Missing)
    }
}

/// Ports: policy source (cockpit.toml).
pub trait PolicySource {
    fn load_config(&self) -> Result<Option<CockpitConfig>>;
}

/// Ports: where outputs are written.
pub trait OutputSink {
    fn write_cockpit_report(&self, json: &str) -> Result<()>;
    fn write_cockpit_comment(&self, md: &str) -> Result<()>;

    /// Write an extra file (e.g. from a post-processor hook). Default is a no-op.
    fn write_extra_file(&self, _name: &str, _content: &[u8]) -> Result<()> {
        Ok(())
    }
}

/// Result of JSON Schema validation.
pub enum SchemaValidationResult {
    /// Receipt conforms to the schema.
    Valid,
    /// Receipt violates the schema. Contains a list of validation error messages.
    Invalid(Vec<String>),
}

/// Ports: schema validation for receipts.
pub trait SchemaValidator {
    /// Validate raw receipt bytes against the sensor.report.v1 JSON schema.
    /// Returns validation result with detailed errors if invalid.
    fn validate_receipt(&self, bytes: &[u8]) -> Result<SchemaValidationResult>;
}

/// A no-op schema validator that always returns Valid (for lax mode).
pub struct NoOpSchemaValidator;

impl SchemaValidator for NoOpSchemaValidator {
    fn validate_receipt(&self, _bytes: &[u8]) -> Result<SchemaValidationResult> {
        Ok(SchemaValidationResult::Valid)
    }
}

/// Request inputs for ingestion.
pub struct IngestRequest {
    pub labels: Vec<String>,
    pub tool: ToolInfo,
    pub run: RunInfo,
    pub schema_validation_override: Option<SchemaValidation>,
}

/// Result of ingestion, including the computed report and recommended exit code.
pub struct IngestResult {
    pub report: CockpitReport,
    pub comment_md: String,
    pub exit_code: i32,
    pub buildfix: Option<BuildfixSummary>,
}
