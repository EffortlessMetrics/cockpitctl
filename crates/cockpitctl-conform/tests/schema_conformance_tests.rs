//! Schema conformance tests for cockpitctl-conform.
//!
//! Validates that the conformance checker correctly identifies conforming
//! and non-conforming receipts against the protocol contracts.

use cockpitctl_conform::{
    ConformChecks, check_ordering, check_path_hygiene, conform_single, validate_cockpit_schema,
};
use cockpitctl_types::*;
use std::collections::BTreeMap;

// ═══════════════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════════════

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

fn make_finding(severity: Severity, code: &str, msg: &str, path: Option<&str>) -> Finding {
    Finding {
        severity,
        check_id: None,
        code: code.to_string(),
        message: msg.to_string(),
        location: path.map(|p| Location {
            path: Some(p.to_string()),
            line: Some(1),
            col: None,
        }),
        help: None,
        url: None,
        fingerprint: None,
        data: None,
    }
}

// ═══════════════════════════════════════════════════════════════════════
// 1. Fully conforming receipt passes all checks
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn fully_conforming_receipt_passes_all_checks() {
    let report = minimal_sensor_report();
    let json = serde_json::to_string(&report).unwrap();
    let result = conform_single(&json, "good-sensor", &all_checks()).unwrap();
    assert!(
        result.is_pass(),
        "fully conforming receipt should pass: {:?}",
        result.violations
    );
}

#[test]
fn conforming_receipt_with_findings_passes() {
    let mut report = minimal_sensor_report();
    report.verdict.status = VerdictStatus::Warn;
    report.verdict.counts.warn = 1;
    report.findings.push(make_finding(
        Severity::Warn,
        "W001",
        "a warning",
        Some("src/lib.rs"),
    ));
    let json = serde_json::to_string(&report).unwrap();
    let result = conform_single(&json, "test-sensor", &all_checks()).unwrap();
    assert!(
        result.is_pass(),
        "valid receipt with findings should pass: {:?}",
        result.violations
    );
}

// ═══════════════════════════════════════════════════════════════════════
// 2. Receipt missing required fields → violation
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn receipt_missing_tool_yields_schema_violation() {
    let json = serde_json::json!({
        "schema": "sensor.report.v1",
        "run": { "started_at": "2026-01-01T00:00:00Z" },
        "verdict": { "status": "pass", "counts": { "info": 0, "warn": 0, "error": 0 } },
        "findings": []
    });
    let result = conform_single(&json.to_string(), "sensor", &all_checks()).unwrap();
    assert!(!result.is_pass());
    assert!(
        result.violations.iter().any(|v| v.check == "schema"),
        "should report schema violation for missing 'tool'"
    );
}

#[test]
fn receipt_missing_verdict_yields_schema_violation() {
    let json = serde_json::json!({
        "schema": "sensor.report.v1",
        "tool": { "name": "t", "version": "1.0.0" },
        "run": { "started_at": "2026-01-01T00:00:00Z" },
        "findings": []
    });
    let result = conform_single(&json.to_string(), "sensor", &all_checks()).unwrap();
    assert!(!result.is_pass());
    assert!(result.violations.iter().any(|v| v.check == "schema"));
}

// ═══════════════════════════════════════════════════════════════════════
// 3. Receipt with wrong verdict value → violation
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn receipt_with_invalid_verdict_status_yields_violation() {
    let json = serde_json::json!({
        "schema": "sensor.report.v1",
        "tool": { "name": "t", "version": "1.0.0" },
        "run": { "started_at": "2026-01-01T00:00:00Z" },
        "verdict": { "status": "critical", "counts": { "info": 0, "warn": 0, "error": 0 } },
        "findings": []
    });
    let result = conform_single(&json.to_string(), "sensor", &all_checks()).unwrap();
    assert!(!result.is_pass());
    assert!(
        result.violations.iter().any(|v| v.check == "schema"),
        "invalid verdict status must produce schema violation"
    );
}

