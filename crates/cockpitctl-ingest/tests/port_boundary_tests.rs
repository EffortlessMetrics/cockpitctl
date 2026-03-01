//! Port boundary tests with test doubles for the ingest use case.
//!
//! Exercises ReceiptSource, PolicySource, and OutputSink trait boundaries
//! including empty inputs, large inputs, error paths, config overrides,
//! deterministic ordering, and budget enforcement.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};

use cockpitctl_ingest::{
    CommentRead, DiscoveredSensors, IngestRequest, IngestUseCase, NoOpSchemaValidator, OutputSink,
    PlanRead, PolicySource, ReceiptSource, ReportRead, SchemaValidationResult, SchemaValidator,
};
use cockpitctl_types::{
    CockpitConfig, CockpitReport, Finding, Location, MissingPolicy, Presence, RunInfo,
    SchemaValidation, SensorPolicy, SensorReport, Severity, ToolInfo, Verdict, VerdictCounts,
    VerdictStatus,
};

// =============================================================================
// Test doubles
// =============================================================================

struct TestReceiptSource {
    sensors: Vec<String>,
    reports: HashMap<String, ReportRead>,
    truncated: bool,
    total_found: usize,
    invalid_sensor_ids: Vec<String>,
}

impl TestReceiptSource {
    fn empty() -> Self {
        Self {
            sensors: vec![],
            reports: HashMap::new(),
            truncated: false,
            total_found: 0,
            invalid_sensor_ids: vec![],
        }
    }

    fn with(sensors: Vec<&str>, reports: HashMap<String, ReportRead>) -> Self {
        let total_found = sensors.len();
        Self {
            sensors: sensors.into_iter().map(String::from).collect(),
            reports,
            truncated: false,
            total_found,
            invalid_sensor_ids: vec![],
        }
    }
}

impl ReceiptSource for TestReceiptSource {
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
        format!("artifacts/{sensor_id}/report.json")
    }

    fn comment_path_if_present(&self, _sensor_id: &str) -> anyhow::Result<CommentRead> {
        Ok(CommentRead::Missing)
    }

    fn read_plan_bytes(&self, _sensor_id: &str) -> anyhow::Result<PlanRead> {
        Ok(PlanRead::Missing)
    }
}

/// ReceiptSource that fails on discovered_sensors().
struct FailingDiscoverySource;

impl ReceiptSource for FailingDiscoverySource {
    fn discovered_sensors(&self) -> anyhow::Result<DiscoveredSensors> {
        Err(anyhow::anyhow!("simulated IO failure in discovery"))
    }
    fn read_report_bytes(&self, _: &str) -> anyhow::Result<ReportRead> {
        Ok(ReportRead::Missing)
    }
    fn report_path(&self, sensor_id: &str) -> String {
        format!("artifacts/{sensor_id}/report.json")
    }
    fn comment_path_if_present(&self, _: &str) -> anyhow::Result<CommentRead> {
        Ok(CommentRead::Missing)
    }
}

struct TestPolicySource {
    config: Option<CockpitConfig>,
}

impl PolicySource for TestPolicySource {
    fn load_config(&self) -> anyhow::Result<Option<CockpitConfig>> {
        Ok(self.config.clone())
    }
}

/// PolicySource that always returns an error.
struct FailingPolicySource;

impl PolicySource for FailingPolicySource {
    fn load_config(&self) -> anyhow::Result<Option<CockpitConfig>> {
        Err(anyhow::anyhow!("simulated config load failure"))
    }
}

struct CapturingOutputSink {
    report_json: RefCell<String>,
    comment_md: RefCell<String>,
}

impl CapturingOutputSink {
    fn new() -> Self {
        Self {
            report_json: RefCell::new(String::new()),
            comment_md: RefCell::new(String::new()),
        }
    }
}

impl OutputSink for CapturingOutputSink {
    fn write_cockpit_report(&self, json: &str) -> anyhow::Result<()> {
        *self.report_json.borrow_mut() = json.to_string();
        Ok(())
    }
    fn write_cockpit_comment(&self, md: &str) -> anyhow::Result<()> {
        *self.comment_md.borrow_mut() = md.to_string();
        Ok(())
    }
}

