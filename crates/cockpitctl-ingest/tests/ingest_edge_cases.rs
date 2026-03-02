//! Edge-case and boundary-condition tests for the ingest pipeline.
//!
//! Focuses on: empty/minimal inputs, policy threshold boundaries,
//! config defaulting, mixed verdicts, findings-count caps, and
//! schema validation mode interactions.

use std::collections::{BTreeMap, HashMap};

use cockpitctl_ingest::{
    CommentRead, DiscoveredSensors, IngestRequest, IngestUseCase, NoOpSchemaValidator, OutputSink,
    PolicySource, ReceiptSource, ReportRead, SchemaValidationResult, SchemaValidator,
};
use cockpitctl_types::{
    CockpitConfig, CockpitReport, Finding, MissingPolicy, Presence, RunInfo, SchemaValidation,
    SensorPolicy, SensorReport, Severity, ToolInfo, Verdict, VerdictCounts, VerdictStatus,
    severity_rank,
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
        panic!("schema validator should not be called");
    }
}

struct AlwaysInvalidValidator {
    errors: Vec<String>,
}

impl SchemaValidator for AlwaysInvalidValidator {
    fn validate_receipt(&self, _bytes: &[u8]) -> anyhow::Result<SchemaValidationResult> {
        Ok(SchemaValidationResult::Invalid(self.errors.clone()))
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
        started_at: "2026-01-01T00:00:00Z".to_string(),
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
    "<!-- rendered -->".to_string()
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

fn sensor_policy(blocking: bool, missing: MissingPolicy) -> SensorPolicy {
    SensorPolicy {
        blocking,
        missing,
        section: None,
        require_label: None,
        repro: None,
    }
}

// =============================================================================
// Tests
// =============================================================================

/// Empty artifacts directory: no sensors discovered, no sensors in config.
/// Should produce a passing report with zero sensors and zero highlights.
#[test]
fn empty_artifacts_no_config_produces_pass() {
    let receipts = StubReceipts::new(vec![]);
    let policy = StubPolicy { cfg: None };
    let output = CaptureOutput::default();
    let uc = IngestUseCase::new(receipts, policy, output, NoOpSchemaValidator, noop_render);

    let result = uc.execute(default_request()).unwrap();
    assert_eq!(result.exit_code, 0);
    assert!(result.report.sensors.is_empty());
    assert!(result.report.highlights.is_empty());
    assert_eq!(result.report.verdict.status, VerdictStatus::Pass);
}

/// Single sensor with pass verdict and empty findings list.
#[test]
fn single_sensor_empty_findings_passes() {
    let mut receipts = StubReceipts::new(vec!["sensor-a".into()]);
    receipts.reports.insert(
        "sensor-a".to_string(),
        ReportRead::Bytes(report_bytes(
            VerdictStatus::Pass,
            VerdictCounts::default(),
            vec![],
        )),
    );

    let policy = StubPolicy { cfg: None };
    let output = CaptureOutput::default();
    let uc = IngestUseCase::new(receipts, policy, output, NoOpSchemaValidator, noop_render);

    let result = uc.execute(default_request()).unwrap();
    assert_eq!(result.exit_code, 0);
    assert_eq!(result.report.sensors.len(), 1);
    assert_eq!(result.report.sensors[0].presence, Presence::Present);
    assert_eq!(result.report.sensors[0].verdict.status, VerdictStatus::Pass);
    assert!(result.report.highlights.is_empty());
}

/// A blocking sensor with fail verdict causes exit code 2.
/// A non-blocking sensor with fail verdict does NOT cause exit code 2.
#[test]
fn blocking_fail_vs_non_blocking_fail_exit_codes() {
    // Non-blocking fail -> exit 0
    let mut receipts = StubReceipts::new(vec!["nb".into()]);
    receipts.reports.insert(
        "nb".to_string(),
        ReportRead::Bytes(report_bytes(
            VerdictStatus::Fail,
            VerdictCounts::default(),
            vec![],
        )),
    );
    let mut cfg = CockpitConfig::default();
    cfg.sensors
        .insert("nb".to_string(), sensor_policy(false, MissingPolicy::Fail));

    let policy = StubPolicy {
        cfg: Some(cfg.clone()),
    };
    let output = CaptureOutput::default();
    let uc = IngestUseCase::new(receipts, policy, output, NoOpSchemaValidator, noop_render);
    let result = uc.execute(default_request()).unwrap();
    assert_eq!(
        result.exit_code, 0,
        "non-blocking fail should not cause exit 2"
    );

    // Blocking fail -> exit 2
    let mut receipts2 = StubReceipts::new(vec!["bl".into()]);
    receipts2.reports.insert(
        "bl".to_string(),
        ReportRead::Bytes(report_bytes(
            VerdictStatus::Fail,
            VerdictCounts::default(),
            vec![],
        )),
    );
    let mut cfg2 = CockpitConfig::default();
    cfg2.sensors
        .insert("bl".to_string(), sensor_policy(true, MissingPolicy::Fail));

    let policy2 = StubPolicy { cfg: Some(cfg2) };
    let output2 = CaptureOutput::default();
    let uc2 = IngestUseCase::new(
        receipts2,
        policy2,
        output2,
        NoOpSchemaValidator,
        noop_render,
    );
    let result2 = uc2.execute(default_request()).unwrap();
    assert_eq!(result2.exit_code, 2, "blocking fail should cause exit 2");
}

/// Empty cockpit.toml (all defaults) should work the same as no config.
#[test]
fn empty_config_all_defaults_matches_no_config() {
    let mut receipts_none = StubReceipts::new(vec!["s".into()]);
    receipts_none.reports.insert(
        "s".to_string(),
        ReportRead::Bytes(report_bytes(
            VerdictStatus::Pass,
            VerdictCounts::default(),
            vec![],
        )),
    );

    let mut receipts_default = StubReceipts::new(vec!["s".into()]);
    receipts_default.reports.insert(
        "s".to_string(),
        ReportRead::Bytes(report_bytes(
            VerdictStatus::Pass,
            VerdictCounts::default(),
            vec![],
        )),
    );

    let output_none = CaptureOutput::default();
    let uc_none = IngestUseCase::new(
        receipts_none,
        StubPolicy { cfg: None },
        output_none,
        NoOpSchemaValidator,
        noop_render,
    );
    let result_none = uc_none.execute(default_request()).unwrap();

    let output_default = CaptureOutput::default();
    let uc_default = IngestUseCase::new(
        receipts_default,
        StubPolicy {
            cfg: Some(CockpitConfig::default()),
        },
        output_default,
        NoOpSchemaValidator,
        noop_render,
    );
    let result_default = uc_default.execute(default_request()).unwrap();

    assert_eq!(result_none.exit_code, result_default.exit_code);
    assert_eq!(
        result_none.report.sensors.len(),
        result_default.report.sensors.len()
    );
    assert_eq!(
        result_none.report.verdict.status,
        result_default.report.verdict.status
    );
}

/// Config with unknown fields (extra keys) should be ignored gracefully.
/// At the ingest layer we verify that default config is accepted without issue.
#[test]
fn config_default_accepted_gracefully() {
    let cfg = CockpitConfig::default();
    let receipts = StubReceipts::new(vec![]);
    let policy = StubPolicy { cfg: Some(cfg) };
    let output = CaptureOutput::default();
    let uc = IngestUseCase::new(receipts, policy, output, NoOpSchemaValidator, noop_render);

    let result = uc.execute(default_request()).unwrap();
    assert_eq!(result.exit_code, 0);
}

/// Mixed pass/warn/fail across multiple sensors.
/// Overall verdict should be fail if any blocking sensor fails.
#[test]
fn mixed_verdicts_across_sensors() {
    let mut receipts = StubReceipts::new(vec!["alpha".into(), "beta".into(), "gamma".into()]);
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
            VerdictStatus::Warn,
            VerdictCounts::default(),
            vec![],
        )),
    );
    receipts.reports.insert(
        "gamma".to_string(),
        ReportRead::Bytes(report_bytes(
            VerdictStatus::Fail,
            VerdictCounts::default(),
            vec![],
        )),
    );

    let mut cfg = CockpitConfig::default();
    cfg.sensors.insert(
        "alpha".to_string(),
        sensor_policy(true, MissingPolicy::Fail),
    );
    cfg.sensors.insert(
        "beta".to_string(),
        sensor_policy(false, MissingPolicy::Warn),
    );
    cfg.sensors.insert(
        "gamma".to_string(),
        sensor_policy(true, MissingPolicy::Fail),
    );

    let policy = StubPolicy { cfg: Some(cfg) };
    let output = CaptureOutput::default();
    let uc = IngestUseCase::new(receipts, policy, output, NoOpSchemaValidator, noop_render);

    let result = uc.execute(default_request()).unwrap();
    assert_eq!(result.exit_code, 2, "blocking sensor gamma fails -> exit 2");
    assert_eq!(result.report.verdict.status, VerdictStatus::Fail);
    assert_eq!(result.report.sensors.len(), 3);
}

