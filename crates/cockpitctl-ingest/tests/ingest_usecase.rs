use std::cell::Cell;
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;

use cockpitctl_ingest::{
    CommentRead, DiscoveredSensors, IngestRequest, IngestUseCase, NoOpSchemaValidator, OutputSink,
    PolicySource, ReceiptSource, ReportRead, SchemaValidationResult, SchemaValidator,
};
use cockpitctl_types::{
    CockpitConfig, Finding, MissingPolicy, RunInfo, SchemaValidation, SensorPolicy, SensorReport,
    Severity, ToolInfo, Verdict, VerdictCounts, VerdictStatus,
};

// -----------------------------------------------------------------------------
// Test doubles
// -----------------------------------------------------------------------------

struct StubReceipts {
    sensors: Vec<String>,
    truncated: bool,
    total_found: usize,
    invalid_sensor_ids: Vec<String>,
    reports: HashMap<String, ReportRead>,
    comments: HashMap<String, CommentRead>,
}

impl StubReceipts {
    fn new(sensors: Vec<String>) -> Self {
        let total_found = sensors.len();
        Self {
            sensors,
            truncated: false,
            total_found,
            invalid_sensor_ids: Vec::new(),
            reports: HashMap::new(),
            comments: HashMap::new(),
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
            Some(ReportRead::Missing) => Ok(ReportRead::Missing),
            Some(ReportRead::UnsafePath) => Ok(ReportRead::UnsafePath),
            Some(ReportRead::Oversized { size, cap }) => Ok(ReportRead::Oversized {
                size: *size,
                cap: *cap,
            }),
            Some(ReportRead::Bytes(bytes)) => Ok(ReportRead::Bytes(bytes.clone())),
            None => Ok(ReportRead::Missing),
        }
    }

    fn report_path(&self, sensor_id: &str) -> String {
        format!("artifacts/{}/report.json", sensor_id)
    }