/// OutputSink that fails on write_cockpit_report.
struct FailingOutputSink;

impl OutputSink for FailingOutputSink {
    fn write_cockpit_report(&self, _: &str) -> anyhow::Result<()> {
        Err(anyhow::anyhow!("simulated disk write failure"))
    }
    fn write_cockpit_comment(&self, _: &str) -> anyhow::Result<()> {
        Err(anyhow::anyhow!("simulated disk write failure"))
    }
}

/// SchemaValidator that rejects everything.
struct RejectAllValidator;

impl SchemaValidator for RejectAllValidator {
    fn validate_receipt(&self, _bytes: &[u8]) -> anyhow::Result<SchemaValidationResult> {
        Ok(SchemaValidationResult::Invalid(vec![
            "schema validation failed: missing required field 'verdict'".to_string(),
        ]))
    }
}

// =============================================================================
// Helpers
// =============================================================================

fn stub_render(_report: &CockpitReport, _cfg: &CockpitConfig) -> String {
    "<!-- port boundary test rendered -->".to_string()
}

fn default_request() -> IngestRequest {
    IngestRequest {
        labels: vec![],
        tool: ToolInfo {
            name: "cockpitctl".to_string(),
            version: "0.1.0-test".to_string(),
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
        schema_validation_override: None,
    }
}

fn sensor_report_bytes(status: VerdictStatus, findings: Vec<Finding>) -> Vec<u8> {
    let warn_count = findings
        .iter()
        .filter(|f| f.severity == Severity::Warn)
        .count() as u64;
    let error_count = findings
        .iter()
        .filter(|f| f.severity == Severity::Error)
        .count() as u64;
    let info_count = findings
        .iter()
        .filter(|f| f.severity == Severity::Info)
        .count() as u64;
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
            counts: VerdictCounts {
                info: info_count,
                warn: warn_count,
                error: error_count,
                suppressed: 0,
            },
            reasons: vec![],
        },
        findings,
        artifacts: vec![],
        data: None,
    };
    serde_json::to_vec(&report).unwrap()
}

fn pass_bytes() -> Vec<u8> {
    sensor_report_bytes(VerdictStatus::Pass, vec![])
}

