//! Comprehensive property-based tests for cockpitctl-conform.
//!
//! Covers path hygiene closure, ordering totality/reflexivity, schema determinism,
//! reason lint, sensor ID format, artifact pointers, check composition, and
//! report aggregation properties.

use cockpitctl_conform::{
    ConformChecks, check_artifact_pointers, check_ordering, check_path_hygiene,
    check_reason_tokens, check_sensor_id_format, conform_single, is_valid_reason_token,
};
use cockpitctl_types::*;
use proptest::prelude::*;
use std::collections::BTreeMap;

// ============================================================================
// Helpers
// ============================================================================

fn minimal_sensor_report() -> SensorReport {
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
            reasons: vec![],
        },
        findings: vec![],
        artifacts: vec![],
        data: None,
    }
}

fn make_finding(severity: Severity, code: &str, message: &str, path: Option<&str>) -> Finding {
    Finding {
        severity,
        check_id: None,
        code: code.to_string(),
        message: message.to_string(),
        location: path.map(|p| Location {
            path: Some(p.to_string()),
            line: None,
            col: None,
        }),
        help: None,
        url: None,
        fingerprint: None,
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

fn only(f: impl FnOnce(&mut ConformChecks)) -> ConformChecks {
    let mut c = ConformChecks {
        path_hygiene: false,
        ordering: false,
        reason_lint: false,
        survivability: false,
        tool_error_identity: false,
        sensor_id_format: false,
        artifact_pointers: false,
    };
    f(&mut c);
    c
}

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

fn valid_sensor_id() -> impl Strategy<Value = String> {
    "[a-zA-Z0-9_-]{1,20}"
}

fn clean_relative_path() -> impl Strategy<Value = String> {
    "[a-z][a-z0-9_/]{0,30}".prop_filter("no double-dot segments", |p| !p.contains(".."))
}

// ============================================================================
// 1. Path hygiene is closed: `..` or `/` prefix or `\` always rejected
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    #[test]
    fn path_hygiene_rejects_dotdot(
        prefix in "[a-z]{1,8}",
        suffix in "[a-z]{1,8}",
    ) {
        let path = format!("{}/../../{}", prefix, suffix);
        let mut report = minimal_sensor_report();
        report.findings = vec![make_finding(Severity::Info, "C1", "test", Some(&path))];
        let violations = check_path_hygiene(&report);
        prop_assert!(
            violations.iter().any(|v| v.contains("path traversal") || v.contains("..")),
            "path with '..' must be flagged: {}", path
        );
    }

    #[test]
    fn path_hygiene_rejects_leading_slash(
        suffix in "[a-z][a-z0-9/]{0,20}",
    ) {
        let path = format!("/{}", suffix);
        let mut report = minimal_sensor_report();
        report.findings = vec![make_finding(Severity::Info, "C1", "test", Some(&path))];
        let violations = check_path_hygiene(&report);
        prop_assert!(
            violations.iter().any(|v| v.contains("absolute")),
            "path starting with '/' must be flagged: {}", path
        );
    }

    #[test]
    fn path_hygiene_rejects_backslash(
        prefix in "[a-z]{1,5}",
        suffix in "[a-z]{1,5}",
    ) {
        let path = format!("{}\\{}", prefix, suffix);
        let mut report = minimal_sensor_report();
        report.findings = vec![make_finding(Severity::Info, "C1", "test", Some(&path))];
        let violations = check_path_hygiene(&report);
        prop_assert!(
            violations.iter().any(|v| v.contains("backslash")),
            "path with backslash must be flagged: {}", path
        );
    }
}

// ============================================================================
// 2. Clean paths always pass
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    #[test]
    fn clean_alphanumeric_hyphen_paths_pass(
        segment in "[a-z][a-z0-9-]{0,15}",
    ) {
        let mut report = minimal_sensor_report();
        report.findings = vec![make_finding(Severity::Info, "C1", "test", Some(&segment))];
        let violations = check_path_hygiene(&report);
        prop_assert!(
            violations.is_empty(),
            "clean path {:?} should pass, got: {:?}", segment, violations
        );
    }
}

// ============================================================================
// 3. Ordering is total: swapped pair always detected
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    #[test]
    fn ordering_detects_swapped_pair(
        sensor_id in valid_sensor_id(),
        code_a in "[A-Z][A-Z0-9]{0,4}",
        code_b in "[A-Z][A-Z0-9]{0,4}",
    ) {
        // Error before Info is canonical; Info before Error is wrong.
        let f_high = make_finding(Severity::Error, &code_a, "high", None);
        let f_low = make_finding(Severity::Info, &code_b, "low", None);

        let key_high = make_sort_key(&f_high, &sensor_id);
        let key_low = make_sort_key(&f_low, &sensor_id);

        // Only assert if the keys are actually distinct.
        if key_high != key_low {
            // Put them in wrong order (low-severity first).
            let mut report = minimal_sensor_report();
            report.findings = vec![f_low, f_high];
            let violations = check_ordering(&report, &sensor_id);
            prop_assert!(
                !violations.is_empty(),
                "out-of-order findings must be detected"
            );
        }
    }
}

