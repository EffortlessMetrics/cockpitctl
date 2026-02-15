//! Property-based tests for cockpitctl-domain.
//!
//! Tests determinism, ordering invariants, and correctness of domain logic.

use cockpitctl_domain::{
    cap_findings, compute_counts, derive_fingerprint, finding_sort_key, overall_verdict,
    select_highlights, sort_findings, sort_sensor_summaries,
};
use cockpitctl_types::{
    CockpitConfig, Finding, Highlight, Location, MissingPolicy, Policy, Presence, SensorPolicy,
    SensorSummary, Severity, Verdict, VerdictCounts, VerdictStatus, severity_rank,
};
use proptest::prelude::*;
use std::collections::BTreeMap;

// ============================================================================
// Strategies for generating arbitrary values
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
        prop::option::of("[A-Z][A-Z0-9_]{0,20}"), // check_id
        "[A-Z][A-Z0-9_./-]{0,30}",                // code
        ".{1,100}",                               // message
        any_location(),
        prop::option::of(".{0,50}"),                  // help
        prop::option::of("https?://[a-z.]+/[a-z/]*"), // url
        prop::option::of("[a-f0-9]{64}"),             // fingerprint
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

fn any_findings(max_len: usize) -> impl Strategy<Value = Vec<Finding>> {
    prop::collection::vec(any_finding(), 0..=max_len)
}

fn any_highlight() -> impl Strategy<Value = Highlight> {
    (
        "[a-z_][a-z0-9_-]{0,20}", // sensor_id
        any_finding(),
    )
        .prop_map(|(sensor_id, finding)| Highlight { sensor_id, finding })
}

fn any_highlights(max_len: usize) -> impl Strategy<Value = Vec<Highlight>> {
    prop::collection::vec(any_highlight(), 0..=max_len)
}

fn any_verdict_counts() -> impl Strategy<Value = VerdictCounts> {
    (0u64..1000, 0u64..1000, 0u64..1000, 0u64..100).prop_map(|(info, warn, error, suppressed)| {
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
        "[a-z_][a-z0-9_-]{0,20}", // id
        any::<bool>(),            // blocking
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

fn any_sensor_summaries(max_len: usize) -> impl Strategy<Value = Vec<SensorSummary>> {
    prop::collection::vec(any_sensor_summary(), 0..=max_len)
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
        any::<bool>(),                                    // warn_is_fail
        1usize..20,                                       // max_highlights
        1usize..50,                                       // max_per_sensor_findings
        prop::collection::vec("[A-Z][a-z]{0,15}", 0..10), // section_order
        prop::collection::btree_map("[a-z_][a-z0-9_-]{0,15}", any_sensor_policy(), 0..10),
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
                    hooks: vec![],
                }
            },
        )
}

// ============================================================================
// derive_fingerprint properties
// ============================================================================

proptest! {
    /// Fingerprint is deterministic: same inputs always produce the same output.
    #[test]
    fn fingerprint_is_deterministic(sensor_id in "[a-z_][a-z0-9_]{0,20}", f in any_finding()) {
        let a = derive_fingerprint(&sensor_id, &f);
        let b = derive_fingerprint(&sensor_id, &f);
        prop_assert_eq!(a, b, "fingerprint must be deterministic");
    }

    /// Fingerprint is a valid hex string of the right length (SHA256 = 64 hex chars).
    #[test]
    fn fingerprint_is_valid_hex(sensor_id in "[a-z_][a-z0-9_]{0,20}", f in any_finding()) {
        let fp = derive_fingerprint(&sensor_id, &f);
        prop_assert_eq!(fp.len(), 64, "SHA256 fingerprint must be 64 hex chars");
        prop_assert!(fp.chars().all(|c| c.is_ascii_hexdigit()), "fingerprint must be hex");
    }

    /// Different sensor_ids produce different fingerprints for the same finding.
    #[test]
    fn fingerprint_varies_with_sensor_id(
        sensor_a in "[a-z]{1,10}",
        sensor_b in "[a-z]{1,10}",
        f in any_finding()
    ) {
        prop_assume!(sensor_a != sensor_b);
        let fp_a = derive_fingerprint(&sensor_a, &f);
        let fp_b = derive_fingerprint(&sensor_b, &f);
        prop_assert_ne!(fp_a, fp_b, "different sensor_ids must produce different fingerprints");
    }

    /// Different codes produce different fingerprints (code is part of the hash).
    #[test]
    fn fingerprint_varies_with_code(
        sensor_id in "[a-z_]{1,10}",
        code_a in "[A-Z]{1,10}",
        code_b in "[A-Z]{1,10}",
        severity in any_severity(),
        message in ".{1,20}"
    ) {
        prop_assume!(code_a != code_b);
        let f_a = Finding {
            severity: severity.clone(),
            check_id: None,
            code: code_a,
            message: message.clone(),
            location: None,
            help: None,
            url: None,
            fingerprint: None,
            data: None,
        };
        let f_b = Finding {
            severity,
            check_id: None,
            code: code_b,
            message,
            location: None,
            help: None,
            url: None,
            fingerprint: None,
            data: None,
        };
        let fp_a = derive_fingerprint(&sensor_id, &f_a);
        let fp_b = derive_fingerprint(&sensor_id, &f_b);
        prop_assert_ne!(fp_a, fp_b, "different codes must produce different fingerprints");
    }
}

