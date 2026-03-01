//! Error path tests for the ingest use case.
//!
//! Verifies that untrusted/malformed inputs produce controlled findings
//! (never panics), and that resource limits are enforced.

use anyhow::Result;
use cockpitctl_ingest::{
    CommentRead, DiscoveredSensors, IngestRequest, IngestUseCase, NoOpSchemaValidator, OutputSink,
    PlanRead, PolicySource, ReceiptSource, ReportRead, SchemaValidationResult, SchemaValidator,
};
use cockpitctl_types::*;
use std::collections::{BTreeMap, HashMap};

// ============================================================================
// Stub / double implementations
// ============================================================================

struct StubReceiptSource {
    sensors: Vec<String>,
    reports: HashMap<String, ReportRead>,
    truncated: bool,
    total_found: usize,
    invalid_sensor_ids: Vec<String>,
}

impl StubReceiptSource {
    fn empty() -> Self {
        Self {
            sensors: vec![],
            reports: HashMap::new(),
            truncated: false,
            total_found: 0,
            invalid_sensor_ids: vec![],
        }
    }

    fn with_sensors(sensors: Vec<String>, reports: HashMap<String, ReportRead>) -> Self {
        let len = sensors.len();
        Self {
            sensors,
            reports,
            truncated: false,
            total_found: len,
            invalid_sensor_ids: vec![],
        }
    }
}

impl ReceiptSource for StubReceiptSource {
    fn discovered_sensors(&self) -> Result<DiscoveredSensors> {
        Ok(DiscoveredSensors {
            sensors: self.sensors.clone(),
            truncated: self.truncated,
            total_found: self.total_found,
            invalid_sensor_ids: self.invalid_sensor_ids.clone(),
        })
    }

    fn read_report_bytes(&self, sensor_id: &str) -> Result<ReportRead> {
        match self.reports.get(sensor_id) {
            Some(ReportRead::Bytes(b)) => Ok(ReportRead::Bytes(b.clone())),
            Some(ReportRead::Missing) => Ok(ReportRead::Missing),
            Some(ReportRead::UnsafePath) => Ok(ReportRead::UnsafePath),
            Some(ReportRead::Oversized { size, cap }) => Ok(ReportRead::Oversized {
                size: *size,
                cap: *cap,
            }),
            None => Ok(ReportRead::Missing),
        }
    }

    fn report_path(&self, sensor_id: &str) -> String {
        format!("artifacts/{}/report.json", sensor_id)
    }

    fn comment_path_if_present(&self, _sensor_id: &str) -> Result<CommentRead> {
        Ok(CommentRead::Missing)
    }

    fn read_plan_bytes(&self, _sensor_id: &str) -> Result<PlanRead> {
        Ok(PlanRead::Missing)
    }
}

/// A receipt source whose `discovered_sensors()` returns an IO error.
struct FailingReceiptSource;

impl ReceiptSource for FailingReceiptSource {
    fn discovered_sensors(&self) -> Result<DiscoveredSensors> {
        Err(anyhow::anyhow!("disk I/O error"))
    }

    fn read_report_bytes(&self, _sensor_id: &str) -> Result<ReportRead> {
        Ok(ReportRead::Missing)
    }

    fn report_path(&self, sensor_id: &str) -> String {
        format!("artifacts/{}/report.json", sensor_id)
    }

    fn comment_path_if_present(&self, _sensor_id: &str) -> Result<CommentRead> {
        Ok(CommentRead::Missing)
    }
}

struct StubPolicySource {
    config: Option<CockpitConfig>,
}

impl PolicySource for StubPolicySource {
    fn load_config(&self) -> Result<Option<CockpitConfig>> {
        Ok(self.config.clone())
    }
}

/// A policy source that returns an error.
struct FailingPolicySource;

impl PolicySource for FailingPolicySource {
    fn load_config(&self) -> Result<Option<CockpitConfig>> {
        Err(anyhow::anyhow!("config file not found"))
    }
}

struct StubOutputSink {
    report: std::cell::RefCell<String>,
    comment: std::cell::RefCell<String>,
}

impl StubOutputSink {
    fn new() -> Self {
        Self {
            report: std::cell::RefCell::new(String::new()),
            comment: std::cell::RefCell::new(String::new()),
        }
    }
}

impl OutputSink for StubOutputSink {
    fn write_cockpit_report(&self, json: &str) -> Result<()> {
        *self.report.borrow_mut() = json.to_string();
        Ok(())
    }

    fn write_cockpit_comment(&self, md: &str) -> Result<()> {
        *self.comment.borrow_mut() = md.to_string();
        Ok(())
    }
}

/// An output sink whose write operations fail.
struct FailingOutputSink;