// ============================================================================
// 4. Ordering is reflexive: single element is always ordered
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    #[test]
    fn single_finding_always_ordered(
        sensor_id in valid_sensor_id(),
        severity in any_severity(),
        code in "[A-Z][A-Z0-9]{0,4}",
    ) {
        let mut report = minimal_sensor_report();
        report.findings = vec![make_finding(severity, &code, "single", None)];
        let violations = check_ordering(&report, &sensor_id);
        prop_assert!(
            violations.is_empty(),
            "single finding must always be ordered: {:?}", violations
        );
    }
}

// ============================================================================
// 5. Schema validation is deterministic: same input → same result
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn schema_validation_deterministic(
        sensor_id in valid_sensor_id(),
        n_findings in 0usize..5,
    ) {
        let mut report = minimal_sensor_report();
        for i in 0..n_findings {
            report.findings.push(make_finding(
                Severity::Info,
                &format!("C{}", i),
                &format!("finding {}", i),
                None,
            ));
        }
        let json = serde_json::to_string(&report).unwrap();

        let r1 = conform_single(&json, &sensor_id, &all_checks()).expect("run 1");
        let r2 = conform_single(&json, &sensor_id, &all_checks()).expect("run 2");

        prop_assert_eq!(r1.violations.len(), r2.violations.len());
        for (v1, v2) in r1.violations.iter().zip(r2.violations.iter()) {
            prop_assert_eq!(&v1.check, &v2.check);
            prop_assert_eq!(&v1.message, &v2.message);
        }
    }
}

// ============================================================================
// 6. Reason lint catches empty tokens
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    #[test]
    fn empty_reason_always_flagged(
        n_good in 0usize..3,
    ) {
        let mut reasons: Vec<String> = (0..n_good).map(|i| format!("ok_{}", i)).collect();
        reasons.push(String::new()); // inject empty token
        let mut report = minimal_sensor_report();
        report.verdict.reasons = reasons;

        let violations = check_reason_tokens(&report);
        prop_assert!(
            violations.iter().any(|v| v.contains("invalid token")),
            "empty reason token must be flagged: {:?}", violations
        );
    }
}

// ============================================================================
// 7. Reason lint passes non-trivial lowercase_underscore tokens
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    #[test]
    fn valid_lowercase_underscore_reason_passes(
        token in "[a-z][a-z0-9_]{0,15}",
    ) {
        prop_assert!(
            is_valid_reason_token(&token),
            "token {:?} should be valid", token
        );

        let mut report = minimal_sensor_report();
        report.verdict.reasons = vec![token];
        let violations = check_reason_tokens(&report);
        prop_assert!(
            violations.is_empty(),
            "valid reason tokens should not trigger violations: {:?}", violations
        );
    }
}

// ============================================================================
// 8. Sensor ID format: valid IDs pass, invalid always fail
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    #[test]
    fn valid_sensor_ids_pass_format(id in valid_sensor_id()) {
        let violations = check_sensor_id_format(&id);
        prop_assert!(
            violations.is_empty(),
            "valid sensor ID {:?} should pass: {:?}", id, violations
        );
    }

    #[test]
    fn sensor_id_with_dot_rejected(
        prefix in "[a-z]{1,5}",
        suffix in "[a-z]{1,5}",
    ) {
        let bad = format!("{}.{}", prefix, suffix);
        let violations = check_sensor_id_format(&bad);
        prop_assert!(
            !violations.is_empty(),
            "sensor ID with dot {:?} must be rejected", bad
        );
    }
}

// ============================================================================
// 9. Artifact pointers: valid pass, traversal/absolute fail
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    #[test]
    fn valid_artifact_pointers_pass(
        id in "[a-z][a-z0-9]{0,10}",
        path in clean_relative_path(),
    ) {
        let mut report = minimal_sensor_report();
        report.artifacts = vec![ArtifactPointer {
            id,
            path,
            mime: "text/plain".to_string(),
            schema: None,
        }];
        let violations = check_artifact_pointers(&report);
        prop_assert!(
            violations.is_empty(),
            "valid artifact should pass: {:?}", violations
        );
    }

    #[test]
    fn artifact_traversal_rejected(
        prefix in "[a-z]{1,5}",
        suffix in "[a-z]{1,5}",
    ) {
        let mut report = minimal_sensor_report();
        report.artifacts = vec![ArtifactPointer {
            id: "art".to_string(),
            path: format!("{}/../{}", prefix, suffix),
            mime: "text/plain".to_string(),
            schema: None,
        }];
        let violations = check_artifact_pointers(&report);
        prop_assert!(
            violations.iter().any(|v| v.contains("..")),
            "artifact with '..' must be flagged"
        );
    }
}

