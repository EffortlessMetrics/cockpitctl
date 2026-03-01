//! Property-based tests for render budget constraints in cockpitctl-render.
//!
//! Tests that rendered output respects size and count budgets for
//! arbitrary inputs, and that comment length is bounded.

use cockpitctl_render::{render_annotations, render_comment, render_github_annotations};
use cockpitctl_types::{
    CockpitConfig, CockpitReport, Finding, Highlight, Location, MissingPolicy, Policy,
    PolicySnapshot, Presence, RunInfo, SensorPolicy, SensorSummary, Severity, ToolInfo, Verdict,
    VerdictCounts, VerdictStatus,
};
use proptest::prelude::*;
use std::collections::BTreeMap;

// ============================================================================
// Strategies
// ============================================================================

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

fn any_location() -> impl Strategy<Value = Option<Location>> {
    prop::option::of(
        (
            prop::option::of("[a-z/_.-]{1,50}"),
            prop::option::of(1u32..10000u32),
            prop::option::of(1u32..1000u32),
        )
            .prop_map(|(path, line, col)| Location { path, line, col }),
    )
}

fn any_finding() -> impl Strategy<Value = Finding> {
    (
        any_severity(),
        prop::option::of("[A-Z][A-Z0-9_]{0,20}"),
        "[A-Z][A-Z0-9_./-]{0,30}",
        ".{1,100}",
        any_location(),
        prop::option::of(".{0,50}"),
        prop::option::of("https?://[a-z.]+/[a-z/]*"),
        prop::option::of("[a-f0-9]{64}"),
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

fn any_highlight() -> impl Strategy<Value = Highlight> {
    ("[a-z_][a-z0-9_-]{0,20}", any_finding())
        .prop_map(|(sensor_id, finding)| Highlight { sensor_id, finding })
}

fn any_highlights(max_len: usize) -> impl Strategy<Value = Vec<Highlight>> {
    prop::collection::vec(any_highlight(), 0..=max_len)
}

fn any_verdict() -> impl Strategy<Value = Verdict> {
    (
        any_verdict_status(),
        (0u64..100, 0u64..100, 0u64..100, 0u64..10).prop_map(|(i, w, e, s)| VerdictCounts {
            info: i,
            warn: w,
            error: e,
            suppressed: s,
        }),
        prop::collection::vec(".{0,30}", 0..3),
    )
        .prop_map(|(status, counts, reasons)| Verdict {
            status,
            counts,
            reasons,
        })
}

fn any_sensor_summary() -> impl Strategy<Value = SensorSummary> {
    (
        "[a-z_][a-z0-9_-]{0,20}",
        any::<bool>(),
        prop_oneof![
            Just(MissingPolicy::Skip),
            Just(MissingPolicy::Warn),
            Just(MissingPolicy::Fail),
        ],
        prop_oneof![
            Just(Presence::Present),
            Just(Presence::Missing),
            Just(Presence::Invalid),
        ],
        any_verdict(),
    )
        .prop_map(|(id, blocking, missing, presence, verdict)| SensorSummary {
            id: id.clone(),
            blocking,
            missing,
            presence,
            report_path: format!("artifacts/{}/report.json", id),
            comment_path: None,
            verdict,
            truncated: false,
            errors: vec![],
            missing_policy_applied: None,
            policy_outcome: None,
        })
}

fn any_cockpit_config() -> impl Strategy<Value = CockpitConfig> {
    (
        any::<bool>(),
        1usize..15,
        1usize..30,
        1usize..30,
        prop::collection::vec("[A-Z][a-z]{0,15}", 0..5),
        prop::collection::btree_map(
            "[a-z_][a-z0-9_-]{0,15}",
            (
                any::<bool>(),
                prop_oneof![
                    Just(MissingPolicy::Skip),
                    Just(MissingPolicy::Warn),
                    Just(MissingPolicy::Fail),
                ],
            )
                .prop_map(|(blocking, missing)| SensorPolicy {
                    blocking,
                    missing,
                    section: None,
                    require_label: None,
                    repro: None,
                }),
            0..5,
        ),
    )
        .prop_map(
            |(
                warn_is_fail,
                max_highlights,
                max_per_sensor_findings,
                max_annotations,
                section_order,
                sensors,
            )| {
                CockpitConfig {
                    policy: Policy {
                        warn_is_fail,
                        max_highlights,
                        max_per_sensor_findings,
                        max_annotations,
                        section_order,
                        schema_validation: Default::default(),
                        max_receipt_size_bytes: 2 * 1024 * 1024,
                    },
                    buildfix: Default::default(),
                    policy_signing: Default::default(),
                    sensors,
                    hooks: vec![],
                }
            },
        )
}

fn any_cockpit_report(max_sensors: usize) -> impl Strategy<Value = CockpitReport> {
    (
        any_verdict(),
        prop::collection::vec(any_sensor_summary(), 0..=max_sensors),
        any_highlights(15),
    )
        .prop_map(|(verdict, sensors, highlights)| CockpitReport {
            schema: "cockpit.report.v1".to_string(),
            tool: ToolInfo {
                name: "cockpitctl".to_string(),
                version: "0.0.0-test".to_string(),
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
            verdict,
            sensors,
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
// Property: comment length is bounded for arbitrary reports
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// Comment length grows proportionally to sensor count but stays reasonable.
    /// With up to 10 sensors and 15 highlights, output should not blow up.
    #[test]
    fn comment_length_bounded(
        report in any_cockpit_report(10),
        cfg in any_cockpit_config(),
    ) {
        let md = render_comment(&report, &cfg);

        // Reasonable upper bound: ~500 chars per sensor + ~500 per highlight + ~2000 base.
        // This is a sanity check, not a strict contract.
        let max_expected = 2000
            + report.sensors.len() * 500
            + report.highlights.len() * 500;
        prop_assert!(
            md.len() < max_expected,
            "comment length {} exceeded expected bound {}",
            md.len(),
            max_expected
        );
    }

    /// Comment with zero sensors still renders with markers and sections.
    #[test]
    fn empty_sensors_comment_valid(cfg in any_cockpit_config()) {
        let report = CockpitReport {
            schema: "cockpit.report.v1".to_string(),
            tool: ToolInfo {
                name: "cockpitctl".to_string(),
                version: "0.0.0-test".to_string(),
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
                status: VerdictStatus::Pass,
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
            data: None,
        };
        let md = render_comment(&report, &cfg);
        prop_assert!(md.contains("<!-- cockpit:begin -->"));
        prop_assert!(md.contains("<!-- cockpit:end -->"));
        prop_assert!(md.contains("## Cockpit"));
    }
}

// ============================================================================
// Property: annotation count is exactly min(highlights, max_annotations)
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    /// Rendered annotation count is exactly min(highlights.len(), max_annotations).
    #[test]
    fn annotation_count_exact(
        highlights in any_highlights(30),
        cfg in any_cockpit_config(),
    ) {
        let blocking = BTreeMap::new();
        let result = render_annotations(&highlights, &cfg, &blocking);
        let expected = highlights.len().min(cfg.policy.max_annotations);
        prop_assert_eq!(
            result.rendered_count, expected,
            "rendered_count must be min(highlights, max_annotations)"
        );
    }

    /// GitHub annotation line count equals rendered_count.
    #[test]
    fn github_annotation_lines_match_count(
        highlights in any_highlights(30),
        cfg in any_cockpit_config(),
    ) {
        let blocking = BTreeMap::new();
        let result = render_github_annotations(&highlights, &cfg, &blocking);
        prop_assert_eq!(
            result.lines.len(),
            result.rendered_count,
            "lines.len() must equal rendered_count"
        );
    }
}

// ============================================================================
// Property: max_annotations=0 produces empty output
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// With max_annotations=0, no annotations are rendered.
    #[test]
    fn zero_budget_produces_empty_annotations(
        highlights in any_highlights(20),
    ) {
        // We can't set max_annotations to 0 via strategy since it's 1..30,
        // so build config manually.
        let mut cfg = CockpitConfig::default();
        cfg.policy.max_annotations = 0;

        let blocking = BTreeMap::new();
        let result = render_annotations(&highlights, &cfg, &blocking);
        prop_assert_eq!(result.rendered_count, 0);

        let gh_result = render_github_annotations(&highlights, &cfg, &blocking);
        prop_assert_eq!(gh_result.rendered_count, 0);
        prop_assert!(gh_result.lines.is_empty());
    }
}

// ============================================================================
// Property: render_comment is non-empty for any valid report
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// render_comment always produces non-empty output.
    #[test]
    fn comment_always_non_empty(
        report in any_cockpit_report(8),
        cfg in any_cockpit_config(),
    ) {
        let md = render_comment(&report, &cfg);
        prop_assert!(!md.is_empty(), "rendered comment must not be empty");
        // Minimum viable comment has markers + heading.
        prop_assert!(md.len() > 50, "comment must be at least 50 chars");
    }
}
