//! Determinism integration tests for the ingest use case.
//!
//! These tests prove that the full ingest pipeline produces identical output
//! regardless of internal iteration order, and that repeated executions are
//! byte-identical.

use std::collections::BTreeMap;
use std::collections::HashMap;

use cockpitctl_ingest::{
    CommentRead, DiscoveredSensors, IngestRequest, IngestUseCase, NoOpSchemaValidator, OutputSink,
    PolicySource, ReceiptSource, ReportRead,
};
use cockpitctl_types::{
    CockpitConfig, Finding, MissingPolicy, RunInfo, SensorPolicy, SensorReport, Severity, ToolInfo,
    Verdict, VerdictCounts, VerdictStatus,
};

// ---------------------------------------------------------------------------
// Test doubles
// ---------------------------------------------------------------------------

struct StubReceipts {
    sensors: Vec<String>,
    reports: HashMap<String, Vec<u8>>,
}

impl ReceiptSource for StubReceipts {
    fn discovered_sensors(&self) -> anyhow::Result<DiscoveredSensors> {
        Ok(DiscoveredSensors {
            sensors: self.sensors.clone(),
            truncated: false,
            total_found: self.sensors.len(),
            invalid_sensor_ids: Vec::new(),
        })
    }

    fn read_report_bytes(&self, sensor_id: &str) -> anyhow::Result<ReportRead> {
        match self.reports.get(sensor_id) {
            Some(bytes) => Ok(ReportRead::Bytes(bytes.clone())),
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

fn make_sensor_report(
    status: VerdictStatus,
    counts: VerdictCounts,
    findings: Vec<Finding>,
) -> Vec<u8> {
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

fn three_sensor_config() -> CockpitConfig {
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
        "lint".to_string(),
        SensorPolicy {
            blocking: true,
            missing: MissingPolicy::Fail,
            section: Some("Quality".to_string()),
            require_label: None,
            repro: None,
        },
    );
    cfg.sensors.insert(
        "coverage".to_string(),
        SensorPolicy {
            blocking: false,
            missing: MissingPolicy::Warn,
            section: Some("Quality".to_string()),
            require_label: None,
            repro: None,
        },
    );
    cfg.policy.section_order = vec!["Build".to_string(), "Quality".to_string()];
    cfg
}

fn three_sensor_reports() -> HashMap<String, Vec<u8>> {
    let mut reports = HashMap::new();
    reports.insert(
        "build".to_string(),
        make_sensor_report(VerdictStatus::Pass, VerdictCounts::default(), vec![]),
    );
    reports.insert(
        "lint".to_string(),
        make_sensor_report(
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
                    code: "lint/complexity".to_string(),
                    message: "Function too complex".to_string(),
                    location: Some(cockpitctl_types::Location {
                        path: Some("src/main.rs".to_string()),
                        line: Some(42),
                        col: None,
                    }),
                    help: None,
                    url: None,
                    fingerprint: None,
                    data: None,
                },
                Finding {
                    severity: Severity::Warn,
                    check_id: None,
                    code: "lint/unused".to_string(),
                    message: "Unused import".to_string(),
                    location: Some(cockpitctl_types::Location {
                        path: Some("src/lib.rs".to_string()),
                        line: Some(1),
                        col: None,
                    }),
                    help: None,
                    url: None,
                    fingerprint: None,
                    data: None,
                },
            ],
        ),
    );
    reports.insert(
        "coverage".to_string(),
        make_sensor_report(
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
                code: "cov/report".to_string(),
                message: "Coverage: 85%".to_string(),
                location: None,
                help: None,
                url: None,
                fingerprint: None,
                data: None,
            }],
        ),
    );
    reports
}

// ---------------------------------------------------------------------------
// Test: identical input → identical CockpitReport JSON
// ---------------------------------------------------------------------------

#[test]
fn determinism_ingest_identical_input_identical_output() {
    let cfg = three_sensor_config();
    let reports = three_sensor_reports();

    // Run ingest twice with identical inputs.
    let mut results = Vec::new();
    for _ in 0..2 {
        let receipts = StubReceipts {
            sensors: vec!["build".into(), "coverage".into(), "lint".into()],
            reports: reports.clone(),
        };
        let policy = StubPolicy {
            cfg: Some(cfg.clone()),
        };
        let output = CaptureOutput::default();
        let uc = IngestUseCase::new(receipts, policy, output, NoOpSchemaValidator, noop_render);
        let result = uc.execute(default_request()).expect("execute");
        let json = serde_json::to_string_pretty(&result.report).unwrap();
        results.push(json);
    }

    assert_eq!(
        results[0], results[1],
        "Two identical ingest runs must produce byte-identical report JSON"
    );

    // Snapshot the canonical output.
    let report: serde_json::Value = serde_json::from_str(&results[0]).unwrap();
    insta::assert_json_snapshot!("determinism_ingest_identical_output", report);
}

// ---------------------------------------------------------------------------
// Test: sensor discovery order doesn't affect output
// ---------------------------------------------------------------------------

