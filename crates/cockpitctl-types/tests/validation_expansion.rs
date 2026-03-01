use std::collections::BTreeMap;

use cockpitctl_types::{
    COCKPIT_REPORT_V1_SCHEMA_JSON, CockpitConfig, CockpitReport, Finding, FindingSortKey,
    Highlight, Location, MissingPolicy, Policy, PolicyOutcome, PolicySensorSnapshot,
    PolicySnapshot, Presence, RunInfo, SENSOR_REPORT_V1_SCHEMA_JSON, SchemaValidation,
    SensorReport, SensorSummary, Severity, ToolInfo, Verdict, VerdictCounts, VerdictStatus,
    severity_rank, verdict_status_rank,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn minimal_run_info() -> RunInfo {
    RunInfo {
        started_at: "2025-01-01T00:00:00Z".into(),
        ended_at: None,
        duration_ms: None,
        host: None,
        git: None,
        ci: None,
        capabilities: BTreeMap::new(),
    }
}

fn minimal_tool_info() -> ToolInfo {
    ToolInfo {
        name: "test-tool".into(),
        version: "0.1.0".into(),
        commit: None,
    }
}

fn minimal_verdict(status: VerdictStatus) -> Verdict {
    Verdict {
        status,
        counts: VerdictCounts::default(),
        reasons: vec![],
    }
}

fn minimal_finding(severity: Severity, code: &str, message: &str) -> Finding {
    Finding {
        severity,
        check_id: None,
        code: code.into(),
        message: message.into(),
        location: None,
        help: None,
        url: None,
        fingerprint: None,
        data: None,
    }
}

fn make_highlight(sensor_id: &str, severity: Severity, code: &str) -> Highlight {
    Highlight {
        sensor_id: sensor_id.into(),
        finding: minimal_finding(severity, code, "msg"),
    }
}

// ---------------------------------------------------------------------------
// 1. VerdictStatus — all 4 valid values accepted
// ---------------------------------------------------------------------------

#[test]
fn verdict_status_all_four_valid_values_deserialize() {
    for (json_str, expected) in [
        ("\"pass\"", VerdictStatus::Pass),
        ("\"warn\"", VerdictStatus::Warn),
        ("\"fail\"", VerdictStatus::Fail),
        ("\"skip\"", VerdictStatus::Skip),
    ] {
        let parsed: VerdictStatus =
            serde_json::from_str(json_str).unwrap_or_else(|e| panic!("{json_str}: {e}"));
        assert_eq!(parsed, expected, "mismatch for {json_str}");
    }
}

// ---------------------------------------------------------------------------
// 2. VerdictStatus — invalid string → deserialization error
// ---------------------------------------------------------------------------

#[test]
fn verdict_status_invalid_string_is_rejected() {
    for bad in [
        "\"error\"",
        "\"unknown\"",
        "\"PASS\"",
        "\"Pass\"",
        "\"\"",
        "42",
    ] {
        let result = serde_json::from_str::<VerdictStatus>(bad);
        assert!(result.is_err(), "expected rejection for {bad}");
    }
}

// ---------------------------------------------------------------------------
// 3. Severity — all levels roundtrip correctly
// ---------------------------------------------------------------------------

#[test]
fn severity_all_levels_roundtrip() {
    for sev in [Severity::Info, Severity::Warn, Severity::Error] {
        let json = serde_json::to_string(&sev).expect("serialize");
        let back: Severity = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(sev, back);
    }
}

// ---------------------------------------------------------------------------
// 4. Severity ordering — error > warn > info
// ---------------------------------------------------------------------------

#[test]
fn severity_ordering_error_gt_warn_gt_info() {
    assert!(
        severity_rank(&Severity::Error) < severity_rank(&Severity::Warn),
        "error should rank before warn"
    );
    assert!(
        severity_rank(&Severity::Warn) < severity_rank(&Severity::Info),
        "warn should rank before info"
    );
    // Transitive
    assert!(severity_rank(&Severity::Error) < severity_rank(&Severity::Info));
}

// ---------------------------------------------------------------------------
// 5. Finding — all required fields present → valid
// ---------------------------------------------------------------------------

#[test]
fn finding_with_all_required_fields_roundtrips() {
    let finding = Finding {
        severity: Severity::Error,
        check_id: Some("clippy.warning".into()),
        code: "unused_var".into(),
        message: "unused variable `x`".into(),
        location: Some(Location {
            path: Some("src/lib.rs".into()),
            line: Some(42),
            col: Some(10),
        }),
        help: Some("prefix with `_`".into()),
        url: Some("https://example.com".into()),
        fingerprint: Some("abc123".into()),
        data: Some(serde_json::json!({"extra": true})),
    };

    let json = serde_json::to_string(&finding).expect("serialize");
    let back: Finding = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(finding, back);
}

// ---------------------------------------------------------------------------
// 6. Finding with optional None fields — still valid
// ---------------------------------------------------------------------------

#[test]
fn finding_with_all_optional_fields_none_roundtrips() {
    let finding = minimal_finding(Severity::Info, "test_code", "test message");
    let json = serde_json::to_string(&finding).expect("serialize");
    let back: Finding = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(finding, back);

    // Optional fields should be absent in JSON
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert!(value.get("check_id").is_none());
    assert!(value.get("location").is_none());
    assert!(value.get("help").is_none());
    assert!(value.get("url").is_none());
    assert!(value.get("fingerprint").is_none());
    assert!(value.get("data").is_none());
}

// ---------------------------------------------------------------------------
// 7. SensorReport — valid report with minimal fields
// ---------------------------------------------------------------------------

#[test]
fn sensor_report_minimal_roundtrips() {
    let report = SensorReport {
        schema: "sensor.report.v1".into(),
        tool: minimal_tool_info(),
        run: minimal_run_info(),
        verdict: minimal_verdict(VerdictStatus::Pass),
        findings: vec![],
        artifacts: vec![],
        data: None,
    };

    let json = serde_json::to_string(&report).expect("serialize");
    let back: SensorReport = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(report, back);
    assert_eq!(back.schema, "sensor.report.v1");
}

// ---------------------------------------------------------------------------
// 8. SensorReport — report with all fields populated
// ---------------------------------------------------------------------------

#[test]
fn sensor_report_fully_populated_roundtrips() {
    let report = SensorReport {
        schema: "sensor.report.v1".into(),
        tool: ToolInfo {
            name: "builddiag".into(),
            version: "2.0.0".into(),
            commit: Some("abc1234".into()),
        },
        run: RunInfo {
            started_at: "2025-06-01T12:00:00Z".into(),
            ended_at: Some("2025-06-01T12:01:00Z".into()),
            duration_ms: Some(60_000),
            host: None,
            git: None,
            ci: None,
            capabilities: BTreeMap::new(),
        },
        verdict: Verdict {
            status: VerdictStatus::Fail,
            counts: VerdictCounts {
                info: 1,
                warn: 2,
                error: 3,
                suppressed: 1,
            },
            reasons: vec!["build_failed".into(), "lint_errors".into()],
        },
        findings: vec![
            Finding {
                severity: Severity::Error,
                check_id: Some("E0001".into()),
                code: "compile_error".into(),
                message: "cannot find value".into(),
                location: Some(Location {
                    path: Some("src/main.rs".into()),
                    line: Some(10),
                    col: Some(5),
                }),
                help: Some("did you mean `foo`?".into()),
                url: None,
                fingerprint: Some("fp1".into()),
                data: None,
            },
            minimal_finding(Severity::Warn, "unused_import", "unused import"),
        ],
        artifacts: vec![],
        data: Some(serde_json::json!({"custom_key": "custom_value"})),
    };

    let json = serde_json::to_string(&report).expect("serialize");
    let back: SensorReport = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(report, back);
    assert_eq!(back.findings.len(), 2);
    assert_eq!(back.verdict.reasons.len(), 2);
}

// ---------------------------------------------------------------------------
// 9. CockpitReport — correct schema version in output
// ---------------------------------------------------------------------------

#[test]
fn cockpit_report_schema_version_is_correct() {
    let report = CockpitReport {
        schema: "cockpit.report.v1".into(),
        tool: minimal_tool_info(),
        run: minimal_run_info(),
        verdict: minimal_verdict(VerdictStatus::Pass),
        sensors: vec![],
        highlights: vec![],
        policy: PolicySnapshot {
            warn_is_fail: false,
            max_highlights: 7,
            max_per_sensor_findings: 20,
            max_annotations: 25,
            section_order: vec!["Highlights".into()],
            sensors: vec![],
        },
        data: None,
    };

    let json = serde_json::to_value(&report).expect("serialize");
    assert_eq!(
        json.get("schema").and_then(|v| v.as_str()),
        Some("cockpit.report.v1")
    );
}

// ---------------------------------------------------------------------------
// 10. CockpitReport — contains all sensor summaries
// ---------------------------------------------------------------------------

#[test]
fn cockpit_report_contains_all_sensor_summaries() {
    let sensors = vec![
        SensorSummary {
            id: "alpha".into(),
            blocking: true,
            missing: MissingPolicy::Fail,
            presence: Presence::Present,
            report_path: "artifacts/alpha/report.json".into(),
            comment_path: None,
            verdict: minimal_verdict(VerdictStatus::Pass),
            truncated: false,
            errors: vec![],
            missing_policy_applied: None,
            policy_outcome: Some(PolicyOutcome::Allowed),
        },
        SensorSummary {
            id: "beta".into(),
            blocking: false,
            missing: MissingPolicy::Skip,
            presence: Presence::Missing,
            report_path: "artifacts/beta/report.json".into(),
            comment_path: None,
            verdict: minimal_verdict(VerdictStatus::Skip),
            truncated: false,
            errors: vec![],
            missing_policy_applied: Some(MissingPolicy::Skip),
            policy_outcome: Some(PolicyOutcome::Informational),
        },
    ];

    let report = CockpitReport {
        schema: "cockpit.report.v1".into(),
        tool: minimal_tool_info(),
        run: minimal_run_info(),
        verdict: minimal_verdict(VerdictStatus::Pass),
        sensors,
        highlights: vec![],
        policy: PolicySnapshot {
            warn_is_fail: false,
            max_highlights: 7,
            max_per_sensor_findings: 20,
            max_annotations: 25,
            section_order: vec![],
            sensors: vec![
                PolicySensorSnapshot {
                    id: "alpha".into(),
                    blocking: true,
                    missing: MissingPolicy::Fail,
                    section: None,
                    require_label: None,
                    repro: None,
                },
                PolicySensorSnapshot {
                    id: "beta".into(),
                    blocking: false,
                    missing: MissingPolicy::Skip,
                    section: None,
                    require_label: None,
                    repro: None,
                },
            ],
        },
        data: None,
    };

    let json = serde_json::to_string(&report).expect("serialize");
    let back: CockpitReport = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back.sensors.len(), 2);
    assert_eq!(back.sensors[0].id, "alpha");
    assert_eq!(back.sensors[1].id, "beta");
    assert_eq!(back.policy.sensors.len(), 2);
}