/// Mixed pass/warn across sensors with no blocking fail -> exit 0.
#[test]
fn mixed_pass_warn_no_blocking_fail_exits_zero() {
    let mut receipts = StubReceipts::new(vec!["a".into(), "b".into()]);
    receipts.reports.insert(
        "a".to_string(),
        ReportRead::Bytes(report_bytes(
            VerdictStatus::Pass,
            VerdictCounts::default(),
            vec![],
        )),
    );
    receipts.reports.insert(
        "b".to_string(),
        ReportRead::Bytes(report_bytes(
            VerdictStatus::Warn,
            VerdictCounts::default(),
            vec![],
        )),
    );

    let mut cfg = CockpitConfig::default();
    cfg.sensors
        .insert("a".to_string(), sensor_policy(true, MissingPolicy::Fail));
    cfg.sensors
        .insert("b".to_string(), sensor_policy(true, MissingPolicy::Fail));

    let policy = StubPolicy { cfg: Some(cfg) };
    let output = CaptureOutput::default();
    let uc = IngestUseCase::new(receipts, policy, output, NoOpSchemaValidator, noop_render);

    let result = uc.execute(default_request()).unwrap();
    assert_eq!(
        result.exit_code, 0,
        "warn without fail should not cause exit 2"
    );
}

/// Ingest with all skip verdict sensors -> exit 0 and all sensors skip.
#[test]
fn all_skip_verdicts_exit_zero() {
    let mut receipts = StubReceipts::new(vec!["x".into(), "y".into()]);
    receipts.reports.insert(
        "x".to_string(),
        ReportRead::Bytes(report_bytes(
            VerdictStatus::Skip,
            VerdictCounts::default(),
            vec![],
        )),
    );
    receipts.reports.insert(
        "y".to_string(),
        ReportRead::Bytes(report_bytes(
            VerdictStatus::Skip,
            VerdictCounts::default(),
            vec![],
        )),
    );

    let policy = StubPolicy { cfg: None };
    let output = CaptureOutput::default();
    let uc = IngestUseCase::new(receipts, policy, output, NoOpSchemaValidator, noop_render);

    let result = uc.execute(default_request()).unwrap();
    assert_eq!(result.exit_code, 0);
    for sensor in &result.report.sensors {
        assert_eq!(sensor.verdict.status, VerdictStatus::Skip);
    }
}

