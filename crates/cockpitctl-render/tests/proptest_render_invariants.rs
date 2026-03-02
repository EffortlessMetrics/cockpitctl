//! Property-based tests for render budget and marker invariants.
//!
//! Tests not covered by existing proptest_budgets.rs / proptest_budget.rs:
//! - GitHub comment size limit (65535 chars)
//! - Marker invariants hold under extreme inputs
//! - Truncation always produces valid markdown structure

use cockpitctl_render::render_comment;
use cockpitctl_types::{
    CockpitConfig, CockpitReport, Finding, Highlight, Location, MissingPolicy, Policy,
    PolicySnapshot, Presence, RunInfo, SensorSummary, Severity, ToolInfo, Verdict, VerdictCounts,
    VerdictStatus,
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

fn any_finding() -> impl Strategy<Value = Finding> {
    (
        any_severity(),
        "[A-Z][A-Z0-9_]{0,15}",
        ".{1,80}",
        prop::option::of(
            (
                prop::option::of("[a-z/_.-]{1,30}"),
                prop::option::of(1u32..10000),
            )
                .prop_map(|(path, line)| Location {
                    path,
                    line,
                    col: None,
                }),
        ),
    )
        .prop_map(|(severity, code, message, location)| Finding {
            severity,
            check_id: None,
            code,
            message,
            location,
            help: None,
            url: None,
            fingerprint: None,
            data: None,
        })
}

fn any_highlight() -> impl Strategy<Value = Highlight> {
    ("[a-z_][a-z0-9_-]{0,15}", any_finding())
        .prop_map(|(sensor_id, finding)| Highlight { sensor_id, finding })
}

fn any_sensor_summary() -> impl Strategy<Value = SensorSummary> {
    (
        "[a-z_][a-z0-9_-]{0,15}",
        any::<bool>(),
        any_verdict_status(),
    )
        .prop_map(|(id, blocking, status)| SensorSummary {
            id: id.clone(),
            blocking,
            missing: MissingPolicy::Skip,
            presence: Presence::Present,
            report_path: format!("artifacts/{id}/report.json"),
            comment_path: None,
            verdict: Verdict {
                status,
                counts: VerdictCounts::default(),
                reasons: vec![],
            },
            truncated: false,
            errors: vec![],
            missing_policy_applied: None,
            policy_outcome: None,
        })
}

fn any_cockpit_report_bounded(
    max_sensors: usize,
    max_highlights: usize,
) -> impl Strategy<Value = CockpitReport> {
    (
        any_verdict_status(),
        prop::collection::vec(any_sensor_summary(), 0..=max_sensors),
        prop::collection::vec(any_highlight(), 0..=max_highlights),
    )
        .prop_map(|(status, sensors, highlights)| CockpitReport {
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
                status,
                counts: VerdictCounts::default(),
                reasons: vec![],
            },
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

fn any_cockpit_config_with_budget() -> impl Strategy<Value = CockpitConfig> {
    (any::<bool>(), 1usize..10, 1usize..30, 1usize..20).prop_map(
        |(warn_is_fail, max_highlights, max_per_sensor_findings, max_annotations)| CockpitConfig {
            policy: Policy {
                warn_is_fail,
                max_highlights,
                max_per_sensor_findings,
                max_annotations,
                section_order: vec![],
                schema_validation: Default::default(),
                max_receipt_size_bytes: 2 * 1024 * 1024,
            },
            sensors: BTreeMap::new(),
            ..Default::default()
        },
    )
}

// ============================================================================
// GitHub comment size never exceeds 65535 chars
// ============================================================================

/// GitHub PR comments are limited to 65535 chars. With budgeted highlights and
/// annotations, the rendered output should stay well under this limit for any
/// realistic input.
const GITHUB_COMMENT_MAX_CHARS: usize = 65_535;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn render_output_within_github_limit(
        report in any_cockpit_report_bounded(20, 20),
        cfg in any_cockpit_config_with_budget(),
    ) {
        let md = render_comment(&report, &cfg);
        prop_assert!(
            md.len() <= GITHUB_COMMENT_MAX_CHARS,
            "rendered comment length {} exceeds GitHub limit {}",
            md.len(),
            GITHUB_COMMENT_MAX_CHARS,
        );
    }

    /// Even with many sensors and highlights, budgets keep output bounded.
    #[test]
    fn render_bounded_with_many_inputs(
        report in any_cockpit_report_bounded(50, 30),
        cfg in any_cockpit_config_with_budget(),
    ) {
        let md = render_comment(&report, &cfg);
        // The output must always be finite and contain markers.
        prop_assert!(!md.is_empty());
        prop_assert!(md.contains("<!-- cockpit:begin -->"));
        prop_assert!(md.contains("<!-- cockpit:end -->"));
    }
}

// ============================================================================
// Marker structure invariants
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    /// Output always contains exactly one begin and one end marker.
    #[test]
    fn exactly_one_begin_and_end_marker(
        report in any_cockpit_report_bounded(10, 10),
        cfg in any_cockpit_config_with_budget(),
    ) {
        let md = render_comment(&report, &cfg);
        let begin_count = md.matches("<!-- cockpit:begin -->").count();
        let end_count = md.matches("<!-- cockpit:end -->").count();
        prop_assert_eq!(begin_count, 1, "expected exactly 1 begin marker, found {}", begin_count);
        prop_assert_eq!(end_count, 1, "expected exactly 1 end marker, found {}", end_count);
    }

    /// The Summary section header is always present.
    #[test]
    fn summary_section_always_present(
        report in any_cockpit_report_bounded(10, 10),
        cfg in any_cockpit_config_with_budget(),
    ) {
        let md = render_comment(&report, &cfg);
        prop_assert!(md.contains("### Summary"), "missing Summary section");
    }

    /// The Highlights section header is always present.
    #[test]
    fn highlights_section_always_present(
        report in any_cockpit_report_bounded(10, 10),
        cfg in any_cockpit_config_with_budget(),
    ) {
        let md = render_comment(&report, &cfg);
        prop_assert!(md.contains("### Highlights"), "missing Highlights section");
    }

    /// The Highlights section contains one entry per highlight in the report.
    /// We verify by counting lines between "### Highlights" and the next "###".
    #[test]
    fn highlights_section_count_matches_report(
        highlights in prop::collection::vec(any_highlight(), 0..10),
    ) {
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
            highlights: highlights.clone(),
            policy: PolicySnapshot {
                warn_is_fail: false,
                max_highlights: 20,
                max_per_sensor_findings: 20,
                max_annotations: 25,
                section_order: vec![],
                sensors: vec![],
            },
            data: None,
        };
        let cfg = CockpitConfig::default();
        let md = render_comment(&report, &cfg);

        // Extract lines between "### Highlights" and the next "###" header
        let lines: Vec<&str> = md.lines().collect();
        let hl_start = lines.iter().position(|l| l.starts_with("### Highlights"));
        let hl_start = hl_start.expect("Highlights header must exist");
        let hl_end = lines[hl_start + 1..]
            .iter()
            .position(|l| l.starts_with("###"))
            .map(|p| p + hl_start + 1)
            .unwrap_or(lines.len());

        // Count numbered lines within the Highlights section
        let numbered_count = lines[hl_start..hl_end]
            .iter()
            .filter(|l| {
                let t = l.trim();
                t.chars().next().is_some_and(|c| c.is_ascii_digit()) && t.contains(". ")
            })
            .count();

        prop_assert_eq!(
            numbered_count,
            highlights.len(),
            "Highlights section has {} numbered lines but report has {} highlights",
            numbered_count,
            highlights.len(),
        );
    }
}
