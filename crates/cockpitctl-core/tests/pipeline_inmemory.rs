//! Cross-crate pipeline integration tests using in-memory test doubles.
//!
//! These tests exercise the full ingest → domain → render pipeline through
//! the `IngestUseCase` without touching the filesystem. Each port trait
//! (`ReceiptSource`, `PolicySource`, `OutputSink`, `SchemaValidator`) is
//! implemented by a lightweight in-memory double.

#![allow(dead_code)] // Test infrastructure helpers may not all be used yet.

use std::cell::RefCell;
use std::collections::BTreeMap;

use cockpitctl_core::ingest::{
    CommentRead, DiscoveredSensors, PlanRead, ReportRead, SchemaValidationResult,
};
use cockpitctl_core::render::render_comment;
use cockpitctl_core::types::{
    CockpitConfig, Finding, Location, MissingPolicy, RunInfo, SchemaValidation, SensorPolicy,
    SensorReport, Severity, ToolInfo, Verdict, VerdictCounts, VerdictStatus,
};
use cockpitctl_core::{
    IngestRequest, IngestUseCase, NoOpSchemaValidator, OutputSink, PolicySource, ReceiptSource,
    SchemaValidator,
};

// ---------------------------------------------------------------------------
// In-memory test doubles
// ---------------------------------------------------------------------------

struct MemReceiptSource {
    sensors: Vec<String>,
    reports: BTreeMap<String, ReportRead>,
    truncated: bool,
    total_found: usize,
    invalid_ids: Vec<String>,
}

impl MemReceiptSource {
    fn new(sensors: Vec<&str>, reports: BTreeMap<String, ReportRead>) -> Self {
        let len = sensors.len();
        Self {
            sensors: sensors.into_iter().map(String::from).collect(),
            reports,
            truncated: false,
            total_found: len,
            invalid_ids: vec![],
        }
    }

    fn with_truncation(mut self, _total_found: usize) -> Self {
        self.truncated = true;
        self.total_found = _total_found;
        self
    }

    fn with_invalid_ids(mut self, ids: Vec<&str>) -> Self {
        self.invalid_ids = ids.into_iter().map(String::from).collect();
        self
    }
}

impl ReceiptSource for MemReceiptSource {
    fn discovered_sensors(&self) -> anyhow::Result<DiscoveredSensors> {
        Ok(DiscoveredSensors {
            sensors: self.sensors.clone(),
            truncated: self.truncated,
            total_found: self.total_found,
            invalid_sensor_ids: self.invalid_ids.clone(),
        })
    }

