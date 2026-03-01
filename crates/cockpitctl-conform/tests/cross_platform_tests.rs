//! Cross-platform conformance checking tests for cockpitctl-conform.
//!
//! These tests verify that path hygiene checks, sensor ID validation, and
//! conformance results are consistent regardless of the host OS.

use cockpitctl_conform::{
    ConformChecks, check_artifact_pointers, check_ordering, check_path_hygiene,
    check_sensor_id_format, conform_single,
};
use cockpitctl_types::*;
use std::collections::BTreeMap;

fn minimal_sensor_report() -> SensorReport {
    SensorReport {
        schema: "sensor.report.v1".to_string(),
        tool: ToolInfo {
            name: "xplat".to_string(),
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

fn make_finding_with_path(code: &str, path: &str) -> Finding {
    Finding {
        severity: Severity::Info,
        check_id: None,
        code: code.to_string(),
        message: format!("finding with path: {}", path),
        location: Some(Location {
            path: Some(path.to_string()),
            line: Some(1),
            col: None,
        }),
        help: None,
        url: None,
        fingerprint: None,
        data: None,
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 1. RECEIPT PATHS WITH BACKSLASHES — FLAGGED AS VIOLATIONS
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn backslash_paths_in_findings_flagged() {
    let mut report = minimal_sensor_report();
    report.findings = vec![
        make_finding_with_path("B1", "src\\lib.rs"),
        make_finding_with_path("B2", "crates\\io\\src\\lib.rs"),
        make_finding_with_path("B3", "C:\\Users\\dev\\project\\file.rs"),
    ];

    let violations = check_path_hygiene(&report);
    // All three should be flagged for backslash usage
    assert!(
        violations
            .iter()
            .filter(|v| v.contains("backslash"))
            .count()
            >= 3,
        "all backslash paths must be flagged: {:?}",
        violations
    );
    // The third should also be flagged for drive letter
    assert!(
        violations.iter().any(|v| v.contains("drive letter")),
        "drive letter path must be flagged: {:?}",
        violations
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// 2. RECEIPT PATHS WITH FORWARD SLASHES — ACCEPTED
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn forward_slash_relative_paths_accepted() {
    let mut report = minimal_sensor_report();
    report.findings = vec![
        make_finding_with_path("F1", "src/lib.rs"),
        make_finding_with_path("F2", "crates/io/src/lib.rs"),
        make_finding_with_path("F3", "tests/integration.rs"),
    ];

    let violations = check_path_hygiene(&report);
    assert!(
        violations.is_empty(),
        "relative forward-slash paths should pass hygiene: {:?}",
        violations
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// 3. MIXED PATH SEPARATORS IN FINDINGS — CONSISTENT DETECTION
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn mixed_separators_in_findings_detected() {
    let mut report = minimal_sensor_report();
    report.findings = vec![
        make_finding_with_path("M1", "src\\main.rs"),
        make_finding_with_path("M2", "src/lib.rs"),
        make_finding_with_path("M3", "crates\\io/src\\lib.rs"),
    ];

    let violations = check_path_hygiene(&report);
    // M1 and M3 should be flagged for backslash; M2 is clean
    let backslash_count = violations
        .iter()
        .filter(|v| v.contains("backslash"))
        .count();
    assert!(
        backslash_count >= 2,
        "expected at least 2 backslash violations, got {}: {:?}",
        backslash_count,
        violations
    );
}

#[test]
fn traversal_with_mixed_separators_detected() {
    let mut report = minimal_sensor_report();
    report.findings = vec![
        make_finding_with_path("T1", "foo/..\\bar"),
        make_finding_with_path("T2", "foo\\../bar"),
        make_finding_with_path("T3", "foo\\..\\bar"),
    ];

    let violations = check_path_hygiene(&report);
    // All should be flagged for both backslash and traversal
    let traversal_count = violations
        .iter()
        .filter(|v| v.contains("traversal"))
        .count();
    let backslash_count = violations
        .iter()
        .filter(|v| v.contains("backslash"))
        .count();
    assert!(
        traversal_count >= 3,
        "all mixed-separator traversals must be caught: {:?}",
        violations
    );
    assert!(
        backslash_count >= 3,
        "all backslashes must be flagged: {:?}",
        violations
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// 4. SENSOR ID VALIDATION — PLATFORM-INDEPENDENT
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn sensor_id_validation_platform_independent() {
    // Valid sensor IDs — must pass on all platforms
    let valid = ["builddiag", "my-sensor", "sensor_v2", "A", "a-B_C-123"];
    for id in &valid {
        assert!(
            check_sensor_id_format(id).is_empty(),
            "valid sensor_id {:?} must pass on all platforms",
            id
        );
    }

    // Invalid sensor IDs — must fail on all platforms
    let invalid = [
        "",
        "has.dot",
        "has space",
        "café",
        "../traversal",
        "foo/bar",
        "foo\\bar",
        "a\x00b",
    ];
    for id in &invalid {
        assert!(
            !check_sensor_id_format(id).is_empty(),
            "invalid sensor_id {:?} must fail on all platforms",
            id
        );
    }
}

#[test]
fn conform_single_with_backslash_paths_flags_hygiene() {
    let mut report = minimal_sensor_report();
    report.findings = vec![make_finding_with_path("W1", "src\\main.rs")];

    let json = serde_json::to_string(&report).unwrap();
    let result = conform_single(&json, "test-sensor", &all_checks()).unwrap();

    assert!(
        !result.is_pass(),
        "backslash path in findings should fail conformance"
    );
    assert!(
        result
            .violations
            .iter()
            .any(|v| v.message.contains("backslash")),
        "should have backslash violation: {:?}",
        result.violations
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// ARTIFACT POINTER PATH HYGIENE — PLATFORM-INDEPENDENT
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn artifact_pointer_backslash_and_traversal_detected() {
    let mut report = minimal_sensor_report();
    report.artifacts = vec![
        ArtifactPointer {
            id: "ok".to_string(),
            path: "output/results.json".to_string(),
            mime: "application/json".to_string(),
            schema: None,
        },
        ArtifactPointer {
            id: "backslash".to_string(),
            path: "output\\results.json".to_string(),
            mime: "application/json".to_string(),
            schema: None,
        },
        ArtifactPointer {
            id: "traversal".to_string(),
            path: "../escape/data.json".to_string(),
            mime: "application/json".to_string(),
            schema: None,
        },
        ArtifactPointer {
            id: "abs-win".to_string(),
            path: "D:\\data\\results.json".to_string(),
            mime: "application/json".to_string(),
            schema: None,
        },
    ];

    let violations = check_artifact_pointers(&report);
    // traversal and drive letter paths should be flagged
    assert!(
        violations.len() >= 2,
        "expected at least 2 artifact pointer violations, got: {:?}",
        violations
    );
    assert!(
        violations.iter().any(|v| v.contains("..")),
        "traversal must be detected: {:?}",
        violations
    );
    assert!(
        violations.iter().any(|v| v.contains("drive letter")),
        "drive letter must be detected: {:?}",
        violations
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// ORDERING CHECK — PATH FIELD IS PLATFORM-AGNOSTIC
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn ordering_check_treats_paths_as_opaque_strings() {
    let mut report = minimal_sensor_report();
    // Two findings with paths that differ only in separator style.
    // Ordering should compare them as plain strings.
    report.findings = vec![
        Finding {
            severity: Severity::Error,
            check_id: None,
            code: "E1".to_string(),
            message: "first".to_string(),
            location: Some(Location {
                path: Some("src/aaa.rs".to_string()),
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
            code: "E1".to_string(),
            message: "second".to_string(),
            location: Some(Location {
                path: Some("src/bbb.rs".to_string()),
                line: Some(1),
                col: None,
            }),
            help: None,
            url: None,
            fingerprint: None,
            data: None,
        },
    ];

    let violations = check_ordering(&report, "sensor");
    assert!(
        violations.is_empty(),
        "lexically ordered paths should pass ordering: {:?}",
        violations
    );
}
