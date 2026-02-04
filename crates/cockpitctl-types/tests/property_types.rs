//! Property-based tests for cockpitctl-types.
//!
//! Tests invariants for ranking functions, sort keys, and type conversions.

use cockpitctl_types::{
    severity_rank, verdict_status_rank, FindingSortKey, Severity, VerdictStatus,
};
use proptest::prelude::*;

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
        0u8..=2,                       // severity_rank (Error=0, Warn=1, Info=2)
        "[a-z_][a-z0-9_]{0,20}",       // sensor_id
        prop::option::of("[a-z/._-]{0,50}").prop_map(|o| o.unwrap_or_default()), // path
        any::<u32>(),                  // line
        "[A-Z][A-Z0-9_]{0,20}",        // code
        ".{0,100}",                    // message
    )
        .prop_map(|(severity_rank, sensor_id, path, line, code, message)| FindingSortKey {
            severity_rank,
            sensor_id,
            path,
            line,
            code,
            message,
        })
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
    assert_eq!(sorted.len(), 4, "Each verdict status must have a unique rank");

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
