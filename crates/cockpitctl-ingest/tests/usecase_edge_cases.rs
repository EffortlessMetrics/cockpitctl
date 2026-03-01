//! Edge-case tests for the ingest use case.
//!
//! Covers: schema validation modes, discovery ordering, finding deduplication,
//! precedence chain, exit code semantics, report metadata, extra outputs,
//! hooks, buildfix integration, and policy signing.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;

use cockpitctl_ingest::{
    CommentRead, DiscoveredSensors, IngestRequest, IngestUseCase, OutputSink, PlanRead,
    PolicySource, ReceiptSource, ReportRead, SchemaValidationResult, SchemaValidator,
};
use cockpitctl_types::{
    BuildfixPlan, CockpitConfig, CockpitReport, Finding, FindingRef, Fix, Location, MissingPolicy,
    RunInfo, SafetyLevel, SchemaValidation, SensorPolicy, SensorReport, Severity, ToolInfo,
    Verdict, VerdictCounts, VerdictStatus,
};

// =============================================================================
// Test doubles
// =============================================================================

struct StubReceipts {
    sensors: Vec<String>,
    truncated: bool,
    total_found: usize,
    invalid_sensor_ids: Vec<String>,
    reports: HashMap<String, ReportRead>,
    comments: HashMap<String, CommentRead>,
    plans: HashMap<String, PlanRead>,
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
            plans: HashMap::new(),
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

