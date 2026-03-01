//! Property-based tests for domain invariants.
//!
//! Covers: determinism, monotonicity, idempotence, isolation, commutativity,
//! budget enforcement, subset guarantees, and collision resistance.

use cockpitctl_domain::{
    cap_findings, compute_counts, compute_policy_outcome, derive_fingerprint, finding_sort_key,
    overall_verdict, select_highlights, sort_findings, summarize_sensor_report,
};
use cockpitctl_types::{
    CockpitConfig, Finding, Highlight, Location, MissingPolicy, Policy, Presence, RunInfo,
    SensorPolicy, SensorReport, SensorSummary, Severity, ToolInfo, Verdict, VerdictCounts,
    VerdictStatus, verdict_status_rank,
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

fn any_highlight() -> impl Strategy<Value = Highlight> {
    ("[a-z_][a-z0-9_-]{0,20}", any_finding())
        .prop_map(|(sensor_id, finding)| Highlight { sensor_id, finding })
}

fn any_highlights(max_len: usize) -> impl Strategy<Value = Vec<Highlight>> {
    prop::collection::vec(any_highlight(), 0..=max_len)
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
        any_verdict_status(),
        (0u64..100, 0u64..100, 0u64..100),
    )
        .prop_map(
            |(id, blocking, missing, presence, status, (info, warn, error))| SensorSummary {
                id: id.clone(),
                blocking,
                missing,
                presence,
                report_path: format!("artifacts/{}/report.json", id),
                comment_path: None,
                verdict: Verdict {
                    status,
                    counts: VerdictCounts {
                        info,
                        warn,
                        error,
                        suppressed: 0,
                    },
                    reasons: vec![],
                },
                truncated: false,
                errors: vec![],
                missing_policy_applied: None,
                policy_outcome: None,
            },
        )
}

fn any_sensor_summaries(max_len: usize) -> impl Strategy<Value = Vec<SensorSummary>> {
    prop::collection::vec(any_sensor_summary(), 0..=max_len)
}

fn any_cockpit_config() -> impl Strategy<Value = CockpitConfig> {
    (
        any::<bool>(),
        1usize..20,
        1usize..50,
        prop::collection::vec("[A-Z][a-z]{0,15}", 0..5),
        prop::collection::btree_map("[a-z_][a-z0-9_-]{0,15}", any_sensor_policy(), 0..5),
    )
        .prop_map(
            |(warn_is_fail, max_highlights, max_per_sensor_findings, section_order, sensors)| {
                CockpitConfig {
                    policy: Policy {
                        warn_is_fail,
                        max_highlights,
                        max_per_sensor_findings,
                        max_annotations: 25,
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

fn make_blocking_summary(id: &str, status: VerdictStatus) -> SensorSummary {
    SensorSummary {
        id: id.to_string(),
        blocking: true,
        missing: MissingPolicy::Skip,
        presence: Presence::Present,
        report_path: format!("artifacts/{}/report.json", id),
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
    }
}

// ============================================================================
// 1. Policy evaluation is deterministic
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// compute_policy_outcome returns the same result for identical inputs.
    #[test]
    fn policy_evaluation_deterministic(blocking in any::<bool>(), status in any_verdict_status()) {
        let a = compute_policy_outcome(blocking, &status);
        let b = compute_policy_outcome(blocking, &status);
        prop_assert_eq!(a, b, "policy evaluation must be deterministic");
    }
}

// ============================================================================
// 2. Adding a failing blocking sensor can only worsen or maintain verdict
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn adding_failing_sensor_worsens_or_maintains_verdict(
        summaries in any_sensor_summaries(5),
        cfg in any_cockpit_config(),
    ) {
        let verdict_before = overall_verdict(&summaries, &cfg);
        let rank_before = verdict_status_rank(&verdict_before.status);

        let mut with_fail = summaries;
        with_fail.push(make_blocking_summary("injected_fail", VerdictStatus::Fail));

        let verdict_after = overall_verdict(&with_fail, &cfg);
        let rank_after = verdict_status_rank(&verdict_after.status);

        // Lower rank = worse (Fail=0). Adding Fail can only lower or keep rank.
        prop_assert!(
            rank_after <= rank_before,
            "adding a failing sensor must not improve verdict: rank {} -> {}",
            rank_before, rank_after
        );
    }
}

// ============================================================================
// 3. Adding a passing blocking sensor cannot worsen verdict
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn adding_passing_sensor_cannot_worsen_verdict(
        summaries in any_sensor_summaries(5),
        cfg in any_cockpit_config(),
    ) {
        let verdict_before = overall_verdict(&summaries, &cfg);
        let rank_before = verdict_status_rank(&verdict_before.status);

        let mut with_pass = summaries;
        with_pass.push(make_blocking_summary("injected_pass", VerdictStatus::Pass));

        let verdict_after = overall_verdict(&with_pass, &cfg);
        let rank_after = verdict_status_rank(&verdict_after.status);

        // Pass has rank 2. Adding Pass to a set can never make things worse.
        prop_assert!(
            rank_after >= rank_before,
            "adding a passing sensor must not worsen verdict: rank {} -> {}",
            rank_before, rank_after
        );
    }
}

// ============================================================================
// 4. Removing a non-blocking sensor doesn't change verdict status
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn removing_non_blocking_sensor_preserves_verdict_status(
        base_summaries in any_sensor_summaries(5),
        extra in any_sensor_summary(),
    ) {
        let cfg = CockpitConfig::default();
        let verdict_without = overall_verdict(&base_summaries, &cfg);

        let mut with_extra = base_summaries;
        let mut non_blocking = extra;
        non_blocking.blocking = false;
        with_extra.push(non_blocking);

        let verdict_with = overall_verdict(&with_extra, &cfg);

        prop_assert_eq!(
            verdict_without.status, verdict_with.status,
            "removing a non-blocking sensor must not change verdict status"
        );
    }
}

// ============================================================================
// 5. Highlight selection is deterministic
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn highlight_selection_deterministic(
        highlights in any_highlights(20),
        cfg in any_cockpit_config(),
    ) {
        let blocking = BTreeMap::new();
        let a = select_highlights(highlights.clone(), &cfg, &blocking);
        let b = select_highlights(highlights, &cfg, &blocking);
        prop_assert_eq!(a, b, "highlight selection must be deterministic");
    }
}

