//! Property-based tests for ordering permutation invariance.
//!
//! Verifies that sort(shuffle(x)) == sort(x) for findings, highlights,
//! and sensor summaries — properties not covered by existing proptest files.

use cockpitctl_domain::{
    finding_sort_key, select_highlights, sort_findings, sort_sensor_summaries,
};
use cockpitctl_types::{
    CockpitConfig, Finding, Highlight, Location, MissingPolicy, Policy, Presence, SensorPolicy,
    SensorSummary, Severity, Verdict, VerdictCounts, VerdictStatus, severity_rank,
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
            prop::option::of("[a-z/_.-]{1,20}"),
            prop::option::of(1u32..5000),
            prop::option::of(1u32..200),
        )
            .prop_map(|(path, line, col)| Location { path, line, col }),
    )
}

fn any_finding() -> impl Strategy<Value = Finding> {
    (
        any_severity(),
        prop::option::of("[A-Z][A-Z0-9_]{0,8}"),
        "[A-Z][A-Z0-9_]{0,10}",
        ".{1,30}",
        any_location(),
        prop::option::of("[a-f0-9]{16}"),
    )
        .prop_map(
            |(severity, check_id, code, message, location, fingerprint)| Finding {
                severity,
                check_id,
                code,
                message,
                location,
                help: None,
                url: None,
                fingerprint,
                data: None,
            },
        )
}

fn any_findings(max: usize) -> impl Strategy<Value = Vec<Finding>> {
    prop::collection::vec(any_finding(), 0..=max)
}

fn any_highlight() -> impl Strategy<Value = Highlight> {
    ("[a-z_][a-z0-9_]{0,8}", any_finding())
        .prop_map(|(sensor_id, finding)| Highlight { sensor_id, finding })
}

fn any_highlights(max: usize) -> impl Strategy<Value = Vec<Highlight>> {
    prop::collection::vec(any_highlight(), 0..=max)
}

fn any_sensor_summary() -> impl Strategy<Value = SensorSummary> {
    (
        "[a-z_][a-z0-9_]{0,8}",
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
    )
        .prop_map(|(id, blocking, missing, presence, status)| SensorSummary {
            id: id.clone(),
            blocking,
            missing,
            presence,
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

fn any_sensor_summaries(max: usize) -> impl Strategy<Value = Vec<SensorSummary>> {
    prop::collection::vec(any_sensor_summary(), 2..=max)
}

fn any_cockpit_config() -> impl Strategy<Value = CockpitConfig> {
    (
        any::<bool>(),
        1usize..15,
        1usize..30,
        prop::collection::vec("[A-Z][a-z]{2,8}", 0..4),
        prop::collection::btree_map(
            "[a-z_]{3,8}",
            (
                any::<bool>(),
                prop_oneof![
                    Just(MissingPolicy::Skip),
                    Just(MissingPolicy::Warn),
                    Just(MissingPolicy::Fail),
                ],
                prop::option::of("[A-Z][a-z]{2,8}"),
            )
                .prop_map(|(blocking, missing, section)| SensorPolicy {
                    blocking,
                    missing,
                    section,
                    require_label: None,
                    repro: None,
                }),
            0..5,
        ),
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
                    sensors,
                    ..Default::default()
                }
            },
        )
}

/// Shuffle a vector using a seed.
fn shuffle_vec<T: Clone>(items: &[T], seed: u64) -> Vec<T> {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut v: Vec<(u64, T)> = items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let mut h = DefaultHasher::new();
            seed.hash(&mut h);
            i.hash(&mut h);
            (h.finish(), item.clone())
        })
        .collect();
    v.sort_by_key(|(k, _)| *k);
    v.into_iter().map(|(_, item)| item).collect()
}

// ============================================================================
// Findings: sort(shuffle(x)) == sort(x)
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    /// sort(shuffle(findings)) == sort(findings) — permutation invariant.
    #[test]
    fn sort_findings_permutation_invariant_shuffled(
        sensor_id in "[a-z_]{1,8}",
        findings in any_findings(20),
        seed in any::<u64>(),
    ) {
        let mut sorted = findings.clone();
        sort_findings(&sensor_id, &mut sorted);

        let mut shuffled = shuffle_vec(&findings, seed);
        sort_findings(&sensor_id, &mut shuffled);

        let keys_sorted: Vec<_> = sorted.iter().map(|f| finding_sort_key(&sensor_id, f)).collect();
        let keys_shuffled: Vec<_> = shuffled.iter().map(|f| finding_sort_key(&sensor_id, f)).collect();
        prop_assert_eq!(&keys_sorted, &keys_shuffled);
    }

    /// sort(sort(findings)) == sort(findings) — idempotent.
    #[test]
    fn sort_findings_double_sort_idempotent(
        sensor_id in "[a-z_]{1,8}",
        mut findings in any_findings(20),
    ) {
        sort_findings(&sensor_id, &mut findings);
        let once = findings.clone();
        sort_findings(&sensor_id, &mut findings);
        let keys_once: Vec<_> = once.iter().map(|f| finding_sort_key(&sensor_id, f)).collect();
        let keys_twice: Vec<_> = findings.iter().map(|f| finding_sort_key(&sensor_id, f)).collect();
        prop_assert_eq!(&keys_once, &keys_twice);
    }
}