/// Multiple sensors where one fails schema validation in strict mode.
/// The failing sensor gets a schema_violation highlight; the other passes.
#[test]
fn one_sensor_fails_schema_in_strict_mode() {
    // "good" sensor uses default tool info
    let mut receipts = StubReceipts::new(vec!["bad".into(), "good".into()]);
    receipts.reports.insert(
        "good".to_string(),
        ReportRead::Bytes(report_bytes(
            VerdictStatus::Pass,
            VerdictCounts::default(),
            vec![],
        )),
    );

    // "bad" sensor uses a different tool name so we can distinguish it
    let bad_report = SensorReport {
        schema: "sensor.report.v1".to_string(),
        tool: ToolInfo {
            name: "bad-tool".to_string(),
            version: "0.0.1".to_string(),
            commit: None,
        },
        run: run_info(),
        verdict: Verdict {
            status: VerdictStatus::Pass,
            counts: VerdictCounts::default(),
            reasons: vec![],
        },
        findings: vec![],
        artifacts: vec![],
        data: None,
    };
    let bad_bytes = serde_json::to_vec(&bad_report).unwrap();
    receipts
        .reports
        .insert("bad".to_string(), ReportRead::Bytes(bad_bytes.clone()));

    let mut cfg = CockpitConfig::default();
    cfg.policy.schema_validation = SchemaValidation::Strict;
    cfg.sensors.insert(
        "good".to_string(),
        sensor_policy(false, MissingPolicy::Warn),
    );
    cfg.sensors
        .insert("bad".to_string(), sensor_policy(true, MissingPolicy::Fail));

    // Validator that fails only for "bad" sensor's bytes
    struct SelectiveValidator {
        bad_bytes: Vec<u8>,
    }
    impl SchemaValidator for SelectiveValidator {
        fn validate_receipt(&self, bytes: &[u8]) -> anyhow::Result<SchemaValidationResult> {
            if bytes == self.bad_bytes {
                Ok(SchemaValidationResult::Invalid(vec![
                    "missing required field".to_string(),
                ]))
            } else {
                Ok(SchemaValidationResult::Valid)
            }
        }
    }

    let validator = SelectiveValidator { bad_bytes };
    let policy = StubPolicy { cfg: Some(cfg) };
    let output = CaptureOutput::default();
    let uc = IngestUseCase::new(receipts, policy, output, validator, noop_render);

    let result = uc.execute(default_request()).unwrap();
    assert_eq!(result.exit_code, 2, "blocking bad sensor -> exit 2");

    let bad_sensor = result
        .report
        .sensors
        .iter()
        .find(|s| s.id == "bad")
        .unwrap();
    assert_eq!(bad_sensor.presence, Presence::Invalid);
    assert!(!bad_sensor.errors.is_empty());

    let good_sensor = result
        .report
        .sensors
        .iter()
        .find(|s| s.id == "good")
        .unwrap();
    assert_eq!(good_sensor.presence, Presence::Present);

    assert!(
        result
            .report
            .highlights
            .iter()
            .any(|h| h.finding.code == "cockpit.schema_violation")
    );
}

