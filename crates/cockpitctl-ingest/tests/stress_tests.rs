//! Stress tests for cockpitctl-ingest: sensor caps, large assemblies, edge cases.

use std::collections::BTreeMap;

use anyhow::Result;
use cockpitctl_ingest::{
    CommentRead, DiscoveredSensors, IngestRequest, IngestUseCase, NoOpSchemaValidator, OutputSink,
    PlanRead, PolicySource, ReceiptSource, ReportRead,
};
use cockpitctl_types::{
    CockpitConfig, CockpitReport, Finding, MissingPolicy, RunInfo, SensorPolicy, SensorReport,
    Severity, ToolInfo, Verdict, VerdictStatus,
};

// ---------------------------------------------------------------------------
// Stub implementations
// ---------------------------------------------------------------------------

struct StubReceiptSource {
    sensors: Vec<String>,
    truncated: bool,
    total_found: usize,
    invalid_sensor_ids: Vec<String>,
    reports: std::collections::HashMap<String, ReportRead>,
}

impl StubReceiptSource {
    fn with_sensors_and_reports(
        sensors: Vec<String>,
        reports: std::collections::HashMap<String, ReportRead>,
    ) -> Self {
        let total = sensors.len();
        Self {
            sensors,
            truncated: false,
            total_found: total,
            invalid_sensor_ids: vec![],
            reports,
        }
    }

