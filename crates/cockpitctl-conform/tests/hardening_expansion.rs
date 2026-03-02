//! Hardening tests for cockpitctl-conform: edge cases in schema validation,
//! path hygiene, ordering, reason tokens, cross-field consistency, and
//! conformance report formatting.

use cockpitctl_conform::{
    ConformChecks, check_artifact_pointers, check_cockpit_reason_tokens, check_ordering,
    check_path_hygiene, check_presence_semantics, check_reason_tokens, check_sensor_id_format,
    check_tool_error_identity, conform_single, is_valid_reason_token, validate_cockpit_schema,
};
use cockpitctl_types::*;
use std::collections::BTreeMap;

// ── helpers ──────────────────────────────────────────────────────────────

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

fn make_finding(severity: Severity, code: &str, message: &str) -> Finding {
    Finding {
        severity,
        check_id: None,
        code: code.to_string(),
        message: message.to_string(),
        location: None,
        help: None,
        url: None,
        fingerprint: None,
        data: None,
    }
}

fn make_finding_at(
    severity: Severity,
    code: &str,
    message: &str,
    path: &str,
    line: u32,
) -> Finding {
    Finding {
        severity,
        check_id: None,
        code: code.to_string(),
        message: message.to_string(),
        location: Some(Location {
            path: Some(path.to_string()),
            line: Some(line),
            col: None,
        }),
        help: None,
        url: None,
        fingerprint: None,
        data: None,
    }
}

fn to_json(report: &SensorReport) -> String {
    serde_json::to_string(report).expect("serialize")
}

// ═══════════════════════════════════════════════════════════════════════════
// 1. Schema validation edge cases
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn schema_missing_findings_field() {
    let json = serde_json::json!({
        "schema": "sensor.report.v1",
        "tool": { "name": "t", "version": "1.0.0" },
        "run": { "started_at": "2026-01-01T00:00:00Z" },
        "verdict": { "status": "pass", "counts": { "info": 0, "warn": 0, "error": 0 } }
    });
    // findings is required by the JSON Schema
    let result = conform_single(&json.to_string(), "sensor", &all_checks()).unwrap();
    assert!(!result.is_pass());
    assert!(result.violations.iter().any(|v| v.check == "schema"));
}

#[test]
fn schema_missing_tool_name() {
    let json = serde_json::json!({
        "schema": "sensor.report.v1",
        "tool": { "version": "1.0.0" },
        "run": { "started_at": "2026-01-01T00:00:00Z" },
        "verdict": { "status": "pass", "counts": { "info": 0, "warn": 0, "error": 0 } },
        "findings": []
    });
    let result = conform_single(&json.to_string(), "sensor", &all_checks()).unwrap();
    assert!(!result.is_pass());
    assert!(result.violations.iter().any(|v| v.check == "schema"));
}

#[test]
fn schema_missing_tool_version() {
    let json = serde_json::json!({
        "schema": "sensor.report.v1",
        "tool": { "name": "t" },
        "run": { "started_at": "2026-01-01T00:00:00Z" },
        "verdict": { "status": "pass", "counts": { "info": 0, "warn": 0, "error": 0 } },
        "findings": []
    });
    let result = conform_single(&json.to_string(), "sensor", &all_checks()).unwrap();
    assert!(!result.is_pass());
    assert!(result.violations.iter().any(|v| v.check == "schema"));
}

#[test]
fn schema_missing_run_started_at() {
    let json = serde_json::json!({
        "schema": "sensor.report.v1",
        "tool": { "name": "t", "version": "1.0.0" },
        "run": {},
        "verdict": { "status": "pass", "counts": { "info": 0, "warn": 0, "error": 0 } },
        "findings": []
    });
    let result = conform_single(&json.to_string(), "sensor", &all_checks()).unwrap();
    assert!(!result.is_pass());
    assert!(result.violations.iter().any(|v| v.check == "schema"));
}