// ============================================================================
// finding_sort_key properties
// ============================================================================

proptest! {
    /// Sort key is deterministic.
    #[test]
    fn sort_key_is_deterministic(sensor_id in "[a-z_]{1,10}", f in any_finding()) {
        let k1 = finding_sort_key(&sensor_id, &f);
        let k2 = finding_sort_key(&sensor_id, &f);
        prop_assert_eq!(k1, k2);
    }

    /// Sort key implements total order (cmp never panics).
    #[test]
    fn sort_key_is_total_order(sensor_id in "[a-z_]{1,10}", a in any_finding(), b in any_finding()) {
        let ka = finding_sort_key(&sensor_id, &a);
        let kb = finding_sort_key(&sensor_id, &b);
        // Just ensure no panic.
        let _ = ka.cmp(&kb);
    }

    /// Sort key respects severity: Error < Warn < Info.
    #[test]
    fn sort_key_respects_severity_order(sensor_id in "[a-z_]{1,10}") {
        let base = Finding {
            severity: Severity::Info,
            check_id: None,
            code: "TEST".to_string(),
            message: "test".to_string(),
            location: None,
            help: None,
            url: None,
            fingerprint: None,
            data: None,
        };

        let mut error = base.clone();
        error.severity = Severity::Error;
        let mut warn = base.clone();
        warn.severity = Severity::Warn;
        let info = base;

        let key_error = finding_sort_key(&sensor_id, &error);
        let key_warn = finding_sort_key(&sensor_id, &warn);
        let key_info = finding_sort_key(&sensor_id, &info);

        prop_assert!(key_error < key_warn, "Error must sort before Warn");
        prop_assert!(key_warn < key_info, "Warn must sort before Info");
    }
}

// ============================================================================
// sort_findings properties
// ============================================================================

proptest! {
    /// Sorting is idempotent: sort(sort(v)) == sort(v).
    #[test]
    fn sort_findings_idempotent(sensor_id in "[a-z_]{1,10}", mut findings in any_findings(30)) {
        sort_findings(&sensor_id, &mut findings);
        let after_first = findings.clone();
        sort_findings(&sensor_id, &mut findings);
        prop_assert_eq!(findings, after_first, "sort must be idempotent");
    }

    /// Sorting preserves element count.
    #[test]
    fn sort_findings_preserves_count(sensor_id in "[a-z_]{1,10}", mut findings in any_findings(30)) {
        let original_count = findings.len();
        sort_findings(&sensor_id, &mut findings);
        prop_assert_eq!(findings.len(), original_count, "sort must not change element count");
    }

    /// After sorting, findings are in non-decreasing order by sort key.
    #[test]
    fn sort_findings_produces_sorted_output(sensor_id in "[a-z_]{1,10}", mut findings in any_findings(30)) {
        sort_findings(&sensor_id, &mut findings);

        for window in findings.windows(2) {
            let key_a = finding_sort_key(&sensor_id, &window[0]);
            let key_b = finding_sort_key(&sensor_id, &window[1]);
            prop_assert!(key_a <= key_b, "sorted findings must be in order");
        }
    }

    /// Error findings come before Warn, which come before Info.
    #[test]
    fn sort_findings_severity_ordering(sensor_id in "[a-z_]{1,10}", mut findings in any_findings(30)) {
        sort_findings(&sensor_id, &mut findings);

        let mut seen_warn = false;
        let mut seen_info = false;

        for f in &findings {
            match f.severity {
                Severity::Error => {
                    prop_assert!(!seen_warn && !seen_info, "Error must come before Warn/Info");
                }
                Severity::Warn => {
                    prop_assert!(!seen_info, "Warn must come before Info");
                    seen_warn = true;
                }
                Severity::Info => {
                    seen_info = true;
                }
            }
        }
    }
}

// ============================================================================
// cap_findings properties
// ============================================================================