    fn with_truncation(
        sensors: Vec<String>,
        reports: std::collections::HashMap<String, ReportRead>,
        total_found: usize,
    ) -> Self {
        Self {
            truncated: true,
            total_found,
            sensors,
            invalid_sensor_ids: vec![],
            reports,
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

struct StubPolicySource {
    config: Option<CockpitConfig>,
}

impl PolicySource for StubPolicySource {
    fn load_config(&self) -> Result<Option<CockpitConfig>> {
        Ok(self.config.clone())
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

fn make_sensor_report(status: VerdictStatus, findings: Vec<Finding>) -> Vec<u8> {
    let counts = cockpitctl_domain::compute_counts(&findings);
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
            counts,
            reasons: vec![],
        },
        findings,
        artifacts: vec![],
        data: None,
    };
    serde_json::to_vec(&report).unwrap()
}

fn make_finding(severity: Severity, code: &str, message: &str) -> Finding {
    Finding {
        severity,
        check_id: None,
        code: code.to_string(),
        message: message.to_string(),
        location: None,
        help: None,
        url: None,
        fingerprint: None,
        data: None,
    }
}

// ---------------------------------------------------------------------------
// 1. 100 sensors: all processed correctly
// ---------------------------------------------------------------------------

#[test]
fn stress_100_sensors_all_processed() {
    let sensor_ids: Vec<String> = (0..100).map(|i| format!("sensor-{:03}", i)).collect();
    let mut reports = std::collections::HashMap::new();
    for id in &sensor_ids {
        reports.insert(
            id.clone(),
            ReportRead::Bytes(make_sensor_report(VerdictStatus::Pass, vec![])),
        );
    }

    let receipts = StubReceiptSource::with_sensors_and_reports(sensor_ids, reports);
    let policy = StubPolicySource { config: None };
    let output = StubOutputSink::new();
    let (tool, run) = make_tool_and_run();

    let uc = IngestUseCase::new(receipts, policy, output, NoOpSchemaValidator, stub_render);
    let result = uc
        .execute(IngestRequest {
            labels: vec![],
            tool,
            run,
            schema_validation_override: None,
        })
        .unwrap();

    assert_eq!(result.report.sensors.len(), 100);
    assert_eq!(result.exit_code, 0);
}

// ---------------------------------------------------------------------------
// 2. 101 sensors with truncation flag: graceful cap enforcement
// ---------------------------------------------------------------------------

#[test]
fn stress_101_sensors_truncation_signaled() {
    // Simulate discovery returning 100 (capped) with truncated=true, total_found=101.
    let sensor_ids: Vec<String> = (0..100).map(|i| format!("sensor-{:03}", i)).collect();
    let mut reports = std::collections::HashMap::new();
    for id in &sensor_ids {
        reports.insert(
            id.clone(),
            ReportRead::Bytes(make_sensor_report(VerdictStatus::Pass, vec![])),
        );
    }

    let receipts = StubReceiptSource::with_truncation(sensor_ids, reports, 101);
    let policy = StubPolicySource { config: None };
    let output = StubOutputSink::new();
    let (tool, run) = make_tool_and_run();

    let uc = IngestUseCase::new(receipts, policy, output, NoOpSchemaValidator, stub_render);
    let result = uc
        .execute(IngestRequest {
            labels: vec![],
            tool,
            run,
            schema_validation_override: None,
        })
        .unwrap();

    assert_eq!(result.report.sensors.len(), 100);
    // A truncation highlight should be present.
    assert!(
        result
            .report
            .highlights
            .iter()
            .any(|h| h.finding.code.contains("sensors_truncated")),
        "should contain sensors_truncated highlight"
    );
}

// ---------------------------------------------------------------------------
// 3. Large report assembly: 50 sensors × 100 findings each → valid report
// ---------------------------------------------------------------------------

#[test]
fn stress_large_report_assembly() {
    let sensor_ids: Vec<String> = (0..50).map(|i| format!("sensor-{:03}", i)).collect();
    let mut reports = std::collections::HashMap::new();
    for (idx, id) in sensor_ids.iter().enumerate() {
        let findings: Vec<Finding> = (0..100)
            .map(|j| {
                make_finding(
                    Severity::Warn,
                    &format!("W{}", j),
                    &format!("warn {} from sensor {}", j, idx),
                )
            })
            .collect();
        reports.insert(
            id.clone(),
            ReportRead::Bytes(make_sensor_report(VerdictStatus::Warn, findings)),
        );
    }

    let receipts = StubReceiptSource::with_sensors_and_reports(sensor_ids, reports);
    let policy = StubPolicySource { config: None };
    let output = StubOutputSink::new();
    let (tool, run) = make_tool_and_run();

    let uc = IngestUseCase::new(receipts, policy, output, NoOpSchemaValidator, stub_render);
    let result = uc
        .execute(IngestRequest {
            labels: vec![],
            tool,
            run,
            schema_validation_override: None,
        })
        .unwrap();

    assert_eq!(result.report.sensors.len(), 50);
    // Highlights are capped at max_highlights (default 7).
    assert!(result.report.highlights.len() <= 7);
    // Each sensor should be truncated since 100 > max_per_sensor_findings (20).
    for s in &result.report.sensors {
        assert!(
            s.truncated,
            "sensor {} should have truncated findings",
            s.id
        );
    }
}

// ---------------------------------------------------------------------------
// 4. Empty/whitespace sensor names: rejection
// ---------------------------------------------------------------------------

#[test]
fn stress_empty_sensor_names_rejected() {
    let invalid_names = vec![
        "".to_string(),       // empty
        " ".to_string(),      // whitespace
        "..".to_string(),     // path traversal
        "../etc".to_string(), // path traversal
    ];

    // Declare them in policy so they are expected (and evaluated).
    let mut cfg = CockpitConfig::default();
    for name in &invalid_names {
        cfg.sensors.insert(
            name.clone(),
            SensorPolicy {
                blocking: true,
                missing: MissingPolicy::Fail,
                ..Default::default()
            },
        );
    }

    let receipts =
        StubReceiptSource::with_sensors_and_reports(vec![], std::collections::HashMap::new());
    let policy = StubPolicySource { config: Some(cfg) };
    let output = StubOutputSink::new();
    let (tool, run) = make_tool_and_run();

    let uc = IngestUseCase::new(receipts, policy, output, NoOpSchemaValidator, stub_render);
    let result = uc
        .execute(IngestRequest {
            labels: vec![],
            tool,
            run,
            schema_validation_override: None,
        })
        .unwrap();

    // Invalid sensor IDs produce path_traversal or missing findings, not panics.
    assert!(!result.report.sensors.is_empty());
    assert_eq!(
        result.exit_code, 2,
        "blocking sensors with invalid IDs should cause policy fail"
    );
}

// ---------------------------------------------------------------------------
// 5. Duplicate sensors: same sensor_id twice → defined behavior
// ---------------------------------------------------------------------------

#[test]
fn stress_duplicate_sensor_ids() {
    let bytes = make_sensor_report(VerdictStatus::Pass, vec![]);
    let mut reports = std::collections::HashMap::new();
    reports.insert("dup-sensor".to_string(), ReportRead::Bytes(bytes));

    // Discovery returns duplicates.
    let sensors = vec!["dup-sensor".to_string(), "dup-sensor".to_string()];
    let receipts = StubReceiptSource::with_sensors_and_reports(sensors, reports);
    let policy = StubPolicySource { config: None };
    let output = StubOutputSink::new();
    let (tool, run) = make_tool_and_run();

    let uc = IngestUseCase::new(receipts, policy, output, NoOpSchemaValidator, stub_render);
    let result = uc
        .execute(IngestRequest {
            labels: vec![],
            tool,
            run,
            schema_validation_override: None,
        })
        .unwrap();

    // Both duplicate entries should produce sensor summaries (no panic).
    assert_eq!(result.report.sensors.len(), 2);
    assert_eq!(result.exit_code, 0);
}

// ---------------------------------------------------------------------------
// 6. Mixed oversized/missing/valid at scale
// ---------------------------------------------------------------------------

#[test]
fn stress_mixed_report_reads() {
    let sensor_ids: Vec<String> = (0..30).map(|i| format!("sensor-{:03}", i)).collect();
    let mut reports = std::collections::HashMap::new();
    for (i, id) in sensor_ids.iter().enumerate() {
        let read = match i % 3 {
            0 => ReportRead::Bytes(make_sensor_report(VerdictStatus::Pass, vec![])),
            1 => ReportRead::Missing,
            _ => ReportRead::Oversized {
                size: 3_000_000,
                cap: 2_097_152,
            },
        };
        reports.insert(id.clone(), read);
    }

    let receipts = StubReceiptSource::with_sensors_and_reports(sensor_ids, reports);
    let policy = StubPolicySource { config: None };
    let output = StubOutputSink::new();
    let (tool, run) = make_tool_and_run();

    let uc = IngestUseCase::new(receipts, policy, output, NoOpSchemaValidator, stub_render);
    let result = uc
        .execute(IngestRequest {
            labels: vec![],
            tool,
            run,
            schema_validation_override: None,
        })
        .unwrap();

    assert_eq!(result.report.sensors.len(), 30);
}
