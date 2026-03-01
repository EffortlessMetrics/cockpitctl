//! Property-based tests for cockpitctl-types.
//!
//! Tests invariants for ranking functions, sort keys, and type conversions.

use cockpitctl_types::{
    ArtifactPointer, CockpitReport, Finding, FindingSortKey, Highlight, Location, MissingPolicy,
    PolicyOutcome, PolicySensorSnapshot, PolicySnapshot, Presence, RunInfo, SensorReport,
    SensorSummary, Severity, ToolInfo, Verdict, VerdictCounts, VerdictStatus, severity_rank,
    verdict_status_rank,
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

fn any_finding_sort_key() -> impl Strategy<Value = FindingSortKey> {
    (
        0u8..=2,                 // severity_rank (Error=0, Warn=1, Info=2)
        "[a-z_][a-z0-9_]{0,20}", // sensor_id
        prop::option::of("[a-z/._-]{0,50}").prop_map(|o| o.unwrap_or_default()), // path
        any::<u32>(),            // line
        "[A-Z][A-Z0-9_]{0,20}",  // code
        ".{0,100}",              // message
    )
        .prop_map(
            |(severity_rank, sensor_id, path, line, code, message)| FindingSortKey {
                severity_rank,
                sensor_id,
                path,
                line,
                code,
                message,
            },
        )
}

// ============================================================================
// Severity ranking properties
// ============================================================================

proptest! {
    /// severity_rank is idempotent: calling it twice yields the same result.
    #[test]
    fn severity_rank_is_idempotent(s in any_severity()) {
        let r1 = severity_rank(&s);
        let r2 = severity_rank(&s);
        prop_assert_eq!(r1, r2);
    }
}

#[test]
fn severity_rank_covers_all_variants() {
    // Exhaustive check: all variants produce valid ranks.
    let ranks: Vec<u8> = vec![
        severity_rank(&Severity::Error),
        severity_rank(&Severity::Warn),
        severity_rank(&Severity::Info),
    ];

    // Each should be distinct.
    let mut sorted = ranks.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted.len(), 3, "Each severity must have a unique rank");

    // Error is most severe (lowest rank).
    assert!(
        severity_rank(&Severity::Error) < severity_rank(&Severity::Warn),
        "Error must rank higher (lower number) than Warn"
    );
    assert!(
        severity_rank(&Severity::Warn) < severity_rank(&Severity::Info),
        "Warn must rank higher (lower number) than Info"
    );
}

#[test]
fn severity_rank_exact_values() {
    // Contract: Error→0, Warn→1, Info→2.
    assert_eq!(severity_rank(&Severity::Error), 0);
    assert_eq!(severity_rank(&Severity::Warn), 1);
    assert_eq!(severity_rank(&Severity::Info), 2);
}

// ============================================================================
// Verdict status ranking properties
// ============================================================================

proptest! {
    /// verdict_status_rank is idempotent: calling it twice yields the same result.
    #[test]
    fn verdict_status_rank_is_idempotent(s in any_verdict_status()) {
        let r1 = verdict_status_rank(&s);
        let r2 = verdict_status_rank(&s);
        prop_assert_eq!(r1, r2);
    }
}

#[test]
fn verdict_status_rank_covers_all_variants() {
    // Exhaustive check: all variants produce valid ranks.
    let ranks: Vec<u8> = vec![
        verdict_status_rank(&VerdictStatus::Fail),
        verdict_status_rank(&VerdictStatus::Warn),
        verdict_status_rank(&VerdictStatus::Pass),
        verdict_status_rank(&VerdictStatus::Skip),
    ];

    // Each should be distinct.
    let mut sorted = ranks.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(
        sorted.len(),
        4,
        "Each verdict status must have a unique rank"
    );

    // Fail is worst (lowest rank), Skip is best (highest rank).
    assert!(
        verdict_status_rank(&VerdictStatus::Fail) < verdict_status_rank(&VerdictStatus::Warn),
        "Fail must rank worse (lower number) than Warn"
    );
    assert!(
        verdict_status_rank(&VerdictStatus::Warn) < verdict_status_rank(&VerdictStatus::Pass),
        "Warn must rank worse (lower number) than Pass"
    );
    assert!(
        verdict_status_rank(&VerdictStatus::Pass) < verdict_status_rank(&VerdictStatus::Skip),
        "Pass must rank worse (lower number) than Skip"
    );
}

