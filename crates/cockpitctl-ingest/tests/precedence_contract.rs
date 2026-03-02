//! Precedence-contract tests for the ingest use case.
//!
//! Exercises config vs CLI overrides, missing policy semantics,
//! warn_is_fail, and label-gate interactions.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};

use cockpitctl_ingest::{
    CommentRead, DiscoveredSensors, IngestRequest, IngestUseCase, NoOpSchemaValidator, OutputSink,
    PlanRead, PolicySource, ReceiptSource, ReportRead, SchemaValidationResult, SchemaValidator,
};
use cockpitctl_types::{
    CockpitConfig, CockpitReport, MissingPolicy, Presence, RunInfo, SchemaValidation, SensorPolicy,
    SensorReport, ToolInfo, Verdict, VerdictCounts, VerdictStatus,
};

// =============================================================================
// Test doubles
// =============================================================================

struct StubReceipts {
    sensors: Vec<String>,
    reports: HashMap<String, ReportRead>,
}

impl StubReceipts {
    fn new(sensors: Vec<&str>, reports: HashMap<String, ReportRead>) -> Self {
        Self {
            sensors: sensors.into_iter().map(String::from).collect(),
            reports,
        }
    }

    fn empty() -> Self {
        Self {
            sensors: vec![],
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

struct StubPolicy {
    cfg: Option<CockpitConfig>,
}

impl PolicySource for StubPolicy {
    fn load_config(&self) -> anyhow::Result<Option<CockpitConfig>> {
        Ok(self.cfg.clone())
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

struct NeverCalledValidator;

impl SchemaValidator for NeverCalledValidator {
    fn validate_receipt(&self, _: &[u8]) -> anyhow::Result<SchemaValidationResult> {
        panic!("validator must not be called in this test");
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

fn make_policy_with_label(
    id: &str,
    blocking: bool,
    missing: MissingPolicy,
    label: &str,
) -> CockpitConfig {
    let mut cfg = CockpitConfig::default();
    cfg.sensors.insert(
        id.to_string(),
        SensorPolicy {
            blocking,
            missing,
            section: None,
            require_label: Some(label.to_string()),
            repro: None,
        },
    );
    cfg
}

// =============================================================================
// Tests: config defaults vs CLI overrides
// =============================================================================

#[test]
fn config_schema_validation_strict_is_used_when_no_cli_override() {
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
        NoOpSchemaValidator,
        stub_render,
    );
    // No override: should use config strict; validator is called (NoOp returns Valid)
    let result = uc.execute(default_request()).unwrap();
    assert_eq!(result.exit_code, 0);
}

#[test]
fn cli_override_lax_suppresses_config_strict() {
    let reports = HashMap::from([(
        "s".to_string(),
        ReportRead::Bytes(sensor_report_bytes(VerdictStatus::Pass)),
    )]);
    let receipts = StubReceipts::new(vec!["s"], reports);
    let mut cfg = make_policy(vec![("s", true, MissingPolicy::Fail)]);
    cfg.policy.schema_validation = SchemaValidation::Strict;
    // CLI overrides to lax, so NeverCalledValidator must not be invoked
    let uc = IngestUseCase::new(
        receipts,
        StubPolicy { cfg: Some(cfg) },
        CaptureSink::default(),
        NeverCalledValidator,
        stub_render,
    );
    let mut req = default_request();
    req.schema_validation_override = Some(SchemaValidation::Lax);
    let result = uc.execute(req).unwrap();
    assert_eq!(result.exit_code, 0, "lax override must skip validation");
}

#[test]
fn cli_override_strict_overrides_config_lax() {
    let reports = HashMap::from([(
        "s".to_string(),
        ReportRead::Bytes(sensor_report_bytes(VerdictStatus::Pass)),
    )]);
    let receipts = StubReceipts::new(vec!["s"], reports);
    let cfg = make_policy(vec![("s", true, MissingPolicy::Fail)]);
    // Config is lax (default), CLI overrides to strict; NoOp validator returns Valid
    let uc = IngestUseCase::new(
        receipts,
        StubPolicy { cfg: Some(cfg) },
        CaptureSink::default(),
        NoOpSchemaValidator,
        stub_render,
    );
    let mut req = default_request();
    req.schema_validation_override = Some(SchemaValidation::Strict);
    let result = uc.execute(req).unwrap();
    assert_eq!(result.exit_code, 0);
}

#[test]
fn no_config_file_uses_default_lax_schema_validation() {
    let reports = HashMap::from([(
        "s".to_string(),
        ReportRead::Bytes(sensor_report_bytes(VerdictStatus::Pass)),
    )]);
    let receipts = StubReceipts::new(vec!["s"], reports);
    // NeverCalledValidator panics if invoked; default is lax, so it must not be called
    let uc = IngestUseCase::new(
        receipts,
        StubPolicy { cfg: None },
        CaptureSink::default(),
        NeverCalledValidator,
        stub_render,
    );
    let result = uc.execute(default_request()).unwrap();
    assert_eq!(result.exit_code, 0, "default config should use lax mode");
}

// =============================================================================
// Tests: missing policy semantics
// =============================================================================

#[test]
fn missing_policy_fail_on_absent_receipt_yields_exit_2() {
    let receipts = StubReceipts::empty();
    let cfg = make_policy(vec![("required", true, MissingPolicy::Fail)]);
    let uc = IngestUseCase::new(
        receipts,
        StubPolicy { cfg: Some(cfg) },
        CaptureSink::default(),
        NoOpSchemaValidator,
        stub_render,
    );
    let result = uc.execute(default_request()).unwrap();
    assert_eq!(result.exit_code, 2);
    assert_eq!(result.report.sensors[0].presence, Presence::Missing);
}

#[test]
fn missing_policy_warn_on_absent_receipt_yields_exit_0() {
    let receipts = StubReceipts::empty();
    let cfg = make_policy(vec![("optional", true, MissingPolicy::Warn)]);
    let uc = IngestUseCase::new(
        receipts,
        StubPolicy { cfg: Some(cfg) },
        CaptureSink::default(),
        NoOpSchemaValidator,
        stub_render,
    );
    let result = uc.execute(default_request()).unwrap();
    assert_eq!(result.exit_code, 0);
    assert_eq!(result.report.sensors[0].presence, Presence::Missing);
    assert!(
        result
            .report
            .highlights
            .iter()
            .any(|h| h.finding.code == "cockpit.missing_receipt"),
        "warn policy should generate highlight"
    );
}

#[test]
fn missing_policy_skip_on_absent_receipt_yields_skip_verdict() {
    let receipts = StubReceipts::empty();
    let cfg = make_policy(vec![("skippable", true, MissingPolicy::Skip)]);
    let uc = IngestUseCase::new(
        receipts,
        StubPolicy { cfg: Some(cfg) },
        CaptureSink::default(),
        NoOpSchemaValidator,
        stub_render,
    );
    let result = uc.execute(default_request()).unwrap();
    assert_eq!(result.exit_code, 0);
    assert_eq!(result.report.sensors[0].verdict.status, VerdictStatus::Skip);
}

#[test]
fn missing_policy_fail_on_present_receipt_does_not_fail() {
    let reports = HashMap::from([(
        "present".to_string(),
        ReportRead::Bytes(sensor_report_bytes(VerdictStatus::Pass)),
    )]);
    let receipts = StubReceipts::new(vec!["present"], reports);
    let cfg = make_policy(vec![("present", true, MissingPolicy::Fail)]);
    let uc = IngestUseCase::new(
        receipts,
        StubPolicy { cfg: Some(cfg) },
        CaptureSink::default(),
        NoOpSchemaValidator,
        stub_render,
    );
    let result = uc.execute(default_request()).unwrap();
    assert_eq!(result.exit_code, 0);
    assert_eq!(result.report.sensors[0].presence, Presence::Present);
}

// =============================================================================
// Tests: warn_is_fail
// =============================================================================

#[test]
fn warn_is_fail_false_allows_warn_verdict_to_pass() {
    let reports = HashMap::from([(
        "w".to_string(),
        ReportRead::Bytes(sensor_report_bytes(VerdictStatus::Warn)),
    )]);
    let receipts = StubReceipts::new(vec!["w"], reports);
    let mut cfg = make_policy(vec![("w", true, MissingPolicy::Fail)]);
    cfg.policy.warn_is_fail = false;
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
        "warn_is_fail=false: warn is acceptable"
    );
}

#[test]
fn warn_is_fail_true_escalates_warn_to_exit_2() {
    let reports = HashMap::from([(
        "w".to_string(),
        ReportRead::Bytes(sensor_report_bytes(VerdictStatus::Warn)),
    )]);
    let receipts = StubReceipts::new(vec!["w"], reports);
    let mut cfg = make_policy(vec![("w", true, MissingPolicy::Fail)]);
    cfg.policy.warn_is_fail = true;
    let uc = IngestUseCase::new(
        receipts,
        StubPolicy { cfg: Some(cfg) },
        CaptureSink::default(),
        NoOpSchemaValidator,
        stub_render,
    );
    let result = uc.execute(default_request()).unwrap();
    assert_eq!(result.exit_code, 2, "warn_is_fail=true: warn escalates");
    assert_eq!(result.report.verdict.status, VerdictStatus::Fail);
}

#[test]
fn warn_is_fail_true_does_not_affect_pass_verdict() {
    let reports = HashMap::from([(
        "p".to_string(),
        ReportRead::Bytes(sensor_report_bytes(VerdictStatus::Pass)),
    )]);
    let receipts = StubReceipts::new(vec!["p"], reports);
    let mut cfg = make_policy(vec![("p", true, MissingPolicy::Fail)]);
    cfg.policy.warn_is_fail = true;
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
        "pass verdict unaffected by warn_is_fail"
    );
}

#[test]
fn warn_is_fail_with_non_blocking_warn_stays_exit_0() {
    let reports = HashMap::from([(
        "w".to_string(),
        ReportRead::Bytes(sensor_report_bytes(VerdictStatus::Warn)),
    )]);
    let receipts = StubReceipts::new(vec!["w"], reports);
    let mut cfg = make_policy(vec![("w", false, MissingPolicy::Fail)]);
    cfg.policy.warn_is_fail = true;
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
        "non-blocking sensor should not escalate even with warn_is_fail"
    );
}

// =============================================================================
// Tests: label-gate interactions
// =============================================================================

#[test]
fn label_gate_missing_label_skips_sensor() {
    let receipts = StubReceipts::empty();
    let cfg = make_policy_with_label("gated", true, MissingPolicy::Fail, "security");
    let uc = IngestUseCase::new(
        receipts,
        StubPolicy { cfg: Some(cfg) },
        CaptureSink::default(),
        NoOpSchemaValidator,
        stub_render,
    );
    let result = uc.execute(default_request()).unwrap();
    assert_eq!(result.exit_code, 0);
    assert_eq!(result.report.sensors[0].verdict.status, VerdictStatus::Skip);
}

#[test]
fn label_gate_present_label_evaluates_sensor() {
    let reports = HashMap::from([(
        "gated".to_string(),
        ReportRead::Bytes(sensor_report_bytes(VerdictStatus::Pass)),
    )]);
    let receipts = StubReceipts::new(vec!["gated"], reports);
    let cfg = make_policy_with_label("gated", true, MissingPolicy::Fail, "security");
    let uc = IngestUseCase::new(
        receipts,
        StubPolicy { cfg: Some(cfg) },
        CaptureSink::default(),
        NoOpSchemaValidator,
        stub_render,
    );
    let mut req = default_request();
    req.labels.push("security".to_string());
    let result = uc.execute(req).unwrap();
    assert_eq!(result.exit_code, 0);
    assert_eq!(result.report.sensors[0].presence, Presence::Present);
}

#[test]
fn label_gate_present_label_missing_receipt_applies_missing_policy() {
    let receipts = StubReceipts::empty();
    let cfg = make_policy_with_label("gated", true, MissingPolicy::Fail, "security");
    let uc = IngestUseCase::new(
        receipts,
        StubPolicy { cfg: Some(cfg) },
        CaptureSink::default(),
        NoOpSchemaValidator,
        stub_render,
    );
    let mut req = default_request();
    req.labels.push("security".to_string());
    let result = uc.execute(req).unwrap();
    assert_eq!(
        result.exit_code, 2,
        "missing receipt with label present should fail"
    );
    assert_eq!(result.report.sensors[0].presence, Presence::Missing);
}

#[test]
fn label_gate_multiple_sensors_mixed_labels() {
    let reports = HashMap::from([(
        "always".to_string(),
        ReportRead::Bytes(sensor_report_bytes(VerdictStatus::Pass)),
    )]);
    let receipts = StubReceipts::new(vec!["always"], reports);
    let mut cfg = CockpitConfig::default();
    cfg.sensors.insert(
        "always".to_string(),
        SensorPolicy {
            blocking: true,
            missing: MissingPolicy::Fail,
            section: None,
            require_label: None,
            repro: None,
        },
    );
    cfg.sensors.insert(
        "gated".to_string(),
        SensorPolicy {
            blocking: true,
            missing: MissingPolicy::Fail,
            section: None,
            require_label: Some("deploy".to_string()),
            repro: None,
        },
    );
    let uc = IngestUseCase::new(
        receipts,
        StubPolicy { cfg: Some(cfg) },
        CaptureSink::default(),
        NoOpSchemaValidator,
        stub_render,
    );
    // "deploy" label not present: gated sensor skipped, "always" passes
    let result = uc.execute(default_request()).unwrap();
    assert_eq!(result.exit_code, 0);
    let always_sensor = result
        .report
        .sensors
        .iter()
        .find(|s| s.id == "always")
        .unwrap();
    assert_eq!(always_sensor.presence, Presence::Present);
    let gated_sensor = result
        .report
        .sensors
        .iter()
        .find(|s| s.id == "gated")
        .unwrap();
    assert_eq!(gated_sensor.verdict.status, VerdictStatus::Skip);
}

// =============================================================================
// Tests: additional precedence edge cases
// =============================================================================

#[test]
fn discovered_sensors_used_when_config_has_no_sensors() {
    let reports = HashMap::from([
        (
            "disc-a".to_string(),
            ReportRead::Bytes(sensor_report_bytes(VerdictStatus::Pass)),
        ),
        (
            "disc-b".to_string(),
            ReportRead::Bytes(sensor_report_bytes(VerdictStatus::Pass)),
        ),
    ]);
    let receipts = StubReceipts::new(vec!["disc-a", "disc-b"], reports);
    // Config exists but has no sensors declared
    let cfg = CockpitConfig::default();
    let uc = IngestUseCase::new(
        receipts,
        StubPolicy { cfg: Some(cfg) },
        CaptureSink::default(),
        NoOpSchemaValidator,
        stub_render,
    );
    let result = uc.execute(default_request()).unwrap();
    assert_eq!(result.report.sensors.len(), 2);
    assert_eq!(result.exit_code, 0);
}

#[test]
fn configured_sensors_override_discovered_sensors() {
    // Discovery finds "discovered", but config expects "expected"
    let receipts = StubReceipts::new(vec!["discovered"], HashMap::new());
    let cfg = make_policy(vec![("expected", true, MissingPolicy::Warn)]);
    let uc = IngestUseCase::new(
        receipts,
        StubPolicy { cfg: Some(cfg) },
        CaptureSink::default(),
        NoOpSchemaValidator,
        stub_render,
    );
    let result = uc.execute(default_request()).unwrap();
    let ids: Vec<&str> = result.report.sensors.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(ids, vec!["expected"]);
}
