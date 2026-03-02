//! Orchestration expansion tests for the ingest use case.
//!
//! Exercises multi-sensor mixed verdicts, port error boundaries,
//! output sink failures, schema validation modes, and edge cases
//! in the orchestration pipeline.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};

use cockpitctl_ingest::{
    CommentRead, DiscoveredSensors, IngestRequest, IngestUseCase, NoOpSchemaValidator, OutputSink,
    PlanRead, PolicySource, ReceiptSource, ReportRead, SchemaValidationResult, SchemaValidator,
};
use cockpitctl_types::{
    CockpitConfig, CockpitReport, Finding, MissingPolicy, RunInfo, SchemaValidation, SensorPolicy,
    SensorReport, Severity, ToolInfo, Verdict, VerdictCounts, VerdictStatus,
};

// =============================================================================
// Test doubles
// =============================================================================

struct StubReceipts {
    sensors: Vec<String>,
    reports: HashMap<String, ReportRead>,
    truncated: bool,
    total_found: usize,
    invalid_sensor_ids: Vec<String>,
}

impl StubReceipts {
    fn new(sensors: Vec<&str>, reports: HashMap<String, ReportRead>) -> Self {
        let total_found = sensors.len();
        Self {
            sensors: sensors.into_iter().map(String::from).collect(),
            reports,
            truncated: false,
            total_found,
            invalid_sensor_ids: vec![],
        }
    }

    fn empty() -> Self {
        Self {
            sensors: vec![],
            reports: HashMap::new(),
            truncated: false,
            total_found: 0,
            invalid_sensor_ids: vec![],
        }
    }
}

impl ReceiptSource for StubReceipts {
    fn discovered_sensors(&self) -> anyhow::Result<DiscoveredSensors> {
        Ok(DiscoveredSensors {
            sensors: self.sensors.clone(),
            truncated: self.truncated,
            total_found: self.total_found,
            invalid_sensor_ids: self.invalid_sensor_ids.clone(),
        })
    }

    fn read_report_bytes(&self, sensor_id: &str) -> anyhow::Result<ReportRead> {
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
        format!("artifacts/{sensor_id}/report.json")
    }

    fn comment_path_if_present(&self, _sensor_id: &str) -> anyhow::Result<CommentRead> {
        Ok(CommentRead::Missing)
    }

    fn read_plan_bytes(&self, _sensor_id: &str) -> anyhow::Result<PlanRead> {
        Ok(PlanRead::Missing)
    }
}

struct FailingDiscovery;

impl ReceiptSource for FailingDiscovery {
    fn discovered_sensors(&self) -> anyhow::Result<DiscoveredSensors> {
        Err(anyhow::anyhow!("simulated discovery IO failure"))
    }
    fn read_report_bytes(&self, _: &str) -> anyhow::Result<ReportRead> {
        Ok(ReportRead::Missing)
    }
    fn report_path(&self, s: &str) -> String {
        format!("artifacts/{s}/report.json")
    }
    fn comment_path_if_present(&self, _: &str) -> anyhow::Result<CommentRead> {
        Ok(CommentRead::Missing)
    }
}

struct FailingReportRead {
    sensors: Vec<String>,
}

impl ReceiptSource for FailingReportRead {
    fn discovered_sensors(&self) -> anyhow::Result<DiscoveredSensors> {
        Ok(DiscoveredSensors {
            sensors: self.sensors.clone(),
            truncated: false,
            total_found: self.sensors.len(),
            invalid_sensor_ids: vec![],
        })
    }
    fn read_report_bytes(&self, _: &str) -> anyhow::Result<ReportRead> {
        Err(anyhow::anyhow!("simulated read IO failure"))
    }
    fn report_path(&self, s: &str) -> String {
        format!("artifacts/{s}/report.json")
    }
    fn comment_path_if_present(&self, _: &str) -> anyhow::Result<CommentRead> {
        Ok(CommentRead::Missing)
    }
}

