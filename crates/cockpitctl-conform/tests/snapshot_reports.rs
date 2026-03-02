//! Snapshot tests for conformance check reports.

use cockpitctl_conform::{
    ConformChecks, check_ordering, check_path_hygiene, check_reason_tokens, check_sensor_id_format,
    conform_single,
};
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

// ---------------------------------------------------------------------------
// Correctly ordered findings pass conformance
// ---------------------------------------------------------------------------

#[test]
fn snapshot_conform_correctly_ordered_findings() {
    let mut report = minimal_sensor_report();
    report.verdict.status = VerdictStatus::Fail;
    report.verdict.counts = VerdictCounts {
        info: 1,
        warn: 1,
        error: 2,
        suppressed: 0,
    };
    // Correctly ordered: severity desc → path → line → code → message
    report.findings = vec![
        Finding {
            severity: Severity::Error,
            check_id: None,
            code: "E1".to_string(),
            message: "error a".to_string(),
            location: Some(Location {
                path: Some("src/a.rs".to_string()),
                line: Some(1),
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
            code: "E2".to_string(),
            message: "error b".to_string(),
            location: Some(Location {
                path: Some("src/b.rs".to_string()),
                line: Some(5),
                col: None,
            }),
            help: None,
            url: None,
            fingerprint: None,
            data: None,
        },
        Finding {
            severity: Severity::Warn,
            check_id: None,
            code: "W1".to_string(),
            message: "warning".to_string(),
            location: Some(Location {
                path: Some("src/a.rs".to_string()),
                line: Some(10),
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
            message: "info".to_string(),
            location: Some(Location {
                path: Some("src/z.rs".to_string()),
                line: Some(1),
                col: None,
            }),
            help: None,
            url: None,
            fingerprint: None,
            data: None,
        },
    ];

    let result =
        conform_single(&to_json(&report), "well-ordered", &all_checks()).expect("should not error");
    insta::assert_debug_snapshot!("conform_correctly_ordered", result);
}

// ---------------------------------------------------------------------------
// Many path hygiene violations
// ---------------------------------------------------------------------------

#[test]
fn snapshot_conform_many_path_violations() {
    let mut report = minimal_sensor_report();
    report.findings = vec![
        Finding {
            severity: Severity::Error,
            check_id: None,
            code: "T1".to_string(),
            message: "dot-dot prefix".to_string(),
            location: Some(Location {
                path: Some("../etc/passwd".to_string()),
                line: None,
                col: None,
            }),
            help: None,
            url: None,
            fingerprint: None,
            data: None,
        },
        Finding {
            severity: Severity::Warn,
            check_id: None,
            code: "T2".to_string(),
            message: "mid traversal".to_string(),
            location: Some(Location {
                path: Some("foo/../../secret".to_string()),
                line: None,
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
            code: "T3".to_string(),
            message: "absolute unix".to_string(),
            location: Some(Location {
                path: Some("/etc/shadow".to_string()),
                line: None,
                col: None,
            }),
            help: None,
            url: None,
            fingerprint: None,
            data: None,
        },
    ];

    let violations = check_path_hygiene(&report);
    insta::assert_debug_snapshot!("conform_many_path_violations", violations);
}

// ---------------------------------------------------------------------------
// Reason token validation snapshot
// ---------------------------------------------------------------------------

#[test]
fn snapshot_conform_bad_reason_tokens() {
    let mut report = minimal_sensor_report();
    report.verdict.reasons = vec![
        "valid_token".to_string(),
        "Bad-Token".to_string(),
        "UPPER_CASE".to_string(),
        "has space".to_string(),
        "".to_string(),
    ];

    let violations = check_reason_tokens(&report);
    insta::assert_debug_snapshot!("conform_bad_reason_tokens", violations);
}

// ---------------------------------------------------------------------------
// Sensor ID format violations
// ---------------------------------------------------------------------------

#[test]
fn snapshot_conform_bad_sensor_ids() {
    let bad_ids = vec![
        ("empty", ""),
        ("dot_separated", "has.dot"),
        ("with_space", "has space"),
        ("traversal", "../escape"),
        ("unicode", "café"),
        ("special_chars", "a@b#c"),
    ];

    let results: Vec<(String, Vec<String>)> = bad_ids
        .into_iter()
        .map(|(label, id)| (label.to_string(), check_sensor_id_format(id)))
        .collect();

    insta::assert_debug_snapshot!("conform_bad_sensor_ids", results);
}

// ---------------------------------------------------------------------------
// Ordering check with findings that have no locations
// ---------------------------------------------------------------------------

#[test]
fn snapshot_conform_ordering_no_locations() {
    let mut report = minimal_sensor_report();
    report.findings = vec![
        Finding {
            severity: Severity::Info,
            check_id: None,
            code: "I1".to_string(),
            message: "info first".to_string(),
            location: None,
            help: None,
            url: None,
            fingerprint: None,
            data: None,
        },
        Finding {
            severity: Severity::Error,
            check_id: None,
            code: "E1".to_string(),
            message: "error second".to_string(),
            location: None,
            help: None,
            url: None,
            fingerprint: None,
            data: None,
        },
    ];

    let violations = check_ordering(&report, "no-loc-sensor");
    insta::assert_debug_snapshot!("conform_ordering_no_locations", violations);
}

// ---------------------------------------------------------------------------
// Full conform_single with clean but complex report
// ---------------------------------------------------------------------------

#[test]
fn snapshot_conform_complex_clean_report() {
    let mut report = minimal_sensor_report();
    report.verdict.status = VerdictStatus::Warn;
    report.verdict.counts = VerdictCounts {
        info: 1,
        warn: 2,
        error: 0,
        suppressed: 0,
    };
    report.verdict.reasons = vec!["low_coverage".to_string(), "deprecated_api".to_string()];
    // Correctly ordered
    report.findings = vec![
        Finding {
            severity: Severity::Warn,
            check_id: None,
            code: "W1".to_string(),
            message: "first warning".to_string(),
            location: Some(Location {
                path: Some("src/a.rs".to_string()),
                line: Some(1),
                col: None,
            }),
            help: Some("Fix the warning".to_string()),
            url: Some("https://example.com".to_string()),
            fingerprint: None,
            data: None,
        },
        Finding {
            severity: Severity::Warn,
            check_id: None,
            code: "W2".to_string(),
            message: "second warning".to_string(),
            location: Some(Location {
                path: Some("src/b.rs".to_string()),
                line: Some(10),
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
            message: "informational".to_string(),
            location: Some(Location {
                path: Some("src/c.rs".to_string()),
                line: Some(20),
                col: None,
            }),
            help: None,
            url: None,
            fingerprint: None,
            data: None,
        },
    ];
    report.artifacts = vec![ArtifactPointer {
        id: "report".to_string(),
        path: "artifacts/coverage/report.html".to_string(),
        mime: "text/html".to_string(),
        schema: None,
    }];

    let result = conform_single(&to_json(&report), "coverage-sensor", &all_checks())
        .expect("should not error");
    insta::assert_debug_snapshot!("conform_complex_clean_report", result);
}
