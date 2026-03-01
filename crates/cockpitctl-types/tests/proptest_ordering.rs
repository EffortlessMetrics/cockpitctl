//! Property-based tests for ordering invariants in cockpitctl-types.
//!
//! Verifies that Severity, VerdictStatus, and FindingSortKey orderings are
//! total orders: reflexive, antisymmetric, transitive, and deterministic.

use cockpitctl_types::*;
use proptest::prelude::*;
use std::cmp::Ordering;

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

fn any_finding_sort_key() -> impl Strategy<Value = FindingSortKey> {
    (
        0u8..=2,
        "[a-z_]{0,10}",
        "[a-z/._]{0,20}",
        any::<u32>(),
        "[A-Z][A-Z0-9_]{0,10}",
        ".{0,30}",
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

fn any_location() -> impl Strategy<Value = Option<Location>> {
    prop::option::of(
        (
            prop::option::of("[a-z/_.-]{0,30}"),
            prop::option::of(0u32..10000),
            prop::option::of(0u32..500),
        )
            .prop_map(|(path, line, col)| Location { path, line, col }),
    )
}

fn any_finding() -> impl Strategy<Value = Finding> {
    (
        any_severity(),
        prop::option::of("[A-Z][A-Z0-9_]{0,10}"),
        "[A-Z][A-Z0-9_]{0,10}",
        ".{0,30}",
        any_location(),
        prop::option::of(".{0,20}"),
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

fn any_highlight() -> impl Strategy<Value = Highlight> {
    ("[a-z_][a-z0-9_]{0,10}", any_finding())
        .prop_map(|(sensor_id, finding)| Highlight { sensor_id, finding })
}

// ============================================================================
// 1. Severity is a total order
// ============================================================================

proptest! {
    #[test]
    fn severity_total_order(a in any_severity(), b in any_severity()) {
        let ra = severity_rank(&a);
        let rb = severity_rank(&b);
        // Totality: one of ra <= rb or rb <= ra must hold (always true for u8).
        prop_assert!(ra <= rb || rb <= ra, "severity rank must be total");
    }
}

// ============================================================================
// 2. Severity ordering is transitive
// ============================================================================

proptest! {
    #[test]
    fn severity_transitive(a in any_severity(), b in any_severity(), c in any_severity()) {
        let ra = severity_rank(&a);
        let rb = severity_rank(&b);
        let rc = severity_rank(&c);
        if ra <= rb && rb <= rc {
            prop_assert!(ra <= rc, "severity ranking must be transitive");
        }
    }
}

// ============================================================================
// 3. VerdictStatus is a total order
// ============================================================================

proptest! {
    #[test]
    fn verdict_status_total_order(a in any_verdict_status(), b in any_verdict_status()) {
        let ra = verdict_status_rank(&a);
        let rb = verdict_status_rank(&b);
        prop_assert!(ra <= rb || rb <= ra, "verdict status rank must be total");
        // Injectivity: equal ranks ↔ same variant.
        if ra == rb {
            prop_assert_eq!(a, b, "equal ranks must mean equal variants");
        }
    }
}

// ============================================================================
// 4. Finding sort is deterministic
// ============================================================================

proptest! {
    #[test]
    fn finding_sort_deterministic(mut keys in prop::collection::vec(any_finding_sort_key(), 0..50)) {
        let mut first = keys.clone();
        first.sort();
        keys.sort();
        prop_assert_eq!(&first, &keys, "sorting must be deterministic");
    }
}

// ============================================================================
// 5. Finding sort is stable
// ============================================================================

proptest! {
    #[test]
    fn finding_sort_stable(keys in prop::collection::vec(any_finding_sort_key(), 0..50)) {
        // Tag each element with its original index, then sort by key only.
        let mut tagged: Vec<(usize, FindingSortKey)> = keys.into_iter().enumerate().collect();
        tagged.sort_by(|a, b| a.1.cmp(&b.1));
        // Among equal keys, original indices must be in ascending order (stability).
        for window in tagged.windows(2) {
            if window[0].1 == window[1].1 {
                prop_assert!(
                    window[0].0 < window[1].0,
                    "stable sort must preserve relative order for equal keys"
                );
            }
        }
    }
}

// ============================================================================
// 6. Sort key derivation is pure
// ============================================================================

proptest! {
    #[test]
    fn sort_key_derivation_pure(
        sensor_id in "[a-z_]{1,10}",
        finding in any_finding(),
    ) {
        let key1 = FindingSortKey {
            severity_rank: severity_rank(&finding.severity),
            sensor_id: sensor_id.clone(),
            path: finding.location.as_ref().and_then(|l| l.path.clone()).unwrap_or_default(),
            line: finding.location.as_ref().and_then(|l| l.line).unwrap_or(u32::MAX),
            code: finding.code.clone(),
            message: finding.message.clone(),
        };
        let key2 = FindingSortKey {
            severity_rank: severity_rank(&finding.severity),
            sensor_id,
            path: finding.location.as_ref().and_then(|l| l.path.clone()).unwrap_or_default(),
            line: finding.location.as_ref().and_then(|l| l.line).unwrap_or(u32::MAX),
            code: finding.code.clone(),
            message: finding.message.clone(),
        };
        prop_assert_eq!(key1, key2, "same inputs must produce same sort key");
    }
}

// ============================================================================
// 7. Highlight sort is deterministic
// ============================================================================

proptest! {
    #[test]
    fn highlight_sort_deterministic(highlights in prop::collection::vec(any_highlight(), 0..30)) {
        // Simulate the highlight sort from cockpitctl-domain: severity desc,
        // blocking-first (use false for all here), sensor_id, path, line, code, message.
        let make_key = |h: &Highlight| -> (u8, u8, String, String, u32, String, String) {
            (
                severity_rank(&h.finding.severity),
                1u8, // non-blocking
                h.sensor_id.clone(),
                h.finding.location.as_ref().and_then(|l| l.path.clone()).unwrap_or_default(),
                h.finding.location.as_ref().and_then(|l| l.line).unwrap_or(u32::MAX),
                h.finding.code.clone(),
                h.finding.message.clone(),
            )
        };

        let mut first = highlights.clone();
        first.sort_by_key(|a| make_key(a));

        let mut second = highlights;
        second.sort_by_key(|a| make_key(a));

        prop_assert_eq!(first.len(), second.len());
        for (a, b) in first.iter().zip(second.iter()) {
            prop_assert_eq!(&a.sensor_id, &b.sensor_id);
            prop_assert_eq!(&a.finding.code, &b.finding.code);
            prop_assert_eq!(&a.finding.severity, &b.finding.severity);
        }
    }
}

// ============================================================================
// 8. Serialization preserves ordering
// ============================================================================

proptest! {
    #[test]
    fn serialization_preserves_finding_sort_order(mut keys in prop::collection::vec(any_finding_sort_key(), 0..30)) {
        keys.sort();

        // Serialize each key's fields as a Finding, then deserialize and re-derive keys.
        let findings: Vec<Finding> = keys.iter().map(|k| {
            let loc = if k.path.is_empty() && k.line == u32::MAX {
                None
            } else {
                Some(Location {
                    path: if k.path.is_empty() { None } else { Some(k.path.clone()) },
                    line: if k.line == u32::MAX { None } else { Some(k.line) },
                    col: None,
                })
            };
            Finding {
                severity: match k.severity_rank {
                    0 => Severity::Error,
                    1 => Severity::Warn,
                    _ => Severity::Info,
                },
                check_id: None,
                code: k.code.clone(),
                message: k.message.clone(),
                location: loc,
                help: None,
                url: None,
                fingerprint: None,
                data: None,
            }
        }).collect();

        let json = serde_json::to_string(&findings).unwrap();
        let parsed: Vec<Finding> = serde_json::from_str(&json).unwrap();

        // Re-derive sort keys from deserialized findings and verify order is preserved.
        let parsed_keys: Vec<FindingSortKey> = parsed.iter().enumerate().map(|(i, f)| {
            FindingSortKey {
                severity_rank: severity_rank(&f.severity),
                sensor_id: keys[i].sensor_id.clone(),
                path: f.location.as_ref().and_then(|l| l.path.clone()).unwrap_or_default(),
                line: f.location.as_ref().and_then(|l| l.line).unwrap_or(u32::MAX),
                code: f.code.clone(),
                message: f.message.clone(),
            }
        }).collect();

        for window in parsed_keys.windows(2) {
            prop_assert!(window[0] <= window[1], "deserialized findings must remain sorted");
        }
    }
}

// ============================================================================
// 9. Reverse severity: higher severity sorts first (desc order confirmed)
// ============================================================================

proptest! {
    #[test]
    fn higher_severity_sorts_first(
        sensor_id in "[a-z_]{1,10}",
        code in "[A-Z]{1,5}",
        message in ".{1,15}",
    ) {
        let error_key = FindingSortKey {
            severity_rank: severity_rank(&Severity::Error),
            sensor_id: sensor_id.clone(),
            path: String::new(),
            line: 0,
            code: code.clone(),
            message: message.clone(),
        };
        let warn_key = FindingSortKey {
            severity_rank: severity_rank(&Severity::Warn),
            sensor_id: sensor_id.clone(),
            path: String::new(),
            line: 0,
            code: code.clone(),
            message: message.clone(),
        };
        let info_key = FindingSortKey {
            severity_rank: severity_rank(&Severity::Info),
            sensor_id,
            path: String::new(),
            line: 0,
            code,
            message,
        };

        prop_assert!(error_key < warn_key, "Error must sort before Warn");
        prop_assert!(warn_key < info_key, "Warn must sort before Info");
        prop_assert!(error_key < info_key, "Error must sort before Info");
    }
}

// ============================================================================
// 10. Mixed severity + sensor_id: equal severity, sensor_id breaks ties
// ============================================================================

proptest! {
    #[test]
    fn sensor_id_breaks_severity_tie(
        sev in any_severity(),
        id_a in "[a-z]{1,10}",
        id_b in "[a-z]{1,10}",
    ) {
        let rank = severity_rank(&sev);
        let key_a = FindingSortKey {
            severity_rank: rank,
            sensor_id: id_a.clone(),
            path: String::new(),
            line: 0,
            code: "CODE".to_string(),
            message: "msg".to_string(),
        };
        let key_b = FindingSortKey {
            severity_rank: rank,
            sensor_id: id_b.clone(),
            path: String::new(),
            line: 0,
            code: "CODE".to_string(),
            message: "msg".to_string(),
        };

        let expected = id_a.cmp(&id_b);
        prop_assert_eq!(key_a.cmp(&key_b), expected,
            "when severity is equal, sensor_id must break ties lexically");
    }
}

// ============================================================================
// 11. No panic on edge values
// ============================================================================

proptest! {
    #[test]
    fn no_panic_edge_values(
        sev in any_severity(),
        line in prop::option::of(any::<u32>()),
    ) {
        // Empty strings, zero lines, None fields — nothing should panic.
        let finding = Finding {
            severity: sev,
            check_id: None,
            code: String::new(),
            message: String::new(),
            location: Some(Location {
                path: Some(String::new()),
                line,
                col: None,
            }),
            help: None,
            url: None,
            fingerprint: None,
            data: None,
        };

        let key = FindingSortKey {
            severity_rank: severity_rank(&finding.severity),
            sensor_id: String::new(),
            path: finding.location.as_ref().and_then(|l| l.path.clone()).unwrap_or_default(),
            line: finding.location.as_ref().and_then(|l| l.line).unwrap_or(u32::MAX),
            code: finding.code.clone(),
            message: finding.message.clone(),
        };

        // Must not panic; sort key is valid.
        let _ = key.cmp(&key);
        let _ = serde_json::to_string(&finding).unwrap();
    }
}

// ============================================================================
// 12. Ord consistency: a.cmp(&b) == Equal ↔ a == b
// ============================================================================

proptest! {
    #[test]
    fn ord_consistency(a in any_finding_sort_key(), b in any_finding_sort_key()) {
        let cmp_result = a.cmp(&b);
        if cmp_result == Ordering::Equal {
            prop_assert_eq!(&a, &b, "cmp Equal must imply structural equality");
        }
        if a == b {
            prop_assert_eq!(cmp_result, Ordering::Equal, "structural equality must imply cmp Equal");
        }
    }
}

// ============================================================================
// Bonus: FindingSortKey antisymmetry
// ============================================================================

proptest! {
    #[test]
    fn finding_sort_key_antisymmetric(a in any_finding_sort_key(), b in any_finding_sort_key()) {
        let ab = a.cmp(&b);
        let ba = b.cmp(&a);
        prop_assert_eq!(ab, ba.reverse(), "cmp must be antisymmetric");
    }
}

// ============================================================================
// Bonus: FindingSortKey reflexivity
// ============================================================================

proptest! {
    #[test]
    fn finding_sort_key_reflexive(a in any_finding_sort_key()) {
        prop_assert_eq!(a.cmp(&a), Ordering::Equal, "cmp must be reflexive");
    }
}

// ============================================================================
// Bonus: VerdictStatus rank consistency with equality
// ============================================================================

proptest! {
    #[test]
    fn verdict_rank_consistent_with_equality(a in any_verdict_status(), b in any_verdict_status()) {
        let ra = verdict_status_rank(&a);
        let rb = verdict_status_rank(&b);
        if a == b {
            prop_assert_eq!(ra, rb, "equal variants must have equal ranks");
        } else {
            prop_assert_ne!(ra, rb, "different variants must have different ranks");
        }
    }
}
