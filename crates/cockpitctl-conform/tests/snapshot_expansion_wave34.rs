//! Wave-34 snapshot expansion for cockpitctl-conform.
//!
//! Covers:
//!  - Conformance report output for each check type individually
//!  - Combined check results with multiple violation types
//!  - Cockpit-level validation

use cockpitctl_conform::{
    ConformChecks, check_cockpit_extended, conform_single, validate_cockpit_schema,
};
use cockpitctl_types::*;
use std::collections::BTreeMap;

// ── Helpers ─────────────────────────────────────────────────────────────

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

fn only_check(field: &str) -> ConformChecks {
    ConformChecks {
        path_hygiene: field == "path_hygiene",
        ordering: field == "ordering",
        reason_lint: field == "reason_lint",
        survivability: field == "survivability",
        tool_error_identity: field == "tool_error_identity",
        sensor_id_format: field == "sensor_id_format",
        artifact_pointers: field == "artifact_pointers",
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

fn to_json(report: &SensorReport) -> String {
    serde_json::to_string(report).expect("serialize")
}

fn make_finding(
    severity: Severity,
    code: &str,
    message: &str,
    path: Option<&str>,
    line: Option<u32>,
) -> Finding {
    Finding {
        severity,
        check_id: None,
        code: code.to_string(),
        message: message.to_string(),
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

// =========================================================================
// 1. Individual check type snapshots
// =========================================================================

#[test]
fn snapshot_path_hygiene_only_violation() {
    let mut report = minimal_sensor_report();
    report.findings = vec![
        make_finding(
            Severity::Error,
            "E1",
            "traversal attempt",
            Some("../../etc/shadow"),
            Some(1),
        ),
        make_finding(
            Severity::Warn,
            "W1",
            "absolute path",
            Some("/usr/bin/env"),
            None,
        ),
        make_finding(
            Severity::Info,
            "I1",
            "backslash path",
            Some("src\\main.rs"),
            Some(5),
        ),
    ];
    let result =
        conform_single(&to_json(&report), "sensor-a", &only_check("path_hygiene")).unwrap();
    insta::assert_debug_snapshot!("path_hygiene_only_violation", result);
}

#[test]
fn snapshot_ordering_only_violation() {
    let mut report = minimal_sensor_report();
    // Info before Warn before Error → wrong canonical order
    report.findings = vec![
        make_finding(
            Severity::Info,
            "I1",
            "info first",
            Some("src/a.rs"),
            Some(1),
        ),
        make_finding(
            Severity::Warn,
            "W1",
            "warn second",
            Some("src/b.rs"),
            Some(2),
        ),
        make_finding(
            Severity::Error,
            "E1",
            "error last",
            Some("src/c.rs"),
            Some(3),
        ),
    ];
    let result = conform_single(&to_json(&report), "sensor-a", &only_check("ordering")).unwrap();
    insta::assert_debug_snapshot!("ordering_only_violation", result);
}

#[test]
fn snapshot_reason_lint_only_violation() {
    let mut report = minimal_sensor_report();
    report.verdict.status = VerdictStatus::Fail;
    report.verdict.counts.error = 1;
    report.findings = vec![make_finding(
        Severity::Error,
        "E1",
        "failure",
        Some("src/a.rs"),
        Some(1),
    )];
    report.verdict.reasons = vec![
        "UPPERCASE_TOKEN".to_string(),
        "has-dashes".to_string(),
        "has spaces".to_string(),
        "valid_token".to_string(),
    ];
    let result = conform_single(&to_json(&report), "sensor-a", &only_check("reason_lint")).unwrap();
    insta::assert_debug_snapshot!("reason_lint_only_violation", result);
}

#[test]
fn snapshot_sensor_id_format_violation() {
    let report = minimal_sensor_report();
    let result = conform_single(
        &to_json(&report),
        "bad.sensor.id!",
        &only_check("sensor_id_format"),
    )
    .unwrap();
    insta::assert_debug_snapshot!("sensor_id_format_violation", result);
}

#[test]
fn snapshot_artifact_pointers_violation() {
    let mut report = minimal_sensor_report();
    report.artifacts = vec![
        ArtifactPointer {
            id: "log".to_string(),
            path: "/absolute/path.log".to_string(),
            mime: "text/plain".to_string(),
            schema: None,
        },
        ArtifactPointer {
            id: "report".to_string(),
            path: "../../traversal/report.json".to_string(),
            mime: "application/json".to_string(),
            schema: None,
        },
        ArtifactPointer {
            id: "".to_string(),
            path: "valid/path.txt".to_string(),
            mime: "text/plain".to_string(),
            schema: None,
        },
    ];
    let result = conform_single(
        &to_json(&report),
        "sensor-a",
        &only_check("artifact_pointers"),
    )
    .unwrap();
    insta::assert_debug_snapshot!("artifact_pointers_violation", result);
}

#[test]
fn snapshot_tool_error_identity_violation() {
    let mut report = minimal_sensor_report();
    report.verdict.status = VerdictStatus::Fail;
    report.verdict.reasons = vec!["tool_error".to_string()];
    // tool_error reason but no finding with check_id or TOOL_ERROR code
    report.findings = vec![make_finding(
        Severity::Error,
        "GENERIC",
        "generic error",
        None,
        None,
    )];
    report.verdict.counts.error = 1;
    let result = conform_single(
        &to_json(&report),
        "sensor-a",
        &only_check("tool_error_identity"),
    )
    .unwrap();
    insta::assert_debug_snapshot!("tool_error_identity_violation", result);
}

// =========================================================================
// 2. Combined check results with multiple violation types
// =========================================================================

#[test]
fn snapshot_all_checks_multiple_violations() {
    let mut report = minimal_sensor_report();
    report.verdict.status = VerdictStatus::Fail;
    report.verdict.counts.error = 2;
    report.verdict.reasons = vec!["Bad-Token".to_string(), "tool_error".to_string()];

    // Ordering violation: Info before Error
    report.findings = vec![
        make_finding(
            Severity::Info,
            "I1",
            "info first",
            Some("src/ok.rs"),
            Some(1),
        ),
        make_finding(
            Severity::Error,
            "E1",
            "error after info",
            Some("../../../etc/passwd"),
            Some(1),
        ),
    ];

    report.artifacts = vec![ArtifactPointer {
        id: "".to_string(),
        path: "/absolute/path.txt".to_string(),
        mime: "text/plain".to_string(),
        schema: None,
    }];

    let result = conform_single(&to_json(&report), "bad.sensor!", &all_checks()).unwrap();
    insta::assert_debug_snapshot!("all_checks_multiple_violations", result);
}

#[test]
fn snapshot_all_checks_clean_report() {
    let mut report = minimal_sensor_report();
    report.verdict.status = VerdictStatus::Warn;
    report.verdict.counts.warn = 2;
    report.verdict.reasons = vec!["unused_import".to_string()];
    report.findings = vec![
        make_finding(
            Severity::Warn,
            "W1",
            "unused import",
            Some("src/lib.rs"),
            Some(3),
        ),
        make_finding(
            Severity::Warn,
            "W2",
            "unused variable",
            Some("src/lib.rs"),
            Some(10),
        ),
    ];
    report.artifacts = vec![ArtifactPointer {
        id: "log".to_string(),
        path: "artifacts/lint/build.log".to_string(),
        mime: "text/plain".to_string(),
        schema: None,
    }];

    let result = conform_single(&to_json(&report), "good-sensor", &all_checks()).unwrap();
    insta::assert_debug_snapshot!("all_checks_clean_report", result);
}

// =========================================================================
// 3. Cockpit-level validation
// =========================================================================

#[test]
fn snapshot_cockpit_schema_valid() {
    let cockpit = CockpitReport {
        schema: "cockpit.report.v1".to_string(),
        tool: ToolInfo {
            name: "cockpitctl".to_string(),
            version: "0.3.0".to_string(),
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
        policy: PolicySnapshot {
            warn_is_fail: false,
            max_highlights: 7,
            max_per_sensor_findings: 20,
            max_annotations: 25,
            section_order: vec![],
            sensors: vec![],
        },
        data: None,
    };

    let json = serde_json::to_string(&cockpit).unwrap();
    let violations = validate_cockpit_schema(&json).unwrap();
    insta::assert_debug_snapshot!("cockpit_schema_valid", violations);
}

#[test]
fn snapshot_cockpit_extended_with_bad_reasons() {
    let cockpit = CockpitReport {
        schema: "cockpit.report.v1".to_string(),
        tool: ToolInfo {
            name: "cockpitctl".to_string(),
            version: "0.3.0".to_string(),
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
            status: VerdictStatus::Fail,
            counts: VerdictCounts {
                info: 0,
                warn: 0,
                error: 1,
                suppressed: 0,
            },
            reasons: vec!["BAD-REASON".to_string(), "good_reason".to_string()],
        },
        sensors: vec![SensorSummary {
            id: "test".to_string(),
            blocking: true,
            missing: MissingPolicy::Fail,
            presence: Presence::Present,
            report_path: "artifacts/test/report.json".to_string(),
            comment_path: None,
            verdict: Verdict {
                status: VerdictStatus::Fail,
                counts: VerdictCounts {
                    info: 0,
                    warn: 0,
                    error: 1,
                    suppressed: 0,
                },
                reasons: vec!["build_error".to_string()],
            },
            truncated: false,
            errors: vec![],
            missing_policy_applied: None,
            policy_outcome: None,
        }],
        highlights: vec![],
        policy: PolicySnapshot {
            warn_is_fail: false,
            max_highlights: 7,
            max_per_sensor_findings: 20,
            max_annotations: 25,
            section_order: vec![],
            sensors: vec![],
        },
        data: None,
    };

    let json = serde_json::to_string(&cockpit).unwrap();
    let violations = check_cockpit_extended(&json, true, true).unwrap();
    insta::assert_debug_snapshot!("cockpit_extended_bad_reasons", violations);
}

#[test]
fn snapshot_cockpit_extended_clean() {
    let cockpit = CockpitReport {
        schema: "cockpit.report.v1".to_string(),
        tool: ToolInfo {
            name: "cockpitctl".to_string(),
            version: "0.3.0".to_string(),
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
        sensors: vec![SensorSummary {
            id: "lint".to_string(),
            blocking: true,
            missing: MissingPolicy::Fail,
            presence: Presence::Present,
            report_path: "artifacts/lint/report.json".to_string(),
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
        }],
        highlights: vec![],
        policy: PolicySnapshot {
            warn_is_fail: false,
            max_highlights: 7,
            max_per_sensor_findings: 20,
            max_annotations: 25,
            section_order: vec![],
            sensors: vec![],
        },
        data: None,
    };

    let json = serde_json::to_string(&cockpit).unwrap();
    let violations = check_cockpit_extended(&json, true, true).unwrap();
    insta::assert_debug_snapshot!("cockpit_extended_clean", violations);
}