#[test]
fn determinism_sensor_discovery_order_irrelevant() {
    let cfg = three_sensor_config();
    let reports = three_sensor_reports();

    // Discovery in different orders should produce identical results.
    let orderings: Vec<Vec<String>> = vec![
        vec!["build".into(), "coverage".into(), "lint".into()],
        vec!["lint".into(), "build".into(), "coverage".into()],
        vec!["coverage".into(), "lint".into(), "build".into()],
    ];

    let mut jsons = Vec::new();
    for order in &orderings {
        let receipts = StubReceipts {
            sensors: order.clone(),
            reports: reports.clone(),
        };
        let policy = StubPolicy {
            cfg: Some(cfg.clone()),
        };
        let output = CaptureOutput::default();
        let uc = IngestUseCase::new(receipts, policy, output, NoOpSchemaValidator, noop_render);
        let result = uc.execute(default_request()).expect("execute");
        let json = serde_json::to_string_pretty(&result.report).unwrap();
        jsons.push(json);
    }

    // All three orderings must produce the same JSON.
    assert_eq!(
        jsons[0], jsons[1],
        "Sensor discovery order must not affect output (ordering 0 vs 1)"
    );
    assert_eq!(
        jsons[0], jsons[2],
        "Sensor discovery order must not affect output (ordering 0 vs 2)"
    );
}

// ---------------------------------------------------------------------------
// Test: highlights/findings order stable regardless of receipt processing order
// ---------------------------------------------------------------------------

#[test]
fn determinism_highlight_order_stable_across_receipt_orders() {
    let mut cfg = CockpitConfig::default();
    cfg.sensors.insert(
        "sensor_a".to_string(),
        SensorPolicy {
            blocking: true,
            missing: MissingPolicy::Fail,
            section: None,
            require_label: None,
            repro: None,
        },
    );
    cfg.sensors.insert(
        "sensor_b".to_string(),
        SensorPolicy {
            blocking: true,
            missing: MissingPolicy::Fail,
            section: None,
            require_label: None,
            repro: None,
        },
    );

    let mut reports = HashMap::new();
    reports.insert(
        "sensor_a".to_string(),
        make_sensor_report(
            VerdictStatus::Fail,
            VerdictCounts {
                info: 0,
                warn: 0,
                error: 2,
                suppressed: 0,
            },
            vec![
                Finding {
                    severity: Severity::Error,
                    check_id: None,
                    code: "err/a1".to_string(),
                    message: "Error A1".to_string(),
                    location: Some(cockpitctl_types::Location {
                        path: Some("src/x.rs".to_string()),
                        line: Some(10),
                        col: None,
                    }),
                    help: None,
                    url: None,
                    fingerprint: None,
                    data: None,
                },
                Finding {
                    severity: Severity::Error,
                    check_id: None,
                    code: "err/a2".to_string(),
                    message: "Error A2".to_string(),
                    location: Some(cockpitctl_types::Location {
                        path: Some("src/y.rs".to_string()),
                        line: Some(5),
                        col: None,
                    }),
                    help: None,
                    url: None,
                    fingerprint: None,
                    data: None,
                },
            ],
        ),
    );
    reports.insert(
        "sensor_b".to_string(),
        make_sensor_report(
            VerdictStatus::Fail,
            VerdictCounts {
                info: 0,
                warn: 1,
                error: 1,
                suppressed: 0,
            },
            vec![
                Finding {
                    severity: Severity::Error,
                    check_id: None,
                    code: "err/b1".to_string(),
                    message: "Error B1".to_string(),
                    location: Some(cockpitctl_types::Location {
                        path: Some("src/a.rs".to_string()),
                        line: Some(1),
                        col: None,
                    }),
                    help: None,
                    url: None,
                    fingerprint: None,
                    data: None,
                },
                Finding {
                    severity: Severity::Warn,
                    check_id: None,
                    code: "warn/b1".to_string(),
                    message: "Warning B1".to_string(),
                    location: Some(cockpitctl_types::Location {
                        path: Some("src/z.rs".to_string()),
                        line: Some(99),
                        col: None,
                    }),
                    help: None,
                    url: None,
                    fingerprint: None,
                    data: None,
                },
            ],
        ),
    );

    // Process in two different discovery orders.
    let orderings: Vec<Vec<String>> = vec![
        vec!["sensor_a".into(), "sensor_b".into()],
        vec!["sensor_b".into(), "sensor_a".into()],
    ];

    let mut jsons = Vec::new();
    for order in &orderings {
        let receipts = StubReceipts {
            sensors: order.clone(),
            reports: reports.clone(),
        };
        let policy = StubPolicy {
            cfg: Some(cfg.clone()),
        };
        let output = CaptureOutput::default();
        let uc = IngestUseCase::new(receipts, policy, output, NoOpSchemaValidator, noop_render);
        let result = uc.execute(default_request()).expect("execute");
        let json = serde_json::to_string_pretty(&result.report).unwrap();
        jsons.push(json);
    }

    assert_eq!(
        jsons[0], jsons[1],
        "Highlight and finding order must be stable regardless of receipt processing order"
    );

    let report: serde_json::Value = serde_json::from_str(&jsons[0]).unwrap();
    insta::assert_json_snapshot!("determinism_highlights_stable_order", report);
}
