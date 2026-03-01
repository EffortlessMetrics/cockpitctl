//! Insta snapshot tests for edge-case verdict combinations.
//!
//! Covers: all verdict mix, zero findings, one finding per severity, all skipped.

use std::collections::{BTreeMap, HashMap};

use cockpitctl_ingest::{
    CommentRead, DiscoveredSensors, IngestRequest, IngestUseCase, OutputSink, PolicySource,
    ReceiptSource, ReportRead, SchemaValidationResult, SchemaValidator,
};
use cockpitctl_types::{
    CockpitConfig, CockpitReport, Finding, Location, MissingPolicy, RunInfo, SensorPolicy,
    SensorReport, Severity, ToolInfo, Verdict, VerdictCounts, VerdictStatus,
};

// =============================================================================
// Test doubles
// =============================================================================

struct StubReceipts {
    sensors: Vec<String>,
    reports: HashMap<String, ReportRead>,
}

impl StubReceipts {
    fn new(sensors: Vec<String>) -> Self {
        Self {
            sensors,
            reports: HashMap::new(),
        }
    }
}

impl ReceiptSource for StubReceipts {
    fn discovered_sensors(&self) -> anyhow::Result<DiscoveredSensors> {
        Ok(DiscoveredSensors {
            sensors: self.sensors.clone(),
            truncated: false,
            total_found: self.sensors.len(),
            invalid_sensor_ids: vec![],
        })
    }

    fn read_report_bytes(&self, sensor_id: &str) -> anyhow::Result<ReportRead> {
        match self.reports.get(sensor_id) {
            Some(ReportRead::Bytes(bytes)) => Ok(ReportRead::Bytes(bytes.clone())),
            _ => Ok(ReportRead::Missing),
        }
    }

    fn report_path(&self, sensor_id: &str) -> String {
        format!("artifacts/{}/report.json", sensor_id)
    }

    fn comment_path_if_present(&self, _sensor_id: &str) -> anyhow::Result<CommentRead> {
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
        panic!("schema validator should not be called");
    }
}

// =============================================================================
// Helpers
// =============================================================================

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

fn noop_render(_report: &CockpitReport, _cfg: &CockpitConfig) -> String {
    "COMMENT".to_string()
}

fn default_sensor_policy(blocking: bool) -> SensorPolicy {
    SensorPolicy {
        blocking,
        missing: MissingPolicy::Fail,
        section: None,
        require_label: None,
        repro: None,
    }
}

// =============================================================================
// Snapshot: all four verdicts (pass/warn/fail/skip mix)
// =============================================================================

#[test]
fn snapshot_all_four_verdicts_mixed() {
    let sensors = vec![
        "alpha-pass".to_string(),
        "beta-warn".to_string(),
        "gamma-fail".to_string(),
        "delta-skip".to_string(),
    ];
    let mut receipts = StubReceipts::new(sensors.clone());

    receipts.reports.insert(
        "alpha-pass".to_string(),
        ReportRead::Bytes(report_bytes(
            VerdictStatus::Pass,
            VerdictCounts {
                info: 2,
                ..Default::default()
            },
            vec![
                Finding {
                    severity: Severity::Info,
                    check_id: None,
                    code: "info/note-1".to_string(),
                    message: "All clear".to_string(),
                    location: None,
                    help: None,
                    url: None,
                    fingerprint: None,
                    data: None,
                },
                Finding {
                    severity: Severity::Info,
                    check_id: None,
                    code: "info/note-2".to_string(),
                    message: "Everything fine".to_string(),
                    location: None,
                    help: None,
                    url: None,
                    fingerprint: None,
                    data: None,
                },
            ],
        )),
    );

    receipts.reports.insert(
        "beta-warn".to_string(),
        ReportRead::Bytes(report_bytes(
            VerdictStatus::Warn,
            VerdictCounts {
                warn: 1,
                ..Default::default()
            },
            vec![Finding {
                severity: Severity::Warn,
                check_id: None,
                code: "warn/coverage".to_string(),
                message: "Coverage below 80%".to_string(),
                location: None,
                help: None,
                url: None,
                fingerprint: None,
                data: None,
            }],
        )),
    );

    receipts.reports.insert(
        "gamma-fail".to_string(),
        ReportRead::Bytes(report_bytes(
            VerdictStatus::Fail,
            VerdictCounts {
                error: 1,
                ..Default::default()
            },
            vec![Finding {
                severity: Severity::Error,
                check_id: Some("SEC-001".to_string()),
                code: "sec/vuln".to_string(),
                message: "Critical vulnerability".to_string(),
                location: Some(Location {
                    path: Some("lib/auth.rs".to_string()),
                    line: Some(42),
                    col: None,
                }),
                help: Some("Upgrade dependency".to_string()),
                url: Some("https://example.com/cve".to_string()),
                fingerprint: None,
                data: None,
            }],
        )),
    );

    receipts.reports.insert(
        "delta-skip".to_string(),
        ReportRead::Bytes(report_bytes(
            VerdictStatus::Skip,
            VerdictCounts::default(),
            vec![],
        )),
    );

    let mut cfg = CockpitConfig::default();
    cfg.sensors
        .insert("alpha-pass".to_string(), default_sensor_policy(false));
    cfg.sensors.insert(
        "beta-warn".to_string(),
        SensorPolicy {
            blocking: false,
            missing: MissingPolicy::Warn,
            ..Default::default()
        },
    );
    cfg.sensors
        .insert("gamma-fail".to_string(), default_sensor_policy(true));
    cfg.sensors.insert(
        "delta-skip".to_string(),
        SensorPolicy {
            blocking: false,
            missing: MissingPolicy::Skip,
            ..Default::default()
        },
    );

    let policy = StubPolicy { cfg: Some(cfg) };
    let output = CaptureOutput::default();
    let uc = IngestUseCase::new(receipts, policy, output, ExplodingValidator, noop_render);
    let result = uc.execute(default_request()).expect("execute");

    assert_eq!(result.exit_code, 2);
    insta::assert_json_snapshot!("all_four_verdicts_mixed", result.report);
}