#[test]
fn verdict_status_rank_exact_values() {
    // Contract: Fail→0, Warn→1, Pass→2, Skip→3.
    assert_eq!(verdict_status_rank(&VerdictStatus::Fail), 0);
    assert_eq!(verdict_status_rank(&VerdictStatus::Warn), 1);
    assert_eq!(verdict_status_rank(&VerdictStatus::Pass), 2);
    assert_eq!(verdict_status_rank(&VerdictStatus::Skip), 3);
}

// ============================================================================
// FindingSortKey ordering properties
// ============================================================================

proptest! {
    /// FindingSortKey implements a total order: cmp never panics.
    #[test]
    fn finding_sort_key_cmp_never_panics(a in any_finding_sort_key(), b in any_finding_sort_key()) {
        // Just ensure no panic.
        let _ = a.cmp(&b);
    }

    /// FindingSortKey ordering is reflexive: a == a.
    #[test]
    fn finding_sort_key_reflexive(a in any_finding_sort_key()) {
        prop_assert_eq!(a.cmp(&a), std::cmp::Ordering::Equal);
    }

    /// FindingSortKey ordering is antisymmetric: if a < b, then b > a.
    #[test]
    fn finding_sort_key_antisymmetric(a in any_finding_sort_key(), b in any_finding_sort_key()) {
        let ab = a.cmp(&b);
        let ba = b.cmp(&a);
        prop_assert_eq!(ab, ba.reverse());
    }

    /// FindingSortKey ordering is transitive: if a < b and b < c, then a < c.
    #[test]
    fn finding_sort_key_transitive(
        a in any_finding_sort_key(),
        b in any_finding_sort_key(),
        c in any_finding_sort_key()
    ) {
        use std::cmp::Ordering::*;

        let ab = a.cmp(&b);
        let bc = b.cmp(&c);
        let ac = a.cmp(&c);

        // If a < b and b < c, then a < c.
        if ab == Less && bc == Less {
            prop_assert_eq!(ac, Less, "transitivity: a < b < c implies a < c");
        }
        // If a > b and b > c, then a > c.
        if ab == Greater && bc == Greater {
            prop_assert_eq!(ac, Greater, "transitivity: a > b > c implies a > c");
        }
        // If a == b and b == c, then a == c.
        if ab == Equal && bc == Equal {
            prop_assert_eq!(ac, Equal, "transitivity: a == b == c implies a == c");
        }
    }

    /// Sorting a vec of FindingSortKeys is idempotent: sort(sort(v)) == sort(v).
    #[test]
    fn finding_sort_key_sort_idempotent(mut keys in prop::collection::vec(any_finding_sort_key(), 0..50)) {
        keys.sort();
        let after_first = keys.clone();
        keys.sort();
        prop_assert_eq!(keys, after_first);
    }

    /// Severity rank dominates the sort: higher severity (lower rank) comes first.
    #[test]
    fn finding_sort_key_severity_dominates(
        sensor_id in "[a-z_]{1,10}",
        path in "[a-z/]{0,20}",
        line in any::<u32>(),
        code in "[A-Z]{1,10}",
        message in ".{0,20}"
    ) {
        let error_key = FindingSortKey {
            severity_rank: 0, // Error
            sensor_id: sensor_id.clone(),
            path: path.clone(),
            line,
            code: code.clone(),
            message: message.clone(),
        };
        let warn_key = FindingSortKey {
            severity_rank: 1, // Warn
            sensor_id: sensor_id.clone(),
            path: path.clone(),
            line,
            code: code.clone(),
            message: message.clone(),
        };
        let info_key = FindingSortKey {
            severity_rank: 2, // Info
            sensor_id,
            path,
            line,
            code,
            message,
        };

        // Error < Warn < Info (in sort order).
        prop_assert!(error_key < warn_key, "Error severity must sort before Warn");
        prop_assert!(warn_key < info_key, "Warn severity must sort before Info");
        prop_assert!(error_key < info_key, "Error severity must sort before Info");
    }
}

// ============================================================================
// Strategies for generating full report types (serialization roundtrips)
// ============================================================================