#[test]
fn schema_missing_verdict_counts() {
    let json = serde_json::json!({
        "schema": "sensor.report.v1",
        "tool": { "name": "t", "version": "1.0.0" },
        "run": { "started_at": "2026-01-01T00:00:00Z" },
        "verdict": { "status": "pass" },
        "findings": []
    });
    let result = conform_single(&json.to_string(), "sensor", &all_checks()).unwrap();
    assert!(!result.is_pass());
    assert!(result.violations.iter().any(|v| v.check == "schema"));
}

#[test]
fn schema_wrong_type_for_verdict_status() {
    let json = serde_json::json!({
        "schema": "sensor.report.v1",
        "tool": { "name": "t", "version": "1.0.0" },
        "run": { "started_at": "2026-01-01T00:00:00Z" },
        "verdict": { "status": 42, "counts": { "info": 0, "warn": 0, "error": 0 } },
        "findings": []
    });
    let result = conform_single(&json.to_string(), "sensor", &all_checks()).unwrap();
    assert!(!result.is_pass());
    assert!(result.violations.iter().any(|v| v.check == "schema"));
}

#[test]
fn schema_extra_unknown_fields_rejected() {
    let json = serde_json::json!({
        "schema": "sensor.report.v1",
        "tool": { "name": "t", "version": "1.0.0" },
        "run": { "started_at": "2026-01-01T00:00:00Z" },
        "verdict": { "status": "pass", "counts": { "info": 0, "warn": 0, "error": 0 } },
        "findings": [],
        "extra_field": "should_be_rejected",
        "another_extra": { "nested": true }
    });
    let result = conform_single(&json.to_string(), "sensor", &all_checks()).unwrap();
    assert!(
        !result.is_pass(),
        "extra unknown fields should be rejected by schema"
    );
    assert!(result.violations.iter().any(|v| v.check == "schema"));
}

// ═══════════════════════════════════════════════════════════════════════════
// 2. Path hygiene edge cases
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn path_hygiene_null_byte_in_path() {
    let mut report = minimal_sensor_report();
    report.findings = vec![Finding {
        severity: Severity::Info,
        check_id: None,
        code: "NUL".to_string(),
        message: "null byte".to_string(),
        location: Some(Location {
            path: Some("src/\0evil.rs".to_string()),
            line: None,
            col: None,
        }),
        help: None,
        url: None,
        fingerprint: None,
        data: None,
    }];
    // Null bytes are unusual; the important thing is no panic
    let _violations = check_path_hygiene(&report);
}

#[test]
fn path_hygiene_control_chars_in_path() {
    let mut report = minimal_sensor_report();
    report.findings = vec![Finding {
        severity: Severity::Info,
        check_id: None,
        code: "CTRL".to_string(),
        message: "control chars".to_string(),
        location: Some(Location {
            path: Some("src/\x01\x1f.rs".to_string()),
            line: None,
            col: None,
        }),
        help: None,
        url: None,
        fingerprint: None,
        data: None,
    }];
    let _violations = check_path_hygiene(&report);
}

#[test]
fn path_hygiene_unicode_path_no_panic() {
    let mut report = minimal_sensor_report();
    report.findings = vec![Finding {
        severity: Severity::Info,
        check_id: None,
        code: "UNI".to_string(),
        message: "unicode".to_string(),
        location: Some(Location {
            path: Some("src/日本語/файл.rs".to_string()),
            line: None,
            col: None,
        }),
        help: None,
        url: None,
        fingerprint: None,
        data: None,
    }];
    // Should not panic; unicode relative paths are acceptable
    let violations = check_path_hygiene(&report);
    assert!(violations.is_empty());
}

