//! Facade integration tests for `cockpitctl-core`.
//!
//! Verifies that every reexported microcrate is fully accessible through
//! `cockpitctl_core`, that trait objects can be formed from the port traits,
//! and that a complete ingest pipeline can be driven using only facade imports.

use std::cell::RefCell;
use std::collections::BTreeMap;

use cockpitctl_core::ingest::{
    CommentRead, DiscoveredSensors, PlanRead, ReportRead, SchemaValidationResult,
};
use cockpitctl_core::types::{MissingPolicy, RunInfo, SensorPolicy, ToolInfo, VerdictStatus};
use cockpitctl_core::{
    CockpitConfig, IngestRequest, IngestResult, IngestUseCase, NoOpSchemaValidator, OutputSink,
    PolicySource, ReceiptSource, SchemaValidator,
};

// ---------------------------------------------------------------------------
// In-memory test doubles (using only facade imports)
// ---------------------------------------------------------------------------

struct MemReceipts {
    sensors: Vec<String>,
    reports: BTreeMap<String, Vec<u8>>,
}

impl ReceiptSource for MemReceipts {
    fn discovered_sensors(&self) -> anyhow::Result<DiscoveredSensors> {
        let len = self.sensors.len();
        Ok(DiscoveredSensors {
            sensors: self.sensors.clone(),
            truncated: false,
            total_found: len,
            invalid_sensor_ids: vec![],
        })
    }