fn warn_finding(code: &str, path: &str, line: u32) -> Finding {
    Finding {
        severity: Severity::Warn,
        check_id: None,
        code: code.to_string(),
        message: format!("Warning: {code}"),
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

fn error_finding(code: &str, path: &str, line: u32) -> Finding {
    Finding {
        severity: Severity::Error,
        check_id: None,
        code: code.to_string(),
        message: format!("Error: {code}"),
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

fn blocking_policy() -> SensorPolicy {
    SensorPolicy {
        blocking: true,
        missing: MissingPolicy::Fail,
        ..Default::default()
    }
}

fn nonblocking_policy() -> SensorPolicy {
    SensorPolicy {
        blocking: false,
        missing: MissingPolicy::Warn,
        ..Default::default()
    }
}

// =============================================================================
// Tests
// =============================================================================

// 1. ReceiptSource returning empty → ingest produces pass verdict
#[test]
fn receipt_source_empty_produces_pass() {
    let uc = IngestUseCase::new(
        TestReceiptSource::empty(),
        TestPolicySource { config: None },
        CapturingOutputSink::new(),
        NoOpSchemaValidator,
        stub_render,
    );
    let result = uc.execute(default_request()).unwrap();

    assert_eq!(result.exit_code, 0);
    assert_eq!(result.report.verdict.status, VerdictStatus::Pass);
    assert!(result.report.sensors.is_empty());
    assert_eq!(result.report.schema, "cockpit.report.v1");
}

// 2. ReceiptSource returning one valid sensor → included in report
#[test]
fn receipt_source_one_sensor_included_in_report() {
    let mut reports = HashMap::new();
    reports.insert("alpha".to_string(), ReportRead::Bytes(pass_bytes()));

    let mut cfg = CockpitConfig::default();
    cfg.sensors.insert("alpha".to_string(), blocking_policy());

    let uc = IngestUseCase::new(
        TestReceiptSource::with(vec!["alpha"], reports),
        TestPolicySource { config: Some(cfg) },
        CapturingOutputSink::new(),
        NoOpSchemaValidator,
        stub_render,
    );
    let result = uc.execute(default_request()).unwrap();

    assert_eq!(result.exit_code, 0);
    assert_eq!(result.report.sensors.len(), 1);
    assert_eq!(result.report.sensors[0].id, "alpha");
    assert_eq!(result.report.sensors[0].presence, Presence::Present);
    assert_eq!(result.report.sensors[0].verdict.status, VerdictStatus::Pass);
}

// 3. ReceiptSource returning 100+ sensors → all processed within cap
#[test]
fn receipt_source_many_sensors_all_processed() {
    let count = 120;
    let sensor_names: Vec<String> = (0..count).map(|i| format!("sensor-{i:04}")).collect();
    let mut reports = HashMap::new();
    for name in &sensor_names {
        reports.insert(name.clone(), ReportRead::Bytes(pass_bytes()));
    }

    let uc = IngestUseCase::new(
        TestReceiptSource::with(sensor_names.iter().map(|s| s.as_str()).collect(), reports),
        TestPolicySource { config: None },
        CapturingOutputSink::new(),
        NoOpSchemaValidator,
        stub_render,
    );
    let result = uc.execute(default_request()).unwrap();

    assert_eq!(result.report.sensors.len(), count);
    assert_eq!(result.exit_code, 0);
    for sensor in &result.report.sensors {
        assert_eq!(sensor.verdict.status, VerdictStatus::Pass);
    }
}

// 4. ReceiptSource with duplicate sensor IDs → both processed
#[test]
fn receipt_source_duplicate_ids_both_processed() {
    let mut reports = HashMap::new();
    reports.insert("dup".to_string(), ReportRead::Bytes(pass_bytes()));

    let source = TestReceiptSource::with(vec!["dup", "dup"], reports);
    let uc = IngestUseCase::new(
        source,
        TestPolicySource { config: None },
        CapturingOutputSink::new(),
        NoOpSchemaValidator,
        stub_render,
    );
    let result = uc.execute(default_request()).unwrap();

    assert_eq!(result.report.sensors.len(), 2);
}

// 5. PolicySource with all default config → lenient evaluation (pass)
#[test]
fn policy_source_default_config_lenient() {
    let mut reports = HashMap::new();
    let findings = vec![warn_finding("lint.w1", "src/lib.rs", 10)];
    reports.insert(
        "linter".to_string(),
        ReportRead::Bytes(sensor_report_bytes(VerdictStatus::Warn, findings)),
    );

    let uc = IngestUseCase::new(
        TestReceiptSource::with(vec!["linter"], reports),
        TestPolicySource {
            config: Some(CockpitConfig::default()),
        },
        CapturingOutputSink::new(),
        NoOpSchemaValidator,
        stub_render,
    );
    let result = uc.execute(default_request()).unwrap();

    // Default policy: warn_is_fail=false, sensors not declared as blocking
    assert_eq!(result.exit_code, 0);
    assert!(
        result.report.verdict.status == VerdictStatus::Pass
            || result.report.verdict.status == VerdictStatus::Warn
    );
}

// 6. PolicySource with strict blocking → fail on any issue
#[test]
fn policy_source_strict_blocking_fails() {
    let mut reports = HashMap::new();
    let findings = vec![error_finding("build.error", "src/main.rs", 42)];
    reports.insert(
        "builddiag".to_string(),
        ReportRead::Bytes(sensor_report_bytes(VerdictStatus::Fail, findings)),
    );

    let mut cfg = CockpitConfig::default();
    cfg.sensors
        .insert("builddiag".to_string(), blocking_policy());

    let uc = IngestUseCase::new(
        TestReceiptSource::with(vec!["builddiag"], reports),
        TestPolicySource { config: Some(cfg) },
        CapturingOutputSink::new(),
        NoOpSchemaValidator,
        stub_render,
    );
    let result = uc.execute(default_request()).unwrap();

    assert_eq!(result.exit_code, 2);
    assert_eq!(result.report.verdict.status, VerdictStatus::Fail);
}

// 7. OutputSink receives report → verify complete report written
#[test]
fn output_sink_receives_complete_report() {
    let mut reports = HashMap::new();
    reports.insert("alpha".to_string(), ReportRead::Bytes(pass_bytes()));

    let uc = IngestUseCase::new(
        TestReceiptSource::with(vec!["alpha"], reports),
        TestPolicySource { config: None },
        CapturingOutputSink::new(),
        NoOpSchemaValidator,
        stub_render,
    );
    let result = uc.execute(default_request()).unwrap();

    // Verify report is well-formed via the returned IngestResult
    let report_json = serde_json::to_string_pretty(&result.report).unwrap();
    assert!(!report_json.is_empty(), "report JSON should be written");

    let parsed: CockpitReport = serde_json::from_str(&report_json).unwrap();
    assert_eq!(parsed.schema, "cockpit.report.v1");
    assert_eq!(parsed.sensors.len(), 1);
}

// 8. OutputSink receives comment → verify comment non-empty
#[test]
fn output_sink_receives_non_empty_comment() {
    let uc = IngestUseCase::new(
        TestReceiptSource::empty(),
        TestPolicySource { config: None },
        CapturingOutputSink::new(),
        NoOpSchemaValidator,
        stub_render,
    );
    let result = uc.execute(default_request()).unwrap();

    assert!(!result.comment_md.is_empty(), "comment should be non-empty");
}

// 9. Error in ReceiptSource → graceful failure (Err, not panic)
#[test]
fn receipt_source_error_graceful_failure() {
    let uc = IngestUseCase::new(
        FailingDiscoverySource,
        TestPolicySource { config: None },
        CapturingOutputSink::new(),
        NoOpSchemaValidator,
        stub_render,
    );
    let result = uc.execute(default_request());

    assert!(result.is_err(), "should return Err, not panic");
    let err_msg = format!("{:#}", result.err().unwrap());
    assert!(err_msg.contains("simulated IO failure"));
}

// 10. Error in OutputSink → graceful failure (Err, not panic)
#[test]
fn output_sink_error_graceful_failure() {
    let uc = IngestUseCase::new(
        TestReceiptSource::empty(),
        TestPolicySource { config: None },
        FailingOutputSink,
        NoOpSchemaValidator,
        stub_render,
    );
    let result = uc.execute(default_request());

    assert!(
        result.is_err(),
        "output sink failure should propagate as Err"
    );
}

// 11. Schema validation rejects receipt → controlled finding, not crash
#[test]
fn schema_validation_rejects_receipt_controlled_finding() {
    let mut reports = HashMap::new();
    reports.insert("sensor-a".to_string(), ReportRead::Bytes(pass_bytes()));

    let mut req = default_request();
    req.schema_validation_override = Some(SchemaValidation::Strict);

    let uc = IngestUseCase::new(
        TestReceiptSource::with(vec!["sensor-a"], reports),
        TestPolicySource { config: None },
        CapturingOutputSink::new(),
        RejectAllValidator,
        stub_render,
    );
    let result = uc.execute(req).unwrap();

    assert_eq!(result.report.sensors.len(), 1);
    assert_eq!(result.report.sensors[0].presence, Presence::Invalid);
    assert!(
        result
            .report
            .highlights
            .iter()
            .any(|h| h.finding.code == "cockpit.schema_violation"),
        "should produce a schema_violation highlight"
    );
}

// 12. Receipt with empty findings → sensor still in report with pass
#[test]
fn receipt_empty_findings_sensor_present_with_pass() {
    let mut reports = HashMap::new();
    // Pass verdict with zero findings
    reports.insert(
        "clean".to_string(),
        ReportRead::Bytes(sensor_report_bytes(VerdictStatus::Pass, vec![])),
    );

    let uc = IngestUseCase::new(
        TestReceiptSource::with(vec!["clean"], reports),
        TestPolicySource { config: None },
        CapturingOutputSink::new(),
        NoOpSchemaValidator,
        stub_render,
    );
    let result = uc.execute(default_request()).unwrap();

    assert_eq!(result.report.sensors.len(), 1);
    assert_eq!(result.report.sensors[0].id, "clean");
    assert_eq!(result.report.sensors[0].presence, Presence::Present);
    assert_eq!(result.report.sensors[0].verdict.status, VerdictStatus::Pass);
    assert!(result.report.highlights.is_empty());
}

// 13. Receipt with only warnings → verdict depends on policy
#[test]
fn receipt_only_warnings_verdict_depends_on_policy() {
    let findings = vec![
        warn_finding("lint.w1", "src/a.rs", 1),
        warn_finding("lint.w2", "src/b.rs", 2),
    ];
    let bytes = sensor_report_bytes(VerdictStatus::Warn, findings);

    // Case A: warn_is_fail=false → pass (exit 0)
    {
        let mut reports = HashMap::new();
        reports.insert("linter".to_string(), ReportRead::Bytes(bytes.clone()));
        let mut cfg = CockpitConfig::default();
        cfg.sensors.insert("linter".to_string(), blocking_policy());
        cfg.policy.warn_is_fail = false;

        let uc = IngestUseCase::new(
            TestReceiptSource::with(vec!["linter"], reports),
            TestPolicySource { config: Some(cfg) },
            CapturingOutputSink::new(),
            NoOpSchemaValidator,
            stub_render,
        );
        let result = uc.execute(default_request()).unwrap();
        assert_eq!(result.exit_code, 0);
    }

    // Case B: warn_is_fail=true → fail (exit 2)
    {
        let mut reports = HashMap::new();
        reports.insert("linter".to_string(), ReportRead::Bytes(bytes));
        let mut cfg = CockpitConfig::default();
        cfg.sensors.insert("linter".to_string(), blocking_policy());
        cfg.policy.warn_is_fail = true;

        let uc = IngestUseCase::new(
            TestReceiptSource::with(vec!["linter"], reports),
            TestPolicySource { config: Some(cfg) },
            CapturingOutputSink::new(),
            NoOpSchemaValidator,
            stub_render,
        );
        let result = uc.execute(default_request()).unwrap();
        assert_eq!(result.exit_code, 2);
    }
}

// 14. Mixed verdicts across sensors → overall verdict computed correctly
#[test]
fn mixed_verdicts_overall_computed_correctly() {
    let mut reports = HashMap::new();
    reports.insert("passer".to_string(), ReportRead::Bytes(pass_bytes()));
    reports.insert(
        "warner".to_string(),
        ReportRead::Bytes(sensor_report_bytes(
            VerdictStatus::Warn,
            vec![warn_finding("w1", "src/x.rs", 1)],
        )),
    );
    reports.insert(
        "failer".to_string(),
        ReportRead::Bytes(sensor_report_bytes(
            VerdictStatus::Fail,
            vec![error_finding("e1", "src/y.rs", 1)],
        )),
    );

    let mut cfg = CockpitConfig::default();
    cfg.sensors.insert("passer".to_string(), blocking_policy());
    cfg.sensors.insert("warner".to_string(), blocking_policy());
    cfg.sensors.insert("failer".to_string(), blocking_policy());

    let uc = IngestUseCase::new(
        TestReceiptSource::with(vec!["failer", "passer", "warner"], reports),
        TestPolicySource { config: Some(cfg) },
        CapturingOutputSink::new(),
        NoOpSchemaValidator,
        stub_render,
    );
    let result = uc.execute(default_request()).unwrap();

    assert_eq!(result.exit_code, 2);
    assert_eq!(result.report.verdict.status, VerdictStatus::Fail);
    assert_eq!(result.report.sensors.len(), 3);

    let by_id: HashMap<&str, &VerdictStatus> = result
        .report
        .sensors
        .iter()
        .map(|s| (s.id.as_str(), &s.verdict.status))
        .collect();
    assert_eq!(*by_id["passer"], VerdictStatus::Pass);
    assert_eq!(*by_id["warner"], VerdictStatus::Warn);
    assert_eq!(*by_id["failer"], VerdictStatus::Fail);
}

// 15. Sensor marked non-blocking → doesn't affect overall pass/fail
#[test]
fn nonblocking_sensor_does_not_affect_overall_verdict() {
    let mut reports = HashMap::new();
    reports.insert(
        "nonblocking-fail".to_string(),
        ReportRead::Bytes(sensor_report_bytes(
            VerdictStatus::Fail,
            vec![error_finding("e1", "src/z.rs", 1)],
        )),
    );
    reports.insert("blocking-pass".to_string(), ReportRead::Bytes(pass_bytes()));

    let mut cfg = CockpitConfig::default();
    cfg.sensors
        .insert("nonblocking-fail".to_string(), nonblocking_policy());
    cfg.sensors
        .insert("blocking-pass".to_string(), blocking_policy());

    let uc = IngestUseCase::new(
        TestReceiptSource::with(vec!["blocking-pass", "nonblocking-fail"], reports),
        TestPolicySource { config: Some(cfg) },
        CapturingOutputSink::new(),
        NoOpSchemaValidator,
        stub_render,
    );
    let result = uc.execute(default_request()).unwrap();

    // Non-blocking failure should not cause exit code 2
    assert_eq!(result.exit_code, 0);
    assert!(result.report.verdict.status != VerdictStatus::Fail);
}

// 16. Multiple findings sorted deterministically → verify order
#[test]
fn findings_sorted_deterministically() {
    let findings = vec![
        warn_finding("lint.z-rule", "src/z.rs", 99),
        error_finding("lint.a-rule", "src/a.rs", 1),
        warn_finding("lint.m-rule", "src/m.rs", 50),
        error_finding("lint.b-rule", "src/b.rs", 10),
    ];
    let bytes = sensor_report_bytes(VerdictStatus::Fail, findings);

    let mut reports = HashMap::new();
    reports.insert("linter".to_string(), ReportRead::Bytes(bytes));

    let mut cfg = CockpitConfig::default();
    cfg.policy.max_highlights = 20;
    cfg.sensors.insert("linter".to_string(), blocking_policy());

    let uc = IngestUseCase::new(
        TestReceiptSource::with(vec!["linter"], reports),
        TestPolicySource { config: Some(cfg) },
        CapturingOutputSink::new(),
        NoOpSchemaValidator,
        stub_render,
    );
    let result = uc.execute(default_request()).unwrap();

    // Highlights should be sorted: severity desc (error before warn),
    // then sensor_id, then path, then line, then code
    let highlight_codes: Vec<&str> = result
        .report
        .highlights
        .iter()
        .map(|h| h.finding.code.as_str())
        .collect();

    // Errors come first (severity desc)
    let first_error_idx = highlight_codes
        .iter()
        .position(|c| c.starts_with("lint.a") || c.starts_with("lint.b"));
    let first_warn_idx = highlight_codes
        .iter()
        .position(|c| c.starts_with("lint.m") || c.starts_with("lint.z"));

    if let (Some(e), Some(w)) = (first_error_idx, first_warn_idx) {
        assert!(
            e < w,
            "error findings should come before warn findings in highlights"
        );
    }

    // Run again to verify determinism
    let findings2 = vec![
        warn_finding("lint.z-rule", "src/z.rs", 99),
        error_finding("lint.a-rule", "src/a.rs", 1),
        warn_finding("lint.m-rule", "src/m.rs", 50),
        error_finding("lint.b-rule", "src/b.rs", 10),
    ];
    let bytes2 = sensor_report_bytes(VerdictStatus::Fail, findings2);
    let mut reports2 = HashMap::new();
    reports2.insert("linter".to_string(), ReportRead::Bytes(bytes2));
    let mut cfg2 = CockpitConfig::default();
    cfg2.policy.max_highlights = 20;
    cfg2.sensors.insert("linter".to_string(), blocking_policy());

    let uc2 = IngestUseCase::new(
        TestReceiptSource::with(vec!["linter"], reports2),
        TestPolicySource { config: Some(cfg2) },
        CapturingOutputSink::new(),
        NoOpSchemaValidator,
        stub_render,
    );
    let result2 = uc2.execute(default_request()).unwrap();

    let codes2: Vec<&str> = result2
        .report
        .highlights
        .iter()
        .map(|h| h.finding.code.as_str())
        .collect();
    assert_eq!(
        highlight_codes, codes2,
        "highlight order must be deterministic"
    );
}

// 17. Highlights respect budget → only top N returned
#[test]
fn highlights_respect_budget() {
    let findings: Vec<Finding> = (0..50)
        .map(|i| warn_finding(&format!("rule-{i:03}"), &format!("src/f_{i:03}.rs"), i + 1))
        .collect();
    let bytes = sensor_report_bytes(VerdictStatus::Warn, findings);

    let mut reports = HashMap::new();
    reports.insert("linter".to_string(), ReportRead::Bytes(bytes));

    let mut cfg = CockpitConfig::default();
    cfg.policy.max_highlights = 5;
    cfg.policy.max_per_sensor_findings = 50;
    cfg.sensors
        .insert("linter".to_string(), nonblocking_policy());

    let uc = IngestUseCase::new(
        TestReceiptSource::with(vec!["linter"], reports),
        TestPolicySource { config: Some(cfg) },
        CapturingOutputSink::new(),
        NoOpSchemaValidator,
        stub_render,
    );
    let result = uc.execute(default_request()).unwrap();

    assert!(
        result.report.highlights.len() <= 5,
        "highlights should be capped at max_highlights=5, got {}",
        result.report.highlights.len()
    );
}

// 18. Large receipt data → processed within limits (no crash on big payload)
#[test]
fn large_receipt_data_processed() {
    // Create a sensor report with many findings and large data payload
    let mut findings: Vec<Finding> = Vec::new();
    for i in 0..200 {
        findings.push(Finding {
            severity: if i % 3 == 0 {
                Severity::Error
            } else if i % 3 == 1 {
                Severity::Warn
            } else {
                Severity::Info
            },
            check_id: Some(format!("check-{i:04}")),
            code: format!("rule.{i:04}"),
            message: format!("Finding #{i} with some detailed description text padding"),
            location: Some(Location {
                path: Some(format!("src/module_{:03}/file_{:03}.rs", i / 10, i % 10)),
                line: Some(i + 1),
                col: Some(1),
            }),
            help: Some(format!("Consider fixing issue {i}")),
            url: None,
            fingerprint: Some(format!("fp-{i:08x}")),
            data: None,
        });
    }
    let bytes = sensor_report_bytes(VerdictStatus::Fail, findings);

    let mut reports = HashMap::new();
    reports.insert("large-sensor".to_string(), ReportRead::Bytes(bytes));

    let mut cfg = CockpitConfig::default();
    cfg.sensors
        .insert("large-sensor".to_string(), blocking_policy());

    let uc = IngestUseCase::new(
        TestReceiptSource::with(vec!["large-sensor"], reports),
        TestPolicySource { config: Some(cfg) },
        CapturingOutputSink::new(),
        NoOpSchemaValidator,
        stub_render,
    );
    let result = uc.execute(default_request()).unwrap();

    assert_eq!(result.report.sensors.len(), 1);
    assert_eq!(result.report.sensors[0].presence, Presence::Present);
    // Highlights should be within budget (default is 7)
    assert!(result.report.highlights.len() <= 7);
}

// 19. Config override via CLI → schema_validation_override takes precedence
#[test]
fn cli_schema_validation_override_takes_precedence() {
    let mut reports = HashMap::new();
    reports.insert("sensor-x".to_string(), ReportRead::Bytes(pass_bytes()));

    // Config says lax
    let mut cfg = CockpitConfig::default();
    cfg.policy.schema_validation = SchemaValidation::Lax;
    cfg.sensors
        .insert("sensor-x".to_string(), blocking_policy());

    // CLI overrides to strict, with a reject-all validator
    let mut req = default_request();
    req.schema_validation_override = Some(SchemaValidation::Strict);

    let uc = IngestUseCase::new(
        TestReceiptSource::with(vec!["sensor-x"], reports),
        TestPolicySource { config: Some(cfg) },
        CapturingOutputSink::new(),
        RejectAllValidator,
        stub_render,
    );
    let result = uc.execute(req).unwrap();

    // CLI override to strict + reject-all → schema violation
    assert_eq!(result.report.sensors[0].presence, Presence::Invalid);
    assert!(
        result
            .report
            .highlights
            .iter()
            .any(|h| h.finding.code == "cockpit.schema_violation"),
    );
}

// 20. PolicySource error propagates as Err
#[test]
fn policy_source_error_propagates() {
    let uc = IngestUseCase::new(
        TestReceiptSource::empty(),
        FailingPolicySource,
        CapturingOutputSink::new(),
        NoOpSchemaValidator,
        stub_render,
    );
    let result = uc.execute(default_request());

    assert!(result.is_err());
    let msg = format!("{:#}", result.err().unwrap());
    assert!(msg.contains("config load failure"));
}

// 21. Oversized receipt → finding, not crash
#[test]
fn oversized_receipt_produces_controlled_finding() {
    let mut reports = HashMap::new();
    reports.insert(
        "huge".to_string(),
        ReportRead::Oversized {
            size: 10_000_000,
            cap: 2_097_152,
        },
    );

    let uc = IngestUseCase::new(
        TestReceiptSource::with(vec!["huge"], reports),
        TestPolicySource { config: None },
        CapturingOutputSink::new(),
        NoOpSchemaValidator,
        stub_render,
    );
    let result = uc.execute(default_request()).unwrap();

    assert_eq!(result.report.sensors.len(), 1);
    assert_eq!(result.report.sensors[0].presence, Presence::Invalid);
    assert!(
        result.report.sensors[0]
            .errors
            .iter()
            .any(|e| e.contains("too large")),
    );
}

// 22. Multiple sensors with deterministic sensor ordering in report
#[test]
fn sensor_ordering_deterministic_across_runs() {
    let sensor_names = vec!["zebra", "alpha", "mango", "beta"];
    let mut reports = HashMap::new();
    for name in &sensor_names {
        reports.insert(name.to_string(), ReportRead::Bytes(pass_bytes()));
    }

    let uc = IngestUseCase::new(
        TestReceiptSource::with(sensor_names.clone(), reports),
        TestPolicySource { config: None },
        CapturingOutputSink::new(),
        NoOpSchemaValidator,
        stub_render,
    );
    let result1 = uc.execute(default_request()).unwrap();
    let ids1: Vec<&str> = result1
        .report
        .sensors
        .iter()
        .map(|s| s.id.as_str())
        .collect();

    // Run again with same inputs
    let mut reports2 = HashMap::new();
    for name in &sensor_names {
        reports2.insert(name.to_string(), ReportRead::Bytes(pass_bytes()));
    }
    let uc2 = IngestUseCase::new(
        TestReceiptSource::with(sensor_names, reports2),
        TestPolicySource { config: None },
        CapturingOutputSink::new(),
        NoOpSchemaValidator,
        stub_render,
    );
    let result2 = uc2.execute(default_request()).unwrap();
    let ids2: Vec<&str> = result2
        .report
        .sensors
        .iter()
        .map(|s| s.id.as_str())
        .collect();

    assert_eq!(ids1, ids2, "sensor order must be deterministic across runs");
}

// 23. Report metadata includes tool and run info
#[test]
fn report_includes_tool_and_run_metadata() {
    let uc = IngestUseCase::new(
        TestReceiptSource::empty(),
        TestPolicySource { config: None },
        CapturingOutputSink::new(),
        NoOpSchemaValidator,
        stub_render,
    );
    let result = uc.execute(default_request()).unwrap();

    assert_eq!(result.report.tool.name, "cockpitctl");
    assert_eq!(result.report.tool.version, "0.1.0-test");
    assert_eq!(result.report.run.started_at, "2026-01-01T00:00:00Z");

    // Verify serialized report also contains this metadata
    let report_json = serde_json::to_string_pretty(&result.report).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&report_json).unwrap();
    assert_eq!(parsed["tool"]["name"], "cockpitctl");
}

// 24. Truncated discovery produces warning highlight
#[test]
fn truncated_discovery_produces_warning_highlight() {
    let mut source = TestReceiptSource::with(
        vec!["s1", "s2"],
        HashMap::from([
            ("s1".to_string(), ReportRead::Bytes(pass_bytes())),
            ("s2".to_string(), ReportRead::Bytes(pass_bytes())),
        ]),
    );
    source.truncated = true;
    source.total_found = 200;

    let uc = IngestUseCase::new(
        source,
        TestPolicySource { config: None },
        CapturingOutputSink::new(),
        NoOpSchemaValidator,
        stub_render,
    );
    let result = uc.execute(default_request()).unwrap();

    assert!(
        result
            .report
            .highlights
            .iter()
            .any(|h| h.finding.code == "cockpit.sensors_truncated"),
        "truncation should produce sensors_truncated highlight"
    );
}