#[test]
fn path_hygiene_unc_path_detected() {
    let mut report = minimal_sensor_report();
    report.findings = vec![Finding {
        severity: Severity::Info,
        check_id: None,
        code: "UNC".to_string(),
        message: "unc path".to_string(),
        location: Some(Location {
            path: Some("\\\\server\\share\\file.rs".to_string()),
            line: None,
            col: None,
        }),
        help: None,
        url: None,
        fingerprint: None,
        data: None,
    }];
    let violations = check_path_hygiene(&report);
    assert!(
        !violations.is_empty(),
        "UNC paths should be flagged: {:?}",
        violations
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// 3. Sensor ID edge cases
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn sensor_id_with_null_byte_rejected() {
    let violations = check_sensor_id_format("sensor\0id");
    assert!(!violations.is_empty());
}

#[test]
fn sensor_id_with_unicode_rejected() {
    let violations = check_sensor_id_format("sensor—id");
    assert!(!violations.is_empty());
}

#[test]
fn sensor_id_very_long_accepted() {
    let long_id: String = "a".repeat(500);
    let violations = check_sensor_id_format(&long_id);
    assert!(violations.is_empty(), "long but valid IDs should pass");
}

#[test]
fn sensor_id_with_slash_rejected() {
    let violations = check_sensor_id_format("sensor/id");
    assert!(!violations.is_empty());
}

#[test]
fn sensor_id_with_path_traversal_rejected() {
    let violations = check_sensor_id_format("../etc");
    assert!(!violations.is_empty());
}

#[test]
fn sensor_id_via_conform_single_rejects_bad_id() {
    let json = to_json(&minimal_sensor_report());
    let result = conform_single(&json, "bad sensor!", &all_checks()).unwrap();
    assert!(
        result
            .violations
            .iter()
            .any(|v| v.check == "sensor_id_format")
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// 4. Ordering verification
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn ordering_many_identical_sort_keys_passes() {
    let mut report = minimal_sensor_report();
    // 20 findings with identical severity, code, message, no location
    report.findings = (0..20)
        .map(|_| make_finding(Severity::Warn, "W1", "same message"))
        .collect();
    let violations = check_ordering(&report, "sensor");
    assert!(
        violations.is_empty(),
        "identical keys should be considered sorted: {:?}",
        violations
    );
}

#[test]
fn ordering_findings_differ_only_in_message() {
    let mut report = minimal_sensor_report();
    // Same severity, code, path, line — differ only in message
    report.findings = vec![
        make_finding(Severity::Error, "E1", "aaa"),
        make_finding(Severity::Error, "E1", "bbb"),
        make_finding(Severity::Error, "E1", "ccc"),
    ];
    let violations = check_ordering(&report, "sensor");
    assert!(
        violations.is_empty(),
        "alphabetical message order should pass: {:?}",
        violations
    );
}

#[test]
fn ordering_findings_differ_only_in_message_reversed() {
    let mut report = minimal_sensor_report();
    report.findings = vec![
        make_finding(Severity::Error, "E1", "zzz"),
        make_finding(Severity::Error, "E1", "aaa"),
    ];
    let violations = check_ordering(&report, "sensor");
    assert!(
        !violations.is_empty(),
        "reversed message order should fail: {:?}",
        violations
    );
}

#[test]
fn ordering_findings_differ_only_in_path() {
    let mut report = minimal_sensor_report();
    report.findings = vec![
        make_finding_at(Severity::Error, "E1", "msg", "a/file.rs", 1),
        make_finding_at(Severity::Error, "E1", "msg", "b/file.rs", 1),
    ];
    let violations = check_ordering(&report, "sensor");
    assert!(violations.is_empty());
}

#[test]
fn ordering_findings_differ_only_in_line() {
    let mut report = minimal_sensor_report();
    report.findings = vec![
        make_finding_at(Severity::Error, "E1", "msg", "same.rs", 10),
        make_finding_at(Severity::Error, "E1", "msg", "same.rs", 20),
    ];
    let violations = check_ordering(&report, "sensor");
    assert!(violations.is_empty());
}

#[test]
fn ordering_line_numbers_reversed_detected() {
    let mut report = minimal_sensor_report();
    report.findings = vec![
        make_finding_at(Severity::Error, "E1", "msg", "same.rs", 20),
        make_finding_at(Severity::Error, "E1", "msg", "same.rs", 10),
    ];
    let violations = check_ordering(&report, "sensor");
    assert!(!violations.is_empty());
}

// ═══════════════════════════════════════════════════════════════════════════
// 5. Reason token validation
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn reason_token_with_leading_underscore() {
    assert!(is_valid_reason_token("_leading"));
}

#[test]
fn reason_token_with_trailing_underscore() {
    assert!(is_valid_reason_token("trailing_"));
}

#[test]
fn reason_token_consecutive_underscores() {
    assert!(is_valid_reason_token("a__b"));
}

#[test]
fn reason_token_single_underscore() {
    assert!(is_valid_reason_token("_"));
}

#[test]
fn reason_token_with_unicode_rejected() {
    assert!(!is_valid_reason_token("café"));
    assert!(!is_valid_reason_token("über"));
    assert!(!is_valid_reason_token("日本語"));
}

#[test]
fn reason_token_with_newline_rejected() {
    assert!(!is_valid_reason_token("line\nbreak"));
}

#[test]
fn reason_token_very_long_accepted() {
    let long_token: String = "a".repeat(1000);
    assert!(is_valid_reason_token(&long_token));
}

#[test]
fn reason_tokens_in_report_multiple_bad_tokens() {
    let mut report = minimal_sensor_report();
    report.verdict.reasons = vec![
        "UPPER".to_string(),
        "has-dash".to_string(),
        "has space".to_string(),
        "ok_token".to_string(),
    ];
    let violations = check_reason_tokens(&report);
    assert_eq!(
        violations.len(),
        3,
        "should flag exactly 3 bad tokens: {:?}",
        violations
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// 6. Cross-field consistency (survivability + tool_error)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn survivability_fail_with_reasons_only_passes() {
    let mut report = minimal_sensor_report();
    report.verdict.status = VerdictStatus::Fail;
    report.verdict.reasons = vec!["tool_error".to_string()];
    // No findings but has reasons — survivability should pass
    let json = to_json(&report);
    let checks = ConformChecks {
        survivability: true,
        path_hygiene: false,
        ordering: false,
        reason_lint: false,
        tool_error_identity: false,
        sensor_id_format: false,
        artifact_pointers: false,
    };
    let result = conform_single(&json, "sensor", &checks).unwrap();
    assert!(
        !result.violations.iter().any(|v| v.check == "survivability"),
        "fail with reasons should pass survivability: {:?}",
        result.violations
    );
}

#[test]
fn survivability_skip_and_warn_not_checked() {
    for status in [VerdictStatus::Skip, VerdictStatus::Warn] {
        let mut report = minimal_sensor_report();
        report.verdict.status = status;
        // No findings, no reasons
        let json = to_json(&report);
        let checks = ConformChecks {
            survivability: true,
            path_hygiene: false,
            ordering: false,
            reason_lint: false,
            tool_error_identity: false,
            sensor_id_format: false,
            artifact_pointers: false,
        };
        let result = conform_single(&json, "sensor", &checks).unwrap();
        assert!(
            !result.violations.iter().any(|v| v.check == "survivability"),
            "survivability only fires for fail, not {:?}: {:?}",
            report.verdict.status,
            result.violations
        );
    }
}

#[test]
fn tool_error_identity_without_tool_error_reason_passes() {
    let mut report = minimal_sensor_report();
    report.verdict.reasons = vec!["some_other_reason".to_string()];
    let violations = check_tool_error_identity(&report);
    assert!(violations.is_empty());
}

#[test]
fn tool_error_identity_with_wrong_check_id_fails() {
    let mut report = minimal_sensor_report();
    report.verdict.reasons = vec!["tool_error".to_string()];
    report.findings.push(Finding {
        severity: Severity::Error,
        check_id: Some("wrong.check".to_string()),
        code: "runtime_error".to_string(),
        message: "boom".to_string(),
        location: None,
        help: None,
        url: None,
        fingerprint: None,
        data: None,
    });
    let violations = check_tool_error_identity(&report);
    assert!(
        !violations.is_empty(),
        "wrong check_id should fail tool_error_identity"
    );
}

#[test]
fn tool_error_identity_with_wrong_code_fails() {
    let mut report = minimal_sensor_report();
    report.verdict.reasons = vec!["tool_error".to_string()];
    report.findings.push(Finding {
        severity: Severity::Error,
        check_id: Some("tool.runtime".to_string()),
        code: "wrong_code".to_string(),
        message: "boom".to_string(),
        location: None,
        help: None,
        url: None,
        fingerprint: None,
        data: None,
    });
    let violations = check_tool_error_identity(&report);
    assert!(
        !violations.is_empty(),
        "wrong code should fail tool_error_identity"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// 7. Artifact pointer edge cases
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn artifact_pointer_with_backslash_path() {
    let mut report = minimal_sensor_report();
    report.artifacts = vec![ArtifactPointer {
        id: "log".to_string(),
        path: "artifacts\\sensor\\log.txt".to_string(),
        mime: "text/plain".to_string(),
        schema: None,
    }];
    let violations = check_artifact_pointers(&report);
    // Backslashes aren't checked by check_artifact_pointers (only .., abs, empty)
    // This test documents current behavior
    let _ = violations;
}

#[test]
fn artifact_pointer_all_empty_fields() {
    let mut report = minimal_sensor_report();
    report.artifacts = vec![ArtifactPointer {
        id: "".to_string(),
        path: "".to_string(),
        mime: "".to_string(),
        schema: None,
    }];
    let violations = check_artifact_pointers(&report);
    assert!(
        violations.len() >= 3,
        "all empty fields should produce >= 3 violations: {:?}",
        violations
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// 8. Cockpit report / presence semantics
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn presence_semantics_missing_with_policy_applied_passes() {
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
            reasons: vec!["missing_receipt".to_string()],
        },
        truncated: false,
        errors: vec![],
        missing_policy_applied: Some(MissingPolicy::Fail),
        policy_outcome: None,
    });
    let violations = check_presence_semantics(&report);
    assert!(
        violations.is_empty(),
        "Missing presence with missing_policy_applied should be ok: {:?}",
        violations
    );
}

#[test]
fn presence_semantics_present_without_policy_applied_passes() {
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
fn cockpit_reason_tokens_sensor_level_bad_token() {
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
            reasons: vec!["INVALID-TOKEN".to_string()],
        },
        truncated: false,
        errors: vec![],
        missing_policy_applied: None,
        policy_outcome: None,
    });
    let violations = check_cockpit_reason_tokens(&report);
    assert!(
        !violations.is_empty(),
        "bad sensor-level reason token should be flagged"
    );
}

#[test]
fn cockpit_reason_tokens_capability_level_bad_token() {
    let mut report = minimal_cockpit_report();
    report.run.capabilities.insert(
        "docker".to_string(),
        Capability {
            status: CapabilityStatus::Available,
            reason: Some("BAD-REASON".to_string()),
        },
    );
    let violations = check_cockpit_reason_tokens(&report);
    assert!(
        !violations.is_empty(),
        "bad capability reason token should be flagged"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// 9. Cockpit schema validation edge cases
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn cockpit_schema_empty_object_fails() {
    let violations = validate_cockpit_schema("{}").unwrap();
    assert!(!violations.is_empty());
}

#[test]
fn cockpit_schema_valid_minimal_passes() {
    let report = minimal_cockpit_report();
    let json = serde_json::to_string(&report).unwrap();
    let violations = validate_cockpit_schema(&json).unwrap();
    assert!(
        violations.is_empty(),
        "minimal valid cockpit report should pass: {:?}",
        violations
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// 10. Conform result formatting / is_pass semantics
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn conform_result_is_pass_with_no_violations() {
    let json = to_json(&minimal_sensor_report());
    let result = conform_single(&json, "valid-sensor", &all_checks()).unwrap();
    assert!(result.is_pass());
    assert!(result.violations.is_empty());
}

#[test]
fn conform_result_violations_contain_check_names() {
    let json = to_json(&minimal_sensor_report());
    let result = conform_single(&json, "bad.sensor.id!", &all_checks()).unwrap();
    assert!(!result.is_pass());
    for v in &result.violations {
        assert!(
            !v.check.is_empty(),
            "violation check name should not be empty"
        );
        assert!(
            !v.message.is_empty(),
            "violation message should not be empty"
        );
    }
}

#[test]
fn conform_single_schema_failure_stops_further_checks() {
    // If schema validation fails, we get only schema violations (early return)
    let result = conform_single("{}", "sensor", &all_checks()).unwrap();
    assert!(!result.is_pass());
    for v in &result.violations {
        assert_eq!(
            v.check, "schema",
            "only schema violations expected on schema failure"
        );
    }
}