    fn read_report_bytes(&self, sensor_id: &str) -> anyhow::Result<ReportRead> {
        Ok(match self.reports.get(sensor_id) {
            Some(bytes) => ReportRead::Bytes(bytes.clone()),
            None => ReportRead::Missing,
        })
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

struct MemPolicy {
    config: Option<CockpitConfig>,
}

impl PolicySource for MemPolicy {
    fn load_config(&self) -> anyhow::Result<Option<CockpitConfig>> {
        Ok(self.config.clone())
    }
}

struct MemSink {
    report_json: RefCell<String>,
    comment_md: RefCell<String>,
}

impl MemSink {
    fn new() -> Self {
        Self {
            report_json: RefCell::new(String::new()),
            comment_md: RefCell::new(String::new()),
        }
    }
}

impl OutputSink for MemSink {
    fn write_cockpit_report(&self, json: &str) -> anyhow::Result<()> {
        *self.report_json.borrow_mut() = json.to_string();
        Ok(())
    }
    fn write_cockpit_comment(&self, md: &str) -> anyhow::Result<()> {
        *self.comment_md.borrow_mut() = md.to_string();
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn tool_info() -> ToolInfo {
    ToolInfo {
        name: "facade-test".to_string(),
        version: "0.0.1".to_string(),
        commit: None,
    }
}

fn run_info() -> RunInfo {
    RunInfo {
        started_at: "2026-06-01T00:00:00Z".to_string(),
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

fn minimal_receipt_bytes(sensor_name: &str, status: &str) -> Vec<u8> {
    serde_json::json!({
        "schema": "sensor.report.v1",
        "tool": { "name": sensor_name, "version": "1.0.0" },
        "run":  { "started_at": "2026-06-01T00:00:00Z" },
        "verdict": {
            "status": status,
            "counts": { "info": 0, "warn": 0, "error": 0 },
            "reasons": []
        },
        "findings": []
    })
    .to_string()
    .into_bytes()
}

fn receipt_with_findings_bytes(sensor_name: &str) -> Vec<u8> {
    serde_json::json!({
        "schema": "sensor.report.v1",
        "tool": { "name": sensor_name, "version": "1.0.0" },
        "run":  { "started_at": "2026-06-01T00:00:00Z" },
        "verdict": {
            "status": "warn",
            "counts": { "info": 1, "warn": 1, "error": 1 },
            "reasons": []
        },
        "findings": [
            {
                "severity": "info",
                "code": "test.note",
                "message": "Informational",
                "location": { "path": "src/a.rs", "line": 1 }
            },
            {
                "severity": "warn",
                "code": "test.warning",
                "message": "Warning found",
                "location": { "path": "src/b.rs", "line": 10 }
            },
            {
                "severity": "error",
                "code": "test.error",
                "message": "Error found",
                "location": { "path": "src/c.rs", "line": 20 }
            }
        ]
    })
    .to_string()
    .into_bytes()
}

fn run_inmemory_pipeline(
    sensors: Vec<&str>,
    reports: BTreeMap<String, Vec<u8>>,
    config: Option<CockpitConfig>,
) -> IngestResult {
    let receipts = MemReceipts {
        sensors: sensors.into_iter().map(String::from).collect(),
        reports,
    };
    let policy = MemPolicy { config };
    let sink = MemSink::new();
    let uc = IngestUseCase::new(
        receipts,
        policy,
        sink,
        NoOpSchemaValidator,
        cockpitctl_core::render_comment,
    );
    uc.execute(default_request())
        .expect("pipeline should succeed")
}

// ===========================================================================
// 1. Types reexported — VerdictStatus, Finding, SensorReport via core
// ===========================================================================

#[test]
fn types_reexported_verdict_finding_sensor_report() {
    // VerdictStatus variants
    let _ = cockpitctl_core::VerdictStatus::Pass;

    // Finding via types module
    let finding = cockpitctl_core::types::Finding {
        severity: cockpitctl_core::types::Severity::Warn,
        check_id: None,
        code: "t.code".to_string(),
        message: "msg".to_string(),
        location: Some(cockpitctl_core::types::Location {
            path: Some("a.rs".into()),
            line: Some(1),
            col: None,
        }),
        help: None,
        url: None,
        fingerprint: None,
        data: None,
    };
    assert_eq!(finding.code, "t.code");

    // SensorReport via facade
    let sr: cockpitctl_core::SensorReport = serde_json::from_str(
        &serde_json::json!({
            "schema": "sensor.report.v1",
            "tool": { "name": "x", "version": "1" },
            "run": { "started_at": "2026-01-01T00:00:00Z" },
            "verdict": { "status": "pass", "counts": { "info": 0, "warn": 0, "error": 0 } },
            "findings": []
        })
        .to_string(),
    )
    .unwrap();
    assert_eq!(sr.schema, "sensor.report.v1");
}

// ===========================================================================
// 2. Domain reexported — policy evaluation functions via core
// ===========================================================================

#[test]
fn domain_reexported_policy_evaluation() {
    use cockpitctl_core::domain;

    // snapshot_policy
    let cfg = CockpitConfig::default();
    let snap = domain::snapshot_policy(&cfg);
    assert!(!snap.warn_is_fail);

    // compute_policy_outcome
    let outcome = domain::compute_policy_outcome(true, &VerdictStatus::Fail);
    assert_eq!(outcome, cockpitctl_core::types::PolicyOutcome::Blocked);

    // explain_code
    let explanation = cockpitctl_core::explain_code("cockpit.missing_receipt");
    assert!(explanation.is_some());

    // all_codes
    let codes = cockpitctl_core::all_codes();
    assert!(!codes.is_empty());
}

// ===========================================================================
// 3. Ingest reexported — IngestUseCase via core
// ===========================================================================

#[test]
fn ingest_reexported_use_case() {
    let mut reports = BTreeMap::new();
    reports.insert("alpha".into(), minimal_receipt_bytes("alpha", "pass"));

    let result = run_inmemory_pipeline(vec!["alpha"], reports, None);
    assert_eq!(result.exit_code, 0);
    assert_eq!(result.report.verdict.status, VerdictStatus::Pass);
    assert_eq!(result.report.sensors.len(), 1);
}

// ===========================================================================
// 4. Render reexported — render functions via core
// ===========================================================================

#[test]
fn render_reexported_comment_and_annotations() {
    let mut reports = BTreeMap::new();
    reports.insert("sensor".into(), minimal_receipt_bytes("sensor", "pass"));

    let result = run_inmemory_pipeline(vec!["sensor"], reports, None);

    // render_comment was used inside the pipeline; verify the output
    assert!(result.comment_md.contains("Cockpit"));

    // Also callable directly through the facade
    let md = cockpitctl_core::render_comment(&result.report, &CockpitConfig::default());
    assert!(!md.is_empty());

    // render_github_annotations accessible
    let sensor_blocking: BTreeMap<String, bool> = result
        .report
        .sensors
        .iter()
        .map(|s| (s.id.clone(), s.blocking))
        .collect();
    let annotations = cockpitctl_core::render_github_annotations(
        &result.report.highlights,
        &CockpitConfig::default(),
        &sensor_blocking,
    );
    assert!(annotations.lines.is_empty()); // no findings → no annotations
}

// ===========================================================================
// 5. IO reexported — FsReceiptSource, FsOutputSink via core
// ===========================================================================

#[test]
fn io_reexported_fs_adapters() {
    use cockpitctl_core::io::{FsLayout, FsOutputSink, FsPolicySource, FsReceiptSource};

    // Verify these types are constructable through the facade
    let layout = FsLayout::new("artifacts", "cockpit.toml");
    let _ = FsReceiptSource::new(layout.clone());
    let _ = FsPolicySource::new(layout.clone());
    let _ = FsOutputSink::new(layout);
}

// ===========================================================================
// 6. Domain-trend reexported — compute_trend accessible via core
// ===========================================================================

#[test]
fn domain_trend_reexported_and_usable() {
    // Build two reports via the pipeline (baseline vs current) and compute trend
    let mut reports_base = BTreeMap::new();
    reports_base.insert("s".into(), minimal_receipt_bytes("s", "pass"));
    let base_result = run_inmemory_pipeline(vec!["s"], reports_base, None);

    let mut reports_curr = BTreeMap::new();
    reports_curr.insert("s".into(), receipt_with_findings_bytes("s"));
    let curr_result = run_inmemory_pipeline(vec!["s"], reports_curr, None);

    let trend = cockpitctl_core::compute_trend(&base_result.report, &curr_result.report);
    // Current has new findings that baseline doesn't
    assert!(
        !trend.new_findings.is_empty()
            || trend.count_deltas.error_delta != 0
            || trend.count_deltas.warn_delta != 0,
        "trend should detect differences between baseline and current"
    );
}

// ===========================================================================
// 7. Complete pipeline through facade — build and run using only core imports
// ===========================================================================

#[test]
fn complete_pipeline_through_facade() {
    // Build a multi-sensor pipeline using only cockpitctl_core imports
    let mut cfg = CockpitConfig::default();
    cfg.policy.warn_is_fail = false;
    cfg.policy.max_highlights = 10;
    cfg.sensors.insert(
        "alpha".into(),
        SensorPolicy {
            blocking: true,
            missing: MissingPolicy::Fail,
            ..Default::default()
        },
    );
    cfg.sensors.insert(
        "beta".into(),
        SensorPolicy {
            blocking: false,
            missing: MissingPolicy::Warn,
            ..Default::default()
        },
    );

    let mut reports = BTreeMap::new();
    reports.insert("alpha".into(), receipt_with_findings_bytes("alpha"));
    reports.insert("beta".into(), minimal_receipt_bytes("beta", "pass"));

    let result = run_inmemory_pipeline(vec!["alpha", "beta"], reports, Some(cfg));

    // alpha has findings → warn verdict; beta passes
    assert_eq!(result.report.sensors.len(), 2);
    assert!(
        !result.report.highlights.is_empty(),
        "should have highlights from alpha findings"
    );
    assert!(!result.comment_md.is_empty());

    // Report should be serializable JSON round-trip
    let json = serde_json::to_string(&result.report).unwrap();
    let parsed: cockpitctl_core::CockpitReport = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.schema, "cockpit.report.v1");
    assert_eq!(parsed.sensors.len(), 2);
}

// ===========================================================================
// 8. All verdicts accessible — pass, warn, fail, skip via core
// ===========================================================================

#[test]
fn all_verdicts_accessible_through_core() {
    let cases: Vec<(VerdictStatus, &str, i32)> = vec![
        (VerdictStatus::Pass, "pass", 0),
        (VerdictStatus::Warn, "warn", 0),
        (VerdictStatus::Fail, "fail", 2),
        (VerdictStatus::Skip, "skip", 0),
    ];

    for (status, status_str, expected_exit) in cases {
        let mut reports = BTreeMap::new();
        reports.insert("sensor".into(), minimal_receipt_bytes("sensor", status_str));

        let mut cfg = CockpitConfig::default();
        cfg.sensors.insert(
            "sensor".into(),
            SensorPolicy {
                blocking: true,
                missing: MissingPolicy::Fail,
                ..Default::default()
            },
        );

        let result = run_inmemory_pipeline(vec!["sensor"], reports, Some(cfg));
        assert_eq!(
            result.report.sensors[0].verdict.status, status,
            "sensor verdict should be {status_str}"
        );
        assert_eq!(
            result.exit_code, expected_exit,
            "exit code for {status_str}"
        );
    }
}

// ===========================================================================
// 9. Severity levels accessible — all levels via core
// ===========================================================================

#[test]
fn severity_levels_accessible_through_core() {
    use cockpitctl_core::types::{Severity, severity_rank};

    let info_rank = severity_rank(&Severity::Info);
    let warn_rank = severity_rank(&Severity::Warn);
    let error_rank = severity_rank(&Severity::Error);

    // Error is most severe (lowest rank number)
    assert!(error_rank < warn_rank);
    assert!(warn_rank < info_rank);

    // All three are constructable and comparable
    assert_ne!(Severity::Info, Severity::Warn);
    assert_ne!(Severity::Warn, Severity::Error);
    assert_ne!(Severity::Info, Severity::Error);
}

// ===========================================================================
// 10. Trait objects work — port traits usable as trait objects through core
// ===========================================================================

#[test]
fn trait_objects_for_port_traits() {
    // ReceiptSource as trait object
    let receipts = MemReceipts {
        sensors: vec!["s".into()],
        reports: BTreeMap::new(),
    };
    let receipt_obj: &dyn ReceiptSource = &receipts;
    let discovered = receipt_obj.discovered_sensors().unwrap();
    assert_eq!(discovered.sensors, vec!["s"]);

    // PolicySource as trait object
    let policy = MemPolicy { config: None };
    let policy_obj: &dyn PolicySource = &policy;
    assert!(policy_obj.load_config().unwrap().is_none());

    // OutputSink as trait object
    let sink = MemSink::new();
    let sink_obj: &dyn OutputSink = &sink;
    sink_obj.write_cockpit_report("{}").unwrap();
    sink_obj.write_cockpit_comment("# Comment").unwrap();
    assert_eq!(*sink.report_json.borrow(), "{}");

    // SchemaValidator as trait object
    let validator = NoOpSchemaValidator;
    let validator_obj: &dyn SchemaValidator = &validator;
    let result = validator_obj.validate_receipt(b"{}").unwrap();
    assert!(matches!(result, SchemaValidationResult::Valid));
}

// ===========================================================================
// 11. Feature-gated exports — domain_buildfix, domain_signing, sarif, trend
// ===========================================================================

#[test]
fn feature_gated_exports_present() {
    // domain_buildfix
    let _ = &cockpitctl_core::match_buildfix_plan;
    let _ = &cockpitctl_core::select_auto_apply_fixes;

    // domain_signing
    let _ = &cockpitctl_core::sign_policy_snapshot;
    let _ = &cockpitctl_core::sign_policy_snapshot_hmac_sha256;
    let _ = &cockpitctl_core::policy_snapshot_sha256_hex;

    // sarif
    let _ = &cockpitctl_core::cockpit_report_to_sarif;
    let _ = &cockpitctl_core::cockpit_report_to_sarif_json;

    // trend
    let _ = &cockpitctl_core::compute_trend;

    // feature_state + feature_grid modules
    let _ = std::any::type_name::<cockpitctl_core::feature_state::RuntimeFeatureState>();
    let _ = std::any::type_name::<cockpitctl_core::feature_grid::FeatureGridState>();

    // io sub-crate modules
    let _ = &cockpitctl_core::io::run_buildfix_actuator;
    let _ = std::any::type_name::<cockpitctl_core::io_schema::JsonSchemaValidator>();
}

// ===========================================================================
// 12. No namespace conflicts — all reexports are unambiguous
// ===========================================================================

#[test]
fn no_namespace_conflicts() {
    // Use both flattened and module-namespaced paths for the same types to
    // verify they resolve identically (same type, no ambiguity).

    // Flattened VerdictStatus vs types::VerdictStatus
    let flat: cockpitctl_core::VerdictStatus = cockpitctl_core::VerdictStatus::Pass;
    let namespaced: cockpitctl_core::types::VerdictStatus =
        cockpitctl_core::types::VerdictStatus::Pass;
    assert_eq!(flat, namespaced);

    // Flattened CockpitConfig vs types::CockpitConfig
    let flat_cfg = cockpitctl_core::CockpitConfig::default();
    let ns_cfg = cockpitctl_core::types::CockpitConfig::default();
    assert_eq!(flat_cfg.policy.warn_is_fail, ns_cfg.policy.warn_is_fail);

    // Flattened IngestUseCase type name vs ingest::IngestUseCase type name
    assert_eq!(
        std::any::type_name::<cockpitctl_core::IngestRequest>(),
        std::any::type_name::<cockpitctl_core::ingest::IngestRequest>(),
    );

    // render_comment: both paths should compile
    let _f1: fn(&cockpitctl_core::CockpitReport, &cockpitctl_core::CockpitConfig) -> String =
        cockpitctl_core::render_comment;
    let _f2: fn(&cockpitctl_core::CockpitReport, &cockpitctl_core::CockpitConfig) -> String =
        cockpitctl_core::render::render_comment;
    assert_eq!(_f1 as usize, _f2 as usize);
}

// ===========================================================================
// Bonus: SARIF export through facade
// ===========================================================================

#[test]
fn sarif_export_through_facade() {
    let mut reports = BTreeMap::new();
    reports.insert("linter".into(), receipt_with_findings_bytes("linter"));

    let mut cfg = CockpitConfig::default();
    cfg.sensors.insert(
        "linter".into(),
        SensorPolicy {
            blocking: true,
            missing: MissingPolicy::Fail,
            ..Default::default()
        },
    );

    let result = run_inmemory_pipeline(vec!["linter"], reports, Some(cfg));

    // SARIF conversion should succeed through the facade
    let sarif_json = cockpitctl_core::cockpit_report_to_sarif_json(&result.report).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&sarif_json).unwrap();
    assert_eq!(parsed["version"], "2.1.0");
    assert!(parsed["runs"].is_array());
}
