//! Insta snapshot tests for the ingest use case.
//!
//! These tests capture the full JSON structure of `CockpitReport` for key
//! scenarios and guard against unintended structural regressions.

use std::collections::{BTreeMap, HashMap};

use cockpitctl_ingest::{
    CommentRead, DiscoveredSensors, IngestRequest, IngestUseCase, OutputSink, PolicySource,
    ReceiptSource, ReportRead, SchemaValidationResult, SchemaValidator,
};
use cockpitctl_types::{
    CockpitConfig, Finding, MissingPolicy, RunInfo, SensorPolicy, SensorReport, Severity, ToolInfo,
    Verdict, VerdictCounts, VerdictStatus,
};

// ---------------------------------------------------------------------------
// Test doubles (mirrors existing test patterns)
// ---------------------------------------------------------------------------

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

struct ExplodingValidator;

impl SchemaValidator for ExplodingValidator {
    fn validate_receipt(&self, _bytes: &[u8]) -> anyhow::Result<SchemaValidationResult> {
        panic!("schema validator should not be called in this test");
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

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

fn noop_render(_report: &cockpitctl_types::CockpitReport, _cfg: &CockpitConfig) -> String {
    "COMMENT".to_string()
}

// ---------------------------------------------------------------------------
// Snapshot tests
// ---------------------------------------------------------------------------

#[test]
fn snapshot_happy_path_two_passing_sensors() {
    let mut receipts = StubReceipts::new(vec!["alpha".into(), "beta".into()]);
    receipts.reports.insert(
        "alpha".to_string(),
        ReportRead::Bytes(report_bytes(
            VerdictStatus::Pass,
            VerdictCounts {
                info: 1,
                warn: 0,
                error: 0,
                suppressed: 0,
            },
            vec![Finding {
                severity: Severity::Info,
                check_id: None,
                code: "info-check".to_string(),
                message: "All good".to_string(),
                location: None,
                help: None,
                url: None,
                fingerprint: None,
                data: None,
            }],
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
    let uc = IngestUseCase::new(receipts, policy, output, ExplodingValidator, noop_render);

    let result = uc.execute(default_request()).expect("execute");
    assert_eq!(result.exit_code, 0);
    insta::assert_json_snapshot!("happy_path_two_passing_sensors", result.report);
}

#[test]
fn snapshot_policy_fail_blocking_sensor_fails() {
    let mut receipts = StubReceipts::new(vec!["linter".into()]);
    receipts.reports.insert(
        "linter".to_string(),
        ReportRead::Bytes(report_bytes(
            VerdictStatus::Fail,
            VerdictCounts {
                info: 0,
                warn: 1,
                error: 3,
                suppressed: 0,
            },
            vec![
                Finding {
                    severity: Severity::Error,
                    check_id: Some("E001".to_string()),
                    code: "lint/unused-var".to_string(),
                    message: "Unused variable `x`".to_string(),
                    location: None,
                    help: Some("Remove or use the variable".to_string()),
                    url: None,
                    fingerprint: None,
                    data: None,
                },
                Finding {
                    severity: Severity::Error,
                    check_id: Some("E002".to_string()),
                    code: "lint/missing-return".to_string(),
                    message: "Missing return statement".to_string(),
                    location: None,
                    help: None,
                    url: None,
                    fingerprint: None,
                    data: None,
                },
                Finding {
                    severity: Severity::Warn,
                    check_id: None,
                    code: "lint/complexity".to_string(),
                    message: "Function is too complex".to_string(),
                    location: None,
                    help: None,
                    url: None,
                    fingerprint: None,
                    data: None,
                },
            ],
        )),
    );

    let mut cfg = CockpitConfig::default();
    cfg.sensors.insert(
        "linter".to_string(),
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
    let uc = IngestUseCase::new(receipts, policy, output, ExplodingValidator, noop_render);

    let result = uc.execute(default_request()).expect("execute");
    assert_eq!(result.exit_code, 2);
    insta::assert_json_snapshot!("policy_fail_blocking_sensor", result.report);
}

#[test]
fn snapshot_empty_sensors_directory() {
    let receipts = StubReceipts::new(vec![]);
    let policy = StubPolicy { cfg: None };
    let output = CaptureOutput::default();
    let uc = IngestUseCase::new(receipts, policy, output, ExplodingValidator, noop_render);

    let result = uc.execute(default_request()).expect("execute");
    assert_eq!(result.exit_code, 0);
    insta::assert_json_snapshot!("empty_sensors_directory", result.report);
}

#[test]
fn snapshot_mixed_verdicts_pass_warn_fail() {
    let mut receipts =
        StubReceipts::new(vec!["build".into(), "coverage".into(), "security".into()]);

    // build: pass
    receipts.reports.insert(
        "build".to_string(),
        ReportRead::Bytes(report_bytes(
            VerdictStatus::Pass,
            VerdictCounts::default(),
            vec![],
        )),
    );

    // coverage: warn
    receipts.reports.insert(
        "coverage".to_string(),
        ReportRead::Bytes(report_bytes(
            VerdictStatus::Warn,
            VerdictCounts {
                info: 0,
                warn: 2,
                error: 0,
                suppressed: 0,
            },
            vec![
                Finding {
                    severity: Severity::Warn,
                    check_id: None,
                    code: "coverage/low".to_string(),
                    message: "Coverage below 80%".to_string(),
                    location: None,
                    help: None,
                    url: None,
                    fingerprint: None,
                    data: None,
                },
                Finding {
                    severity: Severity::Warn,
                    check_id: None,
                    code: "coverage/uncovered-fn".to_string(),
                    message: "Function `foo` has no tests".to_string(),
                    location: None,
                    help: None,
                    url: None,
                    fingerprint: None,
                    data: None,
                },
            ],
        )),
    );

    // security: fail with errors
    receipts.reports.insert(
        "security".to_string(),
        ReportRead::Bytes(report_bytes(
            VerdictStatus::Fail,
            VerdictCounts {
                info: 0,
                warn: 0,
                error: 1,
                suppressed: 0,
            },
            vec![Finding {
                severity: Severity::Error,
                check_id: None,
                code: "sec/vuln".to_string(),
                message: "Critical vulnerability found".to_string(),
                location: None,
                help: None,
                url: Some("https://example.com/cve-123".to_string()),
                fingerprint: None,
                data: None,
            }],
        )),
    );

    let mut cfg = CockpitConfig::default();
    cfg.sensors.insert(
        "build".to_string(),
        SensorPolicy {
            blocking: true,
            missing: MissingPolicy::Fail,
            section: None,
            require_label: None,
            repro: None,
        },
    );
    cfg.sensors.insert(
        "coverage".to_string(),
        SensorPolicy {
            blocking: false,
            missing: MissingPolicy::Warn,
            section: None,
            require_label: None,
            repro: None,
        },
    );
    cfg.sensors.insert(
        "security".to_string(),
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
    let uc = IngestUseCase::new(receipts, policy, output, ExplodingValidator, noop_render);

    let result = uc.execute(default_request()).expect("execute");
    assert_eq!(result.exit_code, 2);
    insta::assert_json_snapshot!("mixed_verdicts_pass_warn_fail", result.report);
}

#[test]
fn snapshot_missing_sensor_with_warn_policy() {
    let receipts = StubReceipts::new(vec![]);
    let mut cfg = CockpitConfig::default();
    cfg.sensors.insert(
        "expected-sensor".to_string(),
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
    let uc = IngestUseCase::new(receipts, policy, output, ExplodingValidator, noop_render);

    let result = uc.execute(default_request()).expect("execute");
    insta::assert_json_snapshot!("missing_sensor_warn_policy", result.report);
}

#[test]
fn snapshot_invalid_json_receipt() {
    let mut receipts = StubReceipts::new(vec!["broken".into()]);
    receipts.reports.insert(
        "broken".to_string(),
        ReportRead::Bytes(b"{ bad json".to_vec()),
    );

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
    let uc = IngestUseCase::new(receipts, policy, output, ExplodingValidator, noop_render);

    let result = uc.execute(default_request()).expect("execute");
    assert_eq!(result.exit_code, 2);
    insta::assert_json_snapshot!("invalid_json_receipt", result.report);
}

#[test]
fn snapshot_oversized_receipt() {
    let mut receipts = StubReceipts::new(vec!["big-sensor".into()]);
    receipts.reports.insert(
        "big-sensor".to_string(),
        ReportRead::Oversized {
            size: 5_000_000,
            cap: 2_097_152,
        },
    );

    let mut cfg = CockpitConfig::default();
    cfg.sensors.insert(
        "big-sensor".to_string(),
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
    let uc = IngestUseCase::new(receipts, policy, output, ExplodingValidator, noop_render);

    let result = uc.execute(default_request()).expect("execute");
    assert_eq!(result.exit_code, 2);
    insta::assert_json_snapshot!("oversized_receipt", result.report);
}