proptest! {
    /// Result length is at most max.
    #[test]
    fn cap_findings_respects_max(findings in any_findings(50), max in 0usize..60) {
        let (capped, _) = cap_findings(findings, max);
        prop_assert!(capped.len() <= max, "capped.len() must be <= max");
    }

    /// Truncated flag is true iff original length > max.
    #[test]
    fn cap_findings_truncated_flag(findings in any_findings(50), max in 0usize..60) {
        let original_len = findings.len();
        let (_, truncated) = cap_findings(findings, max);
        prop_assert_eq!(truncated, original_len > max, "truncated flag must match");
    }

    /// If len <= max, all elements are preserved.
    #[test]
    fn cap_findings_preserves_when_under_max(findings in any_findings(30)) {
        let original = findings.clone();
        let max = original.len() + 10;
        let (capped, truncated) = cap_findings(findings, max);
        prop_assert_eq!(capped, original, "when under max, all findings preserved");
        prop_assert!(!truncated, "no truncation when under max");
    }

    /// Capped findings are the first N elements (order preserved).
    #[test]
    fn cap_findings_preserves_prefix(findings in any_findings(50), max in 1usize..30) {
        let original = findings.clone();
        let (capped, _) = cap_findings(findings, max);

        for (i, f) in capped.iter().enumerate() {
            prop_assert_eq!(f, &original[i], "capped must be prefix of original");
        }
    }
}

// ============================================================================
// compute_counts properties
// ============================================================================

/// Empty findings produce zero counts.
#[test]
fn compute_counts_empty() {
    let counts = compute_counts(&[]);
    assert_eq!(counts.info, 0);
    assert_eq!(counts.warn, 0);
    assert_eq!(counts.error, 0);
}

proptest! {
    /// Sum of counts equals length of findings.
    #[test]
    fn compute_counts_sum_equals_len(findings in any_findings(50)) {
        let counts = compute_counts(&findings);
        let sum = counts.info + counts.warn + counts.error;
        prop_assert_eq!(sum as usize, findings.len(), "counts sum must equal len");
    }

    /// Counts match actual severity distribution.
    #[test]
    fn compute_counts_matches_distribution(findings in any_findings(50)) {
        let counts = compute_counts(&findings);

        let actual_info = findings.iter().filter(|f| matches!(f.severity, Severity::Info)).count() as u64;
        let actual_warn = findings.iter().filter(|f| matches!(f.severity, Severity::Warn)).count() as u64;
        let actual_error = findings.iter().filter(|f| matches!(f.severity, Severity::Error)).count() as u64;

        prop_assert_eq!(counts.info, actual_info);
        prop_assert_eq!(counts.warn, actual_warn);
        prop_assert_eq!(counts.error, actual_error);
    }

    /// Counts are deterministic.
    #[test]
    fn compute_counts_deterministic(findings in any_findings(50)) {
        let c1 = compute_counts(&findings);
        let c2 = compute_counts(&findings);
        prop_assert_eq!(c1, c2);
    }
}

// ============================================================================
// select_highlights properties
// ============================================================================

proptest! {
    /// Result length is at most max_highlights.
    #[test]
    fn select_highlights_respects_max(
        highlights in any_highlights(30),
        cfg in any_cockpit_config()
    ) {
        let sensor_blocking = BTreeMap::new();
        let selected = select_highlights(highlights, &cfg, &sensor_blocking);
        prop_assert!(
            selected.len() <= cfg.policy.max_highlights,
            "selected.len() must be <= max_highlights"
        );
    }

    /// No duplicate fingerprints in result.
    #[test]
    fn select_highlights_no_duplicate_fingerprints(
        highlights in any_highlights(30),
        cfg in any_cockpit_config()
    ) {
        let sensor_blocking = BTreeMap::new();
        let selected = select_highlights(highlights, &cfg, &sensor_blocking);

        let fingerprints: Vec<_> = selected
            .iter()
            .filter_map(|h| h.finding.fingerprint.clone())
            .collect();
        let unique_count = {
            let mut fps = fingerprints.clone();
            fps.sort();
            fps.dedup();
            fps.len()
        };

        prop_assert_eq!(
            fingerprints.len(),
            unique_count,
            "no duplicate fingerprints in highlights"
        );
    }

    /// Selection is deterministic.
    #[test]
    fn select_highlights_deterministic(
        highlights in any_highlights(20),
        cfg in any_cockpit_config()
    ) {
        let sensor_blocking = BTreeMap::new();
        let s1 = select_highlights(highlights.clone(), &cfg, &sensor_blocking);
        let s2 = select_highlights(highlights, &cfg, &sensor_blocking);
        prop_assert_eq!(s1, s2, "highlight selection must be deterministic");
    }

    /// All selected highlights have a fingerprint (normalization).
    #[test]
    fn select_highlights_normalizes_fingerprints(
        highlights in any_highlights(20),
        cfg in any_cockpit_config()
    ) {
        let sensor_blocking = BTreeMap::new();
        let selected = select_highlights(highlights, &cfg, &sensor_blocking);

        for h in &selected {
            prop_assert!(
                h.finding.fingerprint.is_some(),
                "all selected highlights must have fingerprints"
            );
        }
    }

    /// Higher severity findings are selected first.
    #[test]
    fn select_highlights_severity_ordering(
        highlights in any_highlights(20),
        cfg in any_cockpit_config()
    ) {
        let sensor_blocking = BTreeMap::new();
        let selected = select_highlights(highlights, &cfg, &sensor_blocking);

        // Verify severity ordering: Error before Warn before Info.
        let mut seen_warn = false;
        let mut seen_info = false;

        for h in &selected {
            match h.finding.severity {
                Severity::Error => {
                    prop_assert!(!seen_warn && !seen_info, "Error must come first");
                }
                Severity::Warn => {
                    prop_assert!(!seen_info, "Warn must come before Info");
                    seen_warn = true;
                }
                Severity::Info => {
                    seen_info = true;
                }
            }
        }
    }
}

