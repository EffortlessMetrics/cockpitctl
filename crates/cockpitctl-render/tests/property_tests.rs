//! Property-based tests for cockpitctl-render.
//!
//! Tests determinism, budget invariants, marker stability, and no-panic guarantees.

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
    prop::option::of((
        prop::option::of("[a-z/_.-]{1,50}"),
        prop::option::of(1u32..10000u32),
        prop::option::of(1u32..1000u32),
    ))
    .prop_map(|opt| opt.map(|(path, line, col)| Location { path, line, col }))
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

fn any_verdict_counts() -> impl Strategy<Value = VerdictCounts> {
    (0u64..100, 0u64..100, 0u64..100, 0u64..10).prop_map(|(info, warn, error, suppressed)| {
        VerdictCounts {
            info,
            warn,
            error,
            suppressed,
        }
    })
}

fn any_verdict() -> impl Strategy<Value = Verdict> {
    (
        any_verdict_status(),
        any_verdict_counts(),
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

fn any_sensor_policy() -> impl Strategy<Value = SensorPolicy> {
    (
        any::<bool>(),
        prop_oneof![
            Just(MissingPolicy::Skip),
            Just(MissingPolicy::Warn),
            Just(MissingPolicy::Fail),
        ],
        prop::option::of("[A-Z][a-z]{0,20}"),
    )
        .prop_map(|(blocking, missing, section)| SensorPolicy {
            blocking,
            missing,
            section,
            require_label: None,
            repro: None,
        })
}

fn any_cockpit_config() -> impl Strategy<Value = CockpitConfig> {
    (
        any::<bool>(),
        1usize..20,
        1usize..50,
        1usize..50,
        prop::collection::vec("[A-Z][a-z]{0,15}", 0..5),
        prop::collection::btree_map("[a-z_][a-z0-9_-]{0,15}", any_sensor_policy(), 0..5),
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

fn any_cockpit_report() -> impl Strategy<Value = CockpitReport> {
    (
        any_verdict(),
        prop::collection::vec(any_sensor_summary(), 0..8),
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
// render_comment: stable markers present
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// Rendered comment always contains begin and end markers.
    #[test]
    fn comment_always_has_markers(
        report in any_cockpit_report(),
        cfg in any_cockpit_config(),
    ) {
        let md = render_comment(&report, &cfg);
        prop_assert!(md.contains("<!-- cockpit:begin -->"), "missing begin marker");
        prop_assert!(md.contains("<!-- cockpit:end -->"), "missing end marker");
    }
}

// ============================================================================
// render_comment: idempotent / deterministic
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// Same input always produces identical output.
    #[test]
    fn comment_is_deterministic(
        report in any_cockpit_report(),
        cfg in any_cockpit_config(),
    ) {
        let a = render_comment(&report, &cfg);
        let b = render_comment(&report, &cfg);
        prop_assert_eq!(a, b, "render_comment must be deterministic");
    }
}

// ============================================================================
// render_comment: no panic on arbitrary input
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    /// render_comment never panics regardless of input.
    #[test]
    fn comment_no_panic(
        report in any_cockpit_report(),
        cfg in any_cockpit_config(),
    ) {
        let _ = render_comment(&report, &cfg);
    }
}

// ============================================================================
// render_annotations: budget / capping invariant
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    /// Rendered annotation count never exceeds max_annotations.
    #[test]
    fn annotations_respect_budget(
        highlights in any_highlights(40),
        cfg in any_cockpit_config(),
    ) {
        let blocking = BTreeMap::new();
        let result = render_annotations(&highlights, &cfg, &blocking);
        prop_assert!(
            result.rendered_count <= cfg.policy.max_annotations,
            "rendered_count {} exceeds max_annotations {}",
            result.rendered_count,
            cfg.policy.max_annotations,
        );
        prop_assert_eq!(result.total_count, highlights.len());
    }

    /// Annotation rendering is deterministic.
    #[test]
    fn annotations_deterministic(
        highlights in any_highlights(20),
        cfg in any_cockpit_config(),
    ) {
        let blocking = BTreeMap::new();
        let a = render_annotations(&highlights, &cfg, &blocking);
        let b = render_annotations(&highlights, &cfg, &blocking);
        prop_assert_eq!(a.content, b.content, "annotations must be deterministic");
        prop_assert_eq!(a.truncated, b.truncated);
        prop_assert_eq!(a.rendered_count, b.rendered_count);
    }
}

// ============================================================================
// render_annotations: truncation message correctness
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    /// When highlights exceed max_annotations, the truncation notice appears.
    #[test]
    fn truncation_message_when_over_budget(
        highlights in prop::collection::vec(any_highlight(), 2..40),
        max_annotations in 1usize..5,
    ) {
        prop_assume!(highlights.len() > max_annotations);

        let mut cfg = CockpitConfig::default();
        cfg.policy.max_annotations = max_annotations;

        let blocking = BTreeMap::new();
        let result = render_annotations(&highlights, &cfg, &blocking);
        prop_assert!(result.truncated, "must be truncated");
        prop_assert!(
            result.content.contains("capped by `max_annotations`"),
            "truncation notice must appear when over budget",
        );
    }

    /// When highlights fit within budget, no truncation notice.
    #[test]
    fn no_truncation_message_when_under_budget(
        highlights in any_highlights(10),
    ) {
        let mut cfg = CockpitConfig::default();
        cfg.policy.max_annotations = highlights.len() + 10;

        let blocking = BTreeMap::new();
        let result = render_annotations(&highlights, &cfg, &blocking);
        prop_assert!(!result.truncated, "should not be truncated");
        prop_assert!(
            !result.content.contains("capped by"),
            "truncation notice must not appear when under budget",
        );
    }
}

// ============================================================================
// render_github_annotations: budget invariant + no panic
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// GitHub annotations never exceed max_annotations.
    #[test]
    fn github_annotations_respect_budget(
        highlights in any_highlights(40),
        cfg in any_cockpit_config(),
    ) {
        let blocking = BTreeMap::new();
        let result = render_github_annotations(&highlights, &cfg, &blocking);
        prop_assert!(
            result.rendered_count <= cfg.policy.max_annotations,
            "rendered_count {} exceeds max {}",
            result.rendered_count,
            cfg.policy.max_annotations,
        );
        prop_assert_eq!(result.lines.len(), result.rendered_count);
    }

    /// GitHub annotations never panic on arbitrary input.
    #[test]
    fn github_annotations_no_panic(
        highlights in any_highlights(30),
        cfg in any_cockpit_config(),
    ) {
        let blocking = BTreeMap::new();
        let _ = render_github_annotations(&highlights, &cfg, &blocking);
    }

    /// GitHub annotations are deterministic.
    #[test]
    fn github_annotations_deterministic(
        highlights in any_highlights(20),
        cfg in any_cockpit_config(),
    ) {
        let blocking = BTreeMap::new();
        let a = render_github_annotations(&highlights, &cfg, &blocking);
        let b = render_github_annotations(&highlights, &cfg, &blocking);
        prop_assert_eq!(a.lines, b.lines, "github annotations must be deterministic");
    }
}