impl OutputSink for FailingOutputSink {
    fn write_cockpit_report(&self, _json: &str) -> Result<()> {
        Err(anyhow::anyhow!("disk full"))
    }

    fn write_cockpit_comment(&self, _md: &str) -> Result<()> {
        Err(anyhow::anyhow!("disk full"))
    }
}

/// A schema validator that always rejects.
struct RejectingSchemaValidator;

impl SchemaValidator for RejectingSchemaValidator {
    fn validate_receipt(&self, _bytes: &[u8]) -> Result<SchemaValidationResult> {
        Ok(SchemaValidationResult::Invalid(vec![
            "missing required field: verdict".to_string(),
        ]))
    }
}

fn stub_render(_report: &CockpitReport, _cfg: &CockpitConfig) -> String {
    "<!-- rendered -->".to_string()
}

fn make_tool_and_run() -> (ToolInfo, RunInfo) {
    (
        ToolInfo {
            name: "cockpitctl".to_string(),
            version: "0.1.0".to_string(),
            commit: None,
        },
        RunInfo {
            started_at: "2026-01-01T00:00:00Z".to_string(),
            ended_at: None,
            duration_ms: None,
            host: None,
            git: None,
            ci: None,
            capabilities: BTreeMap::new(),
        },
    )
}

fn default_request() -> IngestRequest {
    let (tool, run) = make_tool_and_run();
    IngestRequest {
        labels: vec![],
        tool,
        run,
        schema_validation_override: None,
    }
}

fn minimal_sensor_report_bytes(status: VerdictStatus) -> Vec<u8> {
    let report = SensorReport {
        schema: "sensor.report.v1".to_string(),
        tool: ToolInfo {
            name: "test".to_string(),
            version: "1.0.0".to_string(),
            commit: None,
        },
        run: RunInfo {
            started_at: "2026-01-01T00:00:00Z".to_string(),
            ended_at: None,
            duration_ms: None,
            host: None,
            git: None,
            ci: None,
            capabilities: BTreeMap::new(),
        },
        verdict: Verdict {
            status,
            counts: VerdictCounts::default(),
            reasons: vec![],
        },
        findings: vec![],
        artifacts: vec![],
        data: None,
    };
    serde_json::to_vec(&report).unwrap()
}

// ============================================================================
// Error path tests
// ============================================================================

/// Receipt source IO error propagates as an anyhow error, not a panic.
#[test]
fn receipt_source_io_error_propagates() {
    let uc = IngestUseCase::new(
        FailingReceiptSource,
        StubPolicySource { config: None },
        StubOutputSink::new(),
        NoOpSchemaValidator,
        stub_render,
    );
    let result = uc.execute(default_request());
    assert!(result.is_err(), "IO error should propagate as Err");
    let msg = format!("{:#}", result.err().unwrap());
    assert!(
        msg.contains("disk I/O error"),
        "error message should contain cause"
    );
}

/// Policy source error propagates as an anyhow error, not a panic.
#[test]
fn policy_source_error_propagates() {
    let uc = IngestUseCase::new(
        StubReceiptSource::empty(),
        FailingPolicySource,
        StubOutputSink::new(),
        NoOpSchemaValidator,
        stub_render,
    );
    let result = uc.execute(default_request());
    assert!(result.is_err(), "missing config should propagate as Err");
}

/// Missing config returns default behavior (no sensors expected → pass).
#[test]
fn policy_source_returns_none_uses_defaults() {
    let uc = IngestUseCase::new(
        StubReceiptSource::empty(),
        StubPolicySource { config: None },
        StubOutputSink::new(),
        NoOpSchemaValidator,
        stub_render,
    );
    let result = uc.execute(default_request()).unwrap();
    // Default config with no sensors discovered → pass, empty report.
    assert_eq!(result.exit_code, 0);
    assert!(result.report.sensors.is_empty());
}

/// Malformed JSON receipt produces a finding (Presence::Invalid), not a panic.
#[test]
fn malformed_json_receipt_produces_finding_not_panic() {
    let mut reports = HashMap::new();
    reports.insert(
        "bad-sensor".to_string(),
        ReportRead::Bytes(b"{{not json at all!!!".to_vec()),
    );
    let receipts = StubReceiptSource::with_sensors(vec!["bad-sensor".to_string()], reports);

    let uc = IngestUseCase::new(
        receipts,
        StubPolicySource { config: None },
        StubOutputSink::new(),
        NoOpSchemaValidator,
        stub_render,
    );
    let result = uc.execute(default_request()).unwrap();

    assert_eq!(result.report.sensors.len(), 1);
    assert_eq!(result.report.sensors[0].presence, Presence::Invalid);
    assert!(
        !result.report.sensors[0].errors.is_empty(),
        "should contain error details"
    );
    assert!(
        !result.report.highlights.is_empty(),
        "invalid receipt should produce a highlight"
    );
}

