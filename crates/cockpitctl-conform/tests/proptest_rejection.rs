//! Property-based tests for cockpitctl-conform rejection of known-bad patterns.
//!
//! Tests that schema validation and conformance checks reliably reject
//! malformed inputs while accepting well-formed ones.

use cockpitctl_conform::{
    ConformChecks, check_ordering, check_path_hygiene, check_reason_tokens, conform_single,
};
use cockpitctl_types::*;
use proptest::prelude::*;
use std::collections::BTreeMap;

fn build_minimal_report(findings: Vec<Finding>, reasons: Vec<String>) -> SensorReport {
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

fn all_checks() -> ConformChecks {
    ConformChecks {
        path_hygiene: true,
        ordering: true,
        reason_lint: true,
        survivability: true,
        tool_error_identity: true,
        sensor_id_format: true,
        artifact_pointers: true,
    }
}

// ============================================================================
// Property: absolute paths are always caught by path hygiene
// ============================================================================

proptest! {
    /// Any finding whose path starts with "/" triggers a path hygiene violation.
    #[test]
    fn absolute_paths_always_rejected(
        suffix in "[a-z][a-z0-9/_.-]{0,20}",
    ) {
        let abs_path = format!("/{}", suffix);
        let report = build_minimal_report(
            vec![Finding {
                severity: Severity::Info,
                check_id: None,
                code: "T1".to_string(),
                message: "test".to_string(),
                location: Some(Location {
                    path: Some(abs_path),
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
            violations.iter().any(|v| v.contains("absolute")),
            "absolute path must be flagged: {:?}",
            violations
        );
    }

    /// Any finding whose path contains backslash triggers a path hygiene violation.
    #[test]
    fn backslash_paths_always_rejected(
        prefix in "[a-z]{1,5}",
        suffix in "[a-z]{1,5}",
    ) {
        let path_with_backslash = format!("{}\\{}", prefix, suffix);
        let report = build_minimal_report(
            vec![Finding {
                severity: Severity::Info,
                check_id: None,
                code: "T1".to_string(),
                message: "test".to_string(),
                location: Some(Location {
                    path: Some(path_with_backslash),
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
            violations.iter().any(|v| v.contains("backslash")),
            "backslash path must be flagged: {:?}",
            violations
        );
    }
}

// ============================================================================
// Property: empty reports always pass all conform checks
// ============================================================================

proptest! {
    /// A report with no findings and valid metadata passes all checks.
    #[test]
    fn empty_report_passes_all_checks(
        sensor_id in "[a-zA-Z0-9_-]{1,20}",
    ) {
        let report = build_minimal_report(vec![], vec![]);
        let json = serde_json::to_string(&report).unwrap();
        let result = conform_single(&json, &sensor_id, &all_checks()).expect("conform_single");

        prop_assert!(
            result.violations.is_empty(),
            "empty report with valid sensor_id should have no violations: {:?}",
            result.violations
        );
    }
}

// ============================================================================
// Property: reason tokens with spaces are always rejected
// ============================================================================

proptest! {
    /// Any reason token containing a space fails validation.
    #[test]
    fn reason_tokens_with_spaces_rejected(
        prefix in "[a-z]{1,5}",
        suffix in "[a-z]{1,5}",
    ) {
        let bad_token = format!("{} {}", prefix, suffix);
        let report = build_minimal_report(vec![], vec![bad_token.clone()]);

        let violations = check_reason_tokens(&report);
        prop_assert!(
            !violations.is_empty(),
            "reason token with space {:?} must be flagged",
            bad_token
        );
    }
}

// ============================================================================
// Property: mis-ordered findings are always caught
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// Deliberately reversing findings triggers an ordering violation.
    #[test]
    fn reversed_findings_detected(
        sensor_id in "[a-z][a-z0-9]{0,8}",
    ) {
        // Create two findings that will have different sort keys.
        let f_error = Finding {
            severity: Severity::Error,
            check_id: None,
            code: "E001".to_string(),
            message: "error finding".to_string(),
            location: None,
            help: None,
            url: None,
            fingerprint: None,
            data: None,
        };
        let f_info = Finding {
            severity: Severity::Info,
            check_id: None,
            code: "I001".to_string(),
            message: "info finding".to_string(),
            location: None,
            help: None,
            url: None,
            fingerprint: None,
            data: None,
        };

        // Info before Error is incorrect ordering.
        let report = build_minimal_report(vec![f_info, f_error], vec![]);
        let violations = check_ordering(&report, &sensor_id);
        prop_assert!(
            !violations.is_empty(),
            "reversed findings must trigger ordering violation"
        );
    }
}

// ============================================================================
// Property: conform_single is idempotent
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// Running conform_single on the same input always gives the same violations.
    #[test]
    fn conform_single_idempotent(
        sensor_id in "[a-zA-Z0-9_-]{1,15}",
        findings_count in 0usize..5,
    ) {
        let findings: Vec<Finding> = (0..findings_count)
            .map(|i| Finding {
                severity: Severity::Info,
                check_id: None,
                code: format!("T{}", i),
                message: format!("finding {}", i),
                location: None,
                help: None,
                url: None,
                fingerprint: None,
                data: None,
            })
            .collect();

        let report = build_minimal_report(findings, vec![]);
        let json = serde_json::to_string(&report).unwrap();

        let r1 = conform_single(&json, &sensor_id, &all_checks()).expect("first run");
        let r2 = conform_single(&json, &sensor_id, &all_checks()).expect("second run");
        let r3 = conform_single(&json, &sensor_id, &all_checks()).expect("third run");

        prop_assert_eq!(r1.violations.len(), r2.violations.len());
        prop_assert_eq!(r2.violations.len(), r3.violations.len());

        for ((v1, v2), v3) in r1
            .violations
            .iter()
            .zip(r2.violations.iter())
            .zip(r3.violations.iter())
        {
            prop_assert_eq!(&v1.check, &v2.check);
            prop_assert_eq!(&v2.check, &v3.check);
            prop_assert_eq!(&v1.message, &v2.message);
            prop_assert_eq!(&v2.message, &v3.message);
        }
    }
}