// ============================================================================
// render_comment: structural invariants
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// Comment always contains required section headers.
    #[test]
    fn comment_has_required_sections(
        report in any_cockpit_report(),
        cfg in any_cockpit_config(),
    ) {
        let md = render_comment(&report, &cfg);
        prop_assert!(md.contains("## Cockpit"), "missing Cockpit heading");
        prop_assert!(md.contains("### Summary"), "missing Summary section");
        prop_assert!(md.contains("### Highlights"), "missing Highlights section");
        prop_assert!(md.contains("### Annotations"), "missing Annotations section");
    }

    /// Begin marker appears before end marker.
    #[test]
    fn markers_in_correct_order(
        report in any_cockpit_report(),
        cfg in any_cockpit_config(),
    ) {
        let md = render_comment(&report, &cfg);
        let begin = md.find("<!-- cockpit:begin -->").unwrap();
        let end = md.find("<!-- cockpit:end -->").unwrap();
        prop_assert!(begin < end, "begin marker must precede end marker");
    }

    /// All sensor IDs from the report appear in the rendered comment.
    #[test]
    fn comment_includes_all_sensor_ids(
        report in any_cockpit_report(),
        cfg in any_cockpit_config(),
    ) {
        let md = render_comment(&report, &cfg);
        for s in &report.sensors {
            prop_assert!(
                md.contains(&s.id),
                "sensor id '{}' must appear in comment",
                s.id,
            );
        }
    }
}