/// Truncated JSON (valid prefix but incomplete) → finding, not panic.
#[test]
fn truncated_json_receipt_produces_finding() {
    let truncated = br#"{"schema":"sensor.report.v1","tool":{"name":"x","version":"1"#;
    let mut reports = HashMap::new();
    reports.insert(
        "truncated".to_string(),
        ReportRead::Bytes(truncated.to_vec()),
    );
    let receipts = StubReceiptSource::with_sensors(vec!["truncated".to_string()], reports);

    let uc = IngestUseCase::new(
        receipts,
        StubPolicySource { config: None },
        StubOutputSink::new(),
        NoOpSchemaValidator,
        stub_render,
    );
    let result = uc.execute(default_request()).unwrap();
    assert_eq!(result.report.sensors[0].presence, Presence::Invalid);
}

/// Empty byte array receipt → finding, not panic.
#[test]
fn empty_bytes_receipt_produces_finding() {
    let mut reports = HashMap::new();
    reports.insert("empty".to_string(), ReportRead::Bytes(vec![]));
    let receipts = StubReceiptSource::with_sensors(vec!["empty".to_string()], reports);

    let uc = IngestUseCase::new(
        receipts,
        StubPolicySource { config: None },
        StubOutputSink::new(),
        NoOpSchemaValidator,
        stub_render,
    );
    let result = uc.execute(default_request()).unwrap();
    assert_eq!(result.report.sensors[0].presence, Presence::Invalid);
}

/// Receipt with wrong schema version parses as SensorReport but may have unexpected fields;
/// with lax validation it should still produce a summary (not crash).
#[test]
fn wrong_schema_version_receipt_no_panic() {
    let report = serde_json::json!({
        "schema": "sensor.report.v999",
        "tool": {"name": "x", "version": "1.0.0"},
        "run": {"started_at": "2026-01-01T00:00:00Z"},
        "verdict": {"status": "pass", "counts": {"info": 0, "warn": 0, "error": 0}},
        "findings": []
    });
    let bytes = serde_json::to_vec(&report).unwrap();
    drop(report);

    let mut reports = HashMap::new();
    reports.insert("wrong-schema".to_string(), ReportRead::Bytes(bytes));
    let receipts = StubReceiptSource::with_sensors(vec!["wrong-schema".to_string()], reports);

    let uc = IngestUseCase::new(
        receipts,
        StubPolicySource { config: None },
        StubOutputSink::new(),
        NoOpSchemaValidator,
        stub_render,
    );
    // Should not panic — lax mode accepts the JSON.
    let result = uc.execute(default_request()).unwrap();
    assert_eq!(result.report.sensors.len(), 1);
}

/// Strict schema validation rejection produces a finding, not a panic.
#[test]
fn strict_schema_violation_produces_finding() {
    let bytes = minimal_sensor_report_bytes(VerdictStatus::Pass);
    let mut reports = HashMap::new();
    reports.insert("strict-fail".to_string(), ReportRead::Bytes(bytes));
    let receipts = StubReceiptSource::with_sensors(vec!["strict-fail".to_string()], reports);

    let mut req = default_request();
    req.schema_validation_override = Some(SchemaValidation::Strict);

    let uc = IngestUseCase::new(
        receipts,
        StubPolicySource { config: None },
        StubOutputSink::new(),
        RejectingSchemaValidator,
        stub_render,
    );
    let result = uc.execute(req).unwrap();
    assert_eq!(result.report.sensors[0].presence, Presence::Invalid);
    assert!(
        result.report.sensors[0]
            .errors
            .iter()
            .any(|e| e.contains("verdict"))
    );
}

/// Output sink write failure propagates as an error.
#[test]
fn output_sink_write_failure_propagates() {
    let uc = IngestUseCase::new(
        StubReceiptSource::empty(),
        StubPolicySource { config: None },
        FailingOutputSink,
        NoOpSchemaValidator,
        stub_render,
    );
    let result = uc.execute(default_request());
    assert!(result.is_err(), "output sink failure should propagate");
}

/// Empty receipt list → valid empty report with pass verdict.
#[test]
fn empty_receipt_list_produces_valid_report() {
    let uc = IngestUseCase::new(
        StubReceiptSource::empty(),
        StubPolicySource { config: None },
        StubOutputSink::new(),
        NoOpSchemaValidator,
        stub_render,
    );
    let result = uc.execute(default_request()).unwrap();

    assert_eq!(result.exit_code, 0);
    assert_eq!(result.report.schema, "cockpit.report.v1");
    assert!(result.report.sensors.is_empty());
    assert!(result.report.highlights.is_empty());
    assert_eq!(result.report.verdict.status, VerdictStatus::Pass);
}