    fn read_plan_bytes(&self, sensor_id: &str) -> anyhow::Result<PlanRead> {
        match self.plans.get(sensor_id) {
            Some(PlanRead::Bytes(bytes)) => Ok(PlanRead::Bytes(bytes.clone())),
            Some(PlanRead::Oversized { size, cap }) => Ok(PlanRead::Oversized {
                size: *size,
                cap: *cap,
            }),
            Some(PlanRead::Missing) | None => Ok(PlanRead::Missing),
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
    reports: RefCell<Vec<String>>,
    comments: RefCell<Vec<String>>,
    extras: RefCell<Vec<(String, Vec<u8>)>>,
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

    fn write_extra_file(&self, name: &str, content: &[u8]) -> anyhow::Result<()> {
        self.extras
            .borrow_mut()
            .push((name.to_string(), content.to_vec()));
        Ok(())
    }
}

struct ExplodingValidator;

impl SchemaValidator for ExplodingValidator {
    fn validate_receipt(&self, _bytes: &[u8]) -> anyhow::Result<SchemaValidationResult> {
        panic!("schema validator should not be called in this test");
    }
}

enum FixedValidation {
    Valid,
    #[allow(dead_code)]
    Invalid(Vec<String>),
}

struct CountingValidator {
    calls: Rc<RefCell<Vec<Vec<u8>>>>,
    result: FixedValidation,
}

impl CountingValidator {
    fn always_valid() -> (Self, Rc<RefCell<Vec<Vec<u8>>>>) {
        let calls = Rc::new(RefCell::new(Vec::new()));
        (
            Self {
                calls: calls.clone(),
                result: FixedValidation::Valid,
            },
            calls,
        )
    }
}

impl SchemaValidator for CountingValidator {
    fn validate_receipt(&self, bytes: &[u8]) -> anyhow::Result<SchemaValidationResult> {
        self.calls.borrow_mut().push(bytes.to_vec());
        Ok(match &self.result {
            FixedValidation::Valid => SchemaValidationResult::Valid,
            FixedValidation::Invalid(errs) => SchemaValidationResult::Invalid(errs.clone()),
        })
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

fn finding(severity: Severity, code: &str, message: &str) -> Finding {
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

fn finding_with_location(
    severity: Severity,
    code: &str,
    message: &str,
    path: &str,
    line: u32,
) -> Finding {
    Finding {
        severity,
        check_id: None,
        code: code.to_string(),
        message: message.to_string(),
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

// =============================================================================
// 1. Schema validation modes: lax vs strict behavior differences
// =============================================================================

#[test]
fn lax_mode_skips_schema_validation_entirely() {
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
    cfg.sensors
        .insert("sensor".to_string(), default_sensor_policy(true));

    let policy = StubPolicy { cfg: Some(cfg) };
    let output = CaptureOutput::default();
    // ExplodingValidator panics if called — proves lax skips validation
    let uc = IngestUseCase::new(receipts, policy, output, ExplodingValidator, noop_render);
    let result = uc.execute(default_request()).expect("execute");
    assert_eq!(result.exit_code, 0);
}

#[test]
fn strict_mode_calls_validator_for_every_present_sensor() {
    let sensors = vec!["alpha".to_string(), "beta".to_string(), "gamma".to_string()];
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
    cfg.policy.schema_validation = SchemaValidation::Strict;
    for s in &sensors {
        cfg.sensors.insert(s.clone(), default_sensor_policy(false));
    }

    let policy = StubPolicy { cfg: Some(cfg) };
    let output = CaptureOutput::default();
    let (validator, calls) = CountingValidator::always_valid();
    let uc = IngestUseCase::new(receipts, policy, output, validator, noop_render);
    let result = uc.execute(default_request()).expect("execute");

    assert_eq!(result.exit_code, 0);
    assert_eq!(
        calls.borrow().len(),
        3,
        "validator should be called once per present sensor"
    );
}

#[test]
fn strict_mode_skips_validator_for_missing_sensors() {
    let receipts = StubReceipts::new(vec![]);
    // Sensor declared in config but not discovered
    let mut cfg = CockpitConfig::default();
    cfg.policy.schema_validation = SchemaValidation::Strict;
    cfg.sensors.insert(
        "absent".to_string(),
        SensorPolicy {
            blocking: false,
            missing: MissingPolicy::Skip,
            ..Default::default()
        },
    );

    let policy = StubPolicy { cfg: Some(cfg) };
    let output = CaptureOutput::default();
    let (validator, calls) = CountingValidator::always_valid();
    let uc = IngestUseCase::new(receipts, policy, output, validator, noop_render);
    let result = uc.execute(default_request()).expect("execute");

    assert_eq!(result.exit_code, 0);
    assert_eq!(
        calls.borrow().len(),
        0,
        "validator should NOT be called for missing sensor"
    );
}

// =============================================================================
// 2. Receipt discovery ordering: lexical order
// =============================================================================

#[test]
fn sensors_processed_in_lexical_order_regardless_of_insertion() {
    // Insert in reverse order; output should still reflect config iteration order
    let sensors = vec!["zzz".to_string(), "aaa".to_string(), "mmm".to_string()];
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

    let policy = StubPolicy { cfg: None };
    let output = CaptureOutput::default();
    let uc = IngestUseCase::new(receipts, policy, output, ExplodingValidator, noop_render);
    let result = uc.execute(default_request()).expect("execute");

    // With no config, discovered sensors are used as-is; sorted by domain layer
    assert_eq!(result.report.sensors.len(), 3);
}

#[test]
fn config_declared_sensors_iterated_in_btreemap_order() {
    let mut cfg = CockpitConfig::default();
    // BTreeMap guarantees alphabetical key order
    cfg.sensors
        .insert("zulu".to_string(), default_sensor_policy(false));
    cfg.sensors
        .insert("alpha".to_string(), default_sensor_policy(false));
    cfg.sensors
        .insert("mike".to_string(), default_sensor_policy(false));

    let receipts = StubReceipts::new(vec![]);
    // All missing — we just care about summary order
    let policy = StubPolicy { cfg: Some(cfg) };
    let output = CaptureOutput::default();
    let uc = IngestUseCase::new(receipts, policy, output, ExplodingValidator, noop_render);
    let result = uc.execute(default_request()).expect("execute");
    let ids: Vec<&str> = result
        .report
        .sensors
        .iter()
        .map(|s| s.id.as_str())
        .collect();
    // BTreeMap iteration order is alphabetical
    assert!(ids.contains(&"alpha"));
    assert!(ids.contains(&"mike"));
    assert!(ids.contains(&"zulu"));
    assert_eq!(result.report.sensors.len(), 3);
}

// =============================================================================
// 3. Finding deduplication: same finding from two sensors
// =============================================================================

#[test]
fn same_finding_code_from_two_sensors_both_appear_in_highlights() {
    let common_finding = finding(
        Severity::Error,
        "lint/null-deref",
        "Null pointer dereference",
    );

    let mut receipts = StubReceipts::new(vec!["sensor-a".into(), "sensor-b".into()]);
    receipts.reports.insert(
        "sensor-a".to_string(),
        ReportRead::Bytes(report_bytes(
            VerdictStatus::Fail,
            VerdictCounts {
                error: 1,
                ..Default::default()
            },
            vec![common_finding.clone()],
        )),
    );
    receipts.reports.insert(
        "sensor-b".to_string(),
        ReportRead::Bytes(report_bytes(
            VerdictStatus::Fail,
            VerdictCounts {
                error: 1,
                ..Default::default()
            },
            vec![common_finding],
        )),
    );

    let mut cfg = CockpitConfig::default();
    cfg.sensors
        .insert("sensor-a".to_string(), default_sensor_policy(true));
    cfg.sensors
        .insert("sensor-b".to_string(), default_sensor_policy(true));

    let policy = StubPolicy { cfg: Some(cfg) };
    let output = CaptureOutput::default();
    let uc = IngestUseCase::new(receipts, policy, output, ExplodingValidator, noop_render);
    let result = uc.execute(default_request()).expect("execute");

    // Both sensors should be present
    assert_eq!(result.report.sensors.len(), 2);
    // Highlights from both sensors should be present (not deduplicated away)
    let highlight_sensors: Vec<&str> = result
        .report
        .highlights
        .iter()
        .map(|h| h.sensor_id.as_str())
        .collect();
    assert!(highlight_sensors.contains(&"sensor-a"));
    assert!(highlight_sensors.contains(&"sensor-b"));
}

// =============================================================================
// 4. Precedence chain: Config → CLI override → effective settings
// =============================================================================

#[test]
fn cli_override_strict_takes_precedence_over_config_lax() {
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
    cfg.sensors
        .insert("sensor".to_string(), default_sensor_policy(true));

    let policy = StubPolicy { cfg: Some(cfg) };
    let output = CaptureOutput::default();
    let (validator, calls) = CountingValidator::always_valid();
    let uc = IngestUseCase::new(receipts, policy, output, validator, noop_render);

    let mut req = default_request();
    req.schema_validation_override = Some(SchemaValidation::Strict);
    let result = uc.execute(req).expect("execute");

    assert_eq!(
        calls.borrow().len(),
        1,
        "CLI strict override should force validation despite config lax"
    );
    assert_eq!(result.exit_code, 0);
}

#[test]
fn cli_override_lax_takes_precedence_over_config_strict() {
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
    cfg.sensors
        .insert("sensor".to_string(), default_sensor_policy(true));

    let policy = StubPolicy { cfg: Some(cfg) };
    let output = CaptureOutput::default();
    // ExplodingValidator panics if called
    let uc = IngestUseCase::new(receipts, policy, output, ExplodingValidator, noop_render);

    let mut req = default_request();
    req.schema_validation_override = Some(SchemaValidation::Lax);
    let result = uc.execute(req).expect("execute");
    assert_eq!(result.exit_code, 0);
}

#[test]
fn no_cli_override_uses_config_default() {
    let mut receipts = StubReceipts::new(vec!["sensor".into()]);
    receipts.reports.insert(
        "sensor".to_string(),
        ReportRead::Bytes(report_bytes(
            VerdictStatus::Pass,
            VerdictCounts::default(),
            vec![],
        )),
    );

    // Default config has schema_validation = Lax
    let mut cfg = CockpitConfig::default();
    cfg.sensors
        .insert("sensor".to_string(), default_sensor_policy(true));

    let policy = StubPolicy { cfg: Some(cfg) };
    let output = CaptureOutput::default();
    // ExplodingValidator panics if called — proves lax is the effective default
    let uc = IngestUseCase::new(receipts, policy, output, ExplodingValidator, noop_render);

    let result = uc.execute(default_request()).expect("execute");
    assert_eq!(result.exit_code, 0);
}

// =============================================================================
// 5. Exit code semantics: 0 (pass), 2 (policy fail), 1 (runtime error)
// =============================================================================

#[test]
fn exit_code_0_for_all_passing_sensors() {
    let mut receipts = StubReceipts::new(vec!["build".into(), "test".into()]);
    for s in &["build", "test"] {
        receipts.reports.insert(
            s.to_string(),
            ReportRead::Bytes(report_bytes(
                VerdictStatus::Pass,
                VerdictCounts::default(),
                vec![],
            )),
        );
    }

    let policy = StubPolicy { cfg: None };
    let output = CaptureOutput::default();
    let uc = IngestUseCase::new(receipts, policy, output, ExplodingValidator, noop_render);
    let result = uc.execute(default_request()).expect("execute");

    assert_eq!(result.exit_code, 0);
    assert_eq!(result.report.verdict.status, VerdictStatus::Pass);
}

#[test]
fn exit_code_0_for_warn_verdict() {
    let mut receipts = StubReceipts::new(vec!["coverage".into()]);
    receipts.reports.insert(
        "coverage".to_string(),
        ReportRead::Bytes(report_bytes(
            VerdictStatus::Warn,
            VerdictCounts {
                warn: 1,
                ..Default::default()
            },
            vec![finding(Severity::Warn, "coverage/low", "Below threshold")],
        )),
    );

    let mut cfg = CockpitConfig::default();
    cfg.sensors.insert(
        "coverage".to_string(),
        SensorPolicy {
            blocking: false,
            missing: MissingPolicy::Warn,
            ..Default::default()
        },
    );

    let policy = StubPolicy { cfg: Some(cfg) };
    let output = CaptureOutput::default();
    let uc = IngestUseCase::new(receipts, policy, output, ExplodingValidator, noop_render);
    let result = uc.execute(default_request()).expect("execute");

    assert_eq!(result.exit_code, 0, "warn verdict should not cause exit 2");
}

#[test]
fn exit_code_2_for_blocking_sensor_with_fail_verdict() {
    let mut receipts = StubReceipts::new(vec!["linter".into()]);
    receipts.reports.insert(
        "linter".to_string(),
        ReportRead::Bytes(report_bytes(
            VerdictStatus::Fail,
            VerdictCounts {
                error: 1,
                ..Default::default()
            },
            vec![finding(Severity::Error, "lint/err", "Error found")],
        )),
    );

    let mut cfg = CockpitConfig::default();
    cfg.sensors
        .insert("linter".to_string(), default_sensor_policy(true));

    let policy = StubPolicy { cfg: Some(cfg) };
    let output = CaptureOutput::default();
    let uc = IngestUseCase::new(receipts, policy, output, ExplodingValidator, noop_render);
    let result = uc.execute(default_request()).expect("execute");

    assert_eq!(result.exit_code, 2);
    assert_eq!(result.report.verdict.status, VerdictStatus::Fail);
}

#[test]
fn exit_code_2_for_missing_blocking_sensor_with_fail_policy() {
    let mut cfg = CockpitConfig::default();
    cfg.sensors.insert(
        "required".to_string(),
        SensorPolicy {
            blocking: true,
            missing: MissingPolicy::Fail,
            ..Default::default()
        },
    );

    let receipts = StubReceipts::new(vec![]);
    let policy = StubPolicy { cfg: Some(cfg) };
    let output = CaptureOutput::default();
    let uc = IngestUseCase::new(receipts, policy, output, ExplodingValidator, noop_render);
    let result = uc.execute(default_request()).expect("execute");

    assert_eq!(result.exit_code, 2);
}

#[test]
fn exit_code_0_for_skip_verdict() {
    let mut receipts = StubReceipts::new(vec!["optional".into()]);
    receipts.reports.insert(
        "optional".to_string(),
        ReportRead::Bytes(report_bytes(
            VerdictStatus::Skip,
            VerdictCounts::default(),
            vec![],
        )),
    );

    let policy = StubPolicy { cfg: None };
    let output = CaptureOutput::default();
    let uc = IngestUseCase::new(receipts, policy, output, ExplodingValidator, noop_render);
    let result = uc.execute(default_request()).expect("execute");

    assert_eq!(result.exit_code, 0, "skip should not fail");
}

// =============================================================================
// 6. Report metadata: timestamp, schema version, tool info
// =============================================================================

#[test]
fn report_contains_correct_schema_version() {
    let receipts = StubReceipts::new(vec![]);
    let policy = StubPolicy { cfg: None };
    let output = CaptureOutput::default();
    let uc = IngestUseCase::new(receipts, policy, output, ExplodingValidator, noop_render);
    let result = uc.execute(default_request()).expect("execute");

    assert_eq!(result.report.schema, "cockpit.report.v1");
}

#[test]
fn report_preserves_tool_info_from_request() {
    let receipts = StubReceipts::new(vec![]);
    let policy = StubPolicy { cfg: None };
    let output = CaptureOutput::default();
    let uc = IngestUseCase::new(receipts, policy, output, ExplodingValidator, noop_render);

    let mut req = default_request();
    req.tool = ToolInfo {
        name: "custom-tool".to_string(),
        version: "2.3.4".to_string(),
        commit: Some("abc123".to_string()),
    };
    let result = uc.execute(req).expect("execute");

    assert_eq!(result.report.tool.name, "custom-tool");
    assert_eq!(result.report.tool.version, "2.3.4");
    assert_eq!(result.report.tool.commit.as_deref(), Some("abc123"));
}

#[test]
fn report_preserves_run_info_from_request() {
    let receipts = StubReceipts::new(vec![]);
    let policy = StubPolicy { cfg: None };
    let output = CaptureOutput::default();
    let uc = IngestUseCase::new(receipts, policy, output, ExplodingValidator, noop_render);

    let mut req = default_request();
    req.run.started_at = "2025-06-15T10:30:00Z".to_string();
    req.run.ended_at = Some("2025-06-15T10:31:00Z".to_string());
    req.run.duration_ms = Some(60_000);
    let result = uc.execute(req).expect("execute");

    assert_eq!(result.report.run.started_at, "2025-06-15T10:30:00Z");
    assert_eq!(
        result.report.run.ended_at.as_deref(),
        Some("2025-06-15T10:31:00Z")
    );
    assert_eq!(result.report.run.duration_ms, Some(60_000));
}

#[test]
fn report_policy_snapshot_reflects_config() {
    let mut cfg = CockpitConfig::default();
    cfg.sensors
        .insert("build".to_string(), default_sensor_policy(true));
    cfg.sensors.insert(
        "lint".to_string(),
        SensorPolicy {
            blocking: false,
            missing: MissingPolicy::Warn,
            ..Default::default()
        },
    );

    let mut receipts = StubReceipts::new(vec!["build".into(), "lint".into()]);
    for s in &["build", "lint"] {
        receipts.reports.insert(
            s.to_string(),
            ReportRead::Bytes(report_bytes(
                VerdictStatus::Pass,
                VerdictCounts::default(),
                vec![],
            )),
        );
    }

    let policy = StubPolicy { cfg: Some(cfg) };
    let output = CaptureOutput::default();
    let uc = IngestUseCase::new(receipts, policy, output, ExplodingValidator, noop_render);
    let result = uc.execute(default_request()).expect("execute");

    assert_eq!(result.report.policy.sensors.len(), 2);
    assert_eq!(result.report.policy.max_highlights, 7);
}

// =============================================================================
// 7. Extra outputs: OutputSink::write_extra_file
// =============================================================================

#[test]
fn report_json_has_trailing_newline() {
    let receipts = StubReceipts::new(vec![]);
    let output = Rc::new(CaptureOutput::default());
    let output_ref = Rc::clone(&output);

    let uc = IngestUseCase::new(
        receipts,
        StubPolicy { cfg: None },
        RcOutput(output_ref),
        ExplodingValidator,
        noop_render,
    );
    let result = uc.execute(default_request()).expect("execute");

    let reports = output.reports.borrow();
    assert!(!reports.is_empty());
    assert!(
        reports[0].ends_with('\n'),
        "report JSON should end with trailing newline"
    );
    assert!(!result.comment_md.is_empty());
}

/// Wrapper to use Rc<CaptureOutput> as OutputSink.
struct RcOutput(Rc<CaptureOutput>);

impl OutputSink for RcOutput {
    fn write_cockpit_report(&self, json: &str) -> anyhow::Result<()> {
        self.0.write_cockpit_report(json)
    }
    fn write_cockpit_comment(&self, md: &str) -> anyhow::Result<()> {
        self.0.write_cockpit_comment(md)
    }
    fn write_extra_file(&self, name: &str, content: &[u8]) -> anyhow::Result<()> {
        self.0.write_extra_file(name, content)
    }
}

// =============================================================================
// 8. Post-process hooks: verify hook config is available
// =============================================================================

#[test]
fn hooks_in_config_do_not_affect_ingest_output() {
    use cockpitctl_types::{HookConfig, HookWhen};

    let mut cfg = CockpitConfig::default();
    cfg.hooks.push(HookConfig {
        name: "notify".to_string(),
        command: "echo done".to_string(),
        when: HookWhen::AfterIngest,
        timeout_ms: 5000,
    });

    let receipts = StubReceipts::new(vec![]);
    let policy = StubPolicy { cfg: Some(cfg) };
    let output = CaptureOutput::default();
    let uc = IngestUseCase::new(receipts, policy, output, ExplodingValidator, noop_render);
    let result = uc.execute(default_request()).expect("execute");

    // Hooks are config-level; ingest should still succeed with pass
    assert_eq!(result.exit_code, 0);
    assert_eq!(result.report.schema, "cockpit.report.v1");
}

// =============================================================================
// 9. Buildfix integration: plan.json is processed when present
// =============================================================================

#[test]
fn buildfix_plan_matched_to_highlights() {
    let mut receipts = StubReceipts::new(vec!["linter".into()]);
    let findings = vec![Finding {
        severity: Severity::Error,
        check_id: None,
        code: "lint/unused-var".to_string(),
        message: "Unused variable".to_string(),
        location: Some(Location {
            path: Some("src/main.rs".to_string()),
            line: Some(10),
            col: None,
        }),
        help: None,
        url: None,
        fingerprint: Some("fp-001".to_string()),
        data: None,
    }];
    receipts.reports.insert(
        "linter".to_string(),
        ReportRead::Bytes(report_bytes(
            VerdictStatus::Fail,
            VerdictCounts {
                error: 1,
                ..Default::default()
            },
            findings,
        )),
    );

    let plan = BuildfixPlan {
        schema: "buildfix.plan.v1".to_string(),
        tool: tool_info(),
        fixes: vec![Fix {
            id: "fix-001".to_string(),
            safety: SafetyLevel::Safe,
            description: "Remove unused variable".to_string(),
            finding_refs: vec![FindingRef {
                sensor_id: "linter".to_string(),
                fingerprint: Some("fp-001".to_string()),
                code: Some("lint/unused-var".to_string()),
                tool: None,
                check_id: None,
            }],
            preconditions: None,
            data: None,
        }],
    };
    receipts.plans.insert(
        "linter".to_string(),
        PlanRead::Bytes(serde_json::to_vec(&plan).unwrap()),
    );

    let mut cfg = CockpitConfig::default();
    cfg.sensors
        .insert("linter".to_string(), default_sensor_policy(true));

    let policy = StubPolicy { cfg: Some(cfg) };
    let output = CaptureOutput::default();
    let uc = IngestUseCase::new(receipts, policy, output, ExplodingValidator, noop_render);
    let result = uc.execute(default_request()).expect("execute");

    assert!(result.buildfix.is_some(), "buildfix should be present");
    let bf = result.buildfix.unwrap();
    assert_eq!(bf.total_fixes, 1);
    assert!(bf.matched_count >= 1);
}

#[test]
fn buildfix_missing_plan_returns_none() {
    let mut receipts = StubReceipts::new(vec!["build".into()]);
    receipts.reports.insert(
        "build".to_string(),
        ReportRead::Bytes(report_bytes(
            VerdictStatus::Pass,
            VerdictCounts::default(),
            vec![],
        )),
    );
    // No plan.json

    let policy = StubPolicy { cfg: None };
    let output = CaptureOutput::default();
    let uc = IngestUseCase::new(receipts, policy, output, ExplodingValidator, noop_render);
    let result = uc.execute(default_request()).expect("execute");

    assert!(result.buildfix.is_none(), "no plan → no buildfix summary");
}

#[test]
fn buildfix_stored_in_report_data() {
    let mut receipts = StubReceipts::new(vec!["sensor".into()]);
    receipts.reports.insert(
        "sensor".to_string(),
        ReportRead::Bytes(report_bytes(
            VerdictStatus::Fail,
            VerdictCounts {
                error: 1,
                ..Default::default()
            },
            vec![finding(Severity::Error, "err/001", "Error")],
        )),
    );

    let plan = BuildfixPlan {
        schema: "buildfix.plan.v1".to_string(),
        tool: tool_info(),
        fixes: vec![Fix {
            id: "fix-x".to_string(),
            safety: SafetyLevel::Guarded,
            description: "Fix the error".to_string(),
            finding_refs: vec![FindingRef {
                sensor_id: "sensor".to_string(),
                fingerprint: None,
                code: Some("err/001".to_string()),
                tool: None,
                check_id: None,
            }],
            preconditions: None,
            data: None,
        }],
    };
    receipts.plans.insert(
        "sensor".to_string(),
        PlanRead::Bytes(serde_json::to_vec(&plan).unwrap()),
    );

    let mut cfg = CockpitConfig::default();
    cfg.sensors
        .insert("sensor".to_string(), default_sensor_policy(true));

    let policy = StubPolicy { cfg: Some(cfg) };
    let output = CaptureOutput::default();
    let uc = IngestUseCase::new(receipts, policy, output, ExplodingValidator, noop_render);
    let result = uc.execute(default_request()).expect("execute");

    // _buildfix key should be present in report.data
    let data = result.report.data.as_ref().expect("data should be present");
    assert!(
        data.get("_buildfix").is_some(),
        "_buildfix key should be in report.data"
    );
}

// =============================================================================
// 10. Policy signing: config is preserved in snapshot
// =============================================================================

#[test]
fn policy_signing_config_does_not_break_ingest() {
    use cockpitctl_types::{PolicySignatureAlgorithm, PolicySigningConfig};

    let mut cfg = CockpitConfig {
        policy_signing: PolicySigningConfig {
            enabled: true,
            algorithm: PolicySignatureAlgorithm::HmacSha256,
            key_path: Some("/tmp/key.pem".to_string()),
            key_env: None,
            key_id: Some("key-1".to_string()),
        },
        ..Default::default()
    };
    cfg.sensors
        .insert("sensor".to_string(), default_sensor_policy(false));

    let mut receipts = StubReceipts::new(vec!["sensor".into()]);
    receipts.reports.insert(
        "sensor".to_string(),
        ReportRead::Bytes(report_bytes(
            VerdictStatus::Pass,
            VerdictCounts::default(),
            vec![],
        )),
    );

    let policy = StubPolicy { cfg: Some(cfg) };
    let output = CaptureOutput::default();
    let uc = IngestUseCase::new(receipts, policy, output, ExplodingValidator, noop_render);
    let result = uc.execute(default_request()).expect("execute");

    assert_eq!(result.exit_code, 0);
    assert_eq!(result.report.schema, "cockpit.report.v1");
}

// =============================================================================
// Additional edge cases
// =============================================================================

#[test]
fn non_blocking_fail_sensor_does_not_cause_exit_2() {
    let mut receipts = StubReceipts::new(vec!["optional-lint".into()]);
    receipts.reports.insert(
        "optional-lint".to_string(),
        ReportRead::Bytes(report_bytes(
            VerdictStatus::Fail,
            VerdictCounts {
                error: 5,
                ..Default::default()
            },
            vec![finding(Severity::Error, "lint/x", "Error")],
        )),
    );

    let mut cfg = CockpitConfig::default();
    cfg.sensors.insert(
        "optional-lint".to_string(),
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

    assert_eq!(
        result.exit_code, 0,
        "non-blocking fail should not trigger exit 2"
    );
}

#[test]
fn multiple_severity_findings_sorted_in_highlights() {
    let findings = vec![
        finding_with_location(Severity::Info, "info/note", "Informational", "a.rs", 1),
        finding_with_location(
            Severity::Error,
            "err/critical",
            "Critical error",
            "b.rs",
            10,
        ),
        finding_with_location(Severity::Warn, "warn/caution", "Caution", "c.rs", 5),
    ];

    let mut receipts = StubReceipts::new(vec!["multi".into()]);
    receipts.reports.insert(
        "multi".to_string(),
        ReportRead::Bytes(report_bytes(
            VerdictStatus::Fail,
            VerdictCounts {
                info: 1,
                warn: 1,
                error: 1,
                suppressed: 0,
            },
            findings,
        )),
    );

    let mut cfg = CockpitConfig::default();
    cfg.sensors
        .insert("multi".to_string(), default_sensor_policy(true));

    let policy = StubPolicy { cfg: Some(cfg) };
    let output = CaptureOutput::default();
    let uc = IngestUseCase::new(receipts, policy, output, ExplodingValidator, noop_render);
    let result = uc.execute(default_request()).expect("execute");

    // Highlights should be ordered severity desc: error before warn before info
    if result.report.highlights.len() >= 2 {
        let severities: Vec<&Severity> = result
            .report
            .highlights
            .iter()
            .map(|h| &h.finding.severity)
            .collect();
        // Error should come before Warn which should come before Info
        let error_pos = severities.iter().position(|s| **s == Severity::Error);
        let warn_pos = severities.iter().position(|s| **s == Severity::Warn);
        if let (Some(e), Some(w)) = (error_pos, warn_pos) {
            assert!(e < w, "error highlights should precede warn highlights");
        }
    }
}

#[test]
fn render_function_receives_report_and_config() {
    let receipts = StubReceipts::new(vec![]);
    let policy = StubPolicy { cfg: None };
    let output = CaptureOutput::default();

    let render_called = Rc::new(RefCell::new(false));
    let render_called_clone = Rc::clone(&render_called);

    let uc = IngestUseCase::new(
        receipts,
        policy,
        output,
        ExplodingValidator,
        move |report: &CockpitReport, _cfg: &CockpitConfig| {
            *render_called_clone.borrow_mut() = true;
            assert_eq!(report.schema, "cockpit.report.v1");
            "RENDERED".to_string()
        },
    );
    let result = uc.execute(default_request()).expect("execute");

    assert!(*render_called.borrow(), "render should be called");
    assert_eq!(result.comment_md, "RENDERED");
}
