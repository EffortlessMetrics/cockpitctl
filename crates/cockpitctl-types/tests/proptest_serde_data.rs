//! Property-based serde roundtrip tests for DTOs with `data: Option<Value>`.
//!
//! Existing proptest_serde.rs and proptest_roundtrip.rs always set data=None.
//! This file tests that arbitrary JSON payloads in the `data` field survive
//! serialization roundtrips without loss.

use cockpitctl_types::*;
use proptest::prelude::*;
use std::collections::BTreeMap;

// ============================================================================
// Strategies for generating arbitrary JSON Values
// ============================================================================

/// Non-null JSON leaf values (excluding Null since `Some(Null)` → `None`
/// is expected serde behavior with `skip_serializing_if + default`).
fn leaf_json_value() -> impl Strategy<Value = serde_json::Value> {
    prop_oneof![
        any::<bool>().prop_map(serde_json::Value::Bool),
        (-1000i64..1000).prop_map(|n| serde_json::Value::Number(n.into())),
        "[a-zA-Z0-9 _.-]{0,30}".prop_map(serde_json::Value::String),
    ]
}

fn arb_json_value() -> impl Strategy<Value = serde_json::Value> {
    leaf_json_value().prop_recursive(3, 32, 4, |inner| {
        prop_oneof![
            prop::collection::vec(inner.clone(), 0..4)
                .prop_map(serde_json::Value::Array),
            prop::collection::btree_map("[a-z_]{1,8}", inner, 0..4)
                .prop_map(|m| serde_json::Value::Object(m.into_iter().collect())),
        ]
    })
}

fn any_severity() -> impl Strategy<Value = Severity> {
    prop_oneof![
        Just(Severity::Info),
        Just(Severity::Warn),
        Just(Severity::Error),
    ]
}

fn any_verdict_status() -> impl Strategy<Value = VerdictStatus> {
    prop_oneof![
        Just(VerdictStatus::Pass),
        Just(VerdictStatus::Warn),
        Just(VerdictStatus::Fail),
        Just(VerdictStatus::Skip),
    ]
}

fn any_tool_info() -> impl Strategy<Value = ToolInfo> {
    (
        "[a-z][a-z0-9-]{0,10}",
        "[0-9]{1,2}\\.[0-9]{1,2}\\.[0-9]{1,2}",
        prop::option::of("[a-f0-9]{7}"),
    )
        .prop_map(|(name, version, commit)| ToolInfo {
            name,
            version,
            commit,
        })
}

fn any_run_info() -> impl Strategy<Value = RunInfo> {
    Just(RunInfo {
        started_at: "2024-01-01T00:00:00Z".to_string(),
        ended_at: None,
        duration_ms: None,
        host: None,
        git: None,
        ci: None,
        capabilities: BTreeMap::new(),
    })
}

fn any_finding_with_data() -> impl Strategy<Value = Finding> {
    (
        any_severity(),
        "[A-Z][A-Z0-9_]{0,8}",
        ".{1,20}",
        prop::option::of(arb_json_value()),
    )
        .prop_map(|(severity, code, message, data)| Finding {
            severity,
            check_id: None,
            code,
            message,
            location: None,
            help: None,
            url: None,
            fingerprint: None,
            data,
        })
}

// ============================================================================
// Finding with data roundtrip
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    /// Finding with arbitrary JSON in `data` survives serde roundtrip.
    #[test]
    fn finding_with_data_roundtrip(f in any_finding_with_data()) {
        let json = serde_json::to_string(&f).unwrap();
        let parsed: Finding = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(&f, &parsed);
    }

    /// SensorReport with arbitrary data payload roundtrips.
    #[test]
    fn sensor_report_with_data_roundtrip(
        tool in any_tool_info(),
        run in any_run_info(),
        findings in prop::collection::vec(any_finding_with_data(), 0..3),
        data in prop::option::of(arb_json_value()),
    ) {
        let report = SensorReport {
            schema: "sensor.report.v1".to_string(),
            tool,
            run,
            verdict: Verdict {
                status: VerdictStatus::Pass,
                counts: VerdictCounts::default(),
                reasons: vec![],
            },
            findings,
            artifacts: vec![],
            data,
        };
        let json = serde_json::to_string(&report).unwrap();
        let parsed: SensorReport = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(&report, &parsed);
    }

    /// CockpitReport with arbitrary data payload roundtrips.
    #[test]
    fn cockpit_report_with_data_roundtrip(
        verdict_status in any_verdict_status(),
        data in prop::option::of(arb_json_value()),
    ) {
        let report = CockpitReport {
            schema: "cockpit.report.v1".to_string(),
            tool: ToolInfo {
                name: "cockpitctl".to_string(),
                version: "0.1.0".to_string(),
                commit: None,
            },
            run: RunInfo {
                started_at: "2024-01-01T00:00:00Z".to_string(),
                ended_at: None,
                duration_ms: None,
                host: None,
                git: None,
                ci: None,
                capabilities: BTreeMap::new(),
            },
            verdict: Verdict {
                status: verdict_status,
                counts: VerdictCounts::default(),
                reasons: vec![],
            },
            sensors: vec![],
            highlights: vec![],
            policy: PolicySnapshot {
                warn_is_fail: false,
                max_highlights: 7,
                max_per_sensor_findings: 20,
                max_annotations: 25,
                section_order: vec![],
                sensors: vec![],
            },
            data,
        };
        let json = serde_json::to_string(&report).unwrap();
        let parsed: CockpitReport = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(&report, &parsed);
    }

    /// Fix with arbitrary data payload roundtrips.
    #[test]
    fn fix_with_data_roundtrip(
        data in prop::option::of(arb_json_value()),
    ) {
        let fix = Fix {
            id: "fix-1".to_string(),
            safety: SafetyLevel::Safe,
            description: "auto fix".to_string(),
            finding_refs: vec![],
            preconditions: None,
            data,
        };
        let json = serde_json::to_string(&fix).unwrap();
        let parsed: Fix = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(&fix, &parsed);
    }

    /// Double-roundtrip: serialize → Value → serialize → parse preserves data.
    #[test]
    fn finding_double_roundtrip(f in any_finding_with_data()) {
        let json1 = serde_json::to_string(&f).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json1).unwrap();
        let json2 = serde_json::to_string(&value).unwrap();
        let parsed: Finding = serde_json::from_str(&json2).unwrap();
        prop_assert_eq!(&f, &parsed);
    }
}
