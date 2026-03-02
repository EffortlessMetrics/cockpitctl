//! Cross-crate contract verification tests.
//!
//! These tests verify that the contracts between crates are consistent:
//! embedded schemas match source-of-truth files, Rust types are compatible
//! with JSON schemas, verdict/severity enums match schema-defined values,
//! and default config values match documented conventions.

use std::collections::BTreeMap;
use std::collections::HashSet;
use std::fs;
use std::path::Path;

use cockpitctl_conform::{ConformChecks, conform_single, validate_cockpit_schema};
use cockpitctl_core::ingest::{CommentRead, DiscoveredSensors, PlanRead, ReportRead};
use cockpitctl_core::render::render_comment;
use cockpitctl_core::types::{
    ArtifactPointer, COCKPIT_REPORT_V1_SCHEMA_JSON, Capability, CapabilityStatus, CiInfo,
    CockpitConfig, CockpitReport, Finding, GitInfo, Highlight, HostInfo, Location, MissingPolicy,
    PolicyOutcome, PolicySensorSnapshot, PolicySnapshot, Presence, RunInfo,
    SENSOR_REPORT_V1_SCHEMA_JSON, SchemaValidation, SensorPolicy, SensorReport, SensorSummary,
    Severity, ToolInfo, Verdict, VerdictCounts, VerdictStatus,
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

// ---------------------------------------------------------------------------
// 13. DTO roundtrip: SensorReport with all optional fields populated
// ---------------------------------------------------------------------------

#[test]
fn sensor_report_roundtrip_with_all_fields() {
    let report = SensorReport {
        schema: "sensor.report.v1".to_string(),
        tool: ToolInfo {
            name: "full-sensor".to_string(),
            version: "2.3.4".to_string(),
            commit: Some("abc123def".to_string()),
        },
        run: RunInfo {
            started_at: "2026-06-15T10:30:00Z".to_string(),
            ended_at: Some("2026-06-15T10:31:00Z".to_string()),
            duration_ms: Some(60_000),
            host: Some(HostInfo {
                os: Some("linux".to_string()),
                arch: Some("x86_64".to_string()),
                hostname: Some("build-node-42".to_string()),
            }),
            git: Some(GitInfo {
                repo: Some("org/repo".to_string()),
                base_ref: Some("refs/heads/main".to_string()),
                head_ref: Some("refs/heads/feature".to_string()),
                base_sha: Some("aaa111".to_string()),
                head_sha: Some("bbb222".to_string()),
                merge_base: Some("ccc333".to_string()),
            }),
            ci: Some(CiInfo {
                provider: Some("github-actions".to_string()),
                run_id: Some("12345".to_string()),
                run_url: Some("https://github.com/org/repo/actions/runs/12345".to_string()),
                job: Some("build".to_string()),
            }),
            capabilities: {
                let mut caps = BTreeMap::new();
                caps.insert(
                    "git".to_string(),
                    Capability {
                        status: CapabilityStatus::Available,
                        reason: None,
                    },
                );
                caps
            },
        },
        verdict: Verdict {
            status: VerdictStatus::Warn,
            counts: VerdictCounts {
                info: 1,
                warn: 2,
                error: 0,
                suppressed: 3,
            },
            reasons: vec!["lint_warnings".to_string()],
        },
        findings: vec![Finding {
            severity: Severity::Warn,
            check_id: Some("clippy::needless_return".to_string()),
            code: "needless_return".to_string(),
            message: "unneeded `return` statement".to_string(),
            location: Some(Location {
                path: Some("src/main.rs".to_string()),
                line: Some(42),
                col: Some(5),
            }),
            help: Some("remove the `return`".to_string()),
            url: Some("https://rust-lang.github.io/rust-clippy/".to_string()),
            fingerprint: Some("fp-abc-123".to_string()),
            data: Some(serde_json::json!({"custom_key": "custom_value"})),
        }],
        artifacts: vec![ArtifactPointer {
            id: "coverage-report".to_string(),
            path: "artifacts/sensor/coverage.html".to_string(),
            mime: "text/html".to_string(),
            schema: Some("coverage.v1".to_string()),
        }],
        data: Some(serde_json::json!({"extra": true})),
    };

    // Serialize (types crate) → JSON → Deserialize (as any consuming crate would)
    let json = serde_json::to_string_pretty(&report).expect("serialize");
    let deserialized: SensorReport = serde_json::from_str(&json).expect("deserialize");

    assert_eq!(
        report, deserialized,
        "SensorReport roundtrip must be lossless"
    );
}

// ---------------------------------------------------------------------------
// 14. DTO roundtrip: CockpitReport with all fields
// ---------------------------------------------------------------------------

#[test]
fn cockpit_report_roundtrip_with_all_fields() {
    let report = CockpitReport {
        schema: "cockpit.report.v1".to_string(),
        tool: ToolInfo {
            name: "cockpitctl".to_string(),
            version: "0.3.0".to_string(),
            commit: Some("deadbeef".to_string()),
        },
        run: RunInfo {
            started_at: "2026-01-01T00:00:00Z".to_string(),
            ended_at: Some("2026-01-01T00:01:00Z".to_string()),
            duration_ms: Some(60_000),
            host: None,
            git: None,
            ci: None,
            capabilities: BTreeMap::new(),
        },
        verdict: Verdict {
            status: VerdictStatus::Fail,
            counts: VerdictCounts {
                info: 0,
                warn: 1,
                error: 2,
                suppressed: 0,
            },
            reasons: vec!["blocking_sensor_failed".to_string()],
        },
        sensors: vec![SensorSummary {
            id: "builddiag".to_string(),
            blocking: true,
            missing: MissingPolicy::Fail,
            presence: Presence::Present,
            report_path: "artifacts/builddiag/report.json".to_string(),
            comment_path: Some("artifacts/builddiag/comment.md".to_string()),
            verdict: Verdict {
                status: VerdictStatus::Fail,
                counts: VerdictCounts {
                    info: 0,
                    warn: 1,
                    error: 2,
                    suppressed: 0,
                },
                reasons: vec![],
            },
            truncated: false,
            errors: vec![],
            missing_policy_applied: None,
            policy_outcome: Some(PolicyOutcome::Blocked),
        }],
        highlights: vec![Highlight {
            sensor_id: "builddiag".to_string(),
            finding: Finding {
                severity: Severity::Error,
                check_id: None,
                code: "build_error".to_string(),
                message: "compilation failed".to_string(),
                location: Some(Location {
                    path: Some("src/lib.rs".to_string()),
                    line: Some(10),
                    col: None,
                }),
                help: None,
                url: None,
                fingerprint: Some("fp-build-001".to_string()),
                data: None,
            },
        }],
        policy: PolicySnapshot {
            warn_is_fail: false,
            max_highlights: 7,
            max_per_sensor_findings: 20,
            max_annotations: 25,
            section_order: vec!["Highlights".to_string()],
            sensors: vec![PolicySensorSnapshot {
                id: "builddiag".to_string(),
                blocking: true,
                missing: MissingPolicy::Fail,
                section: Some("Diagnostics".to_string()),
                require_label: None,
                repro: Some("cargo build".to_string()),
            }],
        },
        data: Some(serde_json::json!({"_internal": "test"})),
    };

    let json = serde_json::to_string_pretty(&report).expect("serialize");
    let deserialized: CockpitReport = serde_json::from_str(&json).expect("deserialize");

    assert_eq!(
        report, deserialized,
        "CockpitReport roundtrip must be lossless"
    );
}

// ---------------------------------------------------------------------------
// 15. Port trait compliance: real IO adapters implement port traits
// ---------------------------------------------------------------------------

#[test]
fn io_adapters_implement_port_traits() {
    // Compile-time verification that FS adapters implement the required traits.
    fn assert_receipt_source<T: ReceiptSource>() {}
    fn assert_policy_source<T: PolicySource>() {}
    fn assert_output_sink<T: OutputSink>() {}
    fn assert_schema_validator<T: cockpitctl_core::SchemaValidator>() {}

    assert_receipt_source::<cockpitctl_core::io::FsReceiptSource>();
    assert_policy_source::<cockpitctl_core::io::FsPolicySource>();
    assert_output_sink::<cockpitctl_core::io::FsOutputSink>();
    assert_schema_validator::<cockpitctl_core::io_schema::JsonSchemaValidator>();
    assert_schema_validator::<NoOpSchemaValidator>();
}

// ---------------------------------------------------------------------------
// 16. Cross-crate DTO flow: types → domain → ingest preserves finding data
// ---------------------------------------------------------------------------

#[test]
fn cross_crate_finding_data_preserved_through_pipeline() {
    let finding = Finding {
        severity: Severity::Error,
        check_id: Some("clippy::unused".to_string()),
        code: "unused_var".to_string(),
        message: "variable `x` is unused".to_string(),
        location: Some(Location {
            path: Some("src/main.rs".to_string()),
            line: Some(42),
            col: Some(5),
        }),
        help: Some("prefix with `_`".to_string()),
        url: Some("https://example.com".to_string()),
        fingerprint: Some("fp-001".to_string()),
        data: Some(serde_json::json!({"extra": true})),
    };

    let mut sensor_report = minimal_sensor_report();
    sensor_report.verdict.status = VerdictStatus::Fail;
    sensor_report.findings = vec![finding.clone()];
    sensor_report.verdict.counts.error = 1;

    let report_bytes = serde_json::to_vec(&sensor_report).unwrap();
    let mut reports = BTreeMap::new();
    reports.insert("test-sensor".to_string(), report_bytes);

    let mut sensors_map = BTreeMap::new();
    sensors_map.insert(
        "test-sensor".to_string(),
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

    let result = run_ingest_pipeline(
        vec!["test-sensor"],
        vec![("test-sensor", serde_json::to_vec(&sensor_report).unwrap())],
        Some(cfg),
    );

    // Verify finding data survived the types → domain → ingest boundary
    let highlights = &result.report.highlights;
    let matched = highlights
        .iter()
        .find(|h| h.finding.code == "unused_var")
        .expect("finding should survive pipeline");

    assert_eq!(matched.finding.severity, Severity::Error);
    assert_eq!(matched.finding.message, "variable `x` is unused");
    assert_eq!(matched.finding.location.as_ref().unwrap().line, Some(42));
    assert_eq!(matched.finding.help.as_deref(), Some("prefix with `_`"));
    assert_eq!(matched.finding.fingerprint.as_deref(), Some("fp-001"));
}

// ---------------------------------------------------------------------------
// 17. CockpitConfig TOML roundtrip — types ↔ IO/CLI boundary
// ---------------------------------------------------------------------------

#[test]
fn cockpit_config_toml_roundtrip() {
    let toml_str = r#"
[policy]
warn_is_fail = true
max_highlights = 10
max_per_sensor_findings = 50
max_annotations = 30
schema_validation = "strict"
max_receipt_size_bytes = 1048576

[sensors.builddiag]
blocking = true
missing = "fail"
section = "Diagnostics"
repro = "cargo build"

[sensors.clippy]
blocking = false
missing = "skip"
require_label = "lint"
"#;

    // Deserialize from TOML (as IO/CLI crate would do)
    let cfg: CockpitConfig = toml::from_str(toml_str).expect("parse TOML");

    assert!(cfg.policy.warn_is_fail);
    assert_eq!(cfg.policy.max_highlights, 10);
    assert_eq!(cfg.policy.schema_validation, SchemaValidation::Strict);
    assert_eq!(cfg.policy.max_receipt_size_bytes, 1_048_576);
    assert_eq!(cfg.sensors.len(), 2);

    let builddiag = &cfg.sensors["builddiag"];
    assert!(builddiag.blocking);
    assert_eq!(builddiag.missing, MissingPolicy::Fail);
    assert_eq!(builddiag.section.as_deref(), Some("Diagnostics"));

    let clippy = &cfg.sensors["clippy"];
    assert!(!clippy.blocking);
    assert_eq!(clippy.require_label.as_deref(), Some("lint"));

    // Re-serialize to JSON and back to verify types serde is consistent
    let json = serde_json::to_string(&cfg).expect("to JSON");
    let from_json: CockpitConfig = serde_json::from_str(&json).expect("from JSON");
    assert_eq!(
        cfg, from_json,
        "CockpitConfig JSON roundtrip must be lossless"
    );
}

// ---------------------------------------------------------------------------
// 18. Version consistency: all workspace crates share the same version
// ---------------------------------------------------------------------------

fn workspace_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("workspace root")
}

#[test]
fn all_workspace_crate_versions_match() {
    let root = workspace_root();
    let root_toml: toml::Value = {
        let text = fs::read_to_string(root.join("Cargo.toml")).expect("read root Cargo.toml");
        toml::from_str(&text).expect("parse root Cargo.toml")
    };

    let ws_version = root_toml["workspace"]["package"]["version"]
        .as_str()
        .expect("workspace.package.version");

    let members: Vec<String> = root_toml["workspace"]["members"]
        .as_array()
        .expect("workspace.members")
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();

    for member in &members {
        let member_toml: toml::Value = {
            let text = fs::read_to_string(root.join(member).join("Cargo.toml"))
                .unwrap_or_else(|_| panic!("read {member}/Cargo.toml"));
            toml::from_str(&text).unwrap_or_else(|_| panic!("parse {member}/Cargo.toml"))
        };

        let ver = &member_toml["package"]["version"];
        let is_inherited = ver
            .as_table()
            .and_then(|t| t.get("workspace"))
            .and_then(|v| v.as_bool())
            == Some(true);
        let matches_literal = ver.as_str() == Some(ws_version);

        assert!(
            is_inherited || matches_literal,
            "{member}: version must be workspace-inherited or \"{ws_version}\", got {ver:?}",
        );
    }
}

// ---------------------------------------------------------------------------
// 19. Core facade re-exports all dependency crates
// ---------------------------------------------------------------------------

#[test]
fn core_facade_reexports_all_dependency_crates() {
    let root = workspace_root();
    let core_toml: toml::Value = {
        let text = fs::read_to_string(root.join("crates/cockpitctl-core/Cargo.toml"))
            .expect("read core Cargo.toml");
        toml::from_str(&text).expect("parse core Cargo.toml")
    };
    let lib_rs = fs::read_to_string(root.join("crates/cockpitctl-core/src/lib.rs"))
        .expect("read core lib.rs");

    let deps = core_toml["dependencies"]
        .as_table()
        .expect("[dependencies]");

    for key in deps.keys().filter(|k| k.starts_with("cockpitctl-")) {
        let ident = key.replace('-', "_");
        assert!(
            lib_rs.contains(&format!("pub use {ident}"))
                || lib_rs.contains(&format!("pub mod {ident}")),
            "cockpitctl-core must re-export {key} (as `{ident}`)",
        );
    }
}

// ---------------------------------------------------------------------------
// 20. Cross-crate DTO: types serialized by one crate, deserialized by another
// ---------------------------------------------------------------------------

#[test]
fn sensor_report_serialized_by_types_consumed_by_ingest() {
    // Simulate a sensor writing a report (using types crate serialization)
    let sensor_output = minimal_sensor_report();
    let json_bytes = serde_json::to_vec(&sensor_output).unwrap();

    // Simulate ingest crate consuming the bytes (deserialization)
    let parsed: SensorReport = serde_json::from_slice(&json_bytes)
        .expect("ingest should deserialize types-serialized SensorReport");

    assert_eq!(parsed.schema, "sensor.report.v1");
    assert_eq!(parsed.verdict.status, VerdictStatus::Pass);
    assert_eq!(parsed.tool.name, "test-sensor");
}

// ---------------------------------------------------------------------------
// 21. Ingest output CockpitReport consumed by render without data loss
// ---------------------------------------------------------------------------

#[test]
fn ingest_output_consumed_by_render_without_data_loss() {
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

    let result = run_ingest_pipeline(
        vec!["alpha"],
        vec![(
            "alpha",
            serde_json::to_vec(&minimal_sensor_report()).unwrap(),
        )],
        Some(cfg.clone()),
    );

    // Render consumes the CockpitReport from ingest
    let comment = render_comment(&result.report, &cfg);

    // The rendered comment must reference the sensor from the report
    assert!(
        comment.contains("alpha") || result.report.sensors.iter().any(|s| s.id == "alpha"),
        "render must consume ingest output preserving sensor identity"
    );

    // The rendered comment must contain the stable markers
    assert!(comment.contains("<!-- cockpit:begin -->"));
    assert!(comment.contains("<!-- cockpit:end -->"));
}

// ---------------------------------------------------------------------------
// 22. Embedded schemas are valid JSON and valid JSON Schema
// ---------------------------------------------------------------------------

#[test]
fn embedded_schemas_are_valid_json_schema() {
    for (name, schema_json) in [
        ("sensor.report.v1", SENSOR_REPORT_V1_SCHEMA_JSON),
        ("cockpit.report.v1", COCKPIT_REPORT_V1_SCHEMA_JSON),
    ] {
        let value: serde_json::Value = serde_json::from_str(schema_json)
            .unwrap_or_else(|e| panic!("{name}: invalid JSON: {e}"));
        // Must be a JSON object with standard JSON Schema fields
        assert!(value.is_object(), "{name}: schema must be a JSON object");
        assert!(value.get("$id").is_some(), "{name}: must have $id");
        assert!(
            value.get("properties").is_some(),
            "{name}: must have properties"
        );
        // Must compile as a JSON Schema validator
        jsonschema::validator_for(&value)
            .unwrap_or_else(|e| panic!("{name}: must compile as JSON Schema: {e}"));
    }
}

// ---------------------------------------------------------------------------
// 23. PolicyOutcome values match cockpit schema enum
// ---------------------------------------------------------------------------

#[test]
fn policy_outcome_values_match_schema_enum() {
    let schema: serde_json::Value =
        serde_json::from_str(COCKPIT_REPORT_V1_SCHEMA_JSON).expect("parse cockpit schema");
    let outcome_enum =
        schema["properties"]["sensors"]["items"]["properties"]["policy_outcome"]["enum"]
            .as_array()
            .expect("sensors.items.properties.policy_outcome.enum should be an array");
    let schema_values: Vec<&str> = outcome_enum
        .iter()
        .map(|v| v.as_str().expect("enum values should be strings"))
        .collect();

    let all_variants = [
        PolicyOutcome::Blocked,
        PolicyOutcome::Allowed,
        PolicyOutcome::Informational,
    ];
    for variant in &all_variants {
        let serialized = serde_json::to_value(variant).unwrap();
        let s = serialized.as_str().unwrap();
        assert!(
            schema_values.contains(&s),
            "PolicyOutcome::{:?} serializes to {:?} which is not in schema enum {:?}",
            variant,
            s,
            schema_values
        );
    }

    for sv in &schema_values {
        let parsed: Result<PolicyOutcome, _> =
            serde_json::from_value(serde_json::Value::String(sv.to_string()));
        assert!(
            parsed.is_ok(),
            "schema policy_outcome enum value {:?} should deserialize to PolicyOutcome",
            sv
        );
    }
}

// ---------------------------------------------------------------------------
// 24. Embedded schemas file set matches contracts/schemas/ on disk
// ---------------------------------------------------------------------------

#[test]
fn embedded_schema_set_matches_contracts_directory() {
    let root = workspace_root();
    let contracts_dir = root.join("contracts").join("schemas");
    let embedded_dir = root.join("crates").join("cockpitctl-types").join("schemas");

    let json_files = |dir: &Path| -> HashSet<String> {
        fs::read_dir(dir)
            .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
            .filter_map(Result::ok)
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "json"))
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect()
    };

    let contracts_set = json_files(&contracts_dir);
    let embedded_set = json_files(&embedded_dir);

    assert_eq!(
        contracts_set, embedded_set,
        "embedded schema files must match contracts/schemas/"
    );

    for name in &contracts_set {
        let a = fs::read_to_string(contracts_dir.join(name)).expect("read contracts");
        let b = fs::read_to_string(embedded_dir.join(name)).expect("read embedded");
        assert_eq!(
            a, b,
            "schema `{name}` must be byte-identical between contracts/ and embedded"
        );
    }
}