/// In lax mode, schema validator is never called even if receipts are present.
#[test]
fn lax_mode_never_calls_validator() {
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
        sensor_policy(true, MissingPolicy::Fail),
    );

    let policy = StubPolicy { cfg: Some(cfg) };
    let output = CaptureOutput::default();
    // ExplodingValidator panics if called; proves lax mode skips it.
    let uc = IngestUseCase::new(receipts, policy, output, ExplodingValidator, noop_render);

    let result = uc.execute(default_request()).unwrap();
    assert_eq!(result.exit_code, 0);
}

/// CLI override for schema_validation=strict overrides config lax.
#[test]
fn cli_override_strict_over_config_lax() {
    let mut receipts = StubReceipts::new(vec!["s".into()]);
    receipts.reports.insert(
        "s".to_string(),
        ReportRead::Bytes(report_bytes(
            VerdictStatus::Pass,
            VerdictCounts::default(),
            vec![],
        )),
    );

    let mut cfg = CockpitConfig::default();
    cfg.policy.schema_validation = SchemaValidation::Lax;
    cfg.sensors
        .insert("s".to_string(), sensor_policy(true, MissingPolicy::Fail));

    let policy = StubPolicy { cfg: Some(cfg) };
    let output = CaptureOutput::default();
    let validator = AlwaysInvalidValidator {
        errors: vec!["bad schema".to_string()],
    };
    let uc = IngestUseCase::new(receipts, policy, output, validator, noop_render);

    let mut req = default_request();
    req.schema_validation_override = Some(SchemaValidation::Strict);
    let result = uc.execute(req).unwrap();
    assert_eq!(
        result.exit_code, 2,
        "CLI strict override should trigger validation"
    );
    assert!(
        result
            .report
            .highlights
            .iter()
            .any(|h| h.finding.code == "cockpit.schema_violation")
    );
}

/// CLI override for schema_validation=lax overrides config strict.
#[test]
fn cli_override_lax_over_config_strict() {
    let mut receipts = StubReceipts::new(vec!["s".into()]);
    receipts.reports.insert(
        "s".to_string(),
        ReportRead::Bytes(report_bytes(
            VerdictStatus::Pass,
            VerdictCounts::default(),
            vec![],
        )),
    );

    let mut cfg = CockpitConfig::default();
    cfg.policy.schema_validation = SchemaValidation::Strict;
    cfg.sensors
        .insert("s".to_string(), sensor_policy(true, MissingPolicy::Fail));

    let policy = StubPolicy { cfg: Some(cfg) };
    let output = CaptureOutput::default();
    // ExplodingValidator would panic if called in strict mode
    let uc = IngestUseCase::new(receipts, policy, output, ExplodingValidator, noop_render);

    let mut req = default_request();
    req.schema_validation_override = Some(SchemaValidation::Lax);
    let result = uc.execute(req).unwrap();
    assert_eq!(
        result.exit_code, 0,
        "CLI lax override should skip validation"
    );
}

/// Sensor with max_per_sensor_findings findings does NOT truncate.
#[test]
fn findings_at_max_count_not_truncated() {
    let max_findings = 20; // default max_per_sensor_findings
    let findings: Vec<Finding> = (0..max_findings)
        .map(|i| make_finding(Severity::Warn, &format!("W{i:03}"), &format!("warning {i}")))
        .collect();

    let mut receipts = StubReceipts::new(vec!["s".into()]);
    receipts.reports.insert(
        "s".to_string(),
        ReportRead::Bytes(report_bytes(
            VerdictStatus::Warn,
            VerdictCounts {
                warn: max_findings as u64,
                ..Default::default()
            },
            findings,
        )),
    );

    let policy = StubPolicy { cfg: None };
    let output = CaptureOutput::default();
    let uc = IngestUseCase::new(receipts, policy, output, NoOpSchemaValidator, noop_render);

    let result = uc.execute(default_request()).unwrap();
    assert_eq!(result.report.sensors.len(), 1);
    assert!(
        !result.report.sensors[0].truncated,
        "exactly at limit should not be truncated"
    );
}