// ---------------------------------------------------------------------------
// 11. Highlight — contains sensor_id and finding fields
// ---------------------------------------------------------------------------

#[test]
fn highlight_contains_sensor_id_and_finding() {
    let highlight = make_highlight("builddiag", Severity::Error, "build_error");

    assert_eq!(highlight.sensor_id, "builddiag");
    assert_eq!(highlight.finding.severity, Severity::Error);
    assert_eq!(highlight.finding.code, "build_error");

    // Roundtrip
    let json = serde_json::to_string(&highlight).expect("serialize");
    let back: Highlight = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(highlight, back);
}

// ---------------------------------------------------------------------------
// 12. Highlight ordering — severity desc via FindingSortKey
// ---------------------------------------------------------------------------

#[test]
fn highlight_ordering_respects_severity_and_sensor() {
    let key_error = FindingSortKey {
        severity_rank: severity_rank(&Severity::Error),
        sensor_id: "builddiag".into(),
        path: "src/lib.rs".into(),
        line: 10,
        code: "E001".into(),
        message: "error".into(),
    };
    let key_warn = FindingSortKey {
        severity_rank: severity_rank(&Severity::Warn),
        sensor_id: "builddiag".into(),
        path: "src/lib.rs".into(),
        line: 10,
        code: "W001".into(),
        message: "warning".into(),
    };
    let key_info = FindingSortKey {
        severity_rank: severity_rank(&Severity::Info),
        sensor_id: "builddiag".into(),
        path: "src/lib.rs".into(),
        line: 10,
        code: "I001".into(),
        message: "info".into(),
    };

    // Error < Warn < Info in sort order (error appears first)
    assert!(key_error < key_warn);
    assert!(key_warn < key_info);

    // Sorting an array should put error first
    let mut keys = [key_info.clone(), key_error.clone(), key_warn.clone()];
    keys.sort();
    assert_eq!(keys[0].severity_rank, severity_rank(&Severity::Error));
    assert_eq!(keys[1].severity_rank, severity_rank(&Severity::Warn));
    assert_eq!(keys[2].severity_rank, severity_rank(&Severity::Info));
}