// =============================================================================
// Snapshot: zero findings across all sensors
// =============================================================================

#[test]
fn snapshot_zero_findings_all_pass() {
    let sensors = vec!["build".to_string(), "lint".to_string(), "test".to_string()];
    let mut receipts = StubReceipts::new(sensors.clone());

    for s in &sensors {
        receipts.reports.insert(
            s.clone(),
            ReportRead::Bytes(report_bytes(
                VerdictStatus::Pass,
                VerdictCounts::default(),
                vec![],
            )),
        );
    }

    let mut cfg = CockpitConfig::default();
    for s in &sensors {
        cfg.sensors.insert(s.clone(), default_sensor_policy(true));
    }

    let policy = StubPolicy { cfg: Some(cfg) };
    let output = CaptureOutput::default();
    let uc = IngestUseCase::new(receipts, policy, output, ExplodingValidator, noop_render);
    let result = uc.execute(default_request()).expect("execute");

    assert_eq!(result.exit_code, 0);
    assert!(result.report.highlights.is_empty());
    insta::assert_json_snapshot!("zero_findings_all_pass", result.report);
}

// =============================================================================
// Snapshot: exactly one finding per severity level
// =============================================================================

#[test]
fn snapshot_one_finding_per_severity() {
    let mut receipts = StubReceipts::new(vec!["multi-severity".into()]);
    receipts.reports.insert(
        "multi-severity".to_string(),
        ReportRead::Bytes(report_bytes(
            VerdictStatus::Fail,
            VerdictCounts {
                info: 1,
                warn: 1,
                error: 1,
                suppressed: 0,
            },
            vec![
                Finding {
                    severity: Severity::Error,
                    check_id: Some("E001".to_string()),
                    code: "err/critical".to_string(),
                    message: "Critical error found".to_string(),
                    location: Some(Location {
                        path: Some("src/main.rs".to_string()),
                        line: Some(10),
                        col: Some(5),
                    }),
                    help: Some("Fix the critical error".to_string()),
                    url: None,
                    fingerprint: Some("fp-err-001".to_string()),
                    data: None,
                },
                Finding {
                    severity: Severity::Warn,
                    check_id: Some("W001".to_string()),
                    code: "warn/deprecated".to_string(),
                    message: "Deprecated function used".to_string(),
                    location: Some(Location {
                        path: Some("src/lib.rs".to_string()),
                        line: Some(25),
                        col: None,
                    }),
                    help: None,
                    url: Some("https://docs.example.com/deprecated".to_string()),
                    fingerprint: None,
                    data: None,
                },
                Finding {
                    severity: Severity::Info,
                    check_id: None,
                    code: "info/metric".to_string(),
                    message: "Code coverage: 87%".to_string(),
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
    cfg.sensors
        .insert("multi-severity".to_string(), default_sensor_policy(true));

    let policy = StubPolicy { cfg: Some(cfg) };
    let output = CaptureOutput::default();
    let uc = IngestUseCase::new(receipts, policy, output, ExplodingValidator, noop_render);
    let result = uc.execute(default_request()).expect("execute");

    assert_eq!(result.exit_code, 2);
    insta::assert_json_snapshot!("one_finding_per_severity", result.report);
}

// =============================================================================
// Snapshot: all sensors skipped
// =============================================================================

#[test]
fn snapshot_all_sensors_skipped() {
    let sensors = vec![
        "sensor-a".to_string(),
        "sensor-b".to_string(),
        "sensor-c".to_string(),
    ];
    let mut receipts = StubReceipts::new(sensors.clone());

    for s in &sensors {
        receipts.reports.insert(
            s.clone(),
            ReportRead::Bytes(report_bytes(
                VerdictStatus::Skip,
                VerdictCounts::default(),
                vec![],
            )),
        );
    }

    let mut cfg = CockpitConfig::default();
    for s in &sensors {
        cfg.sensors.insert(
            s.clone(),
            SensorPolicy {
                blocking: false,
                missing: MissingPolicy::Skip,
                ..Default::default()
            },
        );
    }

    let policy = StubPolicy { cfg: Some(cfg) };
    let output = CaptureOutput::default();
    let uc = IngestUseCase::new(receipts, policy, output, ExplodingValidator, noop_render);
    let result = uc.execute(default_request()).expect("execute");

    assert_eq!(result.exit_code, 0);
    for sensor in &result.report.sensors {
        assert_eq!(sensor.verdict.status, VerdictStatus::Skip);
    }
    insta::assert_json_snapshot!("all_sensors_skipped", result.report);
}
