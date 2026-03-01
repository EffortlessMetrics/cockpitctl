//! Cross-crate contract verification tests.
//!
//! These tests verify that the contracts between crates are consistent:
//! embedded schemas match source-of-truth files, Rust types are compatible
//! with JSON schemas, verdict/severity enums match schema-defined values,
//! and default config values match documented conventions.

use std::collections::BTreeMap;

use cockpitctl_conform::{ConformChecks, conform_single, validate_cockpit_schema};
use cockpitctl_core::ingest::{CommentRead, DiscoveredSensors, PlanRead, ReportRead};
use cockpitctl_core::render::render_comment;
use cockpitctl_core::types::{
    COCKPIT_REPORT_V1_SCHEMA_JSON, CockpitConfig, CockpitReport, Finding, MissingPolicy, Presence,
    RunInfo, SENSOR_REPORT_V1_SCHEMA_JSON, SchemaValidation, SensorPolicy, SensorReport, Severity,
    ToolInfo, Verdict, VerdictCounts, VerdictStatus,
};
use cockpitctl_core::{
    IngestRequest, IngestUseCase, NoOpSchemaValidator, OutputSink, PolicySource, ReceiptSource,
};

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

fn minimal_sensor_report() -> SensorReport {
    SensorReport {
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
            status: VerdictStatus::Pass,
            counts: VerdictCounts::default(),
            reasons: vec![],
        },
        findings: vec![],
        artifacts: vec![],
        data: None,
    }
}

fn all_checks() -> ConformChecks {
    ConformChecks {
        path_hygiene: true,
        ordering: true,
        reason_lint: true,
        survivability: true,
        tool_error_identity: true,
        sensor_id_format: true,
        artifact_pointers: true,
    }
}

// ---------------------------------------------------------------------------
// 1. Embedded sensor schema matches contracts/
// ---------------------------------------------------------------------------

#[test]
fn embedded_sensor_schema_matches_contracts() {
    let contracts_bytes = include_str!("../../../contracts/schemas/sensor.report.v1.json");
    assert_eq!(
        SENSOR_REPORT_V1_SCHEMA_JSON, contracts_bytes,
        "embedded sensor schema in types crate must be byte-identical to contracts/schemas/"
    );
}

// ---------------------------------------------------------------------------
// 2. Embedded cockpit schema matches contracts/
// ---------------------------------------------------------------------------

#[test]
fn embedded_cockpit_schema_matches_contracts() {
    let contracts_bytes = include_str!("../../../contracts/schemas/cockpit.report.v1.json");
    assert_eq!(
        COCKPIT_REPORT_V1_SCHEMA_JSON, contracts_bytes,
        "embedded cockpit schema in types crate must be byte-identical to contracts/schemas/"
    );
}

// ---------------------------------------------------------------------------
// 3. Types SensorReport serialization matches sensor schema
// ---------------------------------------------------------------------------

#[test]
fn types_sensor_report_validates_against_schema() {
    let report = minimal_sensor_report();
    let json = serde_json::to_string_pretty(&report).unwrap();

    let schema: serde_json::Value =
        serde_json::from_str(SENSOR_REPORT_V1_SCHEMA_JSON).expect("parse sensor schema");
    let validator = jsonschema::validator_for(&schema).expect("compile sensor schema");

    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    let errors: Vec<_> = validator.iter_errors(&value).collect();
    assert!(
        errors.is_empty(),
        "SensorReport serialization should validate against sensor.report.v1 schema: {:?}",
        errors
    );
}

// ---------------------------------------------------------------------------
// 4. Types CockpitReport serialization matches cockpit schema
// ---------------------------------------------------------------------------

#[test]
fn types_cockpit_report_validates_against_schema() {
    let report = CockpitReport {
        schema: "cockpit.report.v1".to_string(),
        tool: ToolInfo {
            name: "cockpitctl".to_string(),
            version: "0.0.1-test".to_string(),
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
            status: VerdictStatus::Pass,
            counts: VerdictCounts::default(),
            reasons: vec![],
        },
        sensors: vec![],
        highlights: vec![],
        policy: cockpitctl_core::types::PolicySnapshot {
            warn_is_fail: false,
            max_highlights: 7,
            max_per_sensor_findings: 20,
            max_annotations: 25,
            section_order: vec![],
            sensors: vec![],
        },
        data: None,
    };
    let json = serde_json::to_string_pretty(&report).unwrap();

    let schema: serde_json::Value =
        serde_json::from_str(COCKPIT_REPORT_V1_SCHEMA_JSON).expect("parse cockpit schema");
    let validator = jsonschema::validator_for(&schema).expect("compile cockpit schema");

    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    let errors: Vec<_> = validator.iter_errors(&value).collect();
    assert!(
        errors.is_empty(),
        "CockpitReport serialization should validate against cockpit.report.v1 schema: {:?}",
        errors
    );
}