/// Sensor with max_per_sensor_findings + 1 findings IS truncated.
#[test]
fn findings_above_max_count_truncated() {
    let max_findings = 20; // default max_per_sensor_findings
    let findings: Vec<Finding> = (0..=max_findings)
        .map(|i| make_finding(Severity::Warn, &format!("W{i:03}"), &format!("warning {i}")))
        .collect();

    let mut receipts = StubReceipts::new(vec!["s".into()]);
    receipts.reports.insert(
        "s".to_string(),
        ReportRead::Bytes(report_bytes(
            VerdictStatus::Warn,
            VerdictCounts {
                warn: (max_findings + 1) as u64,
                ..Default::default()
            },
            findings,
        )),
    );

    let policy = StubPolicy { cfg: None };
    let output = CaptureOutput::default();
    let uc = IngestUseCase::new(receipts, policy, output, NoOpSchemaValidator, noop_render);

    let result = uc.execute(default_request()).unwrap();
    assert_eq!(result.report.sensors.len(), 1);
    assert!(
        result.report.sensors[0].truncated,
        "above limit should be truncated"
    );
}

/// Sensor with zero findings but warn verdict is accepted without issue.
#[test]
fn warn_verdict_zero_findings_accepted() {
    let mut receipts = StubReceipts::new(vec!["s".into()]);
    receipts.reports.insert(
        "s".to_string(),
        ReportRead::Bytes(report_bytes(
            VerdictStatus::Warn,
            VerdictCounts::default(),
            vec![],
        )),
    );

    let mut cfg = CockpitConfig::default();
    cfg.sensors
        .insert("s".to_string(), sensor_policy(true, MissingPolicy::Fail));

    let policy = StubPolicy { cfg: Some(cfg) };
    let output = CaptureOutput::default();
    let uc = IngestUseCase::new(receipts, policy, output, NoOpSchemaValidator, noop_render);

    let result = uc.execute(default_request()).unwrap();
    assert_eq!(result.exit_code, 0, "warn does not fail");
    assert_eq!(result.report.sensors[0].verdict.status, VerdictStatus::Warn);
}

/// Missing sensor with missing=skip produces skip verdict and no highlight.
#[test]
fn missing_sensor_skip_policy_no_highlight() {
    let receipts = StubReceipts::new(vec![]);
    let mut cfg = CockpitConfig::default();
    cfg.sensors.insert(
        "absent".to_string(),
        sensor_policy(true, MissingPolicy::Skip),
    );

    let policy = StubPolicy { cfg: Some(cfg) };
    let output = CaptureOutput::default();
    let uc = IngestUseCase::new(receipts, policy, output, ExplodingValidator, noop_render);

    let result = uc.execute(default_request()).unwrap();
    assert_eq!(result.report.sensors.len(), 1);
    assert_eq!(result.report.sensors[0].verdict.status, VerdictStatus::Skip);
    assert_eq!(
        result.report.sensors[0].missing_policy_applied,
        Some(MissingPolicy::Skip)
    );
    assert!(result.report.highlights.is_empty());
    assert_eq!(result.exit_code, 0);
}

/// Missing sensor with missing=warn produces warn verdict and a highlight.
#[test]
fn missing_sensor_warn_policy_produces_highlight() {
    let receipts = StubReceipts::new(vec![]);
    let mut cfg = CockpitConfig::default();
    cfg.sensors.insert(
        "absent".to_string(),
        sensor_policy(true, MissingPolicy::Warn),
    );

    let policy = StubPolicy { cfg: Some(cfg) };
    let output = CaptureOutput::default();
    let uc = IngestUseCase::new(receipts, policy, output, ExplodingValidator, noop_render);

    let result = uc.execute(default_request()).unwrap();
    assert_eq!(result.report.sensors.len(), 1);
    assert_eq!(
        result.report.sensors[0].missing_policy_applied,
        Some(MissingPolicy::Warn)
    );
    assert!(!result.report.highlights.is_empty());
    assert_eq!(
        result.report.highlights[0].finding.code,
        "cockpit.missing_receipt"
    );
}

/// Missing sensor with missing=fail produces fail verdict and exit 2 when blocking.
#[test]
fn missing_blocking_sensor_fail_policy_exit_2() {
    let receipts = StubReceipts::new(vec![]);
    let mut cfg = CockpitConfig::default();
    cfg.sensors.insert(
        "absent".to_string(),
        sensor_policy(true, MissingPolicy::Fail),
    );

    let policy = StubPolicy { cfg: Some(cfg) };
    let output = CaptureOutput::default();
    let uc = IngestUseCase::new(receipts, policy, output, ExplodingValidator, noop_render);

    let result = uc.execute(default_request()).unwrap();
    assert_eq!(result.exit_code, 2);
    assert_eq!(result.report.verdict.status, VerdictStatus::Fail);
}

