//! Property-based tests for cockpitctl-conform.
//!
//! Tests conformance checking invariants using proptest strategies.

use cockpitctl_conform::{
    check_ordering, check_path_hygiene, check_reason_tokens, check_sensor_id_format,
    is_valid_reason_token,
};
use cockpitctl_types::*;
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

/// Generate a valid reason token matching `^[a-z0-9_]+$`.
fn valid_reason_token() -> impl Strategy<Value = String> {
    "[a-z0-9_]{1,20}"
}

/// Generate a valid sensor ID matching `[a-zA-Z0-9_-]+`.
fn valid_sensor_id() -> impl Strategy<Value = String> {
    "[a-zA-Z0-9_-]{1,20}"
}

/// Generate a clean relative path (no traversal, no backslash, no absolute).
fn clean_relative_path() -> impl Strategy<Value = String> {
    "[a-z][a-z0-9_/.-]{0,40}".prop_filter("no double-dot segments", |p| !p.contains(".."))
}

/// Generate a valid Location with clean paths.
fn clean_location() -> impl Strategy<Value = Option<Location>> {
    prop::option::of(
        (
            prop::option::of(clean_relative_path()),
            prop::option::of(0u32..10000),
        )
            .prop_map(|(path, line)| Location {
                path,
                line,
                col: None,
            }),
    )
}