    fn comment_path_if_present(&self, sensor_id: &str) -> anyhow::Result<CommentRead> {
        match self.comments.get(sensor_id) {
            Some(CommentRead::Present(p)) => Ok(CommentRead::Present(p.clone())),
            Some(CommentRead::UnsafePath) => Ok(CommentRead::UnsafePath),
            Some(CommentRead::Missing) | None => Ok(CommentRead::Missing),
        }
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

#[derive(Default)]
struct CaptureOutput {
    reports: std::cell::RefCell<Vec<String>>,
    comments: std::cell::RefCell<Vec<String>>,
}

impl OutputSink for CaptureOutput {
    fn write_cockpit_report(&self, json: &str) -> anyhow::Result<()> {
        self.reports.borrow_mut().push(json.to_string());
        Ok(())
    }

    fn write_cockpit_comment(&self, md: &str) -> anyhow::Result<()> {
        self.comments.borrow_mut().push(md.to_string());
        Ok(())
    }
}

#[derive(Clone)]
enum FixedValidation {
    Valid,
    Invalid(Vec<String>),
}

#[derive(Clone)]
struct CountingValidator {
    calls: Rc<Cell<usize>>,
    result: FixedValidation,
}

impl CountingValidator {
    fn new(result: FixedValidation) -> (Self, Rc<Cell<usize>>) {
        let calls = Rc::new(Cell::new(0));
        (
            Self {
                calls: calls.clone(),
                result,
            },
            calls,
        )
    }
}

impl SchemaValidator for CountingValidator {
    fn validate_receipt(&self, _bytes: &[u8]) -> anyhow::Result<SchemaValidationResult> {
        self.calls.set(self.calls.get() + 1);
        Ok(match &self.result {
            FixedValidation::Valid => SchemaValidationResult::Valid,
            FixedValidation::Invalid(errs) => SchemaValidationResult::Invalid(errs.clone()),
        })
    }
}

struct ExplodingValidator;

impl SchemaValidator for ExplodingValidator {
    fn validate_receipt(&self, _bytes: &[u8]) -> anyhow::Result<SchemaValidationResult> {
        panic!("schema validator should not be called in this test");
    }
}

struct ErroringReceipts {
    sensors: Vec<String>,
    err_on_discover: bool,
    err_on_comment: bool,
    err_on_read: bool,
}

impl ErroringReceipts {
    fn with_sensors(sensors: Vec<String>) -> Self {
        Self {
            sensors,
            err_on_discover: false,
            err_on_comment: false,
            err_on_read: false,
        }
    }
}

impl ReceiptSource for ErroringReceipts {
    fn discovered_sensors(&self) -> anyhow::Result<DiscoveredSensors> {
        if self.err_on_discover {
            anyhow::bail!("discovery failed");
        }
        Ok(DiscoveredSensors {
            sensors: self.sensors.clone(),
            truncated: false,
            total_found: self.sensors.len(),
            invalid_sensor_ids: vec![],
        })
    }

    fn read_report_bytes(&self, sensor_id: &str) -> anyhow::Result<ReportRead> {
        if self.err_on_read {
            anyhow::bail!("read failed for {}", sensor_id);
        }
        Ok(ReportRead::Bytes(report_bytes(
            VerdictStatus::Pass,
            VerdictCounts::default(),
            vec![],
        )))
    }

    fn report_path(&self, sensor_id: &str) -> String {
        format!("artifacts/{}/report.json", sensor_id)
    }

    fn comment_path_if_present(&self, _sensor_id: &str) -> anyhow::Result<CommentRead> {
        if self.err_on_comment {
            anyhow::bail!("comment read failed");
        }
        Ok(CommentRead::Missing)
    }
}

struct ErroringPolicy;

impl PolicySource for ErroringPolicy {
    fn load_config(&self) -> anyhow::Result<Option<CockpitConfig>> {
        anyhow::bail!("policy load failed");
    }
}

struct ErroringValidator;

impl SchemaValidator for ErroringValidator {
    fn validate_receipt(&self, _bytes: &[u8]) -> anyhow::Result<SchemaValidationResult> {
        anyhow::bail!("schema validator failed");
    }
}

struct ErroringOutput {
    fail_report: bool,
    fail_comment: bool,
}

impl OutputSink for ErroringOutput {
    fn write_cockpit_report(&self, _json: &str) -> anyhow::Result<()> {
        if self.fail_report {
            anyhow::bail!("report write failed");
        }
        Ok(())
    }

    fn write_cockpit_comment(&self, _md: &str) -> anyhow::Result<()> {
        if self.fail_comment {
            anyhow::bail!("comment write failed");
        }
        Ok(())
    }
}

// -----------------------------------------------------------------------------
// Helpers
// -----------------------------------------------------------------------------

fn tool_info() -> ToolInfo {
    ToolInfo {
        name: "test".to_string(),
        version: "1.0.0".to_string(),
        commit: None,
    }
}

fn run_info() -> RunInfo {
    RunInfo {
        started_at: "2026-02-01T00:00:00Z".to_string(),
        ended_at: None,
        duration_ms: None,
        host: None,
        git: None,
        ci: None,
        capabilities: BTreeMap::new(),
    }
}

fn report_bytes(status: VerdictStatus, counts: VerdictCounts, findings: Vec<Finding>) -> Vec<u8> {
    let report = SensorReport {
        schema: "sensor.report.v1".to_string(),
        tool: tool_info(),
        run: run_info(),
        verdict: Verdict {
            status,
            counts,
            reasons: vec![],
        },
        findings,
        artifacts: vec![],
        data: None,
    };
    serde_json::to_vec(&report).expect("serialize report")
}

fn default_request() -> IngestRequest {
    IngestRequest {
        labels: vec![],
        tool: tool_info(),
        run: run_info(),
        schema_validation_override: None,
    }
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[test]
fn ingest_uses_discovered_sensors_when_config_empty() {
    let mut receipts = StubReceipts::new(vec!["alpha".into(), "beta".into()]);
    receipts.reports.insert(
        "alpha".to_string(),
        ReportRead::Bytes(report_bytes(
            VerdictStatus::Pass,
            VerdictCounts::default(),
            vec![],
        )),
    );
    receipts.reports.insert(
        "beta".to_string(),
        ReportRead::Bytes(report_bytes(
            VerdictStatus::Pass,
            VerdictCounts::default(),
            vec![],
        )),
    );

    let policy = StubPolicy { cfg: None };
    let output = CaptureOutput::default();
    let uc = IngestUseCase::new(receipts, policy, output, ExplodingValidator, |_, _| {
        "COMMENT".to_string()
    });

    let result = uc.execute(default_request()).expect("execute");
    assert_eq!(result.report.sensors.len(), 2);
    assert_eq!(result.exit_code, 0);
}

#[test]
fn ingest_prefers_configured_sensors_over_discovered() {
    let receipts = StubReceipts::new(vec!["discovered".into()]);
    let mut cfg = CockpitConfig::default();
    cfg.sensors.insert(
        "expected".to_string(),
        SensorPolicy {
            blocking: true,
            missing: MissingPolicy::Warn,
            section: None,
            require_label: None,
            repro: None,
        },
    );

    let policy = StubPolicy { cfg: Some(cfg) };
    let output = CaptureOutput::default();
    let uc = IngestUseCase::new(receipts, policy, output, ExplodingValidator, |_, _| {
        "COMMENT".to_string()
    });

    let result = uc.execute(default_request()).expect("execute");
    let sensors: Vec<_> = result
        .report
        .sensors
        .iter()
        .map(|s| s.id.as_str())
        .collect();
    assert_eq!(sensors, vec!["expected"]);
}

#[test]
fn ingest_missing_receipt_emits_highlight_for_warn_policy() {
    let receipts = StubReceipts::new(vec![]);
    let mut cfg = CockpitConfig::default();
    cfg.sensors.insert(
        "missing".to_string(),
        SensorPolicy {
            blocking: true,
            missing: MissingPolicy::Warn,
            section: None,
            require_label: None,
            repro: None,
        },
    );

    let policy = StubPolicy { cfg: Some(cfg) };
    let output = CaptureOutput::default();
    let uc = IngestUseCase::new(receipts, policy, output, ExplodingValidator, |_, _| {
        "COMMENT".to_string()
    });

    let result = uc.execute(default_request()).expect("execute");
    assert_eq!(result.report.highlights.len(), 1);
    let h = &result.report.highlights[0];
    assert_eq!(h.finding.code, "cockpit.missing_receipt");
    assert_eq!(h.finding.severity, Severity::Warn);
    assert_eq!(
        result.report.sensors[0].missing_policy_applied,
        Some(MissingPolicy::Warn)
    );
}

#[test]
fn ingest_label_gate_skips_when_label_missing() {
    let receipts = StubReceipts::new(vec![]);
    let mut cfg = CockpitConfig::default();
    cfg.sensors.insert(
        "gated".to_string(),
        SensorPolicy {
            blocking: true,
            missing: MissingPolicy::Fail,
            section: None,
            require_label: Some("needs-label".to_string()),
            repro: None,
        },
    );

    let policy = StubPolicy { cfg: Some(cfg) };
    let output = CaptureOutput::default();
    let uc = IngestUseCase::new(receipts, policy, output, ExplodingValidator, |_, _| {
        "COMMENT".to_string()
    });

    let result = uc.execute(default_request()).expect("execute");
    assert_eq!(result.report.sensors.len(), 1);
    let summary = &result.report.sensors[0];
    assert_eq!(summary.verdict.status, VerdictStatus::Skip);
    assert!(result.report.highlights.is_empty());
}

#[test]
fn ingest_invalid_sensor_id_expected_emits_path_traversal() {
    let receipts = StubReceipts::new(vec![]);
    let mut cfg = CockpitConfig::default();
    cfg.sensors.insert(
        "bad..id".to_string(),
        SensorPolicy {
            blocking: true,
            missing: MissingPolicy::Fail,
            section: None,
            require_label: None,
            repro: None,
        },
    );

    let policy = StubPolicy { cfg: Some(cfg) };
    let output = CaptureOutput::default();
    let uc = IngestUseCase::new(receipts, policy, output, ExplodingValidator, |_, _| {
        "COMMENT".to_string()
    });

    let result = uc.execute(default_request()).expect("execute");
    assert_eq!(result.report.highlights.len(), 1);
    let h = &result.report.highlights[0];
    assert_eq!(h.finding.code, "cockpit.path_traversal");
    assert_eq!(result.report.sensors[0].verdict.status, VerdictStatus::Fail);
    assert_eq!(result.exit_code, 2);
}

#[test]
fn ingest_discovery_invalid_ids_emit_highlight() {
    let mut receipts = StubReceipts::new(vec![]);
    receipts.invalid_sensor_ids.push("bad.id".to_string());
    let policy = StubPolicy { cfg: None };
    let output = CaptureOutput::default();
    let uc = IngestUseCase::new(receipts, policy, output, ExplodingValidator, |_, _| {
        "COMMENT".to_string()
    });

    let result = uc.execute(default_request()).expect("execute");
    assert_eq!(result.report.highlights.len(), 1);
    assert_eq!(
        result.report.highlights[0].finding.code,
        "cockpit.path_traversal"
    );
}

#[test]
fn ingest_oversized_receipt_emits_oversized_highlight() {
    let mut receipts = StubReceipts::new(vec!["big".into()]);
    receipts.reports.insert(
        "big".to_string(),
        ReportRead::Oversized { size: 10, cap: 2 },
    );

    let mut cfg = CockpitConfig::default();
    cfg.sensors.insert(
        "big".to_string(),
        SensorPolicy {
            blocking: true,
            missing: MissingPolicy::Fail,
            section: None,
            require_label: None,
            repro: None,
        },
    );

    let policy = StubPolicy { cfg: Some(cfg) };
    let output = CaptureOutput::default();
    let uc = IngestUseCase::new(receipts, policy, output, ExplodingValidator, |_, _| {
        "COMMENT".to_string()
    });

    let result = uc.execute(default_request()).expect("execute");
    assert_eq!(result.exit_code, 2);
    assert_eq!(
        result.report.highlights[0].finding.code,
        "cockpit.receipt_oversized"
    );
}

#[test]
fn ingest_strict_schema_validation_blocks_invalid_receipt() {
    let mut receipts = StubReceipts::new(vec!["bad".into()]);
    receipts.reports.insert(
        "bad".to_string(),
        ReportRead::Bytes(report_bytes(
            VerdictStatus::Pass,
            VerdictCounts::default(),
            vec![],
        )),
    );

    let mut cfg = CockpitConfig::default();
    cfg.policy.schema_validation = SchemaValidation::Strict;
    cfg.sensors.insert(
        "bad".to_string(),
        SensorPolicy {
            blocking: true,
            missing: MissingPolicy::Fail,
            section: None,
            require_label: None,
            repro: None,
        },
    );

    let policy = StubPolicy { cfg: Some(cfg) };
    let output = CaptureOutput::default();
    let (validator, calls) =
        CountingValidator::new(FixedValidation::Invalid(vec!["missing schema".to_string()]));
    let uc = IngestUseCase::new(receipts, policy, output, validator, |_, _| {
        "COMMENT".to_string()
    });

    let result = uc.execute(default_request()).expect("execute");
    assert_eq!(calls.get(), 1, "validator should be called in strict mode");
    assert_eq!(result.exit_code, 2);
    assert_eq!(
        result.report.highlights[0].finding.code,
        "cockpit.schema_violation"
    );
    assert!(!result.report.sensors[0].errors.is_empty());
}

#[test]
fn ingest_strict_schema_validation_allows_valid_receipt() {
    let mut receipts = StubReceipts::new(vec!["ok".into()]);
    receipts.reports.insert(
        "ok".to_string(),
        ReportRead::Bytes(report_bytes(
            VerdictStatus::Pass,
            VerdictCounts::default(),
            vec![],
        )),
    );

    let mut cfg = CockpitConfig::default();
    cfg.policy.schema_validation = SchemaValidation::Strict;
    cfg.sensors.insert(
        "ok".to_string(),
        SensorPolicy {
            blocking: true,
            missing: MissingPolicy::Fail,
            section: None,
            require_label: None,
            repro: None,
        },
    );

    let policy = StubPolicy { cfg: Some(cfg) };
    let output = CaptureOutput::default();
    let (validator, calls) = CountingValidator::new(FixedValidation::Valid);
    let uc = IngestUseCase::new(receipts, policy, output, validator, |_, _| {
        "COMMENT".to_string()
    });

    let result = uc.execute(default_request()).expect("execute");
    assert_eq!(calls.get(), 1, "validator should be called in strict mode");
    assert_eq!(result.exit_code, 0);
    assert!(result.report.highlights.is_empty());
}

#[test]
fn ingest_invalid_json_surfaces_invalid_receipt() {
    let mut receipts = StubReceipts::new(vec!["broken".into()]);
    receipts
        .reports
        .insert("broken".to_string(), ReportRead::Bytes(b"{".to_vec()));

    let mut cfg = CockpitConfig::default();
    cfg.sensors.insert(
        "broken".to_string(),
        SensorPolicy {
            blocking: true,
            missing: MissingPolicy::Fail,
            section: None,
            require_label: None,
            repro: None,
        },
    );

    let policy = StubPolicy { cfg: Some(cfg) };
    let output = CaptureOutput::default();
    let uc = IngestUseCase::new(receipts, policy, output, ExplodingValidator, |_, _| {
        "COMMENT".to_string()
    });

    let result = uc.execute(default_request()).expect("execute");
    assert_eq!(result.exit_code, 2);
    assert_eq!(
        result.report.highlights[0].finding.code,
        "cockpit.invalid_receipt"
    );
}

#[test]
fn ingest_comment_unsafe_path_emits_highlight_and_continues() {
    let mut receipts = StubReceipts::new(vec!["sensor".into()]);
    receipts.reports.insert(
        "sensor".to_string(),
        ReportRead::Bytes(report_bytes(
            VerdictStatus::Pass,
            VerdictCounts::default(),
            vec![],
        )),
    );
    receipts
        .comments
        .insert("sensor".to_string(), CommentRead::UnsafePath);

    let mut cfg = CockpitConfig::default();
    cfg.sensors.insert(
        "sensor".to_string(),
        SensorPolicy {
            blocking: true,
            missing: MissingPolicy::Fail,
            section: None,
            require_label: None,
            repro: None,
        },
    );

    let policy = StubPolicy { cfg: Some(cfg) };
    let output = CaptureOutput::default();
    let uc = IngestUseCase::new(receipts, policy, output, ExplodingValidator, |_, _| {
        "COMMENT".to_string()
    });

    let result = uc.execute(default_request()).expect("execute");
    assert_eq!(result.report.sensors.len(), 1);
    assert!(result.report.sensors[0].comment_path.is_none());
    assert_eq!(result.report.highlights.len(), 1);
    assert_eq!(
        result.report.highlights[0].finding.code,
        "cockpit.path_traversal"
    );
}

#[test]
fn ingest_comment_path_present_is_propagated() {
    let mut receipts = StubReceipts::new(vec!["alpha".into()]);
    receipts.reports.insert(
        "alpha".to_string(),
        ReportRead::Bytes(report_bytes(
            VerdictStatus::Pass,
            VerdictCounts::default(),
            vec![],
        )),
    );
    receipts.comments.insert(
        "alpha".to_string(),
        CommentRead::Present("artifacts/alpha/comment.md".to_string()),
    );

    let policy = StubPolicy { cfg: None };
    let output = CaptureOutput::default();
    let uc = IngestUseCase::new(receipts, policy, output, ExplodingValidator, |_, _| {
        "COMMENT".to_string()
    });

    let result = uc.execute(default_request()).expect("execute");
    assert_eq!(result.report.sensors.len(), 1);
    let summary = &result.report.sensors[0];
    assert_eq!(
        summary.comment_path.as_deref(),
        Some("artifacts/alpha/comment.md")
    );
}

#[test]
fn ingest_schema_validation_override_lax_skips_validator() {
    let mut receipts = StubReceipts::new(vec!["sensor".into()]);
    receipts.reports.insert(
        "sensor".to_string(),
        ReportRead::Bytes(report_bytes(
            VerdictStatus::Pass,
            VerdictCounts::default(),
            vec![],
        )),
    );

    let mut cfg = CockpitConfig::default();
    cfg.policy.schema_validation = SchemaValidation::Strict;
    cfg.sensors.insert(
        "sensor".to_string(),
        SensorPolicy {
            blocking: true,
            missing: MissingPolicy::Fail,
            section: None,
            require_label: None,
            repro: None,
        },
    );

    let policy = StubPolicy { cfg: Some(cfg) };
    let output = CaptureOutput::default();
    let uc = IngestUseCase::new(receipts, policy, output, ExplodingValidator, |_, _| {
        "COMMENT".to_string()
    });

    let mut req = default_request();
    req.schema_validation_override = Some(SchemaValidation::Lax);
    let result = uc.execute(req).expect("execute");
    assert_eq!(result.exit_code, 0);
}

#[test]
fn ingest_schema_validation_override_strict_forces_validator() {
    let mut receipts = StubReceipts::new(vec!["sensor".into()]);
    receipts.reports.insert(
        "sensor".to_string(),
        ReportRead::Bytes(report_bytes(
            VerdictStatus::Pass,
            VerdictCounts::default(),
            vec![],
        )),
    );

    let mut cfg = CockpitConfig::default();
    cfg.policy.schema_validation = SchemaValidation::Lax;
    cfg.sensors.insert(
        "sensor".to_string(),
        SensorPolicy {
            blocking: true,
            missing: MissingPolicy::Fail,
            section: None,
            require_label: None,
            repro: None,
        },
    );

    let policy = StubPolicy { cfg: Some(cfg) };
    let output = CaptureOutput::default();
    let (validator, calls) = CountingValidator::new(FixedValidation::Valid);
    let uc = IngestUseCase::new(receipts, policy, output, validator, |_, _| {
        "COMMENT".to_string()
    });

    let mut req = default_request();
    req.schema_validation_override = Some(SchemaValidation::Strict);
    let result = uc.execute(req).expect("execute");
    assert_eq!(result.exit_code, 0);
    assert_eq!(
        calls.get(),
        1,
        "validator should be called in override strict"
    );
}

#[test]
fn ingest_truncated_discovery_emits_highlight() {
    let mut receipts = StubReceipts::new(vec!["alpha".into()]);
    receipts.truncated = true;
    receipts.total_found = 5;
    receipts.reports.insert(
        "alpha".to_string(),
        ReportRead::Bytes(report_bytes(
            VerdictStatus::Pass,
            VerdictCounts::default(),
            vec![],
        )),
    );

    let policy = StubPolicy { cfg: None };
    let output = CaptureOutput::default();
    let uc = IngestUseCase::new(receipts, policy, output, ExplodingValidator, |_, _| {
        "COMMENT".to_string()
    });

    let result = uc.execute(default_request()).expect("execute");
    assert_eq!(result.report.sensors.len(), 1);
    assert!(
        result
            .report
            .highlights
            .iter()
            .any(|h| h.finding.code == "cockpit.sensors_truncated")
    );
}

#[test]
fn ingest_label_gate_allows_when_label_present() {
    let mut receipts = StubReceipts::new(vec!["gated".into()]);
    receipts.reports.insert(
        "gated".to_string(),
        ReportRead::Bytes(report_bytes(
            VerdictStatus::Pass,
            VerdictCounts::default(),
            vec![],
        )),
    );

    let mut cfg = CockpitConfig::default();
    cfg.sensors.insert(
        "gated".to_string(),
        SensorPolicy {
            blocking: true,
            missing: MissingPolicy::Fail,
            section: None,
            require_label: Some("needs-label".to_string()),
            repro: None,
        },
    );

    let policy = StubPolicy { cfg: Some(cfg) };
    let output = CaptureOutput::default();
    let uc = IngestUseCase::new(receipts, policy, output, ExplodingValidator, |_, _| {
        "COMMENT".to_string()
    });

    let mut req = default_request();
    req.labels.push("needs-label".to_string());
    let result = uc.execute(req).expect("execute");
    assert_eq!(
        result.report.sensors[0].presence,
        cockpitctl_types::Presence::Present
    );
    assert!(result.report.highlights.is_empty());
}

#[test]
fn noop_schema_validator_always_returns_valid() {
    let validator = NoOpSchemaValidator;
    let result = validator.validate_receipt(b"{ not json }").unwrap();
    assert!(matches!(result, SchemaValidationResult::Valid));
}

#[test]
fn ingest_report_unsafe_path_emits_highlight() {
    let mut receipts = StubReceipts::new(vec!["safe_sensor".into()]);
    receipts
        .reports
        .insert("safe_sensor".to_string(), ReportRead::UnsafePath);

    let mut cfg = CockpitConfig::default();
    cfg.sensors.insert(
        "safe_sensor".to_string(),
        SensorPolicy {
            blocking: true,
            missing: MissingPolicy::Fail,
            section: None,
            require_label: None,
            repro: None,
        },
    );
    let policy = StubPolicy { cfg: Some(cfg) };
    let output = CaptureOutput::default();

    let uc = IngestUseCase::new(receipts, policy, output, ExplodingValidator, |_r, _cfg| {
        "comment".to_string()
    });

    let result = uc.execute(default_request()).expect("ingest");
    assert_eq!(result.exit_code, 2);

    let highlight_codes: Vec<String> = result
        .report
        .highlights
        .iter()
        .map(|h| h.finding.code.clone())
        .collect();
    assert!(
        highlight_codes
            .iter()
            .any(|c| c == "cockpit.path_traversal"),
        "expected path traversal highlight"
    );
}

#[test]
fn ingest_propagates_discovery_error() {
    let receipts = ErroringReceipts {
        sensors: vec![],
        err_on_discover: true,
        err_on_comment: false,
        err_on_read: false,
    };
    let policy = StubPolicy { cfg: None };
    let output = CaptureOutput::default();

    let uc = IngestUseCase::new(receipts, policy, output, ExplodingValidator, |_r, _cfg| {
        "comment".to_string()
    });

    let err = uc.execute(default_request()).err().expect("expected error");
    assert!(format!("{:#}", err).contains("discover sensors"));
}

#[test]
fn ingest_propagates_policy_load_error() {
    let receipts = ErroringReceipts::with_sensors(vec!["sensor".to_string()]);
    let output = CaptureOutput::default();
    let uc = IngestUseCase::new(
        receipts,
        ErroringPolicy,
        output,
        ExplodingValidator,
        |_r, _cfg| "comment".to_string(),
    );

    let err = uc.execute(default_request()).err().expect("expected error");
    assert!(format!("{:#}", err).contains("load cockpit.toml"));
}

#[test]
fn ingest_propagates_comment_read_error() {
    let receipts = ErroringReceipts {
        sensors: vec!["sensor".to_string()],
        err_on_discover: false,
        err_on_comment: true,
        err_on_read: false,
    };
    let policy = StubPolicy { cfg: None };
    let output = CaptureOutput::default();
    let uc = IngestUseCase::new(receipts, policy, output, ExplodingValidator, |_r, _cfg| {
        "comment".to_string()
    });

    let err = uc.execute(default_request()).err().expect("expected error");
    assert!(format!("{:#}", err).contains("comment read failed"));
}

#[test]
fn ingest_propagates_report_read_error() {
    let receipts = ErroringReceipts {
        sensors: vec!["sensor".to_string()],
        err_on_discover: false,
        err_on_comment: false,
        err_on_read: true,
    };
    let policy = StubPolicy { cfg: None };
    let output = CaptureOutput::default();
    let uc = IngestUseCase::new(receipts, policy, output, ExplodingValidator, |_r, _cfg| {
        "comment".to_string()
    });

    let err = uc.execute(default_request()).err().expect("expected error");
    assert!(format!("{:#}", err).contains("read failed"));
}

#[test]
fn ingest_propagates_schema_validator_error() {
    let receipts = ErroringReceipts::with_sensors(vec!["sensor".to_string()]);
    let mut cfg = CockpitConfig::default();
    cfg.policy.schema_validation = SchemaValidation::Strict;
    cfg.sensors.insert(
        "sensor".to_string(),
        SensorPolicy {
            blocking: true,
            missing: MissingPolicy::Fail,
            section: None,
            require_label: None,
            repro: None,
        },
    );
    let policy = StubPolicy { cfg: Some(cfg) };
    let output = CaptureOutput::default();

    let uc = IngestUseCase::new(receipts, policy, output, ErroringValidator, |_r, _cfg| {
        "comment".to_string()
    });

    let err = uc.execute(default_request()).err().expect("expected error");
    assert!(format!("{:#}", err).contains("schema validator failed"));
}

#[test]
fn ingest_propagates_report_write_error() {
    let receipts = ErroringReceipts::with_sensors(vec!["sensor".to_string()]);
    let policy = StubPolicy { cfg: None };
    let output = ErroringOutput {
        fail_report: true,
        fail_comment: false,
    };

    let uc = IngestUseCase::new(receipts, policy, output, ExplodingValidator, |_r, _cfg| {
        "comment".to_string()
    });

    let err = uc.execute(default_request()).err().expect("expected error");
    assert!(format!("{:#}", err).contains("report write failed"));
}

#[test]
fn ingest_propagates_comment_write_error() {
    let receipts = ErroringReceipts::with_sensors(vec!["sensor".to_string()]);
    let policy = StubPolicy { cfg: None };
    let output = ErroringOutput {
        fail_report: false,
        fail_comment: true,
    };

    let uc = IngestUseCase::new(receipts, policy, output, ExplodingValidator, |_r, _cfg| {
        "comment".to_string()
    });

    let err = uc.execute(default_request()).err().expect("expected error");
    assert!(format!("{:#}", err).contains("comment write failed"));
}
