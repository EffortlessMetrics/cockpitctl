//! Expanded conformance snapshot tests covering edge cases for each check type.

use cockpitctl_conform::{ConformChecks, conform_single};
use cockpitctl_types::*;
use std::collections::BTreeMap;

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

fn to_json(report: &SensorReport) -> String {
    serde_json::to_string(report).expect("serialize")
}

fn make_finding(severity: Severity, code: &str, msg: &str, path: Option<&str>) -> Finding {
    Finding {
        severity,
        check_id: None,
        code: code.to_string(),
        message: msg.to_string(),
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

// ─────────────────────────────────────────────────────────────────────────────
// Path hygiene: various traversal patterns
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn snapshot_path_hygiene_multiple_traversal_patterns() {
    let mut report = minimal_sensor_report();
    report.findings = vec![
        make_finding(Severity::Info, "T1", "parent traversal", Some("../secret")),
        make_finding(Severity::Info, "T2", "mid traversal", Some("foo/../../bar")),
        make_finding(
            Severity::Info,
            "T3",
            "backslash traversal",
            Some("foo\\..\\bar"),
        ),
        make_finding(
            Severity::Info,
            "T4",
            "drive letter absolute",
            Some("D:\\Windows\\System32"),
        ),
        make_finding(Severity::Info, "T5", "unix absolute", Some("/etc/passwd")),
        make_finding(
            Severity::Info,
            "T6",
            "clean path passes",
            Some("src/lib.rs"),
        ),
    ];
    // Findings are out of order (all same severity, but codes are ok) — only test path_hygiene.
    let checks = only(|c| c.path_hygiene = true);
    let result =
        conform_single(&to_json(&report), "test_sensor", &checks).expect("should not error");
    insta::assert_debug_snapshot!("path_hygiene_multiple_traversals", result);
}

// ─────────────────────────────────────────────────────────────────────────────
// Ordering: findings not in canonical order
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn snapshot_ordering_info_before_warn_before_error() {
    let mut report = minimal_sensor_report();
    // Reverse of canonical: info, warn, error — canonical is error, warn, info
    report.findings = vec![
        make_finding(Severity::Info, "I1", "info first", None),
        make_finding(Severity::Warn, "W1", "warn second", None),
        make_finding(Severity::Error, "E1", "error third", None),
    ];
    let checks = only(|c| c.ordering = true);
    let result =
        conform_single(&to_json(&report), "test_sensor", &checks).expect("should not error");
    insta::assert_debug_snapshot!("ordering_reverse_severity", result);
}

#[test]
fn snapshot_ordering_correct_order_passes() {
    let mut report = minimal_sensor_report();
    // Canonical order: error first, then warn, then info
    report.findings = vec![
        make_finding(Severity::Error, "E1", "error first", None),
        make_finding(Severity::Warn, "W1", "warn second", None),
        make_finding(Severity::Info, "I1", "info third", None),
    ];
    let checks = only(|c| c.ordering = true);
    let result =
        conform_single(&to_json(&report), "test_sensor", &checks).expect("should not error");
    insta::assert_debug_snapshot!("ordering_correct_passes", result);
}

// ─────────────────────────────────────────────────────────────────────────────
// Reason lint: invalid reason tokens
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn snapshot_reason_lint_invalid_tokens() {
    let mut report = minimal_sensor_report();
    report.verdict.reasons = vec![
        "".to_string(),           // empty
        "Has Space".to_string(),  // spaces
        "UPPER".to_string(),      // uppercase
        "has-dash".to_string(),   // dashes
        "ok_token".to_string(),   // valid — should not appear in violations
        "has.dot".to_string(),    // dot
        "a!b@c#".to_string(),     // special chars
        "waaaaay_too_long_token_name_that_exceeds_reasonable_length_but_still_lowercase_and_underscore".to_string(),
    ];
    let checks = only(|c| c.reason_lint = true);
    let result =
        conform_single(&to_json(&report), "test_sensor", &checks).expect("should not error");
    insta::assert_debug_snapshot!("reason_lint_invalid_tokens", result);
}

#[test]
fn snapshot_reason_lint_capability_invalid_token() {
    let mut report = minimal_sensor_report();
    report.run.capabilities.insert(
        "git".to_string(),
        Capability {
            status: CapabilityStatus::Available,
            reason: Some("Bad-Reason".to_string()),
        },
    );
    let checks = only(|c| c.reason_lint = true);
    let result =
        conform_single(&to_json(&report), "test_sensor", &checks).expect("should not error");
    insta::assert_debug_snapshot!("reason_lint_capability_invalid", result);
}

// ─────────────────────────────────────────────────────────────────────────────
// Tool error identity: missing canonical finding
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn snapshot_tool_error_identity_missing_canonical() {
    let mut report = minimal_sensor_report();
    report.verdict.reasons = vec!["tool_error".to_string()];
    // No findings with check_id="tool.runtime" + code="runtime_error"
    let checks = only(|c| c.tool_error_identity = true);
    let result =
        conform_single(&to_json(&report), "test_sensor", &checks).expect("should not error");
    insta::assert_debug_snapshot!("tool_error_identity_missing", result);
}

#[test]
fn snapshot_tool_error_identity_with_canonical() {
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
    let checks = only(|c| c.tool_error_identity = true);
    let result =
        conform_single(&to_json(&report), "test_sensor", &checks).expect("should not error");
    insta::assert_debug_snapshot!("tool_error_identity_present", result);
}

// ─────────────────────────────────────────────────────────────────────────────
// Sensor ID format: invalid chars
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn snapshot_sensor_id_format_invalid() {
    let report = minimal_sensor_report();
    let checks = only(|c| c.sensor_id_format = true);

    let result =
        conform_single(&to_json(&report), "bad.sensor.id", &checks).expect("should not error");
    insta::assert_debug_snapshot!("sensor_id_format_dots", result);
}

#[test]
fn snapshot_sensor_id_format_traversal() {
    let report = minimal_sensor_report();
    let checks = only(|c| c.sensor_id_format = true);

    let result =
        conform_single(&to_json(&report), "../traversal", &checks).expect("should not error");
    insta::assert_debug_snapshot!("sensor_id_format_traversal", result);
}

#[test]
fn snapshot_sensor_id_format_empty() {
    let report = minimal_sensor_report();
    let checks = only(|c| c.sensor_id_format = true);

    let result = conform_single(&to_json(&report), "", &checks).expect("should not error");
    insta::assert_debug_snapshot!("sensor_id_format_empty", result);
}

#[test]
fn snapshot_sensor_id_format_valid() {
    let report = minimal_sensor_report();
    let checks = only(|c| c.sensor_id_format = true);

    let result =
        conform_single(&to_json(&report), "my-sensor_v2", &checks).expect("should not error");
    insta::assert_debug_snapshot!("sensor_id_format_valid", result);
}

// ─────────────────────────────────────────────────────────────────────────────
// Schema conformance: missing required fields
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn snapshot_schema_missing_findings_field() {
    let json = serde_json::json!({
        "schema": "sensor.report.v1",
        "tool": { "name": "t", "version": "1.0.0" },
        "run": { "started_at": "2026-01-01T00:00:00Z" },
        "verdict": { "status": "pass", "counts": { "info": 0, "warn": 0, "error": 0 } }
    });
    let result =
        conform_single(&json.to_string(), "sensor_a", &all_checks()).expect("should not error");
    insta::assert_debug_snapshot!("schema_missing_findings", result);
}

#[test]
fn snapshot_schema_missing_tool_field() {
    let json = serde_json::json!({
        "schema": "sensor.report.v1",
        "run": { "started_at": "2026-01-01T00:00:00Z" },
        "verdict": { "status": "pass", "counts": { "info": 0, "warn": 0, "error": 0 } },
        "findings": []
    });
    let result =
        conform_single(&json.to_string(), "sensor_a", &all_checks()).expect("should not error");
    insta::assert_debug_snapshot!("schema_missing_tool", result);
}

// ─────────────────────────────────────────────────────────────────────────────
// Valid receipt: fully conformant → empty violations
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn snapshot_fully_conformant_receipt() {
    let mut report = minimal_sensor_report();
    // Add findings in correct order (error before info) with clean paths
    report.findings = vec![
        Finding {
            severity: Severity::Error,
            check_id: None,
            code: "E1".to_string(),
            message: "error finding".to_string(),
            location: Some(Location {
                path: Some("src/main.rs".to_string()),
                line: Some(1),
                col: None,
            }),
            help: None,
            url: None,
            fingerprint: None,
            data: None,
        },
        Finding {
            severity: Severity::Info,
            check_id: None,
            code: "I1".to_string(),
            message: "info finding".to_string(),
            location: Some(Location {
                path: Some("src/lib.rs".to_string()),
                line: Some(5),
                col: None,
            }),
            help: None,
            url: None,
            fingerprint: None,
            data: None,
        },
    ];
    report.verdict.status = VerdictStatus::Fail;
    report.verdict.counts = VerdictCounts {
        info: 1,
        warn: 0,
        error: 1,
        suppressed: 0,
    };
    report.verdict.reasons = vec!["lint_failure".to_string()];
    report.artifacts = vec![ArtifactPointer {
        id: "log".to_string(),
        path: "artifacts/log.txt".to_string(),
        mime: "text/plain".to_string(),
        schema: None,
    }];

    let result =
        conform_single(&to_json(&report), "good-sensor", &all_checks()).expect("should not error");
    insta::assert_debug_snapshot!("fully_conformant_receipt", result);
}

// ─────────────────────────────────────────────────────────────────────────────
// Multiple violations: receipt triggering many checks at once
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn snapshot_many_violations_combined() {
    let mut report = minimal_sensor_report();
    report.verdict.status = VerdictStatus::Fail;
    // No findings or reasons → survivability violation (overridden below with bad reasons)
    report.verdict.reasons = vec!["INVALID".to_string(), "tool_error".to_string()];
    // Findings out of order with bad paths
    report.findings = vec![
        make_finding(Severity::Info, "I1", "info first", Some("../traversal")),
        make_finding(Severity::Error, "E1", "error second", Some("/abs/path")),
    ];
    // No canonical tool_error finding → tool_error_identity violation
    // Bad sensor ID
    let result =
        conform_single(&to_json(&report), "bad.id!", &all_checks()).expect("should not error");
    insta::assert_debug_snapshot!("many_violations_combined", result);
}
