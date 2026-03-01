//! Property-based tests for cockpitctl-sarif conversion.

use std::collections::BTreeMap;

use cockpitctl_sarif::{cockpit_report_to_sarif, cockpit_report_to_sarif_json};
use cockpitctl_types::*;
use proptest::prelude::*;

// ============================================================================
// Strategies
// ============================================================================

fn arb_severity() -> impl Strategy<Value = Severity> {
    prop_oneof![
        Just(Severity::Error),
        Just(Severity::Warn),
        Just(Severity::Info),
    ]
}

fn arb_location() -> impl Strategy<Value = Option<Location>> {
    prop_oneof![
        Just(None),
        (
            proptest::option::of("[a-z0-9/_.-]{1,40}"),
            proptest::option::of(0u32..10_000),
            proptest::option::of(0u32..500),
        )
            .prop_map(|(path, line, col)| Some(Location { path, line, col })),
    ]
}

fn arb_finding() -> impl Strategy<Value = Finding> {
    (
        arb_severity(),
        proptest::option::of("[a-z]{1,10}"),
        "[a-zA-Z0-9:_/-]{1,30}",
        "[a-zA-Z0-9 .,'!?()-]{1,80}",
        arb_location(),
        proptest::option::of("[a-zA-Z0-9 ]{0,40}"),
        proptest::option::of("https?://[a-z.]+/[a-z0-9/]{0,30}"),
        proptest::option::of("[a-f0-9]{8,32}"),
    )
        .prop_map(
            |(severity, check_id, code, message, location, help, url, fingerprint)| Finding {
                severity,
                check_id,
                code,
                message,
                location,
                help,
                url,
                fingerprint,
                data: None,
            },
        )
}

fn arb_highlight() -> impl Strategy<Value = Highlight> {
    ("[a-z][a-z0-9_-]{0,19}", arb_finding())
        .prop_map(|(sensor_id, finding)| Highlight { sensor_id, finding })
}

fn arb_verdict_status() -> impl Strategy<Value = VerdictStatus> {
    prop_oneof![
        Just(VerdictStatus::Pass),
        Just(VerdictStatus::Warn),
        Just(VerdictStatus::Fail),
        Just(VerdictStatus::Skip),
    ]
}

fn arb_cockpit_report() -> impl Strategy<Value = CockpitReport> {
    (
        arb_verdict_status(),
        prop::collection::vec(arb_highlight(), 0..15),
    )
        .prop_map(|(status, highlights)| CockpitReport {
            schema: "cockpit.report.v1".to_string(),
            tool: ToolInfo {
                name: "cockpitctl".to_string(),
                version: "0.3.0".to_string(),
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
                    info: 0,
                    warn: 0,
                    error: 0,
                    suppressed: 0,
                },
                reasons: vec![],
            },
            sensors: vec![],
            highlights,
            policy: PolicySnapshot {
                warn_is_fail: false,
                max_highlights: 7,
                max_per_sensor_findings: 20,
                max_annotations: 25,
                section_order: vec![],
                sensors: vec![],
            },
            data: None,
        })
}

// ============================================================================
// Properties
// ============================================================================

proptest! {
    /// Any `CockpitReport` produces valid, parseable SARIF JSON with correct
    /// schema version.
    #[test]
    fn valid_sarif_json(report in arb_cockpit_report()) {
        let json = cockpit_report_to_sarif_json(&report)
            .expect("serialization must not fail");

        let parsed: serde_json::Value = serde_json::from_str(&json)
            .expect("output must be valid JSON");

        prop_assert_eq!(parsed["version"].as_str(), Some("2.1.0"));
        prop_assert!(parsed["$schema"].as_str()
            .is_some_and(|s| s.contains("sarif-schema-2.1.0")));
        prop_assert!(parsed["runs"].is_array());
    }

    /// Conversion never panics regardless of input shape.
    #[test]
    fn no_panic_on_arbitrary_input(report in arb_cockpit_report()) {
        let _sarif = cockpit_report_to_sarif(&report);
        let _json  = cockpit_report_to_sarif_json(&report);
    }

    /// Same report always produces identical SARIF output (determinism).
    #[test]
    fn deterministic_output(report in arb_cockpit_report()) {
        let json1 = cockpit_report_to_sarif_json(&report).unwrap();
        let json2 = cockpit_report_to_sarif_json(&report).unwrap();
        prop_assert_eq!(&json1, &json2);
    }

    /// The number of SARIF results equals the number of input highlights.
    #[test]
    fn result_count_matches_highlights(report in arb_cockpit_report()) {
        let sarif = cockpit_report_to_sarif(&report);
        let n_results = sarif.runs[0].results.len();
        let n_highlights = report.highlights.len();
        prop_assert_eq!(n_results, n_highlights);
    }

    /// Every severity maps to one of the three valid SARIF levels.
    #[test]
    fn severity_maps_to_valid_level(sev in arb_severity()) {
        let report = CockpitReport {
            schema: "cockpit.report.v1".to_string(),
            tool: ToolInfo {
                name: "cockpitctl".to_string(),
                version: "0.3.0".to_string(),
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
                counts: VerdictCounts { info: 0, warn: 0, error: 0, suppressed: 0 },
                reasons: vec![],
            },
            sensors: vec![],
            highlights: vec![Highlight {
                sensor_id: "s".to_string(),
                finding: Finding {
                    severity: sev,
                    check_id: None,
                    code: "code".to_string(),
                    message: "msg".to_string(),
                    location: None,
                    help: None,
                    url: None,
                    fingerprint: None,
                    data: None,
                },
            }],
            policy: PolicySnapshot {
                warn_is_fail: false,
                max_highlights: 7,
                max_per_sensor_findings: 20,
                max_annotations: 25,
                section_order: vec![],
                sensors: vec![],
            },
            data: None,
        };
        let sarif = cockpit_report_to_sarif(&report);
        let level = &sarif.runs[0].results[0].level;
        prop_assert!(
            ["error", "warning", "note"].contains(&level.as_str()),
            "unexpected SARIF level: {level}"
        );
    }

    /// Rule IDs collected in the tool driver match the codes from highlights.
    #[test]
    fn rule_ids_match_highlight_codes(report in arb_cockpit_report()) {
        let sarif = cockpit_report_to_sarif(&report);
        let driver_rule_ids: std::collections::BTreeSet<&str> = sarif.runs[0]
            .tool
            .driver
            .rules
            .iter()
            .map(|r| r.id.as_str())
            .collect();
        let expected_codes: std::collections::BTreeSet<&str> = report
            .highlights
            .iter()
            .map(|h| h.finding.code.as_str())
            .collect();
        prop_assert_eq!(driver_rule_ids, expected_codes);
    }
}