// ---------------------------------------------------------------------------
// 13. Config — default config values are correct
// ---------------------------------------------------------------------------

#[test]
fn config_defaults_match_documented_values() {
    let cfg = CockpitConfig::default();
    let p = &cfg.policy;

    assert!(!p.warn_is_fail);
    assert_eq!(p.max_highlights, 7);
    assert_eq!(p.max_per_sensor_findings, 20);
    assert_eq!(p.max_annotations, 25);
    assert_eq!(p.schema_validation, SchemaValidation::Lax);
    assert_eq!(p.max_receipt_size_bytes, 2 * 1024 * 1024);
    assert_eq!(p.section_order.len(), 9);
    assert_eq!(p.section_order[0], "Highlights");
    assert_eq!(*p.section_order.last().unwrap(), "Other");
    assert!(cfg.sensors.is_empty());
    assert!(cfg.hooks.is_empty());
}

// ---------------------------------------------------------------------------
// 14. Config — all config fields roundtrip through serde
// ---------------------------------------------------------------------------

#[test]
fn config_full_roundtrip_through_json() {
    let cfg = CockpitConfig::default();
    let json = serde_json::to_string(&cfg).expect("serialize");
    let back: CockpitConfig = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(cfg, back);
}

#[test]
fn config_custom_values_roundtrip_through_json() {
    let mut sensors = BTreeMap::new();
    sensors.insert(
        "builddiag".into(),
        cockpitctl_types::SensorPolicy {
            blocking: true,
            missing: MissingPolicy::Fail,
            section: Some("Diagnostics".into()),
            require_label: Some("ci:builddiag".into()),
            repro: Some("cargo build 2>&1".into()),
        },
    );

    let cfg = CockpitConfig {
        policy: Policy {
            warn_is_fail: true,
            max_highlights: 10,
            max_per_sensor_findings: 50,
            max_annotations: 100,
            section_order: vec!["Custom".into()],
            schema_validation: SchemaValidation::Strict,
            max_receipt_size_bytes: 4 * 1024 * 1024,
        },
        sensors,
        ..CockpitConfig::default()
    };

    let json = serde_json::to_string(&cfg).expect("serialize");
    let back: CockpitConfig = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(cfg, back);
    assert!(back.policy.warn_is_fail);
    assert_eq!(back.policy.schema_validation, SchemaValidation::Strict);
    assert_eq!(back.sensors.len(), 1);
    assert!(back.sensors["builddiag"].blocking);
}