// ============================================================================
// 10. ConformResult roundtrip: violation count is stable across serialization
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn sensor_report_serde_roundtrip_preserves_conformance(
        sensor_id in valid_sensor_id(),
        n_findings in 0usize..4,
    ) {
        let mut report = minimal_sensor_report();
        for i in 0..n_findings {
            report.findings.push(make_finding(
                Severity::Info,
                &format!("C{}", i),
                &format!("msg {}", i),
                None,
            ));
        }

        let json1 = serde_json::to_string(&report).unwrap();
        let parsed: SensorReport = serde_json::from_str(&json1).unwrap();
        let json2 = serde_json::to_string(&parsed).unwrap();

        // The serialized forms must be identical (deterministic serde).
        prop_assert_eq!(&json1, &json2, "serde roundtrip must be stable");

        // Conformance results must be identical on both.
        let checks = only(|c| { c.path_hygiene = true; c.ordering = true; c.reason_lint = true; });
        let r1 = conform_single(&json1, &sensor_id, &checks).expect("run 1");
        let r2 = conform_single(&json2, &sensor_id, &checks).expect("run 2");
        prop_assert_eq!(r1.violations.len(), r2.violations.len());
    }
}

// ============================================================================
// 11. Multiple checks compose: running independently = running together
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn checks_compose_independently(
        sensor_id in valid_sensor_id(),
    ) {
        // Build a report that may trigger path_hygiene and ordering violations.
        let mut report = minimal_sensor_report();
        report.findings = vec![
            make_finding(Severity::Info, "I1", "info", Some("src/a.rs")),
            make_finding(Severity::Error, "E1", "error", Some("src/b.rs")),
        ];
        let json = serde_json::to_string(&report).unwrap();

        // Run each check individually.
        let r_path = conform_single(&json, &sensor_id, &only(|c| c.path_hygiene = true))
            .expect("path check");
        let r_order = conform_single(&json, &sensor_id, &only(|c| c.ordering = true))
            .expect("ordering check");
        let r_reason = conform_single(&json, &sensor_id, &only(|c| c.reason_lint = true))
            .expect("reason check");
        let r_sensor = conform_single(&json, &sensor_id, &only(|c| c.sensor_id_format = true))
            .expect("sensor id check");
        let r_artifact = conform_single(&json, &sensor_id, &only(|c| c.artifact_pointers = true))
            .expect("artifact check");

        // Run all together.
        let combined = ConformChecks {
            path_hygiene: true,
            ordering: true,
            reason_lint: true,
            survivability: false,
            tool_error_identity: false,
            sensor_id_format: true,
            artifact_pointers: true,
        };
        let r_all = conform_single(&json, &sensor_id, &combined).expect("all checks");

        let sum = r_path.violations.len()
            + r_order.violations.len()
            + r_reason.violations.len()
            + r_sensor.violations.len()
            + r_artifact.violations.len();

        prop_assert_eq!(
            sum,
            r_all.violations.len(),
            "individual check violations must sum to combined: {} vs {}",
            sum, r_all.violations.len()
        );
    }
}

// ============================================================================
// 12. Report aggregation: multiple sensors produce independent violations
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn multiple_sensor_reports_independent(
        id_a in "[a-z]{3,8}",
        id_b in "[a-z]{3,8}",
    ) {
        // Two clean reports should each pass independently.
        let report_a = minimal_sensor_report();
        let report_b = minimal_sensor_report();

        let json_a = serde_json::to_string(&report_a).unwrap();
        let json_b = serde_json::to_string(&report_b).unwrap();

        let checks = all_checks();
        let r_a = conform_single(&json_a, &id_a, &checks).expect("sensor a");
        let r_b = conform_single(&json_b, &id_b, &checks).expect("sensor b");

        prop_assert!(
            r_a.is_pass(),
            "clean report for sensor {:?} should pass: {:?}", id_a, r_a.violations
        );
        prop_assert!(
            r_b.is_pass(),
            "clean report for sensor {:?} should pass: {:?}", id_b, r_b.violations
        );

        // A report with bad paths should fail regardless of the other sensor.
        let mut bad_report = minimal_sensor_report();
        bad_report.findings = vec![make_finding(Severity::Info, "T1", "bad", Some("../escape"))];
        let bad_json = serde_json::to_string(&bad_report).unwrap();

        let only_path = only(|c| c.path_hygiene = true);
        let r_bad = conform_single(&bad_json, &id_a, &only_path).expect("bad sensor");
        prop_assert!(
            !r_bad.is_pass(),
            "report with traversal must fail path hygiene"
        );

        // The clean sensor should still pass.
        let r_clean = conform_single(&json_b, &id_b, &only_path).expect("clean sensor");
        prop_assert!(
            r_clean.is_pass(),
            "clean sensor should be unaffected by other sensor's violations"
        );
    }
}