// ---------------------------------------------------------------------------
// 5. Conform validates types output — no false positives
// ---------------------------------------------------------------------------

#[test]
fn conform_accepts_valid_sensor_report_from_types() {
    let report = minimal_sensor_report();
    let json = serde_json::to_string(&report).unwrap();

    let result = conform_single(&json, "test-sensor", &all_checks()).unwrap();
    assert!(
        result.is_pass(),
        "conform should accept a valid SensorReport from types crate: {:?}",
        result.violations
    );
}

// ---------------------------------------------------------------------------
// 6. Ingest output matches cockpit schema (validated by conform)
// ---------------------------------------------------------------------------

struct IngestMemReceipts {
    sensors: Vec<String>,
    reports: BTreeMap<String, Vec<u8>>,
}

impl ReceiptSource for IngestMemReceipts {
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
        match self.reports.get(sensor_id) {
            Some(b) => Ok(ReportRead::Bytes(b.clone())),
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

struct IngestMemPolicy(Option<CockpitConfig>);

impl PolicySource for IngestMemPolicy {
    fn load_config(&self) -> anyhow::Result<Option<CockpitConfig>> {
        Ok(self.0.clone())
    }
}

struct IngestMemSink;

impl OutputSink for IngestMemSink {
    fn write_cockpit_report(&self, _json: &str) -> anyhow::Result<()> {
        Ok(())
    }

    fn write_cockpit_comment(&self, _md: &str) -> anyhow::Result<()> {
        Ok(())
    }
}

#[test]
fn ingest_output_validates_against_cockpit_schema() {
    let report_bytes = serde_json::to_vec(&minimal_sensor_report()).unwrap();
    let mut reports = BTreeMap::new();
    reports.insert("alpha".to_string(), report_bytes);

    let mut sensors_map = BTreeMap::new();
    sensors_map.insert(
        "alpha".to_string(),
        SensorPolicy {
            blocking: true,
            missing: MissingPolicy::Fail,
            ..Default::default()
        },
    );
    let cfg = CockpitConfig {
        sensors: sensors_map,
        ..Default::default()
    };

    let receipts = IngestMemReceipts {
        sensors: vec!["alpha".to_string()],
        reports,
    };
    let policy = IngestMemPolicy(Some(cfg.clone()));

    let uc = IngestUseCase::new(
        receipts,
        policy,
        IngestMemSink,
        NoOpSchemaValidator,
        render_comment,
    );

    let result = uc
        .execute(IngestRequest {
            labels: vec![],
            tool: ToolInfo {
                name: "cockpitctl".to_string(),
                version: "0.0.1-test".to_string(),
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
        })
        .expect("ingest should succeed");

    // Validate the serialized report against the cockpit schema
    let report_json = serde_json::to_string_pretty(&result.report).unwrap();
    let violations = validate_cockpit_schema(&report_json).expect("schema validation infra");
    assert!(
        violations.is_empty(),
        "ingest output should validate against cockpit.report.v1 schema: {:?}",
        violations
    );

    // Also verify exit code is valid
    assert!(
        matches!(result.exit_code, 0 | 2),
        "exit code should be 0 or 2, got {}",
        result.exit_code
    );
}

// ---------------------------------------------------------------------------
// 7. Render output uses correct markers — matches template contract
// ---------------------------------------------------------------------------

#[test]
fn render_output_contains_stable_markers() {
    let report = CockpitReport {
        schema: "cockpit.report.v1".to_string(),
        tool: ToolInfo {
            name: "cockpitctl".to_string(),
            version: "0.0.1-test".to_string(),
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
            status: VerdictStatus::Pass,
            counts: VerdictCounts::default(),
            reasons: vec![],
        },
        sensors: vec![],
        highlights: vec![],
        policy: cockpitctl_core::types::PolicySnapshot {
            warn_is_fail: false,
            max_highlights: 7,
            max_per_sensor_findings: 20,
            max_annotations: 25,
            section_order: vec![],
            sensors: vec![],
        },
        data: None,
    };

    let cfg = CockpitConfig::default();
    let comment = render_comment(&report, &cfg);

    // Template contract: comments must begin with cockpit:begin and end with cockpit:end
    let template = include_str!("../../../templates/cockpit.comment.v1.md");
    assert!(
        template.contains("<!-- cockpit:begin -->"),
        "template should contain begin marker"
    );
    assert!(
        template.contains("<!-- cockpit:end -->"),
        "template should contain end marker"
    );

    // Rendered output must use the same markers
    assert!(
        comment.contains("<!-- cockpit:begin -->"),
        "rendered comment must contain begin marker"
    );
    assert!(
        comment.contains("<!-- cockpit:end -->"),
        "rendered comment must contain end marker"
    );
}

// ---------------------------------------------------------------------------
// 8. All verdict strings match schema enum — pass/warn/fail/skip only
// ---------------------------------------------------------------------------

#[test]
fn verdict_status_values_match_schema_enum() {
    // Extract verdict enum from sensor schema
    let schema: serde_json::Value =
        serde_json::from_str(SENSOR_REPORT_V1_SCHEMA_JSON).expect("parse sensor schema");
    let verdict_enum = schema["properties"]["verdict"]["properties"]["status"]["enum"]
        .as_array()
        .expect("verdict.status.enum should be an array");
    let schema_values: Vec<&str> = verdict_enum
        .iter()
        .map(|v| v.as_str().expect("enum values should be strings"))
        .collect();

    // All Rust enum variants must serialize to a value in the schema
    let all_variants = [
        VerdictStatus::Pass,
        VerdictStatus::Warn,
        VerdictStatus::Fail,
        VerdictStatus::Skip,
    ];
    for variant in &all_variants {
        let serialized = serde_json::to_value(variant).unwrap();
        let s = serialized.as_str().unwrap();
        assert!(
            schema_values.contains(&s),
            "VerdictStatus::{:?} serializes to {:?} which is not in schema enum {:?}",
            variant,
            s,
            schema_values
        );
    }

    // Schema must not have values the Rust type doesn't cover
    for sv in &schema_values {
        let parsed: Result<VerdictStatus, _> =
            serde_json::from_value(serde_json::Value::String(sv.to_string()));
        assert!(
            parsed.is_ok(),
            "schema enum value {:?} should deserialize to VerdictStatus",
            sv
        );
    }
}

// ---------------------------------------------------------------------------
// 9. All severity levels match schema — match allowed values
// ---------------------------------------------------------------------------

#[test]
fn severity_values_match_schema_enum() {
    let schema: serde_json::Value =
        serde_json::from_str(SENSOR_REPORT_V1_SCHEMA_JSON).expect("parse sensor schema");
    let severity_enum = schema["properties"]["findings"]["items"]["properties"]["severity"]["enum"]
        .as_array()
        .expect("findings.items.properties.severity.enum should be an array");
    let schema_values: Vec<&str> = severity_enum
        .iter()
        .map(|v| v.as_str().expect("enum values should be strings"))
        .collect();

    // All Rust enum variants must match
    let all_variants = [Severity::Info, Severity::Warn, Severity::Error];
    for variant in &all_variants {
        let serialized = serde_json::to_value(variant).unwrap();
        let s = serialized.as_str().unwrap();
        assert!(
            schema_values.contains(&s),
            "Severity::{:?} serializes to {:?} which is not in schema enum {:?}",
            variant,
            s,
            schema_values
        );
    }

    // And vice versa
    for sv in &schema_values {
        let parsed: Result<Severity, _> =
            serde_json::from_value(serde_json::Value::String(sv.to_string()));
        assert!(
            parsed.is_ok(),
            "schema severity enum value {:?} should deserialize to Severity",
            sv
        );
    }
}

// ---------------------------------------------------------------------------
// 10. Config defaults match documentation — schema_validation=lax default
// ---------------------------------------------------------------------------

#[test]
fn config_defaults_match_documentation() {
    let cfg = CockpitConfig::default();

    // Documented default: schema_validation = lax
    assert_eq!(
        cfg.policy.schema_validation,
        SchemaValidation::Lax,
        "default schema_validation should be Lax"
    );

    // Documented defaults from AGENTS.md / README
    assert_eq!(cfg.policy.max_highlights, 7);
    assert_eq!(cfg.policy.max_per_sensor_findings, 20);
    assert_eq!(cfg.policy.max_annotations, 25);
    assert!(!cfg.policy.warn_is_fail);
    assert_eq!(cfg.policy.max_receipt_size_bytes, 2 * 1024 * 1024); // 2MB
}

// ---------------------------------------------------------------------------
// 11. Exit code contract — 0, 2 only from ingest (1 = runtime error from caller)
// ---------------------------------------------------------------------------

#[test]
fn exit_code_contract_pass_and_fail() {
    // Test pass scenario (exit code 0)
    let pass_report = serde_json::to_vec(&minimal_sensor_report()).unwrap();
    let pass_result = run_ingest_pipeline(
        vec!["alpha"],
        vec![("alpha", pass_report)],
        Some(CockpitConfig {
            sensors: {
                let mut m = BTreeMap::new();
                m.insert(
                    "alpha".to_string(),
                    SensorPolicy {
                        blocking: true,
                        missing: MissingPolicy::Fail,
                        ..Default::default()
                    },
                );
                m
            },
            ..Default::default()
        }),
    );
    assert_eq!(pass_result.exit_code, 0, "pass should yield exit code 0");

    // Test fail scenario (exit code 2)
    let mut fail_report = minimal_sensor_report();
    fail_report.verdict.status = VerdictStatus::Fail;
    fail_report.findings = vec![Finding {
        severity: Severity::Error,
        check_id: None,
        code: "test.error".to_string(),
        message: "a failure".to_string(),
        location: None,
        help: None,
        url: None,
        fingerprint: None,
        data: None,
    }];
    fail_report.verdict.counts.error = 1;
    let fail_bytes = serde_json::to_vec(&fail_report).unwrap();

    let fail_result = run_ingest_pipeline(
        vec!["alpha"],
        vec![("alpha", fail_bytes)],
        Some(CockpitConfig {
            sensors: {
                let mut m = BTreeMap::new();
                m.insert(
                    "alpha".to_string(),
                    SensorPolicy {
                        blocking: true,
                        missing: MissingPolicy::Fail,
                        ..Default::default()
                    },
                );
                m
            },
            ..Default::default()
        }),
    );
    assert_eq!(
        fail_result.exit_code, 2,
        "policy fail should yield exit code 2"
    );
}

fn run_ingest_pipeline(
    sensors: Vec<&str>,
    reports: Vec<(&str, Vec<u8>)>,
    config: Option<CockpitConfig>,
) -> cockpitctl_core::IngestResult {
    let mut report_map = BTreeMap::new();
    for (id, bytes) in reports {
        report_map.insert(id.to_string(), bytes);
    }

    let receipts = IngestMemReceipts {
        sensors: sensors.into_iter().map(String::from).collect(),
        reports: report_map,
    };
    let policy = IngestMemPolicy(config);

    let uc = IngestUseCase::new(
        receipts,
        policy,
        IngestMemSink,
        NoOpSchemaValidator,
        render_comment,
    );
    uc.execute(IngestRequest {
        labels: vec![],
        tool: ToolInfo {
            name: "cockpitctl".to_string(),
            version: "0.0.1-test".to_string(),
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
    })
    .expect("ingest pipeline should succeed")
}

// ---------------------------------------------------------------------------
// 12. Schema version strings are consistent — sensor.report.v1, cockpit.report.v1
// ---------------------------------------------------------------------------

#[test]
fn schema_version_strings_consistent() {
    // Sensor schema $id
    let sensor_schema: serde_json::Value =
        serde_json::from_str(SENSOR_REPORT_V1_SCHEMA_JSON).expect("parse sensor schema");
    assert_eq!(
        sensor_schema["$id"].as_str().unwrap(),
        "urn:effortless:sensor.report.v1",
        "sensor schema $id must match expected URN"
    );

    // Cockpit schema $id
    let cockpit_schema: serde_json::Value =
        serde_json::from_str(COCKPIT_REPORT_V1_SCHEMA_JSON).expect("parse cockpit schema");
    assert_eq!(
        cockpit_schema["$id"].as_str().unwrap(),
        "urn:effortless:cockpit.report.v1",
        "cockpit schema $id must match expected URN"
    );

    // Cockpit schema enforces the schema field with const
    assert_eq!(
        cockpit_schema["properties"]["schema"]["const"]
            .as_str()
            .unwrap(),
        "cockpit.report.v1",
        "cockpit schema should enforce schema field = cockpit.report.v1"
    );

    // Types default schema strings match
    let sensor = minimal_sensor_report();
    assert_eq!(sensor.schema, "sensor.report.v1");
}

// ---------------------------------------------------------------------------
// Bonus: Presence enum matches cockpit schema
// ---------------------------------------------------------------------------

#[test]
fn presence_values_match_schema_enum() {
    let schema: serde_json::Value =
        serde_json::from_str(COCKPIT_REPORT_V1_SCHEMA_JSON).expect("parse cockpit schema");
    let presence_enum = schema["properties"]["sensors"]["items"]["properties"]["presence"]["enum"]
        .as_array()
        .expect("sensors.items.properties.presence.enum should be an array");
    let schema_values: Vec<&str> = presence_enum
        .iter()
        .map(|v| v.as_str().expect("enum values should be strings"))
        .collect();

    let all_variants = [Presence::Present, Presence::Missing, Presence::Invalid];
    for variant in &all_variants {
        let serialized = serde_json::to_value(variant).unwrap();
        let s = serialized.as_str().unwrap();
        assert!(
            schema_values.contains(&s),
            "Presence::{:?} serializes to {:?} which is not in schema enum {:?}",
            variant,
            s,
            schema_values
        );
    }
}