// ---------------------------------------------------------------------------
// 15. Embedded schema bytes — non-empty and valid JSON
// ---------------------------------------------------------------------------

#[test]
fn embedded_sensor_schema_is_nonempty_valid_json() {
    assert!(
        !SENSOR_REPORT_V1_SCHEMA_JSON.is_empty(),
        "sensor schema should not be empty"
    );
    let value: serde_json::Value = serde_json::from_str(SENSOR_REPORT_V1_SCHEMA_JSON)
        .expect("sensor schema must be valid JSON");
    assert!(value.is_object(), "schema should be a JSON object");
    assert!(
        value.get("$schema").is_some() || value.get("type").is_some(),
        "schema should have $schema or type field"
    );
}

#[test]
fn embedded_cockpit_schema_is_nonempty_valid_json() {
    assert!(
        !COCKPIT_REPORT_V1_SCHEMA_JSON.is_empty(),
        "cockpit schema should not be empty"
    );
    let value: serde_json::Value = serde_json::from_str(COCKPIT_REPORT_V1_SCHEMA_JSON)
        .expect("cockpit schema must be valid JSON");
    assert!(value.is_object(), "schema should be a JSON object");
    assert!(
        value.get("$schema").is_some() || value.get("type").is_some(),
        "schema should have $schema or type field"
    );
}

