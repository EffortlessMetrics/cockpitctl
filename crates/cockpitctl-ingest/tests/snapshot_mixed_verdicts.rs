//! Snapshot tests for ingest pipeline with mixed verdicts and edge cases.

use std::collections::{BTreeMap, HashMap};

use cockpitctl_ingest::{
    CommentRead, DiscoveredSensors, IngestRequest, IngestUseCase, OutputSink, PolicySource,
    ReceiptSource, ReportRead, SchemaValidationResult, SchemaValidator,
};
use cockpitctl_types::{
    CockpitConfig, Finding, Location, MissingPolicy, RunInfo, SensorPolicy, SensorReport, Severity,
    ToolInfo, Verdict, VerdictCounts, VerdictStatus,
};

// ---------------------------------------------------------------------------
// Test doubles
// ---------------------------------------------------------------------------

struct StubReceipts {
    sensors: Vec<String>,
    truncated: bool,
    total_found: usize,
    invalid_sensor_ids: Vec<String>,
    reports: HashMap<String, ReportRead>,
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

struct NoOpValidator;

impl SchemaValidator for NoOpValidator {
    fn validate_receipt(&self, _bytes: &[u8]) -> anyhow::Result<SchemaValidationResult> {
        Ok(SchemaValidationResult::Valid)
    }
}

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

fn make_finding(code: &str, severity: Severity, path: &str, line: u32) -> Finding {
    Finding {
        severity,
        check_id: None,
        code: code.to_string(),
        message: format!("Message for {}", code),
        location: Some(Location {
            path: Some(path.to_string()),
            line: Some(line),
            col: None,
        }),
        help: None,
        url: None,
        fingerprint: None,
        data: None,
    }
}

// ---------------------------------------------------------------------------
// Full pipeline: mixed verdicts (pass + warn + fail + skip)
// ---------------------------------------------------------------------------

#[test]
fn snapshot_pipeline_all_four_verdicts() {
    let mut receipts = StubReceipts::new(vec![
        "build".into(),
        "coverage".into(),
        "lint".into(),
        "optional".into(),
    ]);

    receipts.reports.insert(
        "build".to_string(),
        ReportRead::Bytes(report_bytes(
            VerdictStatus::Pass,
            VerdictCounts::default(),
            vec![],
        )),
    );

    receipts.reports.insert(
        "coverage".to_string(),
        ReportRead::Bytes(report_bytes(
            VerdictStatus::Warn,
            VerdictCounts {
                info: 0,
                warn: 1,
                error: 0,
                suppressed: 0,
            },
            vec![make_finding("cov/low", Severity::Warn, "src/lib.rs", 1)],
        )),
    );

    receipts.reports.insert(
        "lint".to_string(),
        ReportRead::Bytes(report_bytes(
            VerdictStatus::Fail,
            VerdictCounts {
                info: 0,
                warn: 0,
                error: 2,
                suppressed: 0,
            },
            vec![
                make_finding("lint/err1", Severity::Error, "src/main.rs", 10),
                make_finding("lint/err2", Severity::Error, "src/main.rs", 20),
            ],
        )),
    );

    receipts.reports.insert(
        "optional".to_string(),
        ReportRead::Bytes(report_bytes(
            VerdictStatus::Skip,
            VerdictCounts::default(),
            vec![],
        )),
    );

    let mut cfg = CockpitConfig::default();
    cfg.sensors.insert(
        "build".to_string(),
        SensorPolicy {
            blocking: true,
            missing: MissingPolicy::Fail,
            section: Some("Build".to_string()),
            require_label: None,
            repro: None,
        },
    );
    cfg.sensors.insert(
        "coverage".to_string(),
        SensorPolicy {
            blocking: false,
            missing: MissingPolicy::Warn,
            section: Some("Coverage".to_string()),
            require_label: None,
            repro: None,
        },
    );
    cfg.sensors.insert(
        "lint".to_string(),
        SensorPolicy {
            blocking: true,
            missing: MissingPolicy::Fail,
            section: Some("Lint".to_string()),
            require_label: None,
            repro: None,
        },
    );
    cfg.sensors.insert(
        "optional".to_string(),
        SensorPolicy {
            blocking: false,
            missing: MissingPolicy::Skip,
            section: Some("Optional".to_string()),
            require_label: None,
            repro: None,
        },
    );
    cfg.policy.section_order = vec![
        "Build".into(),
        "Lint".into(),
        "Coverage".into(),
        "Optional".into(),
    ];

    let policy = StubPolicy { cfg: Some(cfg) };
    let output = CaptureOutput::default();
    let uc = IngestUseCase::new(receipts, policy, output, NoOpValidator, noop_render);

    let result = uc.execute(default_request()).expect("execute");
    assert_eq!(result.exit_code, 2);
    insta::assert_json_snapshot!("pipeline_all_four_verdicts", result.report);
}

// ---------------------------------------------------------------------------
// Pipeline with truncated sensor discovery
// ---------------------------------------------------------------------------

#[test]
fn snapshot_pipeline_truncated_discovery() {
    let mut receipts = StubReceipts::new(vec!["alpha".into(), "beta".into()]);
    receipts.truncated = true;
    receipts.total_found = 150;

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
    let uc = IngestUseCase::new(receipts, policy, output, NoOpValidator, noop_render);

    let result = uc.execute(default_request()).expect("execute");
    insta::assert_json_snapshot!("pipeline_truncated_discovery", result.report);
}

// ---------------------------------------------------------------------------
// Pipeline with invalid sensor ID (path traversal)
// ---------------------------------------------------------------------------

#[test]
fn snapshot_pipeline_invalid_sensor_ids() {
    let mut receipts = StubReceipts::new(vec!["good-sensor".into()]);
    receipts.invalid_sensor_ids = vec!["../escape".to_string(), "../../root".to_string()];

    receipts.reports.insert(
        "good-sensor".to_string(),
        ReportRead::Bytes(report_bytes(
            VerdictStatus::Pass,
            VerdictCounts::default(),
            vec![],
        )),
    );

    let policy = StubPolicy { cfg: None };
    let output = CaptureOutput::default();
    let uc = IngestUseCase::new(receipts, policy, output, NoOpValidator, noop_render);

    let result = uc.execute(default_request()).expect("execute");
    insta::assert_json_snapshot!("pipeline_invalid_sensor_ids", result.report);
}
