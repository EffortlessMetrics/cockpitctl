//! Substrate Bridge tests: prove the IngestUseCase works identically with
//! in-memory adapters vs. filesystem adapters.
//!
//! This validates the "Internal Speed, External Audit" model — library consumers
//! can wire in-memory adapters for speed while CI uses filesystem adapters for
//! artifact persistence.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use cockpitctl_core::ingest::{
    CommentRead, DiscoveredSensors, IngestRequest, IngestUseCase, NoOpSchemaValidator, OutputSink,
    PolicySource, ReceiptSource, ReportRead,
};
use cockpitctl_core::render::render_comment;
use cockpitctl_core::types::{CockpitConfig, RunInfo, ToolInfo};

// ─────────────────────────────────────────────────────────────────────────────
// In-memory adapters
// ─────────────────────────────────────────────────────────────────────────────

struct InMemoryReceiptSource {
    /// sensor_id → raw report.json bytes
    reports: BTreeMap<String, Vec<u8>>,
    /// sensor_id → comment path (if present)
    comments: BTreeMap<String, String>,
    /// Whether discovery was truncated
    truncated: bool,
    /// Total found (for truncation reporting)
    total_found: usize,
}

impl InMemoryReceiptSource {
    fn new(reports: BTreeMap<String, Vec<u8>>) -> Self {
        let total_found = reports.len();
        Self {
            reports,
            comments: BTreeMap::new(),
            truncated: false,
            total_found,
        }
    }

    fn with_truncation(mut self, truncated: bool, total_found: usize) -> Self {
        self.truncated = truncated;
        self.total_found = total_found;
        self
    }
}