/// Multiple sensors with a mix of present/missing/skip shows correct presence.
#[test]
fn mixed_present_missing_skip_sensors() {
    let mut receipts = StubReceipts::new(vec!["present".into()]);
    receipts.reports.insert(
        "present".to_string(),
        ReportRead::Bytes(report_bytes(
            VerdictStatus::Pass,
            VerdictCounts::default(),
            vec![],
        )),
    );

    let mut cfg = CockpitConfig::default();
    cfg.sensors.insert(
        "present".to_string(),
        sensor_policy(false, MissingPolicy::Fail),
    );
    cfg.sensors.insert(
        "missing-skip".to_string(),
        sensor_policy(false, MissingPolicy::Skip),
    );
    cfg.sensors.insert(
        "missing-warn".to_string(),
        sensor_policy(false, MissingPolicy::Warn),
    );

    let policy = StubPolicy { cfg: Some(cfg) };
    let output = CaptureOutput::default();
    let uc = IngestUseCase::new(receipts, policy, output, NoOpSchemaValidator, noop_render);

    let result = uc.execute(default_request()).unwrap();
    assert_eq!(result.report.sensors.len(), 3);

    let present = result
        .report
        .sensors
        .iter()
        .find(|s| s.id == "present")
        .unwrap();
    assert_eq!(present.presence, Presence::Present);

    let skip = result
        .report
        .sensors
        .iter()
        .find(|s| s.id == "missing-skip")
        .unwrap();
    assert_eq!(skip.presence, Presence::Missing);
    assert_eq!(skip.verdict.status, VerdictStatus::Skip);

    let warn = result
        .report
        .sensors
        .iter()
        .find(|s| s.id == "missing-warn")
        .unwrap();
    assert_eq!(warn.presence, Presence::Missing);
    assert_eq!(warn.verdict.status, VerdictStatus::Warn);
}

/// Config with explicit overrides differs from defaults.
#[test]
fn config_explicit_overrides_applied() {
    let mut cfg = CockpitConfig::default();
    cfg.policy.max_highlights = 3;
    cfg.policy.max_per_sensor_findings = 5;
    cfg.policy.warn_is_fail = true;

    // With warn_is_fail, a warn sensor on a blocking sensor -> fail verdict.
    let mut receipts = StubReceipts::new(vec!["s".into()]);
    receipts.reports.insert(
        "s".to_string(),
        ReportRead::Bytes(report_bytes(
            VerdictStatus::Warn,
            VerdictCounts::default(),
            vec![],
        )),
    );
    cfg.sensors
        .insert("s".to_string(), sensor_policy(true, MissingPolicy::Fail));

    let policy = StubPolicy { cfg: Some(cfg) };
    let output = CaptureOutput::default();
    let uc = IngestUseCase::new(receipts, policy, output, NoOpSchemaValidator, noop_render);

    let result = uc.execute(default_request()).unwrap();
    // warn_is_fail causes the warn verdict to be promoted to fail
    assert_eq!(
        result.exit_code, 2,
        "warn_is_fail should promote warn to fail"
    );
    assert_eq!(result.report.verdict.status, VerdictStatus::Fail);
}

/// Sensor with findings of mixed severity produces highlights sorted by severity desc.
#[test]
fn findings_sorted_by_severity_in_highlights() {
    let findings = vec![
        make_finding(Severity::Info, "I001", "info finding"),
        make_finding(Severity::Error, "E001", "error finding"),
        make_finding(Severity::Warn, "W001", "warn finding"),
    ];

    let mut receipts = StubReceipts::new(vec!["s".into()]);
    receipts.reports.insert(
        "s".to_string(),
        ReportRead::Bytes(report_bytes(
            VerdictStatus::Fail,
            VerdictCounts {
                info: 1,
                warn: 1,
                error: 1,
                ..Default::default()
            },
            findings,
        )),
    );

    let mut cfg = CockpitConfig::default();
    cfg.sensors
        .insert("s".to_string(), sensor_policy(true, MissingPolicy::Fail));

    let policy = StubPolicy { cfg: Some(cfg) };
    let output = CaptureOutput::default();
    let uc = IngestUseCase::new(receipts, policy, output, NoOpSchemaValidator, noop_render);

    let result = uc.execute(default_request()).unwrap();
    // Highlights should be sorted: error first, then warn, then info
    if result.report.highlights.len() >= 2 {
        assert!(
            severity_rank(&result.report.highlights[0].finding.severity)
                <= severity_rank(&result.report.highlights[1].finding.severity),
            "highlights should be sorted by severity descending"
        );
    }
}

/// Report always writes output (report.json + comment.md) even on policy failure.
#[test]
fn output_written_even_on_policy_fail() {
    let receipts = StubReceipts::new(vec![]);
    let mut cfg = CockpitConfig::default();
    cfg.sensors.insert(
        "blocker".to_string(),
        sensor_policy(true, MissingPolicy::Fail),
    );

    let policy = StubPolicy { cfg: Some(cfg) };
    let output = CaptureOutput::default();
    let uc = IngestUseCase::new(receipts, policy, output, ExplodingValidator, noop_render);

    let result = uc.execute(default_request()).unwrap();
    assert_eq!(result.exit_code, 2);
    // The comment_md and report should still be populated
    assert!(!result.comment_md.is_empty());
    // Report should be serializable
    let parsed: serde_json::Value =
        serde_json::to_value(&result.report).expect("report should serialize");
    assert!(parsed.is_object());
}