struct StubPolicy {
    cfg: Option<CockpitConfig>,
}

impl PolicySource for StubPolicy {
    fn load_config(&self) -> anyhow::Result<Option<CockpitConfig>> {
        Ok(self.cfg.clone())
    }
}

struct FailingPolicy;

impl PolicySource for FailingPolicy {
    fn load_config(&self) -> anyhow::Result<Option<CockpitConfig>> {
        Err(anyhow::anyhow!("config read failed"))
    }
}

#[derive(Default)]
struct CaptureSink {
    reports: RefCell<Vec<String>>,
    comments: RefCell<Vec<String>>,
}

impl OutputSink for CaptureSink {
    fn write_cockpit_report(&self, json: &str) -> anyhow::Result<()> {
        self.reports.borrow_mut().push(json.to_string());
        Ok(())
    }
    fn write_cockpit_comment(&self, md: &str) -> anyhow::Result<()> {
        self.comments.borrow_mut().push(md.to_string());
        Ok(())
    }
}

struct FailingReportSink;

impl OutputSink for FailingReportSink {
    fn write_cockpit_report(&self, _: &str) -> anyhow::Result<()> {
        Err(anyhow::anyhow!("report write IO failure"))
    }
    fn write_cockpit_comment(&self, _: &str) -> anyhow::Result<()> {
        Ok(())
    }
}

struct FailingCommentSink;

impl OutputSink for FailingCommentSink {
    fn write_cockpit_report(&self, _: &str) -> anyhow::Result<()> {
        Ok(())
    }
    fn write_cockpit_comment(&self, _: &str) -> anyhow::Result<()> {
        Err(anyhow::anyhow!("comment write IO failure"))
    }
}

struct AlwaysInvalidValidator;

impl SchemaValidator for AlwaysInvalidValidator {
    fn validate_receipt(&self, _: &[u8]) -> anyhow::Result<SchemaValidationResult> {
        Ok(SchemaValidationResult::Invalid(vec![
            "field X is required".to_string(),
        ]))
    }
}

struct FailingValidator;

impl SchemaValidator for FailingValidator {
    fn validate_receipt(&self, _: &[u8]) -> anyhow::Result<SchemaValidationResult> {
        Err(anyhow::anyhow!("validator internal error"))
    }
}

// =============================================================================
// Helpers
// =============================================================================

fn tool_info() -> ToolInfo {
    ToolInfo {
        name: "cockpitctl".to_string(),
        version: "0.1.0-test".to_string(),
        commit: None,
    }
}

fn run_info() -> RunInfo {
    RunInfo {
        started_at: "2026-01-01T00:00:00Z".to_string(),
        ended_at: None,
        duration_ms: None,
        host: None,
        git: None,
        ci: None,
        capabilities: BTreeMap::new(),
    }
}

fn default_request() -> IngestRequest {
    IngestRequest {
        labels: vec![],
        tool: tool_info(),
        run: run_info(),
        schema_validation_override: None,
    }
}

fn stub_render(_report: &CockpitReport, _cfg: &CockpitConfig) -> String {
    "<!-- rendered -->".to_string()
}