fn any_tool_info() -> impl Strategy<Value = ToolInfo> {
    (
        "[a-z][a-z0-9-]{0,15}",
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
    "2024-0[1-9]-[012][1-9]T[01][0-9]:[0-5][0-9]:[0-5][0-9]Z".prop_map(|started_at| RunInfo {
        started_at,
        ended_at: None,
        duration_ms: None,
        host: None,
        git: None,
        ci: None,
        capabilities: BTreeMap::new(),
    })
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
        prop::collection::vec(".{0,20}", 0..3),
    )
        .prop_map(|(status, counts, reasons)| Verdict {
            status,
            counts,
            reasons,
        })
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
        prop::option::of(".{0,30}"),
        prop::option::of("https://example\\.com"),
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

fn any_artifact_pointer() -> impl Strategy<Value = ArtifactPointer> {
    (
        "[a-z][a-z0-9_]{0,10}",
        "[a-z/._]{1,20}",
        Just("application/json".to_string()),
        prop::option::of("[a-z._]{1,10}"),
    )
        .prop_map(|(id, path, mime, schema)| ArtifactPointer {
            id,
            path,
            mime,
            schema,
        })
}

fn any_sensor_report() -> impl Strategy<Value = SensorReport> {
    (
        any_tool_info(),
        any_run_info(),
        any_verdict(),
        prop::collection::vec(any_finding(), 0..5),
        prop::collection::vec(any_artifact_pointer(), 0..2),
    )
        .prop_map(|(tool, run, verdict, findings, artifacts)| SensorReport {
            schema: "sensor.report.v1".to_string(),
            tool,
            run,
            verdict,
            findings,
            artifacts,
            data: None,
        })
}

fn any_missing_policy() -> impl Strategy<Value = MissingPolicy> {
    prop_oneof![
        Just(MissingPolicy::Skip),
        Just(MissingPolicy::Warn),
        Just(MissingPolicy::Fail),
    ]
}

fn any_presence() -> impl Strategy<Value = Presence> {
    prop_oneof![
        Just(Presence::Present),
        Just(Presence::Missing),
        Just(Presence::Invalid),
    ]
}

fn any_policy_outcome() -> impl Strategy<Value = PolicyOutcome> {
    prop_oneof![
        Just(PolicyOutcome::Blocked),
        Just(PolicyOutcome::Allowed),
        Just(PolicyOutcome::Informational),
    ]
}

fn any_sensor_summary() -> impl Strategy<Value = SensorSummary> {
    (
        "[a-z_][a-z0-9_]{0,10}",
        any::<bool>(),
        any_missing_policy(),
        any_presence(),
        any_verdict(),
        any::<bool>(),
        prop::option::of(any_missing_policy()),
        prop::option::of(any_policy_outcome()),
    )
        .prop_map(
            |(
                id,
                blocking,
                missing,
                presence,
                verdict,
                truncated,
                missing_applied,
                policy_outcome,
            )| {
                SensorSummary {
                    id: id.clone(),
                    blocking,
                    missing,
                    presence,
                    report_path: format!("artifacts/{}/report.json", id),
                    comment_path: None,
                    verdict,
                    truncated,
                    errors: vec![],
                    missing_policy_applied: missing_applied,
                    policy_outcome,
                }
            },
        )
}

fn any_highlight() -> impl Strategy<Value = Highlight> {
    ("[a-z_][a-z0-9_]{0,10}", any_finding())
        .prop_map(|(sensor_id, finding)| Highlight { sensor_id, finding })
}

fn any_policy_sensor_snapshot() -> impl Strategy<Value = PolicySensorSnapshot> {
    ("[a-z_][a-z0-9_]{0,10}", any::<bool>(), any_missing_policy()).prop_map(
        |(id, blocking, missing)| PolicySensorSnapshot {
            id,
            blocking,
            missing,
            section: None,
            require_label: None,
            repro: None,
        },
    )
}

fn any_policy_snapshot() -> impl Strategy<Value = PolicySnapshot> {
    (
        any::<bool>(),
        1usize..20,
        1usize..50,
        1usize..50,
        prop::collection::vec(any_policy_sensor_snapshot(), 0..5),
    )
        .prop_map(
            |(warn_is_fail, max_highlights, max_per_sensor, max_annotations, sensors)| {
                PolicySnapshot {
                    warn_is_fail,
                    max_highlights,
                    max_per_sensor_findings: max_per_sensor,
                    max_annotations,
                    section_order: vec![],
                    sensors,
                }
            },
        )
}

fn any_cockpit_report() -> impl Strategy<Value = CockpitReport> {
    (
        any_tool_info(),
        any_run_info(),
        any_verdict(),
        prop::collection::vec(any_sensor_summary(), 0..5),
        prop::collection::vec(any_highlight(), 0..5),
        any_policy_snapshot(),
    )
        .prop_map(
            |(tool, run, verdict, sensors, highlights, policy)| CockpitReport {
                schema: "cockpit.report.v1".to_string(),
                tool,
                run,
                verdict,
                sensors,
                highlights,
                policy,
                data: None,
            },
        )
}

// ============================================================================
// Serialization roundtrip properties
// ============================================================================

proptest! {
    /// SensorReport survives a JSON roundtrip: serialize → deserialize preserves all fields.
    #[test]
    fn sensor_report_json_roundtrip(report in any_sensor_report()) {
        let json = serde_json::to_string(&report).expect("serialize");
        let parsed: SensorReport = serde_json::from_str(&json).expect("deserialize");
        prop_assert_eq!(report, parsed);
    }

    /// CockpitReport survives a JSON roundtrip: serialize → deserialize preserves all fields.
    #[test]
    fn cockpit_report_json_roundtrip(report in any_cockpit_report()) {
        let json = serde_json::to_string(&report).expect("serialize");
        let parsed: CockpitReport = serde_json::from_str(&json).expect("deserialize");
        prop_assert_eq!(report, parsed);
    }
}

// ============================================================================
// Finding sort stability property
// ============================================================================

proptest! {
    /// Sorting FindingSortKeys is stable: elements with equal keys retain their original order.
    #[test]
    fn finding_sort_key_sort_is_stable(keys in prop::collection::vec(any_finding_sort_key(), 0..50)) {
        let mut tagged: Vec<(usize, FindingSortKey)> = keys.into_iter().enumerate().collect();
        tagged.sort_by(|a, b| a.1.cmp(&b.1));
        for window in tagged.windows(2) {
            if window[0].1 == window[1].1 {
                prop_assert!(
                    window[0].0 < window[1].0,
                    "stable sort must preserve original order for equal keys"
                );
            }
        }
    }
}

// ============================================================================
// VerdictStatus ordering property
// ============================================================================

proptest! {
    /// For any pair of verdict statuses, the ranking function is injective and antisymmetric.
    #[test]
    fn verdict_status_ordering_any_pair(a in any_verdict_status(), b in any_verdict_status()) {
        let ra = verdict_status_rank(&a);
        let rb = verdict_status_rank(&b);
        if ra == rb {
            prop_assert_eq!(a, b, "equal ranks must correspond to the same variant");
        }
        if ra < rb {
            prop_assert!(rb > ra, "ranking must be antisymmetric");
        }
    }

    /// Ranking is transitive across any three verdict statuses.
    #[test]
    fn verdict_status_ranking_transitive(
        a in any_verdict_status(),
        b in any_verdict_status(),
        c in any_verdict_status()
    ) {
        let ra = verdict_status_rank(&a);
        let rb = verdict_status_rank(&b);
        let rc = verdict_status_rank(&c);
        if ra <= rb && rb <= rc {
            prop_assert!(ra <= rc, "ranking must be transitive");
        }
    }
}

// ============================================================================
// Severity ordering property
// ============================================================================

proptest! {
    /// For any pair of severities, the ranking function is injective and antisymmetric.
    #[test]
    fn severity_ordering_any_pair(a in any_severity(), b in any_severity()) {
        let ra = severity_rank(&a);
        let rb = severity_rank(&b);
        if ra == rb {
            prop_assert_eq!(a, b, "equal ranks must correspond to the same variant");
        }
        if ra < rb {
            prop_assert!(rb > ra, "ranking must be antisymmetric");
        }
    }

    /// Ranking is transitive across any three severities.
    #[test]
    fn severity_ranking_transitive(
        a in any_severity(),
        b in any_severity(),
        c in any_severity()
    ) {
        let ra = severity_rank(&a);
        let rb = severity_rank(&b);
        let rc = severity_rank(&c);
        if ra <= rb && rb <= rc {
            prop_assert!(ra <= rc, "ranking must be transitive");
        }
    }
}