/// Discovered sensors with no config: all discovered are treated as expected.
#[test]
fn discovered_sensors_become_expected_without_config() {
    let mut receipts = StubReceipts::new(vec!["d1".into(), "d2".into(), "d3".into()]);
    for id in &["d1", "d2", "d3"] {
        receipts.reports.insert(
            id.to_string(),
            ReportRead::Bytes(report_bytes(
                VerdictStatus::Pass,
                VerdictCounts::default(),
                vec![],
            )),
        );
    }

    let policy = StubPolicy { cfg: None };
    let output = CaptureOutput::default();
    let uc = IngestUseCase::new(receipts, policy, output, NoOpSchemaValidator, noop_render);

    let result = uc.execute(default_request()).unwrap();
    assert_eq!(result.report.sensors.len(), 3);
    let ids: Vec<&str> = result
        .report
        .sensors
        .iter()
        .map(|s| s.id.as_str())
        .collect();
    assert!(ids.contains(&"d1"));
    assert!(ids.contains(&"d2"));
    assert!(ids.contains(&"d3"));
}

/// Config sensors override discovered: only config sensors appear in report.
#[test]
fn config_sensors_override_discovered() {
    let mut receipts = StubReceipts::new(vec!["discovered".into()]);
    receipts.reports.insert(
        "discovered".to_string(),
        ReportRead::Bytes(report_bytes(
            VerdictStatus::Pass,
            VerdictCounts::default(),
            vec![],
        )),
    );

    let mut cfg = CockpitConfig::default();
    cfg.sensors.insert(
        "expected-only".to_string(),
        sensor_policy(false, MissingPolicy::Skip),
    );

    let policy = StubPolicy { cfg: Some(cfg) };
    let output = CaptureOutput::default();
    let uc = IngestUseCase::new(receipts, policy, output, NoOpSchemaValidator, noop_render);

    let result = uc.execute(default_request()).unwrap();
    assert_eq!(result.report.sensors.len(), 1);
    assert_eq!(result.report.sensors[0].id, "expected-only");
}

/// Invalid sensor ID with path traversal in discovered list emits highlight.
#[test]
fn invalid_sensor_id_in_discovery_emits_traversal_highlight() {
    let mut receipts = StubReceipts::new(vec![]);
    receipts.invalid_sensor_ids.push("../escape".to_string());

    let policy = StubPolicy { cfg: None };
    let output = CaptureOutput::default();
    let uc = IngestUseCase::new(receipts, policy, output, NoOpSchemaValidator, noop_render);

    let result = uc.execute(default_request()).unwrap();
    assert!(
        result
            .report
            .highlights
            .iter()
            .any(|h| h.finding.code == "cockpit.path_traversal")
    );
}

/// Truncated discovery (too many sensors) emits a sensors_truncated highlight.
#[test]
fn truncated_discovery_emits_sensors_truncated() {
    let mut receipts = StubReceipts::new(vec!["a".into()]);
    receipts.truncated = true;
    receipts.total_found = 100;
    receipts.reports.insert(
        "a".to_string(),
        ReportRead::Bytes(report_bytes(
            VerdictStatus::Pass,
            VerdictCounts::default(),
            vec![],
        )),
    );

    let policy = StubPolicy { cfg: None };
    let output = CaptureOutput::default();
    let uc = IngestUseCase::new(receipts, policy, output, NoOpSchemaValidator, noop_render);

    let result = uc.execute(default_request()).unwrap();
    assert!(
        result
            .report
            .highlights
            .iter()
            .any(|h| h.finding.code == "cockpit.sensors_truncated")
    );
}

/// Oversized receipt produces an oversized highlight.
#[test]
fn oversized_receipt_produces_highlight() {
    let mut receipts = StubReceipts::new(vec!["big".into()]);
    receipts.reports.insert(
        "big".to_string(),
        ReportRead::Oversized {
            size: 3_000_000,
            cap: 2_000_000,
        },
    );

    let mut cfg = CockpitConfig::default();
    cfg.sensors
        .insert("big".to_string(), sensor_policy(true, MissingPolicy::Fail));

    let policy = StubPolicy { cfg: Some(cfg) };
    let output = CaptureOutput::default();
    let uc = IngestUseCase::new(receipts, policy, output, ExplodingValidator, noop_render);

    let result = uc.execute(default_request()).unwrap();
    assert_eq!(result.exit_code, 2);
    assert!(
        result
            .report
            .highlights
            .iter()
            .any(|h| h.finding.code == "cockpit.receipt_oversized")
    );
}