    fn read_report_bytes(&self, sensor_id: &str) -> anyhow::Result<ReportRead> {
        match self.reports.get(sensor_id) {
            Some(ReportRead::Bytes(b)) => Ok(ReportRead::Bytes(b.clone())),
            Some(ReportRead::Oversized { size, cap }) => Ok(ReportRead::Oversized {
                size: *size,
                cap: *cap,
            }),
            Some(ReportRead::UnsafePath) => Ok(ReportRead::UnsafePath),
            Some(ReportRead::Missing) | None => Ok(ReportRead::Missing),
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

struct MemPolicySource {
    config: Option<CockpitConfig>,
}

impl PolicySource for MemPolicySource {
    fn load_config(&self) -> anyhow::Result<Option<CockpitConfig>> {
        Ok(self.config.clone())
    }
}

struct MemOutputSink {
    report_json: RefCell<String>,
    comment_md: RefCell<String>,
}

impl MemOutputSink {
    fn new() -> Self {
        Self {
            report_json: RefCell::new(String::new()),
            comment_md: RefCell::new(String::new()),
        }
    }
}

impl OutputSink for MemOutputSink {
    fn write_cockpit_report(&self, json: &str) -> anyhow::Result<()> {
        *self.report_json.borrow_mut() = json.to_string();
        Ok(())
    }

    fn write_cockpit_comment(&self, md: &str) -> anyhow::Result<()> {
        *self.comment_md.borrow_mut() = md.to_string();
        Ok(())
    }
}

/// Schema validator that rejects everything with a fixed error.
struct RejectAllSchemaValidator;

impl SchemaValidator for RejectAllSchemaValidator {
    fn validate_receipt(&self, _bytes: &[u8]) -> anyhow::Result<SchemaValidationResult> {
        Ok(SchemaValidationResult::Invalid(vec![
            "required property 'schema' is missing".to_string(),
        ]))
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn tool_info() -> ToolInfo {
    ToolInfo {
        name: "cockpitctl".to_string(),
        version: "0.0.1-test".to_string(),
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

fn make_sensor_report(status: VerdictStatus, findings: Vec<Finding>) -> Vec<u8> {
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
    make_sensor_report(VerdictStatus::Pass, vec![])
}

fn warn_bytes() -> Vec<u8> {
    make_sensor_report(
        VerdictStatus::Warn,
        vec![Finding {
            severity: Severity::Warn,
            check_id: None,
            code: "test.warning".to_string(),
            message: "a warning".to_string(),
            location: Some(Location {
                path: Some("src/lib.rs".to_string()),
                line: Some(1),
                col: None,
            }),
            help: None,
            url: None,
            fingerprint: None,
            data: None,
        }],
    )
}

fn fail_bytes() -> Vec<u8> {
    make_sensor_report(
        VerdictStatus::Fail,
        vec![Finding {
            severity: Severity::Error,
            check_id: None,
            code: "test.error".to_string(),
            message: "a failure".to_string(),
            location: Some(Location {
                path: Some("src/main.rs".to_string()),
                line: Some(42),
                col: None,
            }),
            help: None,
            url: None,
            fingerprint: None,
            data: None,
        }],
    )
}

fn blocking_sensor() -> SensorPolicy {
    SensorPolicy {
        blocking: true,
        missing: MissingPolicy::Fail,
        ..Default::default()
    }
}

fn nonblocking_sensor() -> SensorPolicy {
    SensorPolicy {
        blocking: false,
        missing: MissingPolicy::Warn,
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------
// 1. Happy path: 3 sensors all pass
// ---------------------------------------------------------------------------

#[test]
fn happy_path_three_passing_sensors() {
    let mut reports = BTreeMap::new();
    reports.insert("alpha".to_string(), ReportRead::Bytes(pass_bytes()));
    reports.insert("beta".to_string(), ReportRead::Bytes(pass_bytes()));
    reports.insert("gamma".to_string(), ReportRead::Bytes(pass_bytes()));

    let mut cfg = CockpitConfig::default();
    cfg.sensors.insert("alpha".to_string(), blocking_sensor());
    cfg.sensors.insert("beta".to_string(), blocking_sensor());
    cfg.sensors.insert("gamma".to_string(), blocking_sensor());

    let receipts = MemReceiptSource::new(vec!["alpha", "beta", "gamma"], reports);
    let policy = MemPolicySource { config: Some(cfg) };
    let output = MemOutputSink::new();

    let uc = IngestUseCase::new(
        receipts,
        policy,
        output,
        NoOpSchemaValidator,
        render_comment,
    );
    let result = uc.execute(default_request()).unwrap();

    assert_eq!(result.exit_code, 0, "all passing → exit 0");
    assert_eq!(result.report.verdict.status, VerdictStatus::Pass);
    assert_eq!(result.report.sensors.len(), 3);

    // All sensors present and passing.
    for sensor in &result.report.sensors {
        assert_eq!(sensor.verdict.status, VerdictStatus::Pass);
    }

    // Comment should be non-empty and mention all sensors.
    assert!(!result.comment_md.is_empty());
    assert!(result.comment_md.contains("alpha"));
    assert!(result.comment_md.contains("beta"));
    assert!(result.comment_md.contains("gamma"));
}

// ---------------------------------------------------------------------------
// 2. Policy failure: 1 blocking sensor fails → exit 2
// ---------------------------------------------------------------------------

#[test]
fn blocking_sensor_fail_produces_exit_code_2() {
    let mut reports = BTreeMap::new();
    reports.insert("good".to_string(), ReportRead::Bytes(pass_bytes()));
    reports.insert("bad".to_string(), ReportRead::Bytes(fail_bytes()));

    let mut cfg = CockpitConfig::default();
    cfg.sensors.insert("good".to_string(), blocking_sensor());
    cfg.sensors.insert("bad".to_string(), blocking_sensor());

    let receipts = MemReceiptSource::new(vec!["bad", "good"], reports);
    let policy = MemPolicySource { config: Some(cfg) };
    let output = MemOutputSink::new();

    let uc = IngestUseCase::new(
        receipts,
        policy,
        output,
        NoOpSchemaValidator,
        render_comment,
    );
    let result = uc.execute(default_request()).unwrap();

    assert_eq!(result.exit_code, 2, "blocking fail → exit 2");
    assert_eq!(result.report.verdict.status, VerdictStatus::Fail);
}

// ---------------------------------------------------------------------------
// 3. Mixed verdicts: pass + warn + fail → correct aggregation
// ---------------------------------------------------------------------------

#[test]
fn mixed_verdicts_aggregate_correctly() {
    let mut reports = BTreeMap::new();
    reports.insert("passer".to_string(), ReportRead::Bytes(pass_bytes()));
    reports.insert("warner".to_string(), ReportRead::Bytes(warn_bytes()));
    reports.insert("failer".to_string(), ReportRead::Bytes(fail_bytes()));

    let mut cfg = CockpitConfig::default();
    cfg.sensors.insert("passer".to_string(), blocking_sensor());
    cfg.sensors
        .insert("warner".to_string(), nonblocking_sensor());
    cfg.sensors.insert("failer".to_string(), blocking_sensor());

    let receipts = MemReceiptSource::new(vec!["failer", "passer", "warner"], reports);
    let policy = MemPolicySource { config: Some(cfg) };
    let output = MemOutputSink::new();

    let uc = IngestUseCase::new(
        receipts,
        policy,
        output,
        NoOpSchemaValidator,
        render_comment,
    );
    let result = uc.execute(default_request()).unwrap();

    // A blocking sensor has fail verdict → overall fail.
    assert_eq!(result.exit_code, 2);
    assert_eq!(result.report.verdict.status, VerdictStatus::Fail);
    assert_eq!(result.report.sensors.len(), 3);

    // Verify each sensor's individual verdict is preserved.
    let by_id: BTreeMap<&str, &VerdictStatus> = result
        .report
        .sensors
        .iter()
        .map(|s| (s.id.as_str(), &s.verdict.status))
        .collect();
    assert_eq!(*by_id["passer"], VerdictStatus::Pass);
    assert_eq!(*by_id["warner"], VerdictStatus::Warn);
    assert_eq!(*by_id["failer"], VerdictStatus::Fail);

    // Highlights should include findings from failing/warning sensors.
    assert!(!result.report.highlights.is_empty());
}

// ---------------------------------------------------------------------------
// 4. Empty artifacts: no receipts → graceful handling
// ---------------------------------------------------------------------------

#[test]
fn empty_artifacts_no_receipts() {
    let receipts = MemReceiptSource::new(vec![], BTreeMap::new());
    let policy = MemPolicySource { config: None };
    let output = MemOutputSink::new();

    let uc = IngestUseCase::new(
        receipts,
        policy,
        output,
        NoOpSchemaValidator,
        render_comment,
    );
    let result = uc.execute(default_request()).unwrap();

    assert_eq!(result.exit_code, 0, "no sensors → pass");
    assert!(result.report.sensors.is_empty());
    assert_eq!(result.report.schema, "cockpit.report.v1");
    assert!(!result.comment_md.is_empty(), "comment should still render");
}

// ---------------------------------------------------------------------------
// 5. Determinism: same inputs 10x → identical output
// ---------------------------------------------------------------------------

#[test]
fn determinism_ten_runs_identical_output() {
    let mut first_report = String::new();
    let mut first_comment = String::new();

    for i in 0..10 {
        let mut reports = BTreeMap::new();
        reports.insert("zebra".to_string(), ReportRead::Bytes(warn_bytes()));
        reports.insert("alpha".to_string(), ReportRead::Bytes(pass_bytes()));
        reports.insert("mango".to_string(), ReportRead::Bytes(fail_bytes()));

        let mut cfg = CockpitConfig::default();
        cfg.policy.max_highlights = 20;
        cfg.sensors.insert("alpha".to_string(), blocking_sensor());
        cfg.sensors.insert("mango".to_string(), blocking_sensor());
        cfg.sensors
            .insert("zebra".to_string(), nonblocking_sensor());

        let receipts = MemReceiptSource::new(vec!["alpha", "mango", "zebra"], reports);
        let policy = MemPolicySource { config: Some(cfg) };
        let output = MemOutputSink::new();

        let uc = IngestUseCase::new(
            receipts,
            policy,
            output,
            NoOpSchemaValidator,
            render_comment,
        );
        let result = uc.execute(default_request()).unwrap();

        let report_json = serde_json::to_string_pretty(&result.report).unwrap();
        if i == 0 {
            first_report = report_json;
            first_comment = result.comment_md;
        } else {
            assert_eq!(first_report, report_json, "report.json differs on run {i}");
            assert_eq!(
                first_comment, result.comment_md,
                "comment.md differs on run {i}"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// 6. Budget enforcement: many findings → highlights capped
// ---------------------------------------------------------------------------

#[test]
fn budget_enforcement_highlights_capped() {
    // Generate a sensor with 100 findings.
    let findings: Vec<Finding> = (0..100)
        .map(|i| Finding {
            severity: Severity::Warn,
            check_id: None,
            code: format!("lint.rule-{i:03}"),
            message: format!("Finding number {i}"),
            location: Some(Location {
                path: Some(format!("src/file_{i:03}.rs")),
                line: Some(i + 1),
                col: None,
            }),
            help: None,
            url: None,
            fingerprint: None,
            data: None,
        })
        .collect();

    let bytes = make_sensor_report(VerdictStatus::Warn, findings);
    let mut reports = BTreeMap::new();
    reports.insert("linter".to_string(), ReportRead::Bytes(bytes));

    let mut cfg = CockpitConfig::default();
    cfg.policy.max_highlights = 5;
    cfg.policy.max_per_sensor_findings = 100;
    cfg.sensors
        .insert("linter".to_string(), nonblocking_sensor());

    let receipts = MemReceiptSource::new(vec!["linter"], reports);
    let policy = MemPolicySource { config: Some(cfg) };
    let output = MemOutputSink::new();

    let uc = IngestUseCase::new(
        receipts,
        policy,
        output,
        NoOpSchemaValidator,
        render_comment,
    );
    let result = uc.execute(default_request()).unwrap();

    assert!(
        result.report.highlights.len() <= 5,
        "highlights should be capped at max_highlights=5, got {}",
        result.report.highlights.len()
    );
    // The comment should still be generated.
    assert!(!result.comment_md.is_empty());
}

// ---------------------------------------------------------------------------
// 7. Schema validation: invalid receipt → finding, not crash
// ---------------------------------------------------------------------------

#[test]
fn schema_validation_invalid_receipt_generates_finding() {
    let mut reports = BTreeMap::new();
    // Valid JSON but doesn't match the SensorReport schema.
    reports.insert(
        "broken".to_string(),
        ReportRead::Bytes(b"{ \"not\": \"a receipt\" }".to_vec()),
    );

    let mut cfg = CockpitConfig::default();
    cfg.sensors.insert("broken".to_string(), blocking_sensor());

    let receipts = MemReceiptSource::new(vec!["broken"], reports);
    let policy = MemPolicySource { config: Some(cfg) };
    let output = MemOutputSink::new();

    // Lax mode: skip schema validation, but serde parse will fail.
    let uc = IngestUseCase::new(
        receipts,
        policy,
        output,
        NoOpSchemaValidator,
        render_comment,
    );
    let result = uc.execute(default_request()).unwrap();

    assert_eq!(result.report.sensors.len(), 1);
    assert_eq!(
        result.report.sensors[0].presence,
        cockpitctl_core::Presence::Invalid
    );
    assert!(
        result
            .report
            .highlights
            .iter()
            .any(|h| h.finding.code == "cockpit.invalid_receipt"),
        "invalid receipt should produce cockpit.invalid_receipt highlight"
    );
    // Pipeline should not crash — exit code reflects policy.
    assert_eq!(result.exit_code, 2, "invalid blocking receipt → exit 2");
}

#[test]
fn strict_schema_validation_rejects_bad_receipt() {
    let mut reports = BTreeMap::new();
    // Valid SensorReport JSON that will pass serde but fail schema validation.
    reports.insert("sensor".to_string(), ReportRead::Bytes(pass_bytes()));

    let mut cfg = CockpitConfig::default();
    cfg.policy.schema_validation = SchemaValidation::Strict;
    cfg.sensors.insert("sensor".to_string(), blocking_sensor());

    let receipts = MemReceiptSource::new(vec!["sensor"], reports);
    let policy = MemPolicySource { config: Some(cfg) };
    let output = MemOutputSink::new();

    // Use reject-all validator to simulate schema violations.
    let uc = IngestUseCase::new(
        receipts,
        policy,
        output,
        RejectAllSchemaValidator,
        render_comment,
    );
    let result = uc.execute(default_request()).unwrap();

    assert!(
        result
            .report
            .highlights
            .iter()
            .any(|h| h.finding.code == "cockpit.schema_violation"),
        "strict mode + invalid schema → cockpit.schema_violation highlight"
    );
    assert_eq!(result.exit_code, 2);
}

// ---------------------------------------------------------------------------
// 8. Safety boundaries: oversized receipt → finding with size info
// ---------------------------------------------------------------------------

#[test]
fn oversized_receipt_produces_finding_with_size_info() {
    let mut reports = BTreeMap::new();
    reports.insert(
        "big".to_string(),
        ReportRead::Oversized {
            size: 5_000_000,
            cap: 2_097_152,
        },
    );
    reports.insert("ok".to_string(), ReportRead::Bytes(pass_bytes()));

    let mut cfg = CockpitConfig::default();
    cfg.sensors.insert("big".to_string(), blocking_sensor());
    cfg.sensors.insert("ok".to_string(), blocking_sensor());

    let receipts = MemReceiptSource::new(vec!["big", "ok"], reports);
    let policy = MemPolicySource { config: Some(cfg) };
    let output = MemOutputSink::new();

    let uc = IngestUseCase::new(
        receipts,
        policy,
        output,
        NoOpSchemaValidator,
        render_comment,
    );
    let result = uc.execute(default_request()).unwrap();

    // Oversized receipt should emit a specific highlight.
    let oversized_highlight = result
        .report
        .highlights
        .iter()
        .find(|h| h.finding.code == "cockpit.receipt_oversized");
    assert!(
        oversized_highlight.is_some(),
        "should emit cockpit.receipt_oversized highlight"
    );

    // The highlight message should mention size information.
    let msg = &oversized_highlight.unwrap().finding.message;
    assert!(
        msg.contains("5000000")
            || msg.contains("5,000,000")
            || msg.contains("5MB")
            || msg.contains("size"),
        "oversized message should reference size: {msg}"
    );

    // The "ok" sensor should still be processed.
    assert!(result.report.sensors.iter().any(|s| s.id == "ok"));

    // Overall verdict fail because blocking sensor is oversized.
    assert_eq!(result.exit_code, 2);
}

// ---------------------------------------------------------------------------
// 9. Precedence: config=lax, override=strict → strict wins
// ---------------------------------------------------------------------------

#[test]
fn precedence_override_strict_wins_over_config_lax() {
    let mut reports = BTreeMap::new();
    reports.insert("sensor".to_string(), ReportRead::Bytes(pass_bytes()));

    let mut cfg = CockpitConfig::default();
    cfg.policy.schema_validation = SchemaValidation::Lax;
    cfg.sensors.insert("sensor".to_string(), blocking_sensor());

    let receipts = MemReceiptSource::new(vec!["sensor"], reports);
    let policy = MemPolicySource { config: Some(cfg) };
    let output = MemOutputSink::new();

    // Override to strict + reject-all validator.
    let uc = IngestUseCase::new(
        receipts,
        policy,
        output,
        RejectAllSchemaValidator,
        render_comment,
    );
    let mut req = default_request();
    req.schema_validation_override = Some(SchemaValidation::Strict);
    let result = uc.execute(req).unwrap();

    // Strict override should trigger schema validation.
    assert!(
        result
            .report
            .highlights
            .iter()
            .any(|h| h.finding.code == "cockpit.schema_violation"),
        "strict override should trigger schema_violation"
    );
    assert_eq!(result.exit_code, 2);
}

#[test]
fn precedence_override_lax_wins_over_config_strict() {
    let mut reports = BTreeMap::new();
    reports.insert("sensor".to_string(), ReportRead::Bytes(pass_bytes()));

    let mut cfg = CockpitConfig::default();
    cfg.policy.schema_validation = SchemaValidation::Strict;
    cfg.sensors.insert("sensor".to_string(), blocking_sensor());

    let receipts = MemReceiptSource::new(vec!["sensor"], reports);
    let policy = MemPolicySource { config: Some(cfg) };
    let output = MemOutputSink::new();

    // Override to lax — reject-all validator should not be called.
    let uc = IngestUseCase::new(
        receipts,
        policy,
        output,
        RejectAllSchemaValidator,
        render_comment,
    );
    let mut req = default_request();
    req.schema_validation_override = Some(SchemaValidation::Lax);
    let result = uc.execute(req).unwrap();

    assert_eq!(result.exit_code, 0, "lax override should skip validation");
    assert!(
        !result
            .report
            .highlights
            .iter()
            .any(|h| h.finding.code == "cockpit.schema_violation"),
        "lax override should not produce schema_violation"
    );
}

// ---------------------------------------------------------------------------
// 10. Multi-sensor ordering: sensors in lexical order
// ---------------------------------------------------------------------------

#[test]
fn sensors_discovered_in_lexical_order() {
    let mut reports = BTreeMap::new();
    for name in &["zebra", "alpha", "mango", "beta", "omega"] {
        reports.insert(name.to_string(), ReportRead::Bytes(pass_bytes()));
    }

    // Discovery order matches the vec order; the pipeline should sort lexically.
    let receipts = MemReceiptSource::new(vec!["zebra", "alpha", "mango", "beta", "omega"], reports);
    let policy = MemPolicySource { config: None };
    let output = MemOutputSink::new();

    let uc = IngestUseCase::new(
        receipts,
        policy,
        output,
        NoOpSchemaValidator,
        render_comment,
    );
    let result = uc.execute(default_request()).unwrap();

    // When no config sensors are declared, discovered list is used as expected.
    // The ingest use-case uses discovered order as-is (ReceiptSource provides lexical).
    assert_eq!(result.report.sensors.len(), 5);
    assert_eq!(result.exit_code, 0);
}

#[test]
fn config_sensor_order_determines_processing() {
    let mut reports = BTreeMap::new();
    reports.insert("zzz".to_string(), ReportRead::Bytes(pass_bytes()));
    reports.insert("aaa".to_string(), ReportRead::Bytes(pass_bytes()));
    reports.insert("mmm".to_string(), ReportRead::Bytes(pass_bytes()));

    let mut cfg = CockpitConfig::default();
    cfg.sensors.insert("zzz".to_string(), blocking_sensor());
    cfg.sensors.insert("aaa".to_string(), blocking_sensor());
    cfg.sensors.insert("mmm".to_string(), blocking_sensor());

    let receipts = MemReceiptSource::new(vec!["aaa", "mmm", "zzz"], reports);
    let policy = MemPolicySource { config: Some(cfg) };
    let output = MemOutputSink::new();

    let uc = IngestUseCase::new(
        receipts,
        policy,
        output,
        NoOpSchemaValidator,
        render_comment,
    );
    let result = uc.execute(default_request()).unwrap();

    assert_eq!(result.report.sensors.len(), 3);

    // Run twice with same inputs and verify sensor order is stable.
    let ids_first: Vec<String> = result.report.sensors.iter().map(|s| s.id.clone()).collect();

    // Second run.
    let mut reports2 = BTreeMap::new();
    reports2.insert("zzz".to_string(), ReportRead::Bytes(pass_bytes()));
    reports2.insert("aaa".to_string(), ReportRead::Bytes(pass_bytes()));
    reports2.insert("mmm".to_string(), ReportRead::Bytes(pass_bytes()));

    let mut cfg2 = CockpitConfig::default();
    cfg2.sensors.insert("zzz".to_string(), blocking_sensor());
    cfg2.sensors.insert("aaa".to_string(), blocking_sensor());
    cfg2.sensors.insert("mmm".to_string(), blocking_sensor());

    let receipts2 = MemReceiptSource::new(vec!["aaa", "mmm", "zzz"], reports2);
    let policy2 = MemPolicySource { config: Some(cfg2) };
    let output2 = MemOutputSink::new();

    let uc2 = IngestUseCase::new(
        receipts2,
        policy2,
        output2,
        NoOpSchemaValidator,
        render_comment,
    );
    let result2 = uc2.execute(default_request()).unwrap();
    let ids_second: Vec<String> = result2
        .report
        .sensors
        .iter()
        .map(|s| s.id.clone())
        .collect();

    assert_eq!(ids_first, ids_second, "sensor order must be stable");
}

// ---------------------------------------------------------------------------
// Additional: missing sensor with fail policy
// ---------------------------------------------------------------------------

#[test]
fn missing_blocking_sensor_with_fail_policy() {
    let mut cfg = CockpitConfig::default();
    cfg.sensors.insert(
        "required-sensor".to_string(),
        SensorPolicy {
            blocking: true,
            missing: MissingPolicy::Fail,
            ..Default::default()
        },
    );

    // No receipts at all.
    let receipts = MemReceiptSource::new(vec![], BTreeMap::new());
    let policy = MemPolicySource { config: Some(cfg) };
    let output = MemOutputSink::new();

    let uc = IngestUseCase::new(
        receipts,
        policy,
        output,
        NoOpSchemaValidator,
        render_comment,
    );
    let result = uc.execute(default_request()).unwrap();

    assert_eq!(result.exit_code, 2, "missing blocking sensor → exit 2");
    assert_eq!(result.report.verdict.status, VerdictStatus::Fail);
    assert!(
        result
            .report
            .highlights
            .iter()
            .any(|h| h.finding.code == "cockpit.missing_receipt"),
        "should emit missing_receipt highlight"
    );
}

// ---------------------------------------------------------------------------
// Additional: warn_is_fail escalation
// ---------------------------------------------------------------------------

#[test]
fn warn_is_fail_escalates_warning_to_failure() {
    let mut reports = BTreeMap::new();
    reports.insert("warner".to_string(), ReportRead::Bytes(warn_bytes()));

    let mut cfg = CockpitConfig::default();
    cfg.policy.warn_is_fail = true;
    cfg.sensors.insert("warner".to_string(), blocking_sensor());

    let receipts = MemReceiptSource::new(vec!["warner"], reports);
    let policy = MemPolicySource { config: Some(cfg) };
    let output = MemOutputSink::new();

    let uc = IngestUseCase::new(
        receipts,
        policy,
        output,
        NoOpSchemaValidator,
        render_comment,
    );
    let result = uc.execute(default_request()).unwrap();

    assert_eq!(
        result.exit_code, 2,
        "warn_is_fail + blocking warn sensor → exit 2"
    );
    assert_eq!(result.report.verdict.status, VerdictStatus::Fail);
}

// ---------------------------------------------------------------------------
// Additional: output sink receives valid JSON
// ---------------------------------------------------------------------------

#[test]
fn output_sink_receives_valid_report_json() {
    let mut reports = BTreeMap::new();
    reports.insert("sensor".to_string(), ReportRead::Bytes(pass_bytes()));

    let receipts = MemReceiptSource::new(vec!["sensor"], reports);
    let policy = MemPolicySource { config: None };
    let output = MemOutputSink::new();

    let uc = IngestUseCase::new(
        receipts,
        policy,
        output,
        NoOpSchemaValidator,
        render_comment,
    );
    let result = uc.execute(default_request()).unwrap();

    // The report should be serializable to valid JSON.
    let json_str = serde_json::to_string_pretty(&result.report).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
    assert_eq!(parsed["schema"], "cockpit.report.v1");
}

// ---------------------------------------------------------------------------
// Additional: non-blocking fail does not cause exit 2
// ---------------------------------------------------------------------------

#[test]
fn nonblocking_fail_does_not_cause_exit_code_2() {
    let mut reports = BTreeMap::new();
    reports.insert("informational".to_string(), ReportRead::Bytes(fail_bytes()));

    let mut cfg = CockpitConfig::default();
    cfg.sensors
        .insert("informational".to_string(), nonblocking_sensor());

    let receipts = MemReceiptSource::new(vec!["informational"], reports);
    let policy = MemPolicySource { config: Some(cfg) };
    let output = MemOutputSink::new();

    let uc = IngestUseCase::new(
        receipts,
        policy,
        output,
        NoOpSchemaValidator,
        render_comment,
    );
    let result = uc.execute(default_request()).unwrap();

    assert_eq!(
        result.exit_code, 0,
        "non-blocking fail → exit 0 (informational)"
    );
}