// ============================================================================
// Highlights: select_highlights(shuffle(x)) == select_highlights(x)
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// select_highlights is permutation-invariant: shuffled input yields same output.
    #[test]
    fn select_highlights_permutation_invariant(
        highlights in any_highlights(15),
        cfg in any_cockpit_config(),
        seed in any::<u64>(),
    ) {
        let blocking: BTreeMap<String, bool> = BTreeMap::new();

        let result_a = select_highlights(highlights.clone(), &cfg, &blocking);
        let shuffled = shuffle_vec(&highlights, seed);
        let result_b = select_highlights(shuffled, &cfg, &blocking);

        // Compare fingerprints (which are normalized by select_highlights).
        let fps_a: Vec<_> = result_a.iter().map(|h| h.finding.fingerprint.clone()).collect();
        let fps_b: Vec<_> = result_b.iter().map(|h| h.finding.fingerprint.clone()).collect();
        prop_assert_eq!(&fps_a, &fps_b);
    }

    /// select_highlights(select_highlights(x)) == select_highlights(x) — idempotent.
    #[test]
    fn select_highlights_idempotent(
        highlights in any_highlights(15),
        cfg in any_cockpit_config(),
    ) {
        let blocking: BTreeMap<String, bool> = BTreeMap::new();

        let result_a = select_highlights(highlights, &cfg, &blocking);
        let result_b = select_highlights(result_a.clone(), &cfg, &blocking);

        let fps_a: Vec<_> = result_a.iter().map(|h| h.finding.fingerprint.clone()).collect();
        let fps_b: Vec<_> = result_b.iter().map(|h| h.finding.fingerprint.clone()).collect();
        prop_assert_eq!(&fps_a, &fps_b);
    }
}

// ============================================================================
// Sensor summaries: sort(shuffle(x)) == sort(x)
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// sort_sensor_summaries is permutation-invariant.
    #[test]
    fn sort_sensor_summaries_permutation_invariant(
        summaries in any_sensor_summaries(10),
        cfg in any_cockpit_config(),
        seed in any::<u64>(),
    ) {
        let mut sorted = summaries.clone();
        sort_sensor_summaries(&mut sorted, &cfg);

        let mut shuffled = shuffle_vec(&summaries, seed);
        sort_sensor_summaries(&mut shuffled, &cfg);

        let ids_sorted: Vec<_> = sorted.iter().map(|s| &s.id).collect();
        let ids_shuffled: Vec<_> = shuffled.iter().map(|s| &s.id).collect();
        prop_assert_eq!(&ids_sorted, &ids_shuffled);
    }

    /// sort_sensor_summaries(sort_sensor_summaries(x)) == sort_sensor_summaries(x).
    #[test]
    fn sort_sensor_summaries_double_sort_idempotent(
        mut summaries in any_sensor_summaries(10),
        cfg in any_cockpit_config(),
    ) {
        sort_sensor_summaries(&mut summaries, &cfg);
        let once: Vec<_> = summaries.iter().map(|s| &s.id).cloned().collect();
        sort_sensor_summaries(&mut summaries, &cfg);
        let twice: Vec<_> = summaries.iter().map(|s| &s.id).cloned().collect();
        prop_assert_eq!(&once, &twice);
    }

    /// sort_findings preserves severity ordering after any permutation.
    #[test]
    fn sort_findings_severity_monotonic_after_shuffle(
        sensor_id in "[a-z_]{1,8}",
        findings in any_findings(20),
        seed in any::<u64>(),
    ) {
        let mut shuffled = shuffle_vec(&findings, seed);
        sort_findings(&sensor_id, &mut shuffled);

        for w in shuffled.windows(2) {
            let rank_a = severity_rank(&w[0].severity);
            let rank_b = severity_rank(&w[1].severity);
            prop_assert!(
                rank_a <= rank_b,
                "severity should be non-increasing: {:?} before {:?}",
                w[0].severity,
                w[1].severity
            );
        }
    }
}