/// Generate a vector of findings sorted in canonical order for a given sensor_id.
fn sorted_findings(sensor_id: String) -> impl Strategy<Value = Vec<Finding>> {
    prop::collection::vec(
        (
            any_severity(),
            clean_location(),
            "[A-Z][A-Z0-9]{0,5}",
            "[a-z ]{1,30}",
        ),
        0..10,
    )
    .prop_map(move |items| {
        let mut findings: Vec<Finding> = items
            .into_iter()
            .map(|(severity, location, code, message)| Finding {
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
            .collect();

        // Sort by the canonical key
        findings.sort_by(|a, b| {
            let key_a = FindingSortKey {
                severity_rank: severity_rank(&a.severity),
                sensor_id: sensor_id.clone(),
                path: a
                    .location
                    .as_ref()
                    .and_then(|l| l.path.as_deref())
                    .unwrap_or("")
                    .to_string(),
                line: a.location.as_ref().and_then(|l| l.line).unwrap_or(0),
                code: a.code.clone(),
                message: a.message.clone(),
            };
            let key_b = FindingSortKey {
                severity_rank: severity_rank(&b.severity),
                sensor_id: sensor_id.clone(),
                path: b
                    .location
                    .as_ref()
                    .and_then(|l| l.path.as_deref())
                    .unwrap_or("")
                    .to_string(),
                line: b.location.as_ref().and_then(|l| l.line).unwrap_or(0),
                code: b.code.clone(),
                message: b.message.clone(),
            };
            key_a.cmp(&key_b)
        });

        findings
    })
}

/// Build a minimal valid SensorReport with given findings and reasons.
fn build_sensor_report(findings: Vec<Finding>, reasons: Vec<String>) -> SensorReport {
    SensorReport {
        schema: "sensor.report.v1".to_string(),
        tool: ToolInfo {
            name: "test".to_string(),
            version: "1.0.0".to_string(),
            commit: None,
        },
        run: RunInfo {
            started_at: "2026-02-01T00:00:00Z".to_string(),
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
            reasons,
        },
        findings,
        artifacts: vec![],
        data: None,
    }
}

/// Strategy for a well-formed SensorReport (sorted findings, valid tokens).
fn well_formed_sensor_report() -> impl Strategy<Value = (SensorReport, String)> {
    valid_sensor_id().prop_flat_map(|sid| {
        (
            sorted_findings(sid.clone()),
            prop::collection::vec(valid_reason_token(), 0..3),
        )
            .prop_map(move |(findings, reasons)| {
                (build_sensor_report(findings, reasons), sid.clone())
            })
    })
}

// ============================================================================
// Property: well-formed receipts pass all checks
// ============================================================================

proptest! {
    #[test]
    fn well_formed_receipts_pass_all_checks((report, sensor_id) in well_formed_sensor_report()) {
        let path_violations = check_path_hygiene(&report);
        prop_assert!(
            path_violations.is_empty(),
            "well-formed report should have no path hygiene violations: {:?}",
            path_violations
        );

        let ordering_violations = check_ordering(&report, &sensor_id);
        prop_assert!(
            ordering_violations.is_empty(),
            "well-formed report should have no ordering violations: {:?}",
            ordering_violations
        );

        let reason_violations = check_reason_tokens(&report);
        prop_assert!(
            reason_violations.is_empty(),
            "well-formed report should have no reason token violations: {:?}",
            reason_violations
        );

        let id_violations = check_sensor_id_format(&sensor_id);
        prop_assert!(
            id_violations.is_empty(),
            "well-formed sensor_id should pass format check: {:?}",
            id_violations
        );
    }
}

// ============================================================================
// Property: path traversal is always caught
// ============================================================================

proptest! {
    /// Any finding whose path contains ".." triggers a path hygiene violation.
    #[test]
    fn path_traversal_always_caught(
        prefix in "[a-z]{1,10}",
        suffix in "[a-z]{1,10}",
    ) {
        let traversal_path = format!("{}/../{}", prefix, suffix);
        let report = build_sensor_report(
            vec![Finding {
                severity: Severity::Info,
                check_id: None,
                code: "T1".to_string(),
                message: "test".to_string(),
                location: Some(Location {
                    path: Some(traversal_path),
                    line: Some(1),
                    col: None,
                }),
                help: None,
                url: None,
                fingerprint: None,
                data: None,
            }],
            vec![],
        );

        let violations = check_path_hygiene(&report);
        prop_assert!(
            violations.iter().any(|v| v.contains("path traversal")),
            "path containing '..' must be flagged: {:?}",
            violations
        );
    }

    /// Sensor IDs containing ".." are rejected by sensor_id_format check
    /// (since ".." contains "." which is not in [a-zA-Z0-9_-]).
    #[test]
    fn sensor_id_with_dots_always_rejected(
        prefix in "[a-z]{0,5}",
        suffix in "[a-z]{0,5}",
    ) {
        let bad_id = format!("{}..{}", prefix, suffix);
        let violations = check_sensor_id_format(&bad_id);
        prop_assert!(
            !violations.is_empty(),
            "sensor_id containing '..' must be rejected: {:?}",
            bad_id
        );
    }
}

// ============================================================================
// Property: schema validation is deterministic
// ============================================================================

proptest! {
    /// Running conform_single twice on the same input yields the same violation count.
    #[test]
    fn schema_validation_deterministic((report, sensor_id) in well_formed_sensor_report()) {
        let json = serde_json::to_string(&report).expect("serialize");
        let checks = cockpitctl_conform::ConformChecks {
            path_hygiene: true,
            ordering: true,
            reason_lint: true,
            survivability: true,
            tool_error_identity: true,
            sensor_id_format: true,
            artifact_pointers: true,
        };

        let result1 = cockpitctl_conform::conform_single(&json, &sensor_id, &checks)
            .expect("first run");
        let result2 = cockpitctl_conform::conform_single(&json, &sensor_id, &checks)
            .expect("second run");

        prop_assert_eq!(
            result1.violations.len(),
            result2.violations.len(),
            "same input must produce same number of violations"
        );

        // Check that each violation message matches
        for (v1, v2) in result1.violations.iter().zip(result2.violations.iter()) {
            prop_assert_eq!(&v1.check, &v2.check);
            prop_assert_eq!(&v1.message, &v2.message);
        }
    }
}

// ============================================================================
// Property: ordering check is idempotent (re-sorting sorted findings is no-op)
// ============================================================================

proptest! {
    /// Sorting already-sorted findings does not change them, and check_ordering
    /// reports no violations for properly sorted findings.
    #[test]
    fn ordering_check_idempotent(
        sensor_id in valid_sensor_id(),
        findings in prop::collection::vec(
            (any_severity(), clean_location(), "[A-Z][A-Z0-9]{0,5}", "[a-z ]{1,30}"),
            0..15,
        ),
    ) {
        let sid = sensor_id.clone();
        let mut items: Vec<Finding> = findings
            .into_iter()
            .map(|(severity, location, code, message)| Finding {
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
            .collect();

        // Sort once
        items.sort_by(|a, b| {
            make_sort_key(a, &sid).cmp(&make_sort_key(b, &sid))
        });

        // check_ordering should pass on sorted findings
        let report = build_sensor_report(items.clone(), vec![]);
        let violations = check_ordering(&report, &sensor_id);
        prop_assert!(
            violations.is_empty(),
            "sorted findings must pass ordering check: {:?}",
            violations
        );

        // Sort again — should be identical
        let mut items2 = items.clone();
        items2.sort_by(|a, b| {
            make_sort_key(a, &sid).cmp(&make_sort_key(b, &sid))
        });
        prop_assert_eq!(
            items.iter().map(|f| &f.code).collect::<Vec<_>>(),
            items2.iter().map(|f| &f.code).collect::<Vec<_>>(),
            "re-sorting already sorted findings must be a no-op"
        );
    }
}

// ============================================================================
// Property: reason token validation
// ============================================================================

proptest! {
    /// Valid reason tokens always pass.
    #[test]
    fn valid_reason_tokens_always_pass(token in valid_reason_token()) {
        prop_assert!(
            is_valid_reason_token(&token),
            "token {:?} should be valid",
            token
        );
    }

    /// Tokens with uppercase letters always fail.
    #[test]
    fn uppercase_reason_tokens_always_fail(token in "[A-Z][a-zA-Z0-9_]{0,10}") {
        prop_assert!(
            !is_valid_reason_token(&token),
            "token {:?} with uppercase should be invalid",
            token
        );
    }

    /// Tokens with hyphens always fail (not in [a-z0-9_]).
    #[test]
    fn hyphenated_reason_tokens_always_fail(
        prefix in "[a-z]{1,5}",
        suffix in "[a-z]{1,5}",
    ) {
        let token = format!("{}-{}", prefix, suffix);
        prop_assert!(
            !is_valid_reason_token(&token),
            "token {:?} with hyphen should be invalid",
            token
        );
    }
}

// ============================================================================
// Property: sensor ID format
// ============================================================================

proptest! {
    /// Valid sensor IDs always pass the format check.
    #[test]
    fn valid_sensor_ids_always_pass(id in valid_sensor_id()) {
        let violations = check_sensor_id_format(&id);
        prop_assert!(
            violations.is_empty(),
            "valid sensor_id {:?} should pass: {:?}",
            id,
            violations
        );
    }

    /// Sensor IDs with disallowed characters always fail.
    #[test]
    fn sensor_ids_with_special_chars_always_fail(
        base in "[a-z]{1,5}",
        special in prop::sample::select(vec!['.', '/', '\\', ' ', '@', '#', '!', '~']),
    ) {
        let bad_id = format!("{}{}{}", base, special, base);
        let violations = check_sensor_id_format(&bad_id);
        prop_assert!(
            !violations.is_empty(),
            "sensor_id {:?} with special char should fail",
            bad_id
        );
    }

    /// Empty sensor ID always fails.
    #[test]
    fn empty_sensor_id_always_fails(_dummy in Just(())) {
        let violations = check_sensor_id_format("");
        prop_assert!(
            !violations.is_empty(),
            "empty sensor_id must fail"
        );
    }
}

// ============================================================================
// Helpers
// ============================================================================

fn make_sort_key(f: &Finding, sensor_id: &str) -> FindingSortKey {
    FindingSortKey {
        severity_rank: severity_rank(&f.severity),
        sensor_id: sensor_id.to_string(),
        path: f
            .location
            .as_ref()
            .and_then(|l| l.path.as_deref())
            .unwrap_or("")
            .to_string(),
        line: f.location.as_ref().and_then(|l| l.line).unwrap_or(0),
        code: f.code.clone(),
        message: f.message.clone(),
    }
}