// ============================================================================
// 6. Highlight budget is respected
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn highlight_budget_respected(
        highlights in any_highlights(30),
        cfg in any_cockpit_config(),
    ) {
        let blocking = BTreeMap::new();
        let selected = select_highlights(highlights, &cfg, &blocking);
        prop_assert!(
            selected.len() <= cfg.policy.max_highlights,
            "highlights {} must be <= budget {}",
            selected.len(), cfg.policy.max_highlights
        );
    }
}

// ============================================================================
// 7. Highlights are a subset of input findings (no phantom highlights)
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn highlights_subset_of_findings(
        sensor_id in "[a-z][a-z0-9]{0,8}",
        report in any_sensor_report(),
        policy in any_sensor_policy(),
        max_findings in 1usize..30,
    ) {
        let original_findings = report.findings.clone();
        let (_, highlights) = summarize_sensor_report(
            &sensor_id,
            &format!("artifacts/{}/report.json", sensor_id),
            None,
            &policy,
            report,
            max_findings,
        );

        for h in &highlights {
            // Skip cockpit-synthesized findings (e.g. receipt_inconsistent).
            if h.finding.code.starts_with("cockpit.") {
                continue;
            }
            // Every non-cockpit highlight finding must have come from the input.
            let found = original_findings.iter().any(|f| {
                f.code == h.finding.code
                    && f.message == h.finding.message
                    && f.severity == h.finding.severity
            });
            prop_assert!(
                found,
                "highlight with code={} message={} not found in original findings",
                h.finding.code, h.finding.message
            );
        }
    }
}

// ============================================================================
// 8. Finding sort is total order (no panics on arbitrary inputs)
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    #[test]
    fn finding_sort_total_order(
        sensor_id in "[a-z_]{1,10}",
        a in any_finding(),
        b in any_finding(),
        c in any_finding(),
    ) {
        let ka = finding_sort_key(&sensor_id, &a);
        let kb = finding_sort_key(&sensor_id, &b);
        let kc = finding_sort_key(&sensor_id, &c);

        // Transitivity: if a <= b and b <= c then a <= c
        if ka <= kb && kb <= kc {
            prop_assert!(ka <= kc, "sort key must be transitive");
        }
        // Totality: exactly one of a < b, a == b, a > b holds (no panic)
        let _ = ka.cmp(&kb);
        let _ = kb.cmp(&kc);
    }
}

// ============================================================================
// 9. Finding sort is deterministic
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn finding_sort_deterministic(
        sensor_id in "[a-z_]{1,10}",
        findings in any_findings(20),
    ) {
        let mut a = findings.clone();
        let mut b = findings;
        sort_findings(&sensor_id, &mut a);
        sort_findings(&sensor_id, &mut b);
        prop_assert_eq!(a, b, "sort must be deterministic");
    }
}

// ============================================================================
// 10. Fingerprint is deterministic
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    #[test]
    fn fingerprint_deterministic(
        sensor_id in "[a-z_][a-z0-9_]{0,15}",
        finding in any_finding(),
    ) {
        let a = derive_fingerprint(&sensor_id, &finding);
        let b = derive_fingerprint(&sensor_id, &finding);
        prop_assert_eq!(a, b, "fingerprint must be deterministic");
    }
}