// ═══════════════════════════════════════════════════════════════════════
// 4. Receipt with valid verdict values → pass
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn all_valid_verdict_statuses_accepted() {
    for status in &["pass", "warn", "fail", "skip"] {
        let json = serde_json::json!({
            "schema": "sensor.report.v1",
            "tool": { "name": "t", "version": "1.0.0" },
            "run": { "started_at": "2026-01-01T00:00:00Z" },
            "verdict": { "status": status, "counts": { "info": 0, "warn": 0, "error": 0 } },
            "findings": []
        });
        let checks = ConformChecks {
            path_hygiene: false,
            ordering: false,
            reason_lint: false,
            survivability: false,
            tool_error_identity: false,
            sensor_id_format: false,
            artifact_pointers: false,
        };
        let result = conform_single(&json.to_string(), "sensor", &checks).unwrap();
        assert!(
            result.is_pass(),
            "verdict status '{status}' should be accepted, violations: {:?}",
            result.violations
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════
// 5. Ordering violations detected
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn out_of_order_findings_detected() {
    let mut report = minimal_sensor_report();
    // info < warn in severity, so warn should come before info in descending order
    report.findings = vec![
        make_finding(Severity::Info, "I001", "info msg", Some("src/a.rs")),
        make_finding(Severity::Warn, "W001", "warn msg", Some("src/a.rs")),
    ];
    let violations = check_ordering(&report, "test-sensor");
    assert!(
        !violations.is_empty(),
        "out-of-order findings should produce ordering violations"
    );
}

#[test]
fn correctly_ordered_findings_pass() {
    let mut report = minimal_sensor_report();
    report.findings = vec![
        make_finding(Severity::Error, "E001", "error msg", Some("src/a.rs")),
        make_finding(Severity::Warn, "W001", "warn msg", Some("src/b.rs")),
        make_finding(Severity::Info, "I001", "info msg", Some("src/c.rs")),
    ];
    let violations = check_ordering(&report, "test-sensor");
    assert!(
        violations.is_empty(),
        "correctly ordered findings should pass: {:?}",
        violations
    );
}

// ═══════════════════════════════════════════════════════════════════════
// 6. Path hygiene violations detected
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn path_traversal_detected() {
    let mut report = minimal_sensor_report();
    report.findings.push(make_finding(
        Severity::Info,
        "T1",
        "traversal",
        Some("../etc/passwd"),
    ));
    let violations = check_path_hygiene(&report);
    assert!(!violations.is_empty(), "path traversal must be detected");
}

#[test]
fn absolute_path_detected() {
    let mut report = minimal_sensor_report();
    report.findings.push(make_finding(
        Severity::Info,
        "A1",
        "absolute",
        Some("/etc/passwd"),
    ));
    let violations = check_path_hygiene(&report);
    assert!(!violations.is_empty(), "absolute path must be detected");
}

#[test]
fn clean_relative_path_passes_hygiene() {
    let mut report = minimal_sensor_report();
    report.findings.push(make_finding(
        Severity::Info,
        "C1",
        "clean",
        Some("src/lib.rs"),
    ));
    let violations = check_path_hygiene(&report);
    assert!(
        violations.is_empty(),
        "clean path should pass: {:?}",
        violations
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Cockpit report conformance
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn valid_cockpit_report_passes_schema_validation() {
    let json = r#"{
      "schema": "cockpit.report.v1",
      "tool": { "name": "cockpitctl", "version": "0.1.0" },
      "run": { "started_at": "2026-01-01T00:00:00Z" },
      "verdict": { "status": "pass", "counts": { "info": 0, "warn": 0, "error": 0 } },
      "sensors": [],
      "highlights": [],
      "policy": {
        "warn_is_fail": false,
        "max_highlights": 5,
        "max_per_sensor_findings": 10,
        "section_order": [],
        "sensors": []
      }
    }"#;
    let violations = validate_cockpit_schema(json).unwrap();
    assert!(
        violations.is_empty(),
        "valid cockpit report should pass: {:?}",
        violations
    );
}

#[test]
fn cockpit_report_missing_required_field_yields_violation() {
    let json = r#"{
      "schema": "cockpit.report.v1",
      "tool": { "name": "cockpitctl", "version": "0.1.0" },
      "run": { "started_at": "2026-01-01T00:00:00Z" },
      "verdict": { "status": "pass", "counts": { "info": 0, "warn": 0, "error": 0 } },
      "highlights": [],
      "policy": {
        "warn_is_fail": false,
        "max_highlights": 5,
        "max_per_sensor_findings": 10,
        "section_order": [],
        "sensors": []
      }
    }"#;
    let violations = validate_cockpit_schema(json).unwrap();
    assert!(
        !violations.is_empty(),
        "missing 'sensors' should produce violations"
    );
    assert!(violations.iter().any(|v| v.check == "schema"));
}