fn sensor_report_bytes(status: VerdictStatus) -> Vec<u8> {
    let report = SensorReport {
        schema: "sensor.report.v1".to_string(),
        tool: ToolInfo {
            name: "test-sensor".to_string(),
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

fn sensor_report_bytes_with_findings(status: VerdictStatus, findings: Vec<Finding>) -> Vec<u8> {
    let report = SensorReport {
        schema: "sensor.report.v1".to_string(),
        tool: ToolInfo {
            name: "test-sensor".to_string(),
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
        findings,
        artifacts: vec![],
        data: None,
    };
    serde_json::to_vec(&report).unwrap()
}

fn make_policy(sensors: Vec<(&str, bool, MissingPolicy)>) -> CockpitConfig {
    let mut cfg = CockpitConfig::default();
    for (id, blocking, missing) in sensors {
        cfg.sensors.insert(
            id.to_string(),
            SensorPolicy {
                blocking,
                missing,
                section: None,
                require_label: None,
                repro: None,
            },
        );
    }
    cfg
}

// =============================================================================
// Tests: multi-sensor mixed verdicts
// =============================================================================

#[test]
fn three_sensors_pass_warn_fail_yields_exit_code_2() {
    let reports = HashMap::from([
        (
            "s-pass".to_string(),
            ReportRead::Bytes(sensor_report_bytes(VerdictStatus::Pass)),
        ),
        (
            "s-warn".to_string(),
            ReportRead::Bytes(sensor_report_bytes(VerdictStatus::Warn)),
        ),
        (
            "s-fail".to_string(),
            ReportRead::Bytes(sensor_report_bytes(VerdictStatus::Fail)),
        ),
    ]);
    let receipts = StubReceipts::new(vec!["s-pass", "s-warn", "s-fail"], reports);
    let cfg = make_policy(vec![
        ("s-pass", true, MissingPolicy::Fail),
        ("s-warn", true, MissingPolicy::Fail),
        ("s-fail", true, MissingPolicy::Fail),
    ]);
    let uc = IngestUseCase::new(
        receipts,
        StubPolicy { cfg: Some(cfg) },
        CaptureSink::default(),
        NoOpSchemaValidator,
        stub_render,
    );
    let result = uc.execute(default_request()).unwrap();
    assert_eq!(result.exit_code, 2);
    assert_eq!(result.report.verdict.status, VerdictStatus::Fail);
    assert_eq!(result.report.sensors.len(), 3);
}

#[test]
fn all_sensors_pass_yields_exit_code_0() {
    let reports = HashMap::from([
        (
            "a".to_string(),
            ReportRead::Bytes(sensor_report_bytes(VerdictStatus::Pass)),
        ),
        (
            "b".to_string(),
            ReportRead::Bytes(sensor_report_bytes(VerdictStatus::Pass)),
        ),
    ]);
    let receipts = StubReceipts::new(vec!["a", "b"], reports);
    let cfg = make_policy(vec![
        ("a", true, MissingPolicy::Fail),
        ("b", true, MissingPolicy::Fail),
    ]);
    let uc = IngestUseCase::new(
        receipts,
        StubPolicy { cfg: Some(cfg) },
        CaptureSink::default(),
        NoOpSchemaValidator,
        stub_render,
    );
    let result = uc.execute(default_request()).unwrap();
    assert_eq!(result.exit_code, 0);
    assert_eq!(result.report.verdict.status, VerdictStatus::Pass);
}

#[test]
fn non_blocking_fail_does_not_escalate_to_exit_2() {
    let reports = HashMap::from([(
        "nb".to_string(),
        ReportRead::Bytes(sensor_report_bytes(VerdictStatus::Fail)),
    )]);
    let receipts = StubReceipts::new(vec!["nb"], reports);
    let cfg = make_policy(vec![("nb", false, MissingPolicy::Fail)]);
    let uc = IngestUseCase::new(
        receipts,
        StubPolicy { cfg: Some(cfg) },
        CaptureSink::default(),
        NoOpSchemaValidator,
        stub_render,
    );
    let result = uc.execute(default_request()).unwrap();
    assert_eq!(
        result.exit_code, 0,
        "non-blocking fail must not cause exit 2"
    );
}

#[test]
fn mixed_blocking_and_non_blocking_uses_blocking_for_verdict() {
    let reports = HashMap::from([
        (
            "blocker".to_string(),
            ReportRead::Bytes(sensor_report_bytes(VerdictStatus::Pass)),
        ),
        (
            "info".to_string(),
            ReportRead::Bytes(sensor_report_bytes(VerdictStatus::Fail)),
        ),
    ]);
    let receipts = StubReceipts::new(vec!["blocker", "info"], reports);
    let cfg = make_policy(vec![
        ("blocker", true, MissingPolicy::Fail),
        ("info", false, MissingPolicy::Warn),
    ]);
    let uc = IngestUseCase::new(
        receipts,
        StubPolicy { cfg: Some(cfg) },
        CaptureSink::default(),
        NoOpSchemaValidator,
        stub_render,
    );
    let result = uc.execute(default_request()).unwrap();
    assert_eq!(result.exit_code, 0);
}

// =============================================================================
// Tests: port error boundaries
// =============================================================================

#[test]
fn discovery_io_error_propagates_as_anyhow() {
    let uc = IngestUseCase::new(
        FailingDiscovery,
        StubPolicy { cfg: None },
        CaptureSink::default(),
        NoOpSchemaValidator,
        stub_render,
    );
    let result = uc.execute(default_request());
    assert!(result.is_err());
    let err = result.err().unwrap();
    assert!(
        format!("{err:#}").contains("discover sensors"),
        "error should wrap discovery context"
    );
}

#[test]
fn report_read_io_error_propagates() {
    let receipts = FailingReportRead {
        sensors: vec!["sensor".to_string()],
    };
    let uc = IngestUseCase::new(
        receipts,
        StubPolicy { cfg: None },
        CaptureSink::default(),
        NoOpSchemaValidator,
        stub_render,
    );
    let result = uc.execute(default_request());
    assert!(result.is_err());
    let err = result.err().unwrap();
    assert!(format!("{err:#}").contains("read IO failure"));
}

#[test]
fn policy_load_error_propagates() {
    let receipts = StubReceipts::empty();
    let uc = IngestUseCase::new(
        receipts,
        FailingPolicy,
        CaptureSink::default(),
        NoOpSchemaValidator,
        stub_render,
    );
    let result = uc.execute(default_request());
    assert!(result.is_err());
    let err = result.err().unwrap();
    assert!(format!("{err:#}").contains("load cockpit.toml"));
}

// =============================================================================
// Tests: output sink failures
// =============================================================================

#[test]
fn report_sink_failure_propagates() {
    let receipts = StubReceipts::empty();
    let uc = IngestUseCase::new(
        receipts,
        StubPolicy { cfg: None },
        FailingReportSink,
        NoOpSchemaValidator,
        stub_render,
    );
    let result = uc.execute(default_request());
    assert!(result.is_err());
    let err = result.err().unwrap();
    assert!(format!("{err:#}").contains("report write IO failure"));
}

#[test]
fn comment_sink_failure_propagates() {
    let receipts = StubReceipts::empty();
    let uc = IngestUseCase::new(
        receipts,
        StubPolicy { cfg: None },
        FailingCommentSink,
        NoOpSchemaValidator,
        stub_render,
    );
    let result = uc.execute(default_request());
    assert!(result.is_err());
    let err = result.err().unwrap();
    assert!(format!("{err:#}").contains("comment write IO failure"));
}

// =============================================================================
// Tests: schema validation modes
// =============================================================================

#[test]
fn strict_validation_invalid_receipt_produces_schema_violation_highlight() {
    let reports = HashMap::from([(
        "s".to_string(),
        ReportRead::Bytes(sensor_report_bytes(VerdictStatus::Pass)),
    )]);
    let receipts = StubReceipts::new(vec!["s"], reports);
    let mut cfg = make_policy(vec![("s", true, MissingPolicy::Fail)]);
    cfg.policy.schema_validation = SchemaValidation::Strict;
    let uc = IngestUseCase::new(
        receipts,
        StubPolicy { cfg: Some(cfg) },
        CaptureSink::default(),
        AlwaysInvalidValidator,
        stub_render,
    );
    let result = uc.execute(default_request()).unwrap();
    assert_eq!(result.exit_code, 2);
    assert!(
        result
            .report
            .highlights
            .iter()
            .any(|h| h.finding.code == "cockpit.schema_violation"),
        "expected schema_violation highlight"
    );
}

#[test]
fn lax_validation_skips_validator_entirely() {
    let reports = HashMap::from([(
        "s".to_string(),
        ReportRead::Bytes(sensor_report_bytes(VerdictStatus::Pass)),
    )]);
    let receipts = StubReceipts::new(vec!["s"], reports);
    let cfg = make_policy(vec![("s", true, MissingPolicy::Fail)]);
    // lax is default; using FailingValidator would panic if called
    let uc = IngestUseCase::new(
        receipts,
        StubPolicy { cfg: Some(cfg) },
        CaptureSink::default(),
        FailingValidator,
        stub_render,
    );
    let result = uc.execute(default_request()).unwrap();
    assert_eq!(result.exit_code, 0, "lax mode must not call validator");
}

#[test]
fn cli_override_strict_forces_validation() {
    let reports = HashMap::from([(
        "s".to_string(),
        ReportRead::Bytes(sensor_report_bytes(VerdictStatus::Pass)),
    )]);
    let receipts = StubReceipts::new(vec!["s"], reports);
    let cfg = make_policy(vec![("s", true, MissingPolicy::Fail)]);
    let uc = IngestUseCase::new(
        receipts,
        StubPolicy { cfg: Some(cfg) },
        CaptureSink::default(),
        AlwaysInvalidValidator,
        stub_render,
    );
    let mut req = default_request();
    req.schema_validation_override = Some(SchemaValidation::Strict);
    let result = uc.execute(req).unwrap();
    assert_eq!(
        result.exit_code, 2,
        "CLI strict override should engage validator"
    );
}

#[test]
fn validator_internal_error_propagates() {
    let reports = HashMap::from([(
        "s".to_string(),
        ReportRead::Bytes(sensor_report_bytes(VerdictStatus::Pass)),
    )]);
    let receipts = StubReceipts::new(vec!["s"], reports);
    let mut cfg = make_policy(vec![("s", true, MissingPolicy::Fail)]);
    cfg.policy.schema_validation = SchemaValidation::Strict;
    let uc = IngestUseCase::new(
        receipts,
        StubPolicy { cfg: Some(cfg) },
        CaptureSink::default(),
        FailingValidator,
        stub_render,
    );
    let result = uc.execute(default_request());
    assert!(result.is_err());
    let err = result.err().unwrap();
    assert!(format!("{err:#}").contains("validator internal error"));
}

// =============================================================================
// Tests: edge cases
// =============================================================================

#[test]
fn empty_discovery_no_policy_yields_pass() {
    let receipts = StubReceipts::empty();
    let uc = IngestUseCase::new(
        receipts,
        StubPolicy { cfg: None },
        CaptureSink::default(),
        NoOpSchemaValidator,
        stub_render,
    );
    let result = uc.execute(default_request()).unwrap();
    assert_eq!(result.exit_code, 0);
    assert!(result.report.sensors.is_empty());
    assert!(result.report.highlights.is_empty());
}

#[test]
fn sensor_with_findings_populates_highlights() {
    let findings = vec![Finding {
        severity: Severity::Error,
        check_id: None,
        code: "TEST001".to_string(),
        message: "something broke".to_string(),
        location: None,
        help: None,
        url: None,
        fingerprint: None,
        data: None,
    }];
    let reports = HashMap::from([(
        "diag".to_string(),
        ReportRead::Bytes(sensor_report_bytes_with_findings(
            VerdictStatus::Fail,
            findings,
        )),
    )]);
    let receipts = StubReceipts::new(vec!["diag"], reports);
    let cfg = make_policy(vec![("diag", true, MissingPolicy::Fail)]);
    let uc = IngestUseCase::new(
        receipts,
        StubPolicy { cfg: Some(cfg) },
        CaptureSink::default(),
        NoOpSchemaValidator,
        stub_render,
    );
    let result = uc.execute(default_request()).unwrap();
    assert_eq!(result.exit_code, 2);
    assert!(
        !result.report.highlights.is_empty(),
        "findings should generate highlights"
    );
}