/// Unsafe path receipt produces a path_traversal highlight.
#[test]
fn unsafe_path_receipt_produces_traversal_highlight() {
    let mut receipts = StubReceipts::new(vec!["unsafe-sensor".into()]);
    receipts
        .reports
        .insert("unsafe-sensor".to_string(), ReportRead::UnsafePath);

    let mut cfg = CockpitConfig::default();
    cfg.sensors.insert(
        "unsafe-sensor".to_string(),
        sensor_policy(true, MissingPolicy::Fail),
    );

    let policy = StubPolicy { cfg: Some(cfg) };
    let output = CaptureOutput::default();
    let uc = IngestUseCase::new(receipts, policy, output, ExplodingValidator, noop_render);

    let result = uc.execute(default_request()).unwrap();
    assert_eq!(result.exit_code, 2);
    assert!(
        result
            .report
            .highlights
            .iter()
            .any(|h| h.finding.code == "cockpit.path_traversal")
    );
}

/// Render function receives both report and config.
#[test]
fn render_receives_report_and_config() {
    use std::sync::atomic::{AtomicBool, Ordering};
    static RENDER_CALLED: AtomicBool = AtomicBool::new(false);

    let receipts = StubReceipts::new(vec![]);
    let policy = StubPolicy { cfg: None };
    let output = CaptureOutput::default();
    let uc = IngestUseCase::new(
        receipts,
        policy,
        output,
        NoOpSchemaValidator,
        |_report: &CockpitReport, _cfg: &CockpitConfig| {
            RENDER_CALLED.store(true, Ordering::SeqCst);
            "rendered".to_string()
        },
    );

    let result = uc.execute(default_request()).unwrap();
    assert!(RENDER_CALLED.load(Ordering::SeqCst));
    assert_eq!(result.comment_md, "rendered");
}

/// Label-gated sensor is skipped when required label is missing.
#[test]
fn label_gate_skips_sensor_when_label_absent() {
    let receipts = StubReceipts::new(vec![]);
    let mut cfg = CockpitConfig::default();
    cfg.sensors.insert(
        "gated".to_string(),
        SensorPolicy {
            blocking: true,
            missing: MissingPolicy::Fail,
            section: None,
            require_label: Some("special-label".to_string()),
            repro: None,
        },
    );

    let policy = StubPolicy { cfg: Some(cfg) };
    let output = CaptureOutput::default();
    let uc = IngestUseCase::new(receipts, policy, output, ExplodingValidator, noop_render);

    let result = uc.execute(default_request()).unwrap();
    assert_eq!(result.exit_code, 0);
    assert_eq!(result.report.sensors[0].verdict.status, VerdictStatus::Skip);
}

/// Label-gated sensor is processed when required label is present.
#[test]
fn label_gate_processes_sensor_when_label_present() {
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
            require_label: Some("special-label".to_string()),
            repro: None,
        },
    );

    let policy = StubPolicy { cfg: Some(cfg) };
    let output = CaptureOutput::default();
    let uc = IngestUseCase::new(receipts, policy, output, NoOpSchemaValidator, noop_render);

    let mut req = default_request();
    req.labels.push("special-label".to_string());
    let result = uc.execute(req).unwrap();
    assert_eq!(result.exit_code, 0);
    assert_eq!(result.report.sensors[0].presence, Presence::Present);
}

/// Custom max_per_sensor_findings of 2 truncates when 3 findings present.
#[test]
fn custom_max_findings_cap_truncates() {
    let findings = vec![
        make_finding(Severity::Warn, "W001", "a"),
        make_finding(Severity::Warn, "W002", "b"),
        make_finding(Severity::Warn, "W003", "c"),
    ];

    let mut receipts = StubReceipts::new(vec!["s".into()]);
    receipts.reports.insert(
        "s".to_string(),
        ReportRead::Bytes(report_bytes(
            VerdictStatus::Warn,
            VerdictCounts {
                warn: 3,
                ..Default::default()
            },
            findings,
        )),
    );

    let mut cfg = CockpitConfig::default();
    cfg.policy.max_per_sensor_findings = 2;

    let policy = StubPolicy { cfg: Some(cfg) };
    let output = CaptureOutput::default();
    let uc = IngestUseCase::new(receipts, policy, output, NoOpSchemaValidator, noop_render);

    let result = uc.execute(default_request()).unwrap();
    assert!(
        result.report.sensors[0].truncated,
        "3 findings with cap=2 should truncate"
    );
}

/// No-op schema validator always returns Valid for any input.
#[test]
fn noop_validator_returns_valid_for_garbage() {
    let result = NoOpSchemaValidator
        .validate_receipt(b"this is not json at all!!!")
        .unwrap();
    assert!(matches!(result, SchemaValidationResult::Valid));
}
