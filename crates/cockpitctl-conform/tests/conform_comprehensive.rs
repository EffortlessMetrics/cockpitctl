//! Comprehensive conformance validation tests covering schema validation,
//! path hygiene, ordering verification, reason-lint, survivability,
//! combined check modes, and edge cases.

use cockpitctl_conform::{
    ConformChecks, check_artifact_pointers, check_cockpit_extended, check_cockpit_reason_tokens,
    check_determinism, check_ordering, check_path_hygiene, check_presence_semantics,
    check_reason_tokens, check_sensor_id_format, check_tool_error_identity, conform_single,
    is_valid_reason_token, validate_cockpit_schema,
};
use cockpitctl_types::*;
use std::collections::BTreeMap;

// ───────────────────────── helpers ─────────────────────────

fn minimal_sensor_report() -> SensorReport {
    SensorReport {
        schema: "sensor.report.v1".to_string(),
        tool: ToolInfo {
            name: "test-tool".to_string(),
            version: "1.0.0".to_string(),
            commit: None,
        },
        run: RunInfo {
            started_at: "2026-01-01T00:00:00Z".to_string(),
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

fn minimal_cockpit_report() -> CockpitReport {
    let cfg = CockpitConfig::default();
    let policy = PolicySnapshot {
        warn_is_fail: cfg.policy.warn_is_fail,
        max_highlights: cfg.policy.max_highlights,
        max_per_sensor_findings: cfg.policy.max_per_sensor_findings,
        max_annotations: cfg.policy.max_annotations,
        section_order: cfg.policy.section_order.clone(),
        sensors: vec![],
    };
    CockpitReport {
        schema: "cockpit.report.v1".to_string(),
        tool: ToolInfo {
            name: "cockpitctl".to_string(),
            version: "0.2.0".to_string(),
            commit: None,
        },
        run: RunInfo {
            started_at: "2026-01-01T00:00:00Z".to_string(),
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
        sensors: vec![],
        highlights: vec![],
        policy,
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

fn no_checks() -> ConformChecks {
    ConformChecks {
        path_hygiene: false,
        ordering: false,
        reason_lint: false,
        survivability: false,
        tool_error_identity: false,
        sensor_id_format: false,
        artifact_pointers: false,
    }
}

fn make_finding(severity: Severity, code: &str, path: Option<&str>, line: Option<u32>) -> Finding {
    Finding {
        severity,
        check_id: None,
        code: code.to_string(),
        message: format!("{} finding", code),
        location: path.map(|p| Location {
            path: Some(p.to_string()),
            line,
            col: None,
        }),
        help: None,
        url: None,
        fingerprint: None,
        data: None,
    }
}

fn to_json(report: &SensorReport) -> String {
    serde_json::to_string(report).expect("serialize sensor report")
}

fn cockpit_json(report: &CockpitReport) -> String {
    serde_json::to_string(report).expect("serialize cockpit report")
}

// ═══════════════════════════════════════════════════════════
//  Schema validation
// ═══════════════════════════════════════════════════════════

mod schema_validation {
    use super::*;

    #[test]
    fn valid_minimal_receipt_passes_schema() {
        let result = conform_single(&to_json(&minimal_sensor_report()), "sensor", &all_checks())
            .expect("infra ok");
        assert!(result.is_pass(), "violations: {:?}", result.violations);
    }

    #[test]
    fn empty_object_fails_schema() {
        let result = conform_single("{}", "sensor", &no_checks()).expect("infra ok");
        assert!(!result.is_pass());
        assert!(result.violations.iter().all(|v| v.check == "schema"));
    }

    #[test]
    fn missing_schema_field_fails() {
        let json = serde_json::json!({
            "tool": { "name": "t", "version": "1.0.0" },
            "run": { "started_at": "2026-01-01T00:00:00Z" },
            "verdict": { "status": "pass", "counts": { "info": 0, "warn": 0, "error": 0 } },
            "findings": []
        });
        let result = conform_single(&json.to_string(), "sensor", &no_checks()).expect("infra ok");
        assert!(!result.is_pass());
        assert!(result.violations.iter().any(|v| v.check == "schema"));
    }

    #[test]
    fn missing_tool_field_fails() {
        let json = serde_json::json!({
            "schema": "sensor.report.v1",
            "run": { "started_at": "2026-01-01T00:00:00Z" },
            "verdict": { "status": "pass", "counts": { "info": 0, "warn": 0, "error": 0 } },
            "findings": []
        });
        let result = conform_single(&json.to_string(), "sensor", &no_checks()).expect("infra ok");
        assert!(!result.is_pass());
        assert!(result.violations.iter().any(|v| v.check == "schema"));
    }

    #[test]
    fn missing_run_field_fails() {
        let json = serde_json::json!({
            "schema": "sensor.report.v1",
            "tool": { "name": "t", "version": "1.0.0" },
            "verdict": { "status": "pass", "counts": { "info": 0, "warn": 0, "error": 0 } },
            "findings": []
        });
        let result = conform_single(&json.to_string(), "sensor", &no_checks()).expect("infra ok");
        assert!(!result.is_pass());
        assert!(result.violations.iter().any(|v| v.check == "schema"));
    }

    #[test]
    fn missing_verdict_field_fails() {
        let json = serde_json::json!({
            "schema": "sensor.report.v1",
            "tool": { "name": "t", "version": "1.0.0" },
            "run": { "started_at": "2026-01-01T00:00:00Z" },
            "findings": []
        });
        let result = conform_single(&json.to_string(), "sensor", &no_checks()).expect("infra ok");
        assert!(!result.is_pass());
        assert!(result.violations.iter().any(|v| v.check == "schema"));
    }

    #[test]
    fn missing_findings_field_fails() {
        let json = serde_json::json!({
            "schema": "sensor.report.v1",
            "tool": { "name": "t", "version": "1.0.0" },
            "run": { "started_at": "2026-01-01T00:00:00Z" },
            "verdict": { "status": "pass", "counts": { "info": 0, "warn": 0, "error": 0 } }
        });
        let result = conform_single(&json.to_string(), "sensor", &no_checks()).expect("infra ok");
        assert!(!result.is_pass());
        assert!(result.violations.iter().any(|v| v.check == "schema"));
    }

    #[test]
    fn invalid_verdict_status_fails_schema() {
        let json = serde_json::json!({
            "schema": "sensor.report.v1",
            "tool": { "name": "t", "version": "1.0.0" },
            "run": { "started_at": "2026-01-01T00:00:00Z" },
            "verdict": { "status": "unknown", "counts": { "info": 0, "warn": 0, "error": 0 } },
            "findings": []
        });
        let result = conform_single(&json.to_string(), "sensor", &no_checks()).expect("infra ok");
        assert!(!result.is_pass());
    }

    #[test]
    fn invalid_json_returns_error() {
        let err = conform_single("{not-valid-json", "sensor", &no_checks());
        assert!(err.is_err());
    }

    #[test]
    fn cockpit_schema_valid_passes() {
        let json = cockpit_json(&minimal_cockpit_report());
        let violations = validate_cockpit_schema(&json).expect("infra ok");
        assert!(violations.is_empty());
    }

    #[test]
    fn cockpit_schema_empty_object_fails() {
        let violations = validate_cockpit_schema("{}").expect("infra ok");
        assert!(!violations.is_empty());
        assert!(violations.iter().all(|v| v.check == "schema"));
    }

    #[test]
    fn cockpit_schema_invalid_json_errors() {
        assert!(validate_cockpit_schema("{bad").is_err());
    }

    #[test]
    fn schema_violations_early_return_skips_extended_checks() {
        // When schema fails, extended checks are skipped (violations only contain schema errors)
        let result = conform_single("{}", "bad.sensor", &all_checks()).expect("infra ok");
        assert!(result.violations.iter().all(|v| v.check == "schema"));
    }

    #[test]
    fn multiple_schema_errors_reported() {
        let result = conform_single("{}", "sensor", &no_checks()).expect("infra ok");
        assert!(
            result.violations.len() > 1,
            "empty object should produce multiple schema violations, got {}",
            result.violations.len()
        );
    }
}

// ═══════════════════════════════════════════════════════════
//  Path hygiene
// ═══════════════════════════════════════════════════════════

mod path_hygiene {
    use super::*;

    #[test]
    fn clean_relative_paths_pass() {
        let mut report = minimal_sensor_report();
        report.findings = vec![
            make_finding(Severity::Info, "C1", Some("src/main.rs"), Some(10)),
            make_finding(Severity::Info, "C2", Some("tests/helpers/utils.rs"), None),
            make_finding(Severity::Info, "C3", Some("Cargo.toml"), Some(1)),
        ];
        let violations = check_path_hygiene(&report);
        assert!(violations.is_empty(), "got: {:?}", violations);
    }

    #[test]
    fn dotdot_traversal_detected() {
        let mut report = minimal_sensor_report();
        report.findings = vec![make_finding(
            Severity::Info,
            "T1",
            Some("../etc/passwd"),
            None,
        )];
        let violations = check_path_hygiene(&report);
        assert!(violations.iter().any(|v| v.contains("path traversal")));
    }

    #[test]
    fn mid_path_traversal_detected() {
        let mut report = minimal_sensor_report();
        report.findings = vec![make_finding(
            Severity::Info,
            "T2",
            Some("foo/../../bar"),
            None,
        )];
        let violations = check_path_hygiene(&report);
        assert!(violations.iter().any(|v| v.contains("path traversal")));
    }

    #[test]
    fn backslash_traversal_detected() {
        let mut report = minimal_sensor_report();
        report.findings = vec![make_finding(
            Severity::Info,
            "T3",
            Some("foo\\..\\bar"),
            None,
        )];
        let violations = check_path_hygiene(&report);
        assert!(
            violations.len() >= 2,
            "expect both traversal and backslash: {:?}",
            violations
        );
    }

    #[test]
    fn unix_absolute_path_detected() {
        let mut report = minimal_sensor_report();
        report.findings = vec![make_finding(
            Severity::Info,
            "A1",
            Some("/etc/passwd"),
            None,
        )];
        let violations = check_path_hygiene(&report);
        assert!(violations.iter().any(|v| v.contains("absolute path")));
    }

    #[test]
    fn windows_drive_letter_detected() {
        let mut report = minimal_sensor_report();
        report.findings = vec![make_finding(
            Severity::Info,
            "A2",
            Some("C:\\Windows\\System32"),
            None,
        )];
        let violations = check_path_hygiene(&report);
        assert!(violations.iter().any(|v| v.contains("drive letter")));
    }

    #[test]
    fn backslash_in_path_detected() {
        let mut report = minimal_sensor_report();
        report.findings = vec![make_finding(
            Severity::Info,
            "B1",
            Some("src\\main.rs"),
            None,
        )];
        let violations = check_path_hygiene(&report);
        assert!(violations.iter().any(|v| v.contains("backslash")));
    }

    #[test]
    fn finding_without_location_passes() {
        let mut report = minimal_sensor_report();
        report.findings = vec![Finding {
            severity: Severity::Info,
            check_id: None,
            code: "N1".to_string(),
            message: "no location".to_string(),
            location: None,
            help: None,
            url: None,
            fingerprint: None,
            data: None,
        }];
        let violations = check_path_hygiene(&report);
        assert!(violations.is_empty());
    }

    #[test]
    fn finding_with_location_no_path_passes() {
        let mut report = minimal_sensor_report();
        report.findings = vec![Finding {
            severity: Severity::Info,
            check_id: None,
            code: "N2".to_string(),
            message: "location without path".to_string(),
            location: Some(Location {
                path: None,
                line: Some(1),
                col: None,
            }),
            help: None,
            url: None,
            fingerprint: None,
            data: None,
        }];
        let violations = check_path_hygiene(&report);
        assert!(violations.is_empty());
    }

    #[test]
    fn multiple_bad_paths_all_reported() {
        let mut report = minimal_sensor_report();
        report.findings = vec![
            make_finding(Severity::Info, "P1", Some("/abs/one"), None),
            make_finding(Severity::Info, "P2", Some("../traversal"), None),
            make_finding(Severity::Info, "P3", Some("back\\slash"), None),
        ];
        let violations = check_path_hygiene(&report);
        assert!(
            violations.len() >= 3,
            "each bad path should produce at least one violation: {:?}",
            violations
        );
    }

    #[test]
    fn path_hygiene_via_conform_single() {
        let mut report = minimal_sensor_report();
        report.findings = vec![make_finding(Severity::Info, "T1", Some("../escape"), None)];
        let checks = ConformChecks {
            path_hygiene: true,
            ..no_checks()
        };
        let result = conform_single(&to_json(&report), "sensor", &checks).expect("infra ok");
        assert!(!result.is_pass());
        assert!(result.violations.iter().any(|v| v.check == "path_hygiene"));
    }
}

// ═══════════════════════════════════════════════════════════
//  Ordering
// ═══════════════════════════════════════════════════════════

mod ordering {
    use super::*;

    #[test]
    fn correctly_ordered_findings_pass() {
        let mut report = minimal_sensor_report();
        // Error (rank 0) before Warn (rank 1) before Info (rank 2) — correct order
        report.findings = vec![
            make_finding(Severity::Error, "E1", Some("a.rs"), Some(1)),
            make_finding(Severity::Warn, "W1", Some("a.rs"), Some(2)),
            make_finding(Severity::Info, "I1", Some("a.rs"), Some(3)),
        ];
        let violations = check_ordering(&report, "sensor");
        assert!(violations.is_empty(), "got: {:?}", violations);
    }

    #[test]
    fn out_of_order_severity_detected() {
        let mut report = minimal_sensor_report();
        // Info before Error — wrong order
        report.findings = vec![
            make_finding(Severity::Info, "I1", None, None),
            make_finding(Severity::Error, "E1", None, None),
        ];
        let violations = check_ordering(&report, "sensor");
        assert!(!violations.is_empty());
    }

    #[test]
    fn same_severity_ordered_by_path() {
        let mut report = minimal_sensor_report();
        report.findings = vec![
            make_finding(Severity::Warn, "W1", Some("a.rs"), Some(1)),
            make_finding(Severity::Warn, "W2", Some("b.rs"), Some(1)),
        ];
        let violations = check_ordering(&report, "sensor");
        assert!(violations.is_empty(), "got: {:?}", violations);
    }

    #[test]
    fn same_severity_wrong_path_order_detected() {
        let mut report = minimal_sensor_report();
        report.findings = vec![
            make_finding(Severity::Warn, "W1", Some("z.rs"), Some(1)),
            make_finding(Severity::Warn, "W2", Some("a.rs"), Some(1)),
        ];
        let violations = check_ordering(&report, "sensor");
        assert!(!violations.is_empty());
    }

    #[test]
    fn same_severity_same_path_ordered_by_line() {
        let mut report = minimal_sensor_report();
        report.findings = vec![
            make_finding(Severity::Error, "E1", Some("a.rs"), Some(10)),
            make_finding(Severity::Error, "E2", Some("a.rs"), Some(20)),
        ];
        let violations = check_ordering(&report, "sensor");
        assert!(violations.is_empty(), "got: {:?}", violations);
    }

    #[test]
    fn same_severity_same_path_wrong_line_order_detected() {
        let mut report = minimal_sensor_report();
        report.findings = vec![
            make_finding(Severity::Error, "E1", Some("a.rs"), Some(20)),
            make_finding(Severity::Error, "E2", Some("a.rs"), Some(10)),
        ];
        let violations = check_ordering(&report, "sensor");
        assert!(!violations.is_empty());
    }

    #[test]
    fn same_severity_same_path_same_line_ordered_by_code() {
        let mut report = minimal_sensor_report();
        report.findings = vec![
            make_finding(Severity::Warn, "A_CODE", Some("x.rs"), Some(5)),
            make_finding(Severity::Warn, "B_CODE", Some("x.rs"), Some(5)),
        ];
        let violations = check_ordering(&report, "sensor");
        assert!(violations.is_empty(), "got: {:?}", violations);
    }

    #[test]
    fn empty_findings_pass_ordering() {
        let report = minimal_sensor_report();
        let violations = check_ordering(&report, "sensor");
        assert!(violations.is_empty());
    }

    #[test]
    fn single_finding_passes_ordering() {
        let mut report = minimal_sensor_report();
        report.findings = vec![make_finding(Severity::Warn, "W1", Some("a.rs"), Some(1))];
        let violations = check_ordering(&report, "sensor");
        assert!(violations.is_empty());
    }

    #[test]
    fn ordering_via_conform_single() {
        let mut report = minimal_sensor_report();
        report.findings = vec![
            make_finding(Severity::Info, "I1", None, None),
            make_finding(Severity::Error, "E1", None, None),
        ];
        let checks = ConformChecks {
            ordering: true,
            ..no_checks()
        };
        let result = conform_single(&to_json(&report), "sensor", &checks).expect("infra ok");
        assert!(!result.is_pass());
        assert!(result.violations.iter().any(|v| v.check == "ordering"));
    }

    #[test]
    fn findings_with_no_location_use_defaults() {
        let mut report = minimal_sensor_report();
        report.findings = vec![
            make_finding(Severity::Error, "A_CODE", None, None),
            make_finding(Severity::Error, "B_CODE", None, None),
        ];
        let violations = check_ordering(&report, "sensor");
        assert!(violations.is_empty(), "got: {:?}", violations);
    }
}

// ═══════════════════════════════════════════════════════════
//  Reason-lint
// ═══════════════════════════════════════════════════════════

mod reason_lint {
    use super::*;

    #[test]
    fn valid_reason_tokens_accepted() {
        assert!(is_valid_reason_token("tool_error"));
        assert!(is_valid_reason_token("missing_receipt"));
        assert!(is_valid_reason_token("a"));
        assert!(is_valid_reason_token("abc123"));
        assert!(is_valid_reason_token("a_b_c"));
    }

    #[test]
    fn invalid_reason_tokens_rejected() {
        assert!(!is_valid_reason_token(""));
        assert!(!is_valid_reason_token("UPPER"));
        assert!(!is_valid_reason_token("has-dash"));
        assert!(!is_valid_reason_token("has space"));
        assert!(!is_valid_reason_token("has.dot"));
        assert!(!is_valid_reason_token("café"));
        assert!(!is_valid_reason_token("with\nnewline"));
    }

    #[test]
    fn sensor_report_bad_verdict_reasons_detected() {
        let mut report = minimal_sensor_report();
        report.verdict.reasons = vec!["Bad-Token".to_string(), "ok_token".to_string()];
        let violations = check_reason_tokens(&report);
        assert_eq!(violations.len(), 1);
        assert!(violations[0].contains("Bad-Token"));
    }

    #[test]
    fn sensor_report_bad_capability_reason_detected() {
        let mut report = minimal_sensor_report();
        report.run.capabilities.insert(
            "git".to_string(),
            Capability {
                status: CapabilityStatus::Available,
                reason: Some("Not-Valid".to_string()),
            },
        );
        let violations = check_reason_tokens(&report);
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn sensor_report_valid_reasons_pass() {
        let mut report = minimal_sensor_report();
        report.verdict.reasons = vec!["tool_error".to_string(), "partial_result".to_string()];
        report.run.capabilities.insert(
            "git".to_string(),
            Capability {
                status: CapabilityStatus::Available,
                reason: Some("ok_reason".to_string()),
            },
        );
        let violations = check_reason_tokens(&report);
        assert!(violations.is_empty());
    }

    #[test]
    fn cockpit_report_bad_verdict_reason_detected() {
        let mut report = minimal_cockpit_report();
        report.verdict.reasons = vec!["Bad-Token".to_string()];
        let violations = check_cockpit_reason_tokens(&report);
        assert!(!violations.is_empty());
    }

    #[test]
    fn cockpit_report_bad_sensor_verdict_reason_detected() {
        let mut report = minimal_cockpit_report();
        report.sensors.push(SensorSummary {
            id: "sensor".to_string(),
            blocking: true,
            missing: MissingPolicy::Fail,
            presence: Presence::Present,
            report_path: "artifacts/sensor/report.json".to_string(),
            comment_path: None,
            verdict: Verdict {
                status: VerdictStatus::Pass,
                counts: VerdictCounts::default(),
                reasons: vec!["Bad-Token".to_string()],
            },
            truncated: false,
            errors: vec![],
            missing_policy_applied: None,
            policy_outcome: None,
        });
        let violations = check_cockpit_reason_tokens(&report);
        assert!(!violations.is_empty());
    }

    #[test]
    fn cockpit_report_bad_capability_reason_detected() {
        let mut report = minimal_cockpit_report();
        report.run.capabilities.insert(
            "ci".to_string(),
            Capability {
                status: CapabilityStatus::Unavailable,
                reason: Some("Not-Valid".to_string()),
            },
        );
        let violations = check_cockpit_reason_tokens(&report);
        assert!(!violations.is_empty());
    }

    #[test]
    fn cockpit_report_valid_reasons_pass() {
        let mut report = minimal_cockpit_report();
        report.verdict.reasons = vec!["policy_fail".to_string()];
        report.sensors.push(SensorSummary {
            id: "sensor".to_string(),
            blocking: true,
            missing: MissingPolicy::Fail,
            presence: Presence::Present,
            report_path: "artifacts/sensor/report.json".to_string(),
            comment_path: None,
            verdict: Verdict {
                status: VerdictStatus::Pass,
                counts: VerdictCounts::default(),
                reasons: vec!["all_clear".to_string()],
            },
            truncated: false,
            errors: vec![],
            missing_policy_applied: None,
            policy_outcome: None,
        });
        let violations = check_cockpit_reason_tokens(&report);
        assert!(violations.is_empty());
    }

    #[test]
    fn reason_lint_via_conform_single() {
        let mut report = minimal_sensor_report();
        report.verdict.reasons = vec!["Bad-Token".to_string()];
        let checks = ConformChecks {
            reason_lint: true,
            ..no_checks()
        };
        let result = conform_single(&to_json(&report), "sensor", &checks).expect("infra ok");
        assert!(!result.is_pass());
        assert!(result.violations.iter().any(|v| v.check == "reason_lint"));
    }

    #[test]
    fn cockpit_extended_reason_lint_toggle() {
        let mut report = minimal_cockpit_report();
        report.verdict.reasons = vec!["Bad-Token".to_string()];
        let json = cockpit_json(&report);

        let with = check_cockpit_extended(&json, true, false).expect("infra ok");
        assert!(!with.is_empty());

        let without = check_cockpit_extended(&json, false, false).expect("infra ok");
        assert!(without.is_empty());
    }
}

// ═══════════════════════════════════════════════════════════
//  Survivability
// ═══════════════════════════════════════════════════════════

mod survivability {
    use super::*;

    #[test]
    fn fail_verdict_no_findings_no_reasons_fails_survivability() {
        let mut report = minimal_sensor_report();
        report.verdict.status = VerdictStatus::Fail;
        let checks = ConformChecks {
            survivability: true,
            ..no_checks()
        };
        let result = conform_single(&to_json(&report), "sensor", &checks).expect("infra ok");
        assert!(!result.is_pass());
        assert!(result.violations.iter().any(|v| v.check == "survivability"));
    }

    #[test]
    fn fail_verdict_with_findings_passes_survivability() {
        let mut report = minimal_sensor_report();
        report.verdict.status = VerdictStatus::Fail;
        report
            .findings
            .push(make_finding(Severity::Error, "E1", None, None));
        let checks = ConformChecks {
            survivability: true,
            ..no_checks()
        };
        let result = conform_single(&to_json(&report), "sensor", &checks).expect("infra ok");
        assert!(
            !result.violations.iter().any(|v| v.check == "survivability"),
            "fail with findings should pass survivability"
        );
    }

    #[test]
    fn fail_verdict_with_reasons_passes_survivability() {
        let mut report = minimal_sensor_report();
        report.verdict.status = VerdictStatus::Fail;
        report.verdict.reasons = vec!["tool_error".to_string()];
        report.findings.push(Finding {
            severity: Severity::Error,
            check_id: Some("tool.runtime".to_string()),
            code: "runtime_error".to_string(),
            message: "boom".to_string(),
            location: None,
            help: None,
            url: None,
            fingerprint: None,
            data: None,
        });
        let checks = ConformChecks {
            survivability: true,
            ..no_checks()
        };
        let result = conform_single(&to_json(&report), "sensor", &checks).expect("infra ok");
        assert!(
            !result.violations.iter().any(|v| v.check == "survivability"),
            "fail with reasons should pass survivability"
        );
    }

    #[test]
    fn pass_verdict_skips_survivability_check() {
        let report = minimal_sensor_report(); // status = Pass, no findings
        let checks = ConformChecks {
            survivability: true,
            ..no_checks()
        };
        let result = conform_single(&to_json(&report), "sensor", &checks).expect("infra ok");
        assert!(result.is_pass());
    }

    #[test]
    fn warn_verdict_skips_survivability_check() {
        let mut report = minimal_sensor_report();
        report.verdict.status = VerdictStatus::Warn;
        let checks = ConformChecks {
            survivability: true,
            ..no_checks()
        };
        let result = conform_single(&to_json(&report), "sensor", &checks).expect("infra ok");
        assert!(
            !result.violations.iter().any(|v| v.check == "survivability"),
            "warn status should not trigger survivability"
        );
    }

    #[test]
    fn skip_verdict_skips_survivability_check() {
        let mut report = minimal_sensor_report();
        report.verdict.status = VerdictStatus::Skip;
        let checks = ConformChecks {
            survivability: true,
            ..no_checks()
        };
        let result = conform_single(&to_json(&report), "sensor", &checks).expect("infra ok");
        assert!(
            !result.violations.iter().any(|v| v.check == "survivability"),
            "skip status should not trigger survivability"
        );
    }
}

// ═══════════════════════════════════════════════════════════
//  Tool error identity
// ═══════════════════════════════════════════════════════════

mod tool_error_identity {
    use super::*;

    #[test]
    fn no_tool_error_reason_passes() {
        let report = minimal_sensor_report();
        let violations = check_tool_error_identity(&report);
        assert!(violations.is_empty());
    }

    #[test]
    fn tool_error_without_canonical_finding_fails() {
        let mut report = minimal_sensor_report();
        report.verdict.reasons = vec!["tool_error".to_string()];
        let violations = check_tool_error_identity(&report);
        assert!(!violations.is_empty());
    }

    #[test]
    fn tool_error_with_canonical_finding_passes() {
        let mut report = minimal_sensor_report();
        report.verdict.reasons = vec!["tool_error".to_string()];
        report.findings.push(Finding {
            severity: Severity::Error,
            check_id: Some("tool.runtime".to_string()),
            code: "runtime_error".to_string(),
            message: "process crashed".to_string(),
            location: None,
            help: None,
            url: None,
            fingerprint: None,
            data: None,
        });
        let violations = check_tool_error_identity(&report);
        assert!(violations.is_empty());
    }

    #[test]
    fn tool_error_with_wrong_check_id_fails() {
        let mut report = minimal_sensor_report();
        report.verdict.reasons = vec!["tool_error".to_string()];
        report.findings.push(Finding {
            severity: Severity::Error,
            check_id: Some("wrong.check".to_string()),
            code: "runtime_error".to_string(),
            message: "process crashed".to_string(),
            location: None,
            help: None,
            url: None,
            fingerprint: None,
            data: None,
        });
        let violations = check_tool_error_identity(&report);
        assert!(!violations.is_empty());
    }

    #[test]
    fn tool_error_with_wrong_code_fails() {
        let mut report = minimal_sensor_report();
        report.verdict.reasons = vec!["tool_error".to_string()];
        report.findings.push(Finding {
            severity: Severity::Error,
            check_id: Some("tool.runtime".to_string()),
            code: "wrong_code".to_string(),
            message: "process crashed".to_string(),
            location: None,
            help: None,
            url: None,
            fingerprint: None,
            data: None,
        });
        let violations = check_tool_error_identity(&report);
        assert!(!violations.is_empty());
    }

    #[test]
    fn tool_error_identity_via_conform_single() {
        let mut report = minimal_sensor_report();
        report.verdict.reasons = vec!["tool_error".to_string()];
        let checks = ConformChecks {
            tool_error_identity: true,
            ..no_checks()
        };
        let result = conform_single(&to_json(&report), "sensor", &checks).expect("infra ok");
        assert!(!result.is_pass());
        assert!(
            result
                .violations
                .iter()
                .any(|v| v.check == "tool_error_identity")
        );
    }
}

// ═══════════════════════════════════════════════════════════
//  Sensor ID format
// ═══════════════════════════════════════════════════════════

mod sensor_id_format {
    use super::*;

    #[test]
    fn valid_sensor_ids() {
        assert!(check_sensor_id_format("builddiag").is_empty());
        assert!(check_sensor_id_format("my-sensor").is_empty());
        assert!(check_sensor_id_format("my_sensor_v2").is_empty());
        assert!(check_sensor_id_format("ABC").is_empty());
        assert!(check_sensor_id_format("a").is_empty());
        assert!(check_sensor_id_format("A-Z_0").is_empty());
    }

    #[test]
    fn invalid_sensor_ids() {
        assert!(!check_sensor_id_format("").is_empty());
        assert!(!check_sensor_id_format("has.dot").is_empty());
        assert!(!check_sensor_id_format("has space").is_empty());
        assert!(!check_sensor_id_format("../traversal").is_empty());
        assert!(!check_sensor_id_format("café").is_empty());
        assert!(!check_sensor_id_format("path/slash").is_empty());
        assert!(!check_sensor_id_format("back\\slash").is_empty());
    }

    #[test]
    fn sensor_id_format_via_conform_single() {
        let checks = ConformChecks {
            sensor_id_format: true,
            ..no_checks()
        };
        let json = to_json(&minimal_sensor_report());

        let good = conform_single(&json, "good-id", &checks).expect("infra ok");
        assert!(good.is_pass());

        let bad = conform_single(&json, "bad.id", &checks).expect("infra ok");
        assert!(!bad.is_pass());
        assert!(bad.violations.iter().any(|v| v.check == "sensor_id_format"));
    }
}

// ═══════════════════════════════════════════════════════════
//  Artifact pointers
// ═══════════════════════════════════════════════════════════

mod artifact_pointers {
    use super::*;

    #[test]
    fn valid_artifact_passes() {
        let mut report = minimal_sensor_report();
        report.artifacts = vec![ArtifactPointer {
            id: "log".to_string(),
            path: "artifacts/log.txt".to_string(),
            mime: "text/plain".to_string(),
            schema: None,
        }];
        let violations = check_artifact_pointers(&report);
        assert!(violations.is_empty());
    }

    #[test]
    fn empty_id_detected() {
        let mut report = minimal_sensor_report();
        report.artifacts = vec![ArtifactPointer {
            id: "".to_string(),
            path: "ok.txt".to_string(),
            mime: "text/plain".to_string(),
            schema: None,
        }];
        let violations = check_artifact_pointers(&report);
        assert!(violations.iter().any(|v| v.contains("id is empty")));
    }

    #[test]
    fn empty_path_detected() {
        let mut report = minimal_sensor_report();
        report.artifacts = vec![ArtifactPointer {
            id: "ok".to_string(),
            path: "".to_string(),
            mime: "text/plain".to_string(),
            schema: None,
        }];
        let violations = check_artifact_pointers(&report);
        assert!(violations.iter().any(|v| v.contains("path is empty")));
    }

    #[test]
    fn empty_mime_detected() {
        let mut report = minimal_sensor_report();
        report.artifacts = vec![ArtifactPointer {
            id: "ok".to_string(),
            path: "ok.txt".to_string(),
            mime: "".to_string(),
            schema: None,
        }];
        let violations = check_artifact_pointers(&report);
        assert!(violations.iter().any(|v| v.contains("mime is empty")));
    }

    #[test]
    fn traversal_in_artifact_path_detected() {
        let mut report = minimal_sensor_report();
        report.artifacts = vec![ArtifactPointer {
            id: "bad".to_string(),
            path: "../escape/file.txt".to_string(),
            mime: "text/plain".to_string(),
            schema: None,
        }];
        let violations = check_artifact_pointers(&report);
        assert!(violations.iter().any(|v| v.contains("..")));
    }

    #[test]
    fn absolute_artifact_path_detected() {
        let mut report = minimal_sensor_report();
        report.artifacts = vec![
            ArtifactPointer {
                id: "unix-abs".to_string(),
                path: "/abs/path.txt".to_string(),
                mime: "text/plain".to_string(),
                schema: None,
            },
            ArtifactPointer {
                id: "win-abs".to_string(),
                path: "C:\\abs\\path.txt".to_string(),
                mime: "text/plain".to_string(),
                schema: None,
            },
        ];
        let violations = check_artifact_pointers(&report);
        assert!(violations.len() >= 2, "got: {:?}", violations);
    }

    #[test]
    fn artifact_pointers_via_conform_single() {
        let mut report = minimal_sensor_report();
        // Use schema-valid artifact (non-empty fields) but with path traversal
        report.artifacts = vec![ArtifactPointer {
            id: "bad-artifact".to_string(),
            path: "../bad".to_string(),
            mime: "text/plain".to_string(),
            schema: None,
        }];
        let checks = ConformChecks {
            artifact_pointers: true,
            ..no_checks()
        };
        let result = conform_single(&to_json(&report), "sensor", &checks).expect("infra ok");
        assert!(!result.is_pass());
        assert!(
            result
                .violations
                .iter()
                .any(|v| v.check == "artifact_pointers")
        );
    }
}

// ═══════════════════════════════════════════════════════════
//  Presence semantics (cockpit report)
// ═══════════════════════════════════════════════════════════

mod presence_semantics {
    use super::*;

    #[test]
    fn missing_policy_applied_with_missing_presence_passes() {
        let mut report = minimal_cockpit_report();
        report.sensors.push(SensorSummary {
            id: "sensor".to_string(),
            blocking: true,
            missing: MissingPolicy::Fail,
            presence: Presence::Missing,
            report_path: "artifacts/sensor/report.json".to_string(),
            comment_path: None,
            verdict: Verdict {
                status: VerdictStatus::Fail,
                counts: VerdictCounts::default(),
                reasons: vec![],
            },
            truncated: false,
            errors: vec![],
            missing_policy_applied: Some(MissingPolicy::Fail),
            policy_outcome: None,
        });
        let violations = check_presence_semantics(&report);
        assert!(violations.is_empty());
    }

    #[test]
    fn missing_policy_applied_with_present_presence_fails() {
        let mut report = minimal_cockpit_report();
        report.sensors.push(SensorSummary {
            id: "sensor".to_string(),
            blocking: true,
            missing: MissingPolicy::Fail,
            presence: Presence::Present,
            report_path: "artifacts/sensor/report.json".to_string(),
            comment_path: None,
            verdict: Verdict {
                status: VerdictStatus::Pass,
                counts: VerdictCounts::default(),
                reasons: vec![],
            },
            truncated: false,
            errors: vec![],
            missing_policy_applied: Some(MissingPolicy::Skip),
            policy_outcome: None,
        });
        let violations = check_presence_semantics(&report);
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn no_missing_policy_applied_passes_regardless() {
        let mut report = minimal_cockpit_report();
        report.sensors.push(SensorSummary {
            id: "sensor".to_string(),
            blocking: true,
            missing: MissingPolicy::Fail,
            presence: Presence::Present,
            report_path: "artifacts/sensor/report.json".to_string(),
            comment_path: None,
            verdict: Verdict {
                status: VerdictStatus::Pass,
                counts: VerdictCounts::default(),
                reasons: vec![],
            },
            truncated: false,
            errors: vec![],
            missing_policy_applied: None,
            policy_outcome: None,
        });
        let violations = check_presence_semantics(&report);
        assert!(violations.is_empty());
    }

    #[test]
    fn presence_semantics_via_cockpit_extended() {
        let mut report = minimal_cockpit_report();
        report.sensors.push(SensorSummary {
            id: "sensor".to_string(),
            blocking: true,
            missing: MissingPolicy::Fail,
            presence: Presence::Present,
            report_path: "artifacts/sensor/report.json".to_string(),
            comment_path: None,
            verdict: Verdict {
                status: VerdictStatus::Pass,
                counts: VerdictCounts::default(),
                reasons: vec![],
            },
            truncated: false,
            errors: vec![],
            missing_policy_applied: Some(MissingPolicy::Skip),
            policy_outcome: None,
        });
        let json = cockpit_json(&report);

        let with = check_cockpit_extended(&json, false, true).expect("infra ok");
        assert!(with.iter().any(|v| v.check == "presence_semantics"));

        let without = check_cockpit_extended(&json, false, false).expect("infra ok");
        assert!(without.is_empty());
    }
}

// ═══════════════════════════════════════════════════════════
//  Determinism
// ═══════════════════════════════════════════════════════════

mod determinism {
    use super::*;

    #[test]
    fn identical_strings_pass() {
        assert!(check_determinism("hello", "hello").is_none());
    }

    #[test]
    fn different_strings_fail() {
        assert!(check_determinism("a", "b").is_some());
    }

    #[test]
    fn empty_strings_pass() {
        assert!(check_determinism("", "").is_none());
    }

    #[test]
    fn whitespace_difference_detected() {
        assert!(check_determinism("a b", "a  b").is_some());
    }
}

// ═══════════════════════════════════════════════════════════
//  Combined check modes (--all flag equivalent)
// ═══════════════════════════════════════════════════════════

mod combined_checks {
    use super::*;

    #[test]
    fn all_checks_on_clean_report_passes() {
        let result = conform_single(
            &to_json(&minimal_sensor_report()),
            "good-sensor",
            &all_checks(),
        )
        .expect("infra ok");
        assert!(result.is_pass(), "violations: {:?}", result.violations);
    }

    #[test]
    fn all_checks_surface_multiple_violation_types() {
        let mut report = minimal_sensor_report();
        report.verdict.status = VerdictStatus::Fail;
        report.verdict.reasons = vec!["Bad-Token".to_string()]; // bad token but provides a reason
        report.findings = vec![make_finding(Severity::Info, "T1", Some("../escape"), None)];
        // Use schema-valid artifacts (non-empty fields) but with path issues
        report.artifacts = vec![ArtifactPointer {
            id: "bad-artifact".to_string(),
            path: "/abs".to_string(),
            mime: "text/plain".to_string(),
            schema: None,
        }];

        let result =
            conform_single(&to_json(&report), "bad.sensor", &all_checks()).expect("infra ok");
        assert!(!result.is_pass());

        let check_names: Vec<&str> = result.violations.iter().map(|v| v.check.as_str()).collect();
        assert!(
            check_names.contains(&"path_hygiene"),
            "missing path_hygiene in {:?}",
            check_names
        );
        assert!(
            check_names.contains(&"reason_lint"),
            "missing reason_lint in {:?}",
            check_names
        );
        assert!(
            check_names.contains(&"sensor_id_format"),
            "missing sensor_id_format in {:?}",
            check_names
        );
        assert!(
            check_names.contains(&"artifact_pointers"),
            "missing artifact_pointers in {:?}",
            check_names
        );
    }

    #[test]
    fn no_checks_enabled_still_validates_schema() {
        let result = conform_single("{}", "sensor", &no_checks()).expect("infra ok");
        assert!(!result.is_pass());
        assert!(result.violations.iter().any(|v| v.check == "schema"));
    }

    #[test]
    fn selective_checks_only_run_enabled() {
        let mut report = minimal_sensor_report();
        report.findings = vec![make_finding(Severity::Info, "T1", Some("../escape"), None)];
        report.verdict.reasons = vec!["Bad-Token".to_string()];

        // Only path_hygiene enabled
        let checks = ConformChecks {
            path_hygiene: true,
            ..no_checks()
        };
        let result = conform_single(&to_json(&report), "sensor", &checks).expect("infra ok");
        assert!(result.violations.iter().all(|v| v.check == "path_hygiene"));

        // Only reason_lint enabled
        let checks = ConformChecks {
            reason_lint: true,
            ..no_checks()
        };
        let result = conform_single(&to_json(&report), "sensor", &checks).expect("infra ok");
        assert!(result.violations.iter().all(|v| v.check == "reason_lint"));
    }
}

// ═══════════════════════════════════════════════════════════
//  Edge cases
// ═══════════════════════════════════════════════════════════

mod edge_cases {
    use super::*;

    #[test]
    fn empty_findings_array_passes_all_checks() {
        let report = minimal_sensor_report();
        let result = conform_single(&to_json(&report), "sensor", &all_checks()).expect("infra ok");
        assert!(result.is_pass(), "violations: {:?}", result.violations);
    }

    #[test]
    fn empty_highlights_cockpit_passes() {
        let report = minimal_cockpit_report();
        let json = cockpit_json(&report);
        let violations = validate_cockpit_schema(&json).expect("infra ok");
        assert!(violations.is_empty());
    }

    #[test]
    fn minimal_valid_report_round_trips() {
        let report = minimal_sensor_report();
        let json = to_json(&report);
        let parsed: SensorReport = serde_json::from_str(&json).expect("parse");
        assert_eq!(parsed.schema, "sensor.report.v1");
        assert_eq!(parsed.findings.len(), 0);
    }

    #[test]
    fn report_with_all_verdict_statuses() {
        for status in [
            VerdictStatus::Pass,
            VerdictStatus::Warn,
            VerdictStatus::Fail,
            VerdictStatus::Skip,
        ] {
            let mut report = minimal_sensor_report();
            report.verdict.status = status.clone();
            if status == VerdictStatus::Fail {
                report.verdict.reasons = vec!["test_reason".to_string()];
            }
            let checks = ConformChecks {
                survivability: true,
                ..no_checks()
            };
            let result = conform_single(&to_json(&report), "sensor", &checks).expect("infra ok");
            assert!(
                result.is_pass(),
                "status {:?} should pass with adequate reasons: {:?}",
                status,
                result.violations
            );
        }
    }

    #[test]
    fn report_with_optional_fields_populated() {
        let mut report = minimal_sensor_report();
        report.tool.commit = Some("abc123".to_string());
        report.run.ended_at = Some("2026-01-01T00:01:00Z".to_string());
        report.run.duration_ms = Some(60000);
        report.findings = vec![Finding {
            severity: Severity::Info,
            check_id: Some("check.id".to_string()),
            code: "C1".to_string(),
            message: "message".to_string(),
            location: Some(Location {
                path: Some("src/lib.rs".to_string()),
                line: Some(42),
                col: Some(10),
            }),
            help: Some("fix this".to_string()),
            url: Some("https://example.com".to_string()),
            fingerprint: Some("abc123".to_string()),
            data: Some(serde_json::json!({"key": "value"})),
        }];
        report.data = Some(serde_json::json!({"extra": true}));
        let result = conform_single(&to_json(&report), "sensor", &all_checks()).expect("infra ok");
        assert!(result.is_pass(), "violations: {:?}", result.violations);
    }

    #[test]
    fn conform_result_is_pass_reflects_violations() {
        let result = conform_single(&to_json(&minimal_sensor_report()), "sensor", &no_checks())
            .expect("infra ok");
        assert!(result.is_pass());
        assert!(result.violations.is_empty());
    }

    #[test]
    fn empty_artifacts_passes() {
        let report = minimal_sensor_report();
        let violations = check_artifact_pointers(&report);
        assert!(violations.is_empty());
    }

    #[test]
    fn no_sensors_cockpit_passes_presence() {
        let report = minimal_cockpit_report();
        let violations = check_presence_semantics(&report);
        assert!(violations.is_empty());
    }

    #[test]
    fn large_findings_array_ordering_check() {
        let mut report = minimal_sensor_report();
        for i in 0..100 {
            let severity = if i < 30 {
                Severity::Error
            } else if i < 60 {
                Severity::Warn
            } else {
                Severity::Info
            };
            report
                .findings
                .push(make_finding(severity, &format!("C{:04}", i), None, None));
        }
        let violations = check_ordering(&report, "sensor");
        assert!(violations.is_empty(), "got: {:?}", violations);
    }

    #[test]
    fn cockpit_extended_invalid_json_errors() {
        assert!(check_cockpit_extended("{bad", true, true).is_err());
    }

    #[test]
    fn cockpit_schema_with_extra_fields_passes() {
        let mut report = minimal_cockpit_report();
        report.data = Some(serde_json::json!({"custom": "field", "nested": {"key": 42}}));
        let json = cockpit_json(&report);
        let violations = validate_cockpit_schema(&json).expect("infra ok");
        assert!(violations.is_empty());
    }
}
