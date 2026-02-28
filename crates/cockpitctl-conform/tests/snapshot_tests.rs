use cockpitctl_conform::{ConformChecks, conform_single, validate_cockpit_schema};
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

#[test]
fn snapshot_valid_receipt_all_checks_pass() {
    let report = minimal_sensor_report();
    let result =
        conform_single(&to_json(&report), "good_sensor", &all_checks()).expect("should not error");
    insta::assert_debug_snapshot!("valid_receipt_all_pass", result);
}

#[test]
fn snapshot_path_traversal_violation() {
    let mut report = minimal_sensor_report();
    report.findings = vec![Finding {
        severity: Severity::Error,
        check_id: None,
        code: "E1".to_string(),
        message: "found issue".to_string(),
        location: Some(Location {
            path: Some("../../../etc/passwd".to_string()),
            line: Some(1),
            col: None,
        }),
        help: None,
        url: None,
        fingerprint: None,
        data: None,
    }];
    let result =
        conform_single(&to_json(&report), "sensor_a", &all_checks()).expect("should not error");
    insta::assert_debug_snapshot!("path_traversal_violation", result);
}

#[test]
fn snapshot_ordering_violation() {
    let mut report = minimal_sensor_report();
    // Info before Error → out of canonical order (severity desc)
    report.findings = vec![
        Finding {
            severity: Severity::Info,
            check_id: None,
            code: "I1".to_string(),
            message: "info finding".to_string(),
            location: Some(Location {
                path: Some("src/main.rs".to_string()),
                line: Some(10),
                col: None,
            }),
            help: None,
            url: None,
            fingerprint: None,
            data: None,
        },
        Finding {
            severity: Severity::Error,
            check_id: None,
            code: "E1".to_string(),
            message: "error finding".to_string(),
            location: Some(Location {
                path: Some("src/main.rs".to_string()),
                line: Some(5),
                col: None,
            }),
            help: None,
            url: None,
            fingerprint: None,
            data: None,
        },
    ];
    let result =
        conform_single(&to_json(&report), "sensor_a", &all_checks()).expect("should not error");
    insta::assert_debug_snapshot!("ordering_violation", result);
}

#[test]
fn snapshot_multiple_violations() {
    let mut report = minimal_sensor_report();
    report.verdict.status = VerdictStatus::Fail;
    // No findings or reasons → survivability violation
    // Bad reason tokens → reason_lint violation
    report.verdict.reasons = vec!["Bad-Token".to_string()];
    report.findings = vec![Finding {
        severity: Severity::Warn,
        check_id: None,
        code: "W1".to_string(),
        message: "a warning".to_string(),
        location: Some(Location {
            path: Some("../traversal/path.rs".to_string()),
            line: Some(1),
            col: None,
        }),
        help: None,
        url: None,
        fingerprint: None,
        data: None,
    }];
    report.artifacts = vec![ArtifactPointer {
        id: "log".to_string(),
        path: "/absolute/path.txt".to_string(),
        mime: "text/plain".to_string(),
        schema: None,
    }];

    let result = conform_single(&to_json(&report), "bad.sensor.id", &all_checks())
        .expect("should not error");
    insta::assert_debug_snapshot!("multiple_violations", result);
}

#[test]
fn snapshot_schema_validation_failure() {
    // Empty object fails schema validation
    let result = conform_single("{}", "sensor_a", &all_checks()).expect("should not error");
    insta::assert_debug_snapshot!("schema_validation_failure", result);
}

#[test]
fn snapshot_cockpit_schema_validation_failure() {
    let violations = validate_cockpit_schema("{}").expect("should not error");
    insta::assert_debug_snapshot!("cockpit_schema_validation_failure", violations);
}