impl ReceiptSource for InMemoryReceiptSource {
    fn discovered_sensors(&self) -> anyhow::Result<DiscoveredSensors> {
        Ok(DiscoveredSensors {
            sensors: self.reports.keys().cloned().collect(),
            truncated: self.truncated,
            total_found: self.total_found,
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

    fn comment_path_if_present(&self, sensor_id: &str) -> anyhow::Result<CommentRead> {
        match self.comments.get(sensor_id) {
            Some(path) => Ok(CommentRead::Present(path.clone())),
            None => Ok(CommentRead::Missing),
        }
    }
}

struct InMemoryPolicySource {
    config: Option<CockpitConfig>,
}

impl PolicySource for InMemoryPolicySource {
    fn load_config(&self) -> anyhow::Result<Option<CockpitConfig>> {
        Ok(self.config.clone())
    }
}

struct OutputSinkInner {
    report: Mutex<Option<String>>,
    comment: Mutex<Option<String>>,
}

#[derive(Clone)]
struct InMemoryOutputSink {
    inner: Arc<OutputSinkInner>,
}

impl InMemoryOutputSink {
    fn new() -> Self {
        Self {
            inner: Arc::new(OutputSinkInner {
                report: Mutex::new(None),
                comment: Mutex::new(None),
            }),
        }
    }

    fn take_report(&self) -> String {
        self.inner.report.lock().unwrap().take().unwrap()
    }
}

impl OutputSink for InMemoryOutputSink {
    fn write_cockpit_report(&self, json: &str) -> anyhow::Result<()> {
        *self.inner.report.lock().unwrap() = Some(json.to_string());
        Ok(())
    }

    fn write_cockpit_comment(&self, md: &str) -> anyhow::Result<()> {
        *self.inner.comment.lock().unwrap() = Some(md.to_string());
        Ok(())
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

fn workspace_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn tool_info() -> ToolInfo {
    ToolInfo {
        name: "cockpitctl".to_string(),
        version: "0.1.0".to_string(),
        commit: None,
    }
}

fn run_info() -> RunInfo {
    RunInfo {
        started_at: "2026-02-02T12:00:00Z".to_string(),
        ended_at: None,
        duration_ms: None,
        host: None,
        git: None,
        ci: None,
        capabilities: BTreeMap::new(),
    }
}

fn make_request() -> IngestRequest {
    IngestRequest {
        labels: Vec::new(),
        tool: tool_info(),
        run: run_info(),
        schema_validation_override: None,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

/// Compare in-memory ingest against golden file from happy_path fixture.
#[test]
fn in_memory_matches_golden_happy_path() {
    let fixture = workspace_root().join("fixtures/happy_path");
    let config_str = std::fs::read_to_string(fixture.join("cockpit.toml")).unwrap();
    let cfg: CockpitConfig = toml::from_str(&config_str).unwrap();

    let mut reports = BTreeMap::new();
    reports.insert(
        "builddiag".to_string(),
        std::fs::read(fixture.join("artifacts/builddiag/report.json")).unwrap(),
    );
    reports.insert(
        "diffguard".to_string(),
        std::fs::read(fixture.join("artifacts/diffguard/report.json")).unwrap(),
    );

    let receipts = InMemoryReceiptSource::new(reports);
    let policy = InMemoryPolicySource {
        config: Some(cfg.clone()),
    };
    let output = InMemoryOutputSink::new();

    let uc = IngestUseCase::new(receipts, policy, output, NoOpSchemaValidator, |r, c| {
        render_comment(r, c)
    });
    let result = uc.execute(make_request()).unwrap();
    assert_eq!(result.exit_code, 0);

    let expected = std::fs::read_to_string(fixture.join("expected/report.json")).unwrap();
    let got = result.report.clone();
    // Serialize to match golden format (pretty + trailing newline).
    let mut got_json = serde_json::to_string_pretty(&got).unwrap();
    got_json.push('\n');
    pretty_assertions::assert_eq!(got_json, expected, "in-memory report differs from golden");
}

/// Missing sensor produces ReportRead::Missing and appropriate synthesized finding.
#[test]
fn in_memory_missing_sensor() {
    let config_toml = r#"
[policy]
warn_is_fail = false

[sensors.builddiag]
blocking = true
missing = "fail"

[sensors.ghost]
blocking = true
missing = "fail"
"#;
    let cfg: CockpitConfig = toml::from_str(config_toml).unwrap();

    let mut reports = BTreeMap::new();
    reports.insert(
        "builddiag".to_string(),
        br#"{
            "schema": "builddiag.report.v1",
            "tool": {"name": "builddiag", "version": "1.0"},
            "run": {"started_at": "2026-02-02T11:59:00Z"},
            "verdict": {"status": "pass", "counts": {"info": 0, "warn": 0, "error": 0}, "reasons": []},
            "findings": []
        }"#
        .to_vec(),
    );
    // "ghost" is expected but not in reports → Missing

    let receipts = InMemoryReceiptSource::new(reports);
    let policy = InMemoryPolicySource { config: Some(cfg) };
    let output = InMemoryOutputSink::new();

    let uc = IngestUseCase::new(receipts, policy, output, NoOpSchemaValidator, |r, c| {
        render_comment(r, c)
    });
    let result = uc.execute(make_request()).unwrap();

    assert_eq!(result.exit_code, 2, "missing blocking sensor should exit 2");
    assert_eq!(
        result.report.verdict.status,
        cockpitctl_core::types::VerdictStatus::Fail
    );

    let has_missing = result
        .report
        .highlights
        .iter()
        .any(|h| h.finding.code == "cockpit.missing_receipt");
    assert!(
        has_missing,
        "should contain cockpit.missing_receipt highlight"
    );
}

/// Invalid JSON bytes produce synthesized invalid_receipt finding.
#[test]
fn in_memory_invalid_json() {
    let config_toml = r#"
[sensors.broken]
blocking = true
missing = "fail"
"#;
    let cfg: CockpitConfig = toml::from_str(config_toml).unwrap();

    let mut reports = BTreeMap::new();
    reports.insert("broken".to_string(), b"{ not valid json }".to_vec());

    let receipts = InMemoryReceiptSource::new(reports);
    let policy = InMemoryPolicySource { config: Some(cfg) };
    let output = InMemoryOutputSink::new();

    let uc = IngestUseCase::new(receipts, policy, output, NoOpSchemaValidator, |r, c| {
        render_comment(r, c)
    });
    let result = uc.execute(make_request()).unwrap();

    assert_eq!(result.exit_code, 2);
    let has_invalid = result
        .report
        .highlights
        .iter()
        .any(|h| h.finding.code == "cockpit.invalid_receipt");
    assert!(
        has_invalid,
        "should contain cockpit.invalid_receipt highlight"
    );
}

/// Skip verdict with capabilities is surfaced correctly.
#[test]
fn in_memory_skip_verdict() {
    let config_toml = r#"
[sensors.builddiag]
blocking = true
missing = "fail"

[sensors.coverage]
blocking = false
missing = "skip"
"#;
    let cfg: CockpitConfig = toml::from_str(config_toml).unwrap();

    let mut reports = BTreeMap::new();
    reports.insert(
        "builddiag".to_string(),
        br#"{
            "schema": "builddiag.report.v1",
            "tool": {"name": "builddiag", "version": "1.0"},
            "run": {"started_at": "2026-02-02T11:59:00Z"},
            "verdict": {"status": "pass", "counts": {"info": 0, "warn": 0, "error": 0}, "reasons": []},
            "findings": []
        }"#
        .to_vec(),
    );
    reports.insert(
        "coverage".to_string(),
        br#"{
            "schema": "coverage.report.v1",
            "tool": {"name": "coverage", "version": "1.0"},
            "run": {
                "started_at": "2026-02-02T11:59:05Z",
                "capabilities": {"baseline": {"status": "unavailable", "reason": "no_baseline"}}
            },
            "verdict": {"status": "skip", "counts": {"info": 0, "warn": 0, "error": 0}, "reasons": ["no_baseline"]},
            "findings": []
        }"#
        .to_vec(),
    );

    let receipts = InMemoryReceiptSource::new(reports);
    let policy = InMemoryPolicySource { config: Some(cfg) };
    let output = InMemoryOutputSink::new();

    let uc = IngestUseCase::new(receipts, policy, output, NoOpSchemaValidator, |r, c| {
        render_comment(r, c)
    });
    let result = uc.execute(make_request()).unwrap();

    assert_eq!(result.exit_code, 0, "skip on non-blocking should pass");

    let coverage = result
        .report
        .sensors
        .iter()
        .find(|s| s.id == "coverage")
        .expect("coverage sensor not found");
    assert_eq!(
        coverage.verdict.status,
        cockpitctl_core::types::VerdictStatus::Skip
    );
    assert!(
        result.report.highlights.is_empty(),
        "no highlights for pass+skip"
    );
}

/// Truncated discovery produces a sensors_truncated warning.
#[test]
fn in_memory_truncated_discovery() {
    let config_toml = r#"
[sensors.sensor_00]
blocking = false
missing = "skip"
"#;
    let cfg: CockpitConfig = toml::from_str(config_toml).unwrap();

    let mut reports = BTreeMap::new();
    reports.insert(
        "sensor_00".to_string(),
        br#"{
            "schema": "sensor.report.v1",
            "tool": {"name": "test", "version": "1.0"},
            "run": {"started_at": "2026-02-02T11:59:00Z"},
            "verdict": {"status": "pass", "counts": {"info": 0, "warn": 0, "error": 0}, "reasons": []},
            "findings": []
        }"#
        .to_vec(),
    );

    // Simulate truncation: 1 sensor processed out of 101 found.
    let receipts = InMemoryReceiptSource::new(reports).with_truncation(true, 101);
    let policy = InMemoryPolicySource { config: Some(cfg) };
    let output = InMemoryOutputSink::new();

    let uc = IngestUseCase::new(receipts, policy, output, NoOpSchemaValidator, |r, c| {
        render_comment(r, c)
    });
    let result = uc.execute(make_request()).unwrap();

    // Assert full highlight structure
    assert_eq!(
        result.report.highlights.len(),
        1,
        "should have exactly one highlight"
    );
    let hl = &result.report.highlights[0];
    assert_eq!(hl.finding.code, "cockpit.sensors_truncated");
    assert_eq!(
        hl.finding.severity,
        cockpitctl_core::types::Severity::Warn,
        "truncation is a warning, not a failure"
    );
    assert!(
        hl.finding.message.contains("1") && hl.finding.message.contains("101"),
        "message should mention counts (1 processed, 101 found): {}",
        hl.finding.message
    );

    // Truncation is a warning, not a failure
    assert_eq!(
        result.report.verdict.status,
        cockpitctl_core::types::VerdictStatus::Pass,
        "truncation warning should not cause failure"
    );
    assert_eq!(result.exit_code, 0, "truncation warning exits 0");
}

/// OutputSink captures both report and comment.
#[test]
fn in_memory_output_sink_captures() {
    let config_toml = r#"
[sensors.builddiag]
blocking = true
missing = "fail"
"#;
    let cfg: CockpitConfig = toml::from_str(config_toml).unwrap();

    let mut reports = BTreeMap::new();
    reports.insert(
        "builddiag".to_string(),
        br#"{
            "schema": "builddiag.report.v1",
            "tool": {"name": "builddiag", "version": "1.0"},
            "run": {"started_at": "2026-02-02T11:59:00Z"},
            "verdict": {"status": "pass", "counts": {"info": 0, "warn": 0, "error": 0}, "reasons": []},
            "findings": []
        }"#
        .to_vec(),
    );

    let receipts = InMemoryReceiptSource::new(reports);
    let policy = InMemoryPolicySource { config: Some(cfg) };
    let output = InMemoryOutputSink::new();

    let output_handle = output.clone();
    let uc = IngestUseCase::new(receipts, policy, output, NoOpSchemaValidator, |r, c| {
        render_comment(r, c)
    });
    let _result = uc.execute(make_request()).unwrap();

    let captured_report = output_handle.take_report();
    assert!(captured_report.contains("cockpit.report.v1"));
    assert!(captured_report.contains("builddiag"));
}