// ---------------------------------------------------------------------------
// Bonus: VerdictStatus ordering via rank helper
// ---------------------------------------------------------------------------

#[test]
fn verdict_status_rank_all_four_are_distinct() {
    let ranks: Vec<u8> = [
        VerdictStatus::Pass,
        VerdictStatus::Warn,
        VerdictStatus::Fail,
        VerdictStatus::Skip,
    ]
    .iter()
    .map(verdict_status_rank)
    .collect();

    // All distinct
    let mut deduped = ranks.clone();
    deduped.sort();
    deduped.dedup();
    assert_eq!(ranks.len(), deduped.len(), "all ranks must be distinct");

    // fail < warn < pass < skip
    assert!(verdict_status_rank(&VerdictStatus::Fail) < verdict_status_rank(&VerdictStatus::Warn));
    assert!(verdict_status_rank(&VerdictStatus::Warn) < verdict_status_rank(&VerdictStatus::Pass));
    assert!(verdict_status_rank(&VerdictStatus::Pass) < verdict_status_rank(&VerdictStatus::Skip));
}

// ---------------------------------------------------------------------------
// Bonus: Severity invalid string rejected
// ---------------------------------------------------------------------------

#[test]
fn severity_invalid_string_is_rejected() {
    for bad in ["\"critical\"", "\"ERROR\"", "\"Warn\"", "\"none\"", "\"\""] {
        let result = serde_json::from_str::<Severity>(bad);
        assert!(result.is_err(), "expected rejection for {bad}");
    }
}

// ---------------------------------------------------------------------------
// Bonus: FindingSortKey deterministic tiebreakers
// ---------------------------------------------------------------------------

#[test]
fn finding_sort_key_tiebreaks_on_sensor_then_path_then_line() {
    let key_a = FindingSortKey {
        severity_rank: 0,
        sensor_id: "alpha".into(),
        path: "a.rs".into(),
        line: 1,
        code: "C".into(),
        message: "m".into(),
    };
    let key_b = FindingSortKey {
        severity_rank: 0,
        sensor_id: "beta".into(),
        path: "a.rs".into(),
        line: 1,
        code: "C".into(),
        message: "m".into(),
    };
    let key_a2 = FindingSortKey {
        severity_rank: 0,
        sensor_id: "alpha".into(),
        path: "b.rs".into(),
        line: 1,
        code: "C".into(),
        message: "m".into(),
    };
    let key_a3 = FindingSortKey {
        severity_rank: 0,
        sensor_id: "alpha".into(),
        path: "a.rs".into(),
        line: 2,
        code: "C".into(),
        message: "m".into(),
    };

    // Same severity: alpha < beta
    assert!(key_a < key_b);
    // Same sensor: a.rs < b.rs
    assert!(key_a < key_a2);
    // Same path: line 1 < line 2
    assert!(key_a < key_a3);
}