// ============================================================================
// 11. Fingerprint differs for different findings (collision resistance)
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    #[test]
    fn fingerprint_collision_resistance(
        sensor_id in "[a-z_]{1,10}",
        msg_a in ".{1,30}",
        msg_b in ".{1,30}",
        severity in any_severity(),
    ) {
        prop_assume!(msg_a != msg_b);
        let f_a = Finding {
            severity: severity.clone(),
            check_id: None,
            code: "TEST".to_string(),
            message: msg_a,
            location: None,
            help: None, url: None, fingerprint: None, data: None,
        };
        let f_b = Finding {
            severity,
            check_id: None,
            code: "TEST".to_string(),
            message: msg_b,
            location: None,
            help: None, url: None, fingerprint: None, data: None,
        };
        let fp_a = derive_fingerprint(&sensor_id, &f_a);
        let fp_b = derive_fingerprint(&sensor_id, &f_b);
        prop_assert_ne!(fp_a, fp_b, "different messages must produce different fingerprints");
    }
}

// ============================================================================
// 12. Normalization is idempotent (sort + cap applied twice = applied once)
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn normalization_idempotent(
        sensor_id in "[a-z_]{1,10}",
        findings in any_findings(30),
        max in 1usize..30,
    ) {
        // First pass: sort + cap
        let mut first = findings;
        sort_findings(&sensor_id, &mut first);
        let (first_capped, _) = cap_findings(first, max);

        // Second pass on already-normalized data
        let mut second = first_capped.clone();
        sort_findings(&sensor_id, &mut second);
        let (second_capped, _) = cap_findings(second, max);

        prop_assert_eq!(
            first_capped, second_capped,
            "normalization (sort+cap) must be idempotent"
        );
    }
}

// ============================================================================
// 13. Verdict composition is commutative (sensor order doesn't matter)
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn verdict_composition_commutative(
        summaries in any_sensor_summaries(6),
        cfg in any_cockpit_config(),
    ) {
        let verdict_forward = overall_verdict(&summaries, &cfg);

        let mut reversed = summaries;
        reversed.reverse();
        let verdict_reversed = overall_verdict(&reversed, &cfg);

        prop_assert_eq!(
            verdict_forward.status, verdict_reversed.status,
            "verdict status must be commutative over sensor order"
        );
        prop_assert_eq!(
            verdict_forward.counts, verdict_reversed.counts,
            "verdict counts must be commutative over sensor order"
        );
    }
}

// ============================================================================
// Additional invariants
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// Cap findings is idempotent: cap(cap(x, n), n) == cap(x, n).
    #[test]
    fn cap_findings_idempotent(findings in any_findings(30), max in 1usize..30) {
        let (first, trunc1) = cap_findings(findings, max);
        let first_len = first.len();
        let (second, trunc2) = cap_findings(first.clone(), max);
        prop_assert_eq!(first, second, "cap must be idempotent");
        prop_assert!(!trunc2, "second cap must never truncate");

        if trunc1 {
            prop_assert_eq!(first_len, max);
        }
    }

    /// Overall verdict with only passing blocking sensors is Pass.
    #[test]
    fn all_passing_blockers_yield_pass(count in 1usize..8) {
        let summaries: Vec<_> = (0..count)
            .map(|i| make_blocking_summary(&format!("s{}", i), VerdictStatus::Pass))
            .collect();
        let cfg = CockpitConfig::default();
        let verdict = overall_verdict(&summaries, &cfg);
        prop_assert_eq!(verdict.status, VerdictStatus::Pass);
    }

    /// Fingerprint changes when location path differs.
    #[test]
    fn fingerprint_sensitive_to_location_path(
        sensor_id in "[a-z_]{1,10}",
        path_a in "[a-z]{1,10}",
        path_b in "[a-z]{1,10}",
    ) {
        prop_assume!(path_a != path_b);
        let base = Finding {
            severity: Severity::Error,
            check_id: None,
            code: "E001".to_string(),
            message: "msg".to_string(),
            location: None,
            help: None, url: None, fingerprint: None, data: None,
        };
        let mut f_a = base.clone();
        f_a.location = Some(Location { path: Some(path_a), line: Some(1), col: None });
        let mut f_b = base;
        f_b.location = Some(Location { path: Some(path_b), line: Some(1), col: None });

        let fp_a = derive_fingerprint(&sensor_id, &f_a);
        let fp_b = derive_fingerprint(&sensor_id, &f_b);
        prop_assert_ne!(fp_a, fp_b, "different paths must produce different fingerprints");
    }

    /// End-to-end: summarize + select pipeline is deterministic.
    #[test]
    fn end_to_end_pipeline_deterministic(
        sensor_id in "[a-z][a-z0-9]{0,6}",
        report in any_sensor_report(),
        policy in any_sensor_policy(),
        max_findings in 1usize..20,
    ) {
        let cfg = CockpitConfig::default();
        let blocking = BTreeMap::new();

        let (_, h1) = summarize_sensor_report(
            &sensor_id,
            &format!("artifacts/{}/report.json", sensor_id),
            None, &policy, report.clone(), max_findings,
        );
        let selected1 = select_highlights(h1, &cfg, &blocking);

        let (_, h2) = summarize_sensor_report(
            &sensor_id,
            &format!("artifacts/{}/report.json", sensor_id),
            None, &policy, report, max_findings,
        );
        let selected2 = select_highlights(h2, &cfg, &blocking);

        prop_assert_eq!(selected1, selected2, "full pipeline must be deterministic");
    }
}
