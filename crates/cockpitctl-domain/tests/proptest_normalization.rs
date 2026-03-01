//! Property-based tests for normalization and summarization in cockpitctl-domain.
//!
//! Tests idempotency of summarize_sensor_report, compute_policy_outcome symmetry,
//! and build_cockpit_report determinism.

use cockpitctl_domain::{
    build_cockpit_report, compute_counts, compute_policy_outcome, sort_findings,
    summarize_sensor_report,
};
use cockpitctl_types::{
    CockpitConfig, Finding, Location, MissingPolicy, PolicyOutcome, Presence, RunInfo,
    SensorPolicy, SensorReport, SensorSummary, Severity, ToolInfo, Verdict, VerdictCounts,
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

fn any_location() -> impl Strategy<Value = Option<Location>> {
    prop::option::of(
        (
            prop::option::of("[a-z/_.-]{1,30}"),
            prop::option::of(1u32..10000),
            prop::option::of(1u32..500),
        )
            .prop_map(|(path, line, col)| Location { path, line, col }),
    )
}

fn any_finding() -> impl Strategy<Value = Finding> {
    (
        any_severity(),
        prop::option::of("[A-Z][A-Z0-9_]{0,10}"),
        "[A-Z][A-Z0-9_]{0,15}",
        ".{1,50}",
        any_location(),
    )
        .prop_map(|(severity, check_id, code, message, location)| Finding {
            severity,
            check_id,
            code,
            message,
            location,
            help: None,
            url: None,
            fingerprint: None,
            data: None,
        })
}

fn any_findings(max: usize) -> impl Strategy<Value = Vec<Finding>> {
    prop::collection::vec(any_finding(), 0..=max)
}

fn tool_info() -> ToolInfo {
    ToolInfo {
        name: "test".to_string(),
        version: "1.0.0".to_string(),
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

fn any_sensor_report() -> impl Strategy<Value = SensorReport> {
    (any_verdict_status(), any_findings(15)).prop_map(|(status, findings)| {
        let counts = compute_counts(&findings);
        SensorReport {
            schema: "sensor.report.v1".to_string(),
            tool: tool_info(),
            run: run_info(),
            verdict: Verdict {
                status,
                counts,
                reasons: vec![],
            },
            findings,
            artifacts: vec![],
            data: None,
        }
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
    )
        .prop_map(|(blocking, missing)| SensorPolicy {
            blocking,
            missing,
            section: None,
            require_label: None,
            repro: None,
        })
}

// ============================================================================
// summarize_sensor_report: idempotent sort
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// summarize_sensor_report produces the same summary when called twice
    /// with the same inputs (determinism).
    #[test]
    fn summarize_is_deterministic(
        sensor_id in "[a-z][a-z0-9]{0,8}",
        report in any_sensor_report(),
        policy in any_sensor_policy(),
        max_findings in 1usize..30,
    ) {
        let (summary1, highlights1) = summarize_sensor_report(
            &sensor_id,
            &format!("artifacts/{}/report.json", sensor_id),
            None,
            &policy,
            report.clone(),
            max_findings,
        );
        let (summary2, highlights2) = summarize_sensor_report(
            &sensor_id,
            &format!("artifacts/{}/report.json", sensor_id),
            None,
            &policy,
            report,
            max_findings,
        );

        prop_assert_eq!(&summary1.id, &summary2.id);
        prop_assert_eq!(&summary1.verdict, &summary2.verdict);
        prop_assert_eq!(summary1.truncated, summary2.truncated);
        prop_assert_eq!(highlights1.len(), highlights2.len());
        for (a, b) in highlights1.iter().zip(highlights2.iter()) {
            prop_assert_eq!(&a.finding.code, &b.finding.code);
            prop_assert_eq!(&a.finding.message, &b.finding.message);
        }
    }

    /// After summarize, all highlight findings are already sorted by severity.
    #[test]
    fn summarize_highlights_sorted_by_severity(
        sensor_id in "[a-z][a-z0-9]{0,8}",
        report in any_sensor_report(),
        policy in any_sensor_policy(),
    ) {
        let (_, highlights) = summarize_sensor_report(
            &sensor_id,
            &format!("artifacts/{}/report.json", sensor_id),
            None,
            &policy,
            report,
            50,
        );

        let mut prev_rank = 0u8;
        for h in &highlights {
            let rank = cockpitctl_types::severity_rank(&h.finding.severity);
            prop_assert!(
                rank >= prev_rank,
                "highlights must be sorted by severity (desc)"
            );
            prev_rank = rank;
        }
    }

    /// summarize_sensor_report never produces more highlights than max_findings.
    #[test]
    fn summarize_respects_max_findings(
        sensor_id in "[a-z][a-z0-9]{0,8}",
        report in any_sensor_report(),
        policy in any_sensor_policy(),
        max_findings in 1usize..20,
    ) {
        let (summary, highlights) = summarize_sensor_report(
            &sensor_id,
            &format!("artifacts/{}/report.json", sensor_id),
            None,
            &policy,
            report,
            max_findings,
        );

        // Highlights from the sensor's own findings are capped at max_findings
        // (may have +1 for inconsistency highlight).
        let sensor_findings: Vec<_> = highlights
            .iter()
            .filter(|h| h.finding.code != "cockpit.receipt_inconsistent")
            .collect();
        prop_assert!(
            sensor_findings.len() <= max_findings,
            "sensor highlights {} must be <= max_findings {}",
            sensor_findings.len(),
            max_findings
        );

        // summary.truncated flag must be correct.
        if summary.truncated {
            prop_assert!(
                sensor_findings.len() == max_findings,
                "if truncated, must have exactly max_findings sensor findings"
            );
        }
    }
}

// ============================================================================
// compute_policy_outcome: exhaustive property
// ============================================================================

proptest! {
    /// Non-blocking sensors always produce Informational outcome.
    #[test]
    fn non_blocking_is_always_informational(status in any_verdict_status()) {
        let outcome = compute_policy_outcome(false, &status);
        prop_assert_eq!(outcome, PolicyOutcome::Informational);
    }

    /// Blocking sensor with Fail produces Blocked.
    #[test]
    fn blocking_fail_is_blocked(_dummy in Just(())) {
        let outcome = compute_policy_outcome(true, &VerdictStatus::Fail);
        prop_assert_eq!(outcome, PolicyOutcome::Blocked);
    }

    /// Blocking sensor with non-Fail produces Allowed.
    #[test]
    fn blocking_non_fail_is_allowed(
        status in prop_oneof![
            Just(VerdictStatus::Pass),
            Just(VerdictStatus::Warn),
            Just(VerdictStatus::Skip),
        ]
    ) {
        let outcome = compute_policy_outcome(true, &status);
        prop_assert_eq!(outcome, PolicyOutcome::Allowed);
    }
}

// ============================================================================
// sort_findings: permutation invariant (sorting any permutation yields same)
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// Sorting any permutation of the same findings set produces the same result.
    #[test]
    fn sort_findings_permutation_invariant(
        sensor_id in "[a-z_]{1,10}",
        findings in any_findings(20),
        seed in any::<u64>(),
    ) {
        let mut sorted_a = findings.clone();
        sort_findings(&sensor_id, &mut sorted_a);

        // Create a simple permutation by rotating based on seed.
        let mut permuted = findings;
        if !permuted.is_empty() {
            let rotate_by = (seed as usize) % permuted.len();
            permuted.rotate_left(rotate_by);
        }
        sort_findings(&sensor_id, &mut permuted);

        // Both should produce the same sorted sequence.
        prop_assert_eq!(sorted_a.len(), permuted.len());
        for (a, b) in sorted_a.iter().zip(permuted.iter()) {
            prop_assert_eq!(&a.code, &b.code);
            prop_assert_eq!(&a.message, &b.message);
            prop_assert_eq!(&a.severity, &b.severity);
        }
    }
}

// ============================================================================
// build_cockpit_report: determinism
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]

    /// Building a cockpit report twice from the same inputs is byte-identical.
    #[test]
    fn build_cockpit_report_deterministic(
        sensor_ids in prop::collection::vec("[a-z][a-z0-9]{0,6}", 1..5),
    ) {
        let mut seen = std::collections::HashSet::new();
        let ids: Vec<String> = sensor_ids
            .into_iter()
            .filter(|id| seen.insert(id.clone()))
            .collect();

        let mut cfg = CockpitConfig::default();
        let summaries: Vec<SensorSummary> = ids
            .iter()
            .map(|id| {
                cfg.sensors.insert(
                    id.clone(),
                    SensorPolicy {
                        blocking: true,
                        missing: MissingPolicy::Fail,
                        section: None,
                        require_label: None,
                        repro: None,
                    },
                );
                SensorSummary {
                    id: id.clone(),
                    blocking: true,
                    missing: MissingPolicy::Fail,
                    presence: Presence::Present,
                    report_path: format!("artifacts/{}/report.json", id),
                    comment_path: None,
                    verdict: Verdict {
                        status: VerdictStatus::Pass,
                        counts: VerdictCounts::default(),
                        reasons: vec![],
                    },
                    truncated: false,
                    errors: vec![],
                    missing_policy_applied: None,
                    policy_outcome: Some(PolicyOutcome::Allowed),
                }
            })
            .collect();

        let report_a = build_cockpit_report(
            &cfg,
            tool_info(),
            run_info(),
            summaries.clone(),
            vec![],
        );
        let report_b = build_cockpit_report(
            &cfg,
            tool_info(),
            run_info(),
            summaries,
            vec![],
        );

        let json_a = serde_json::to_string_pretty(&report_a).unwrap();
        let json_b = serde_json::to_string_pretty(&report_b).unwrap();
        prop_assert_eq!(json_a, json_b, "build_cockpit_report must be deterministic");
    }
}
