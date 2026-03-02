//! Golden tests that verify the ingest pipeline produces byte-identical
//! `cockpit.report.v1` JSON regardless of sensor discovery order.
//!
//! The canonical order is always lexical by sensor_id. These tests prove that
//! shuffled, reversed, and interleaved discovery orders converge to the same
//! JSON output.

use std::collections::{BTreeMap, HashMap};

use cockpitctl_ingest::{
    CommentRead, DiscoveredSensors, IngestRequest, IngestUseCase, NoOpSchemaValidator, OutputSink,
    PolicySource, ReceiptSource, ReportRead,
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

fn finding(code: &str, severity: Severity, path: &str, line: u32) -> Finding {
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
// Five-sensor config with findings across multiple sensors
// ---------------------------------------------------------------------------

fn five_sensor_config() -> CockpitConfig {
    let mut cfg = CockpitConfig::default();
    let sensors = [
        ("alpha", true, MissingPolicy::Fail, "Build"),
        ("beta", true, MissingPolicy::Fail, "Lint"),
        ("gamma", true, MissingPolicy::Fail, "Tests"),
        ("delta", false, MissingPolicy::Warn, "Coverage"),
        ("epsilon", false, MissingPolicy::Skip, "Optional"),
    ];
    for (id, blocking, missing, section) in &sensors {
        cfg.sensors.insert(
            id.to_string(),
            SensorPolicy {
                blocking: *blocking,
                missing: *missing,
                section: Some(section.to_string()),
                require_label: None,
                repro: None,
            },
        );
    }
    cfg.policy.section_order = vec![
        "Build".into(),
        "Lint".into(),
        "Tests".into(),
        "Coverage".into(),
        "Optional".into(),
    ];
    cfg.policy.max_highlights = 10;
    cfg
}

fn five_sensor_reports() -> HashMap<String, Vec<u8>> {
    let mut reports = HashMap::new();
    reports.insert(
        "alpha".to_string(),
        make_sensor_report(VerdictStatus::Pass, VerdictCounts::default(), vec![]),
    );
    reports.insert(
        "beta".to_string(),
        make_sensor_report(
            VerdictStatus::Warn,
            VerdictCounts {
                info: 0,
                warn: 2,
                error: 0,
                suppressed: 0,
            },
            vec![
                finding("lint/complexity", Severity::Warn, "src/main.rs", 42),
                finding("lint/unused", Severity::Warn, "src/lib.rs", 1),
            ],
        ),
    );
    reports.insert(
        "gamma".to_string(),
        make_sensor_report(
            VerdictStatus::Fail,
            VerdictCounts {
                info: 0,
                warn: 0,
                error: 1,
                suppressed: 0,
            },
            vec![finding(
                "test/failure",
                Severity::Error,
                "tests/integration.rs",
                88,
            )],
        ),
    );
    reports.insert(
        "delta".to_string(),
        make_sensor_report(
            VerdictStatus::Pass,
            VerdictCounts {
                info: 1,
                warn: 0,
                error: 0,
                suppressed: 0,
            },
            vec![finding("cov/report", Severity::Info, "src/lib.rs", 1)],
        ),
    );
    reports.insert(
        "epsilon".to_string(),
        make_sensor_report(VerdictStatus::Pass, VerdictCounts::default(), vec![]),
    );
    reports
}

fn run_ingest(sensor_order: Vec<String>) -> String {
    let cfg = five_sensor_config();
    let reports = five_sensor_reports();
    let receipts = StubReceipts {
        sensors: sensor_order,
        reports,
    };
    let policy = StubPolicy {
        cfg: Some(cfg.clone()),
    };
    let output = CaptureOutput::default();
    let uc = IngestUseCase::new(receipts, policy, output, NoOpSchemaValidator, noop_render);
    let result = uc.execute(default_request()).expect("execute");
    serde_json::to_string_pretty(&result.report).unwrap()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Canonical (lexical) discovery order produces a golden snapshot.
#[test]
fn golden_report_five_sensors_canonical_order() {
    let json = run_ingest(vec![
        "alpha".into(),
        "beta".into(),
        "delta".into(),
        "epsilon".into(),
        "gamma".into(),
    ]);
    let report: serde_json::Value = serde_json::from_str(&json).unwrap();
    insta::assert_json_snapshot!("golden_report_five_sensors_canonical", report);
}

/// Reverse discovery order must produce the same JSON as canonical order.
#[test]
fn golden_report_reverse_order_matches_canonical() {
    let canonical = run_ingest(vec![
        "alpha".into(),
        "beta".into(),
        "delta".into(),
        "epsilon".into(),
        "gamma".into(),
    ]);
    let reversed = run_ingest(vec![
        "gamma".into(),
        "epsilon".into(),
        "delta".into(),
        "beta".into(),
        "alpha".into(),
    ]);
    assert_eq!(
        canonical, reversed,
        "Reverse discovery order must produce identical JSON"
    );
}

/// Interleaved discovery order must produce the same JSON.
#[test]
fn golden_report_interleaved_order_matches_canonical() {
    let canonical = run_ingest(vec![
        "alpha".into(),
        "beta".into(),
        "delta".into(),
        "epsilon".into(),
        "gamma".into(),
    ]);
    let interleaved = run_ingest(vec![
        "gamma".into(),
        "alpha".into(),
        "epsilon".into(),
        "beta".into(),
        "delta".into(),
    ]);
    assert_eq!(
        canonical, interleaved,
        "Interleaved discovery order must produce identical JSON"
    );
}

/// All 6 permutations of 3 sensors produce identical JSON.
#[test]
fn golden_report_all_permutations_three_sensors() {
    // Use a 3-sensor subset for exhaustive permutation testing
    let mut cfg = CockpitConfig::default();
    let sensors_def = [
        ("s1", true, MissingPolicy::Fail, "A"),
        ("s2", true, MissingPolicy::Fail, "B"),
        ("s3", false, MissingPolicy::Warn, "C"),
    ];
    for (id, blocking, missing, section) in &sensors_def {
        cfg.sensors.insert(
            id.to_string(),
            SensorPolicy {
                blocking: *blocking,
                missing: *missing,
                section: Some(section.to_string()),
                require_label: None,
                repro: None,
            },
        );
    }
    cfg.policy.section_order = vec!["A".into(), "B".into(), "C".into()];

    let mut reports = HashMap::new();
    reports.insert(
        "s1".to_string(),
        make_sensor_report(
            VerdictStatus::Pass,
            VerdictCounts::default(),
            vec![finding("s1/ok", Severity::Info, "src/a.rs", 1)],
        ),
    );
    reports.insert(
        "s2".to_string(),
        make_sensor_report(
            VerdictStatus::Fail,
            VerdictCounts {
                info: 0,
                warn: 0,
                error: 1,
                suppressed: 0,
            },
            vec![finding("s2/err", Severity::Error, "src/b.rs", 10)],
        ),
    );
    reports.insert(
        "s3".to_string(),
        make_sensor_report(
            VerdictStatus::Warn,
            VerdictCounts {
                info: 0,
                warn: 1,
                error: 0,
                suppressed: 0,
            },
            vec![finding("s3/warn", Severity::Warn, "src/c.rs", 5)],
        ),
    );

    let permutations: Vec<Vec<String>> = vec![
        vec!["s1".into(), "s2".into(), "s3".into()],
        vec!["s1".into(), "s3".into(), "s2".into()],
        vec!["s2".into(), "s1".into(), "s3".into()],
        vec!["s2".into(), "s3".into(), "s1".into()],
        vec!["s3".into(), "s1".into(), "s2".into()],
        vec!["s3".into(), "s2".into(), "s1".into()],
    ];

    let mut jsons = Vec::new();
    for order in &permutations {
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
        jsons.push(serde_json::to_string_pretty(&result.report).unwrap());
    }

    // All 6 permutations must be identical.
    for (i, json) in jsons.iter().enumerate().skip(1) {
        assert_eq!(
            jsons[0], *json,
            "Permutation {} differs from canonical order",
            i
        );
    }

    // Snapshot the canonical output.
    let report: serde_json::Value = serde_json::from_str(&jsons[0]).unwrap();
    insta::assert_json_snapshot!("golden_report_all_permutations_three_sensors", report);
}
