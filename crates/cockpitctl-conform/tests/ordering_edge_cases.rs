//! Edge-case tests for conformance ordering, path hygiene, reason-lint,
//! schema validation, and survivability.

use cockpitctl_conform::{
    ConformChecks, check_ordering, check_path_hygiene, check_reason_tokens, check_sensor_id_format,
    conform_single, validate_cockpit_schema,
};
use cockpitctl_types::*;
use std::collections::BTreeMap;

// ── helpers ──────────────────────────────────────────────────────────

fn minimal_report() -> SensorReport {
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

fn finding(severity: Severity, code: &str, message: &str) -> Finding {
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

fn finding_at(severity: Severity, code: &str, path: &str, line: Option<u32>) -> Finding {
    Finding {
        severity,
        check_id: None,
        code: code.to_string(),
        message: "m".to_string(),
        location: Some(Location {
            path: Some(path.to_string()),
            line,
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

// ── 1–4: ordering basics ────────────────────────────────────────────

#[test]
fn ordering_empty_findings_passes() {
    let report = minimal_report();
    let violations = check_ordering(&report, "sensor");
    assert!(violations.is_empty());
}

#[test]
fn ordering_single_finding_passes() {
    let mut report = minimal_report();
    report.findings = vec![finding(Severity::Error, "E1", "boom")];
    let violations = check_ordering(&report, "sensor");
    assert!(violations.is_empty());
}

#[test]
fn ordering_already_sorted_passes() {
    let mut report = minimal_report();
    // Error (rank 0) before Warn (rank 1) before Info (rank 2)
    report.findings = vec![
        finding(Severity::Error, "E1", "err"),
        finding(Severity::Warn, "W1", "warn"),
        finding(Severity::Info, "I1", "info"),
    ];
    let violations = check_ordering(&report, "sensor");
    assert!(violations.is_empty());
}

#[test]
fn ordering_reversed_findings_fails() {
    let mut report = minimal_report();
    // Info before Error is out of order
    report.findings = vec![
        finding(Severity::Info, "I1", "info"),
        finding(Severity::Error, "E1", "err"),
    ];
    let violations = check_ordering(&report, "sensor");
    assert!(!violations.is_empty(), "reversed order must be detected");
}

// ── 5: message tiebreak ─────────────────────────────────────────────

#[test]
fn ordering_message_tiebreak() {
    let mut report = minimal_report();
    // Same severity, same code — differ only in message; "aaa" < "zzz"
    report.findings = vec![
        finding(Severity::Warn, "W1", "aaa"),
        finding(Severity::Warn, "W1", "zzz"),
    ];
    assert!(check_ordering(&report, "sensor").is_empty());

    // Swap → should fail
    report.findings = vec![
        finding(Severity::Warn, "W1", "zzz"),
        finding(Severity::Warn, "W1", "aaa"),
    ];
    assert!(!check_ordering(&report, "sensor").is_empty());
}

// ── 6: numeric line sort ────────────────────────────────────────────

#[test]
fn ordering_line_numeric_not_lexical() {
    let mut report = minimal_report();
    // Lines 1, 2, 10 — correct numeric order.
    // Lexical would put "10" before "2".
    report.findings = vec![
        finding_at(Severity::Info, "C", "src/a.rs", Some(1)),
        finding_at(Severity::Info, "C", "src/a.rs", Some(2)),
        finding_at(Severity::Info, "C", "src/a.rs", Some(10)),
    ];
    assert!(check_ordering(&report, "sensor").is_empty());
}

// ── 7: None path handling ───────────────────────────────────────────

#[test]
fn ordering_none_path_consistent() {
    let mut report = minimal_report();
    // None path maps to "" which sorts before any non-empty path
    report.findings = vec![
        finding(Severity::Info, "C", "no-path"),
        finding_at(Severity::Info, "C", "src/a.rs", None),
    ];
    // "" < "src/a.rs" so no-path (None → "") comes first
    assert!(check_ordering(&report, "sensor").is_empty());
}

// ── 8: None line handling ───────────────────────────────────────────

#[test]
fn ordering_none_line_consistent() {
    let mut report = minimal_report();
    // None line maps to 0, which sorts before line 5
    report.findings = vec![
        finding_at(Severity::Info, "C", "src/a.rs", None),
        finding_at(Severity::Info, "C", "src/a.rs", Some(5)),
    ];
    assert!(check_ordering(&report, "sensor").is_empty());
}

// ── 9: 100 findings, same severity ─────────────────────────────────

#[test]
fn ordering_100_same_severity_secondary_keys() {
    let mut report = minimal_report();
    // All Info, code from C000..C099, message "m" — already in lex order
    report.findings = (0..100)
        .map(|i| finding(Severity::Info, &format!("C{i:03}"), "m"))
        .collect();
    assert!(check_ordering(&report, "sensor").is_empty());
}

// ── 10: Unicode in messages ─────────────────────────────────────────

#[test]
fn ordering_unicode_messages_do_not_break() {
    let mut report = minimal_report();
    report.findings = vec![
        finding(Severity::Info, "C", "日本語"),
        finding(Severity::Info, "C", "中文"),
    ];
    // We only care that it does not panic; ordering is byte-level
    let _ = check_ordering(&report, "sensor");
}

// ── 11: path hygiene — clean ────────────────────────────────────────

#[test]
fn path_hygiene_clean_path_passes() {
    let mut report = minimal_report();
    report.findings = vec![finding_at(Severity::Info, "C", "src/lib.rs", Some(1))];
    assert!(check_path_hygiene(&report).is_empty());
}

// ── 12: path hygiene — ".." in sensor_id ────────────────────────────

#[test]
fn path_hygiene_dotdot_sensor_id_rejected() {
    let violations = check_sensor_id_format("../traversal");
    assert!(
        !violations.is_empty(),
        "sensor_id with '..' must be rejected"
    );
}

// ── 13: path hygiene — absolute path ────────────────────────────────

#[test]
fn path_hygiene_absolute_path_rejected() {
    let mut report = minimal_report();
    report.findings = vec![finding_at(Severity::Info, "C", "/etc/passwd", Some(1))];
    let v = check_path_hygiene(&report);
    assert!(v.iter().any(|m| m.contains("absolute")));
}

// ── 14: path hygiene — backslash in sensor_id ───────────────────────

#[test]
fn path_hygiene_backslash_sensor_id_rejected() {
    let violations = check_sensor_id_format("foo\\bar");
    assert!(
        !violations.is_empty(),
        "sensor_id with backslash must be rejected"
    );
}

// ── 15: reason-lint — empty reason ──────────────────────────────────

#[test]
fn reason_lint_empty_reason_warning() {
    let mut report = minimal_report();
    report.verdict.reasons = vec!["".to_string()];
    let violations = check_reason_tokens(&report);
    assert!(!violations.is_empty(), "empty reason token must be flagged");
}

// ── 16: reason-lint — non-empty reason passes ───────────────────────

#[test]
fn reason_lint_valid_reason_passes() {
    let mut report = minimal_report();
    report.verdict.reasons = vec!["tool_error".to_string()];
    let violations = check_reason_tokens(&report);
    assert!(violations.is_empty());
}

// ── 17: schema validation — valid receipt ───────────────────────────

#[test]
fn schema_valid_receipt_passes() {
    let report = minimal_report();
    let json = serde_json::to_string(&report).unwrap();
    let result = conform_single(&json, "good-sensor", &only(|_| {})).unwrap();
    assert!(result.is_pass());
}

// ── 18: schema validation — missing required field ──────────────────

#[test]
fn schema_missing_required_field_fails() {
    let json = serde_json::json!({
        "schema": "sensor.report.v1",
        "tool": { "name": "t", "version": "1.0.0" },
        "verdict": { "status": "pass", "counts": { "info": 0, "warn": 0, "error": 0 } },
        "findings": []
    });
    let result = conform_single(&json.to_string(), "sensor", &all_checks()).unwrap();
    assert!(!result.is_pass());
    assert!(result.violations.iter().any(|v| v.check == "schema"));
}

// ── 19: mixed valid/invalid findings ────────────────────────────────

#[test]
fn mixed_valid_and_invalid_findings_reports_individual_issues() {
    let mut report = minimal_report();
    report.findings = vec![
        finding_at(Severity::Info, "C", "src/ok.rs", Some(1)),
        finding_at(Severity::Info, "C", "../escape.rs", Some(1)),
        finding_at(Severity::Info, "C", "/abs/path.rs", Some(1)),
    ];
    let violations = check_path_hygiene(&report);
    // Only the two bad paths, not the clean one
    assert_eq!(violations.len(), 2);
}

// ── 20: survivability — corrupt JSON ────────────────────────────────

#[test]
fn survivability_corrupt_json_controlled_error() {
    let result = conform_single("{{not json!!", "sensor", &all_checks());
    assert!(result.is_err(), "corrupt JSON must be Err, not panic");
}

// ── bonus: ordering via conform_single integration ──────────────────

#[test]
fn conform_single_ordering_reversed_detected() {
    let mut report = minimal_report();
    report.findings = vec![
        finding(Severity::Info, "I1", "info"),
        finding(Severity::Error, "E1", "err"),
    ];
    let json = serde_json::to_string(&report).unwrap();
    let result = conform_single(&json, "sensor", &only(|c| c.ordering = true)).unwrap();
    assert!(result.violations.iter().any(|v| v.check == "ordering"));
}

#[test]
fn conform_single_path_hygiene_traversal_detected() {
    let mut report = minimal_report();
    report.findings = vec![finding_at(Severity::Info, "C", "foo/../../etc", Some(1))];
    let json = serde_json::to_string(&report).unwrap();
    let result = conform_single(&json, "sensor", &only(|c| c.path_hygiene = true)).unwrap();
    assert!(result.violations.iter().any(|v| v.check == "path_hygiene"));
}

#[test]
fn validate_cockpit_schema_empty_object_fails() {
    let violations = validate_cockpit_schema("{}").unwrap();
    assert!(!violations.is_empty());
}

#[test]
fn ordering_code_tiebreak() {
    let mut report = minimal_report();
    // Same severity, same path/line — differ only in code
    report.findings = vec![
        finding(Severity::Warn, "A01", "m"),
        finding(Severity::Warn, "B02", "m"),
    ];
    assert!(check_ordering(&report, "sensor").is_empty());

    report.findings = vec![
        finding(Severity::Warn, "B02", "m"),
        finding(Severity::Warn, "A01", "m"),
    ];
    assert!(!check_ordering(&report, "sensor").is_empty());
}

#[test]
fn ordering_path_tiebreak() {
    let mut report = minimal_report();
    report.findings = vec![
        finding_at(Severity::Error, "C", "a.rs", Some(1)),
        finding_at(Severity::Error, "C", "z.rs", Some(1)),
    ];
    assert!(check_ordering(&report, "sensor").is_empty());

    report.findings = vec![
        finding_at(Severity::Error, "C", "z.rs", Some(1)),
        finding_at(Severity::Error, "C", "a.rs", Some(1)),
    ];
    assert!(!check_ordering(&report, "sensor").is_empty());
}