// ============================================================================
// sort_sensor_summaries properties
// ============================================================================

proptest! {
    /// Sorting is idempotent.
    #[test]
    fn sort_sensor_summaries_idempotent(
        mut summaries in any_sensor_summaries(20),
        cfg in any_cockpit_config()
    ) {
        sort_sensor_summaries(&mut summaries, &cfg);
        let after_first = summaries.clone();
        sort_sensor_summaries(&mut summaries, &cfg);
        prop_assert_eq!(summaries, after_first, "sort must be idempotent");
    }

    /// Sorting preserves element count.
    #[test]
    fn sort_sensor_summaries_preserves_count(
        mut summaries in any_sensor_summaries(20),
        cfg in any_cockpit_config()
    ) {
        let original_count = summaries.len();
        sort_sensor_summaries(&mut summaries, &cfg);
        prop_assert_eq!(summaries.len(), original_count, "sort must not change count");
    }

    /// Sorting is deterministic.
    #[test]
    fn sort_sensor_summaries_deterministic(
        summaries in any_sensor_summaries(20),
        cfg in any_cockpit_config()
    ) {
        let mut s1 = summaries.clone();
        let mut s2 = summaries;
        sort_sensor_summaries(&mut s1, &cfg);
        sort_sensor_summaries(&mut s2, &cfg);
        prop_assert_eq!(s1, s2, "sorting must be deterministic");
    }
}

// ============================================================================
// overall_verdict properties
// ============================================================================