/// Sensor ID with path traversal (`..`) is rejected with a finding.
#[test]
fn path_traversal_sensor_id_rejected() {
    let mut cfg = CockpitConfig::default();
    cfg.sensors.insert(
        "../escape".to_string(),
        SensorPolicy {
            blocking: true,
            ..Default::default()
        },
    );

    let uc = IngestUseCase::new(
        StubReceiptSource::empty(),
        StubPolicySource { config: Some(cfg) },
        StubOutputSink::new(),
        NoOpSchemaValidator,
        stub_render,
    );
    let result = uc.execute(default_request()).unwrap();

    // The path-traversal sensor should be synthesized with a failure.
    let sensor = result
        .report
        .sensors
        .iter()
        .find(|s| s.id == "../escape")
        .expect("sensor should be present in report");
    assert_eq!(sensor.verdict.status, VerdictStatus::Fail);
    assert!(!result.report.highlights.is_empty());
}

/// Receipt exceeds size limit → oversized finding produced.
#[test]
fn receipt_oversized_produces_finding() {
    let mut reports = HashMap::new();
    reports.insert(
        "big-sensor".to_string(),
        ReportRead::Oversized {
            size: 5_000_000,
            cap: 2_097_152,
        },
    );
    let receipts = StubReceiptSource::with_sensors(vec!["big-sensor".to_string()], reports);

    let uc = IngestUseCase::new(
        receipts,
        StubPolicySource { config: None },
        StubOutputSink::new(),
        NoOpSchemaValidator,
        stub_render,
    );
    let result = uc.execute(default_request()).unwrap();

    assert_eq!(result.report.sensors[0].presence, Presence::Invalid);
    assert!(
        result.report.sensors[0]
            .errors
            .iter()
            .any(|e| e.contains("too large"))
    );
    assert!(!result.report.highlights.is_empty());
}

/// Sensor discovery truncation produces a warning highlight.
#[test]
fn sensor_discovery_truncation_produces_highlight() {
    let mut source = StubReceiptSource::with_sensors(
        vec!["s1".to_string(), "s2".to_string()],
        HashMap::from([
            (
                "s1".to_string(),
                ReportRead::Bytes(minimal_sensor_report_bytes(VerdictStatus::Pass)),
            ),
            (
                "s2".to_string(),
                ReportRead::Bytes(minimal_sensor_report_bytes(VerdictStatus::Pass)),
            ),
        ]),
    );
    source.truncated = true;
    source.total_found = 150;

    let uc = IngestUseCase::new(
        source,
        StubPolicySource { config: None },
        StubOutputSink::new(),
        NoOpSchemaValidator,
        stub_render,
    );
    let result = uc.execute(default_request()).unwrap();

    // Should have a truncation highlight.
    assert!(
        result
            .report
            .highlights
            .iter()
            .any(|h| h.finding.code == "cockpit.sensors_truncated"),
        "should contain sensors_truncated highlight"
    );
}

/// Invalid sensor IDs discovered produce path-traversal highlights.
#[test]
fn invalid_sensor_ids_in_discovery_produce_highlights() {
    let mut source = StubReceiptSource::empty();
    source.invalid_sensor_ids = vec!["../bad".to_string(), "also/bad".to_string()];

    let uc = IngestUseCase::new(
        source,
        StubPolicySource { config: None },
        StubOutputSink::new(),
        NoOpSchemaValidator,
        stub_render,
    );
    let result = uc.execute(default_request()).unwrap();

    let traversal_count = result
        .report
        .highlights
        .iter()
        .filter(|h| h.finding.code == "cockpit.path_traversal")
        .count();
    assert_eq!(
        traversal_count, 2,
        "each invalid ID should produce a highlight"
    );
}

/// Unsafe path for report.json → finding, not crash.
#[test]
fn unsafe_path_report_produces_finding() {
    let mut reports = HashMap::new();
    reports.insert("symlink-sensor".to_string(), ReportRead::UnsafePath);
    let receipts = StubReceiptSource::with_sensors(vec!["symlink-sensor".to_string()], reports);

    let uc = IngestUseCase::new(
        receipts,
        StubPolicySource { config: None },
        StubOutputSink::new(),
        NoOpSchemaValidator,
        stub_render,
    );
    let result = uc.execute(default_request()).unwrap();

    assert_eq!(result.report.sensors[0].verdict.status, VerdictStatus::Fail);
    assert!(
        result
            .report
            .highlights
            .iter()
            .any(|h| h.finding.code == "cockpit.path_traversal"),
        "should produce a path_traversal highlight"
    );
}