proptest! {
    /// Overall verdict aggregates counts from all sensors.
    #[test]
    fn overall_verdict_aggregates_counts(
        summaries in any_sensor_summaries(10),
        cfg in any_cockpit_config()
    ) {
        let verdict = overall_verdict(&summaries, &cfg);

        let expected_info: u64 = summaries.iter().map(|s| s.verdict.counts.info).sum();
        let expected_warn: u64 = summaries.iter().map(|s| s.verdict.counts.warn).sum();
        let expected_error: u64 = summaries.iter().map(|s| s.verdict.counts.error).sum();

        prop_assert_eq!(verdict.counts.info, expected_info);
        prop_assert_eq!(verdict.counts.warn, expected_warn);
        prop_assert_eq!(verdict.counts.error, expected_error);
    }

    /// Overall verdict is deterministic.
    #[test]
    fn overall_verdict_deterministic(
        summaries in any_sensor_summaries(10),
        cfg in any_cockpit_config()
    ) {
        let v1 = overall_verdict(&summaries, &cfg);
        let v2 = overall_verdict(&summaries, &cfg);
        prop_assert_eq!(v1, v2, "overall_verdict must be deterministic");
    }

    /// With no blocking sensors, verdict is Pass.
    #[test]
    fn overall_verdict_pass_with_no_blockers(summaries in any_sensor_summaries(10)) {
        // Create summaries with all blocking=false.
        let non_blocking: Vec<_> = summaries
            .into_iter()
            .map(|mut s| {
                s.blocking = false;
                s
            })
            .collect();

        let cfg = CockpitConfig::default();
        let verdict = overall_verdict(&non_blocking, &cfg);

        prop_assert_eq!(verdict.status, VerdictStatus::Pass, "no blockers means Pass");
    }

    /// Blocking sensor with Fail verdict produces Fail overall.
    #[test]
    fn overall_verdict_fail_propagates(mut summary in any_sensor_summary()) {
        summary.blocking = true;
        summary.verdict.status = VerdictStatus::Fail;

        let cfg = CockpitConfig::default();
        let verdict = overall_verdict(&[summary], &cfg);

        prop_assert_eq!(verdict.status, VerdictStatus::Fail, "blocking Fail must propagate");
    }

    /// warn_is_fail policy escalates Warn to Fail.
    #[test]
    fn overall_verdict_warn_is_fail_escalation(mut summary in any_sensor_summary()) {
        summary.blocking = true;
        summary.verdict.status = VerdictStatus::Warn;

        let mut cfg = CockpitConfig::default();
        cfg.policy.warn_is_fail = true;

        let verdict = overall_verdict(&[summary.clone()], &cfg);
        prop_assert_eq!(
            verdict.status,
            VerdictStatus::Fail,
            "warn_is_fail must escalate Warn to Fail"
        );

        // Without warn_is_fail, it stays Warn.
        cfg.policy.warn_is_fail = false;
        let verdict2 = overall_verdict(&[summary], &cfg);
        prop_assert_eq!(verdict2.status, VerdictStatus::Warn, "without flag, stays Warn");
    }

    /// Worst status among blocking sensors wins (baseline is Pass).
    /// Note: Skip doesn't worsen the verdict - the baseline is Pass.
    #[test]
    fn overall_verdict_worst_status_wins(
        status_a in any_verdict_status(),
        status_b in any_verdict_status()
    ) {
        let s_a = SensorSummary {
            id: "sensor_a".to_string(),
            blocking: true,
            missing: MissingPolicy::Skip,
            presence: Presence::Present,
            report_path: "a/report.json".to_string(),
            comment_path: None,
            verdict: Verdict {
                status: status_a.clone(),
                counts: VerdictCounts::default(),
                reasons: vec![],
            },
            truncated: false,
            errors: vec![],
            missing_policy_applied: None,
            policy_outcome: None,
        };
        let s_b = SensorSummary {
            id: "sensor_b".to_string(),
            blocking: true,
            missing: MissingPolicy::Skip,
            presence: Presence::Present,
            report_path: "b/report.json".to_string(),
            comment_path: None,
            verdict: Verdict {
                status: status_b.clone(),
                counts: VerdictCounts::default(),
                reasons: vec![],
            },
            truncated: false,
            errors: vec![],
            missing_policy_applied: None,
            policy_outcome: None,
        };

        let cfg = CockpitConfig::default();
        let verdict = overall_verdict(&[s_a, s_b], &cfg);

        // Worst = lower rank number. Baseline is Pass (rank 2).
        // Skip (rank 3) doesn't worsen the verdict.
        use cockpitctl_types::verdict_status_rank;
        let rank_a = verdict_status_rank(&status_a);
        let rank_b = verdict_status_rank(&status_b);
        let baseline_rank = verdict_status_rank(&VerdictStatus::Pass); // 2

        // Expected is the minimum of: sensor ranks AND the baseline (Pass).
        // Skip (3) > Pass (2), so Skip doesn't worsen it.
        let expected_rank = rank_a.min(rank_b).min(baseline_rank);
        let actual_rank = verdict_status_rank(&verdict.status);

        prop_assert_eq!(
            actual_rank, expected_rank,
            "overall verdict must be worst of blocking sensors and Pass baseline"
        );
    }
}

// ============================================================================
// Cross-function invariants
// ============================================================================

proptest! {
    /// sort then cap preserves severity ordering.
    #[test]
    fn sort_then_cap_preserves_severity_order(
        sensor_id in "[a-z_]{1,10}",
        mut findings in any_findings(50),
        max in 1usize..30
    ) {
        sort_findings(&sensor_id, &mut findings);
        let (capped, _) = cap_findings(findings, max);

        // Check severity ordering is preserved.
        let mut prev_rank = 0u8;
        for f in &capped {
            let rank = severity_rank(&f.severity);
            prop_assert!(rank >= prev_rank, "severity order must be preserved after cap");
            prev_rank = rank;
        }
    }

    /// compute_counts is consistent before and after sorting.
    #[test]
    fn compute_counts_invariant_under_sort(
        sensor_id in "[a-z_]{1,10}",
        mut findings in any_findings(30)
    ) {
        let counts_before = compute_counts(&findings);
        sort_findings(&sensor_id, &mut findings);
        let counts_after = compute_counts(&findings);
        prop_assert_eq!(counts_before, counts_after, "counts must not change after sort");
    }
}
