//! Conformance tests for tokmd sensor receipts.
//!
//! Validates that the tokmd fixture receipt conforms to sensor.report.v1
//! and passes all conformance checks (path hygiene, ordering, etc.).

use cockpitctl_conform::{ConformChecks, conform_single};
use std::path::Path;

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

fn workspace_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
}

fn read_tokmd_fixture() -> String {
    let path = workspace_root()
        .join("fixtures")
        .join("tokmd_receipt")
        .join("artifacts")
        .join("tokmd")
        .join("report.json");
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read tokmd fixture at {}: {e}", path.display()))
}

#[test]
fn tokmd_fixture_passes_all_conformance_checks() {
    let json = read_tokmd_fixture();
    let result = conform_single(&json, "tokmd", &all_checks()).unwrap();
    assert!(
        result.is_pass(),
        "tokmd fixture should pass all conformance checks, violations: {:?}",
        result.violations
    );
}

#[test]
fn tokmd_fixture_passes_path_hygiene() {
    let json = read_tokmd_fixture();
    let checks = ConformChecks {
        path_hygiene: true,
        ordering: false,
        reason_lint: false,
        survivability: false,
        tool_error_identity: false,
        sensor_id_format: false,
        artifact_pointers: false,
    };
    let result = conform_single(&json, "tokmd", &checks).unwrap();
    assert!(
        result.is_pass(),
        "tokmd fixture paths should be clean: {:?}",
        result.violations
    );
}

#[test]
fn tokmd_fixture_passes_ordering_check() {
    let json = read_tokmd_fixture();
    let checks = ConformChecks {
        path_hygiene: false,
        ordering: true,
        reason_lint: false,
        survivability: false,
        tool_error_identity: false,
        sensor_id_format: false,
        artifact_pointers: false,
    };
    let result = conform_single(&json, "tokmd", &checks).unwrap();
    assert!(
        result.is_pass(),
        "tokmd fixture findings should be correctly ordered: {:?}",
        result.violations
    );
}

#[test]
fn tokmd_fixture_passes_sensor_id_format() {
    let json = read_tokmd_fixture();
    let checks = ConformChecks {
        path_hygiene: false,
        ordering: false,
        reason_lint: false,
        survivability: false,
        tool_error_identity: false,
        sensor_id_format: true,
        artifact_pointers: false,
    };
    let result = conform_single(&json, "tokmd", &checks).unwrap();
    assert!(
        result.is_pass(),
        "tokmd sensor ID should be valid: {:?}",
        result.violations
    );
}

#[test]
fn tokmd_fixture_is_valid_json_and_has_correct_schema() {
    let json = read_tokmd_fixture();
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
    assert_eq!(
        parsed["schema"].as_str(),
        Some("sensor.report.v1"),
        "tokmd receipt must declare sensor.report.v1 schema"
    );
    assert_eq!(
        parsed["tool"]["name"].as_str(),
        Some("tokmd"),
        "tool name must be tokmd"
    );
}

#[test]
fn tokmd_fixture_verdict_counts_match_findings() {
    let json = read_tokmd_fixture();
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
    let findings = parsed["findings"].as_array().expect("findings array");
    let counts = &parsed["verdict"]["counts"];

    let info_count = findings
        .iter()
        .filter(|f| f["severity"].as_str() == Some("info"))
        .count();
    let warn_count = findings
        .iter()
        .filter(|f| f["severity"].as_str() == Some("warn"))
        .count();
    let error_count = findings
        .iter()
        .filter(|f| f["severity"].as_str() == Some("error"))
        .count();

    assert_eq!(
        counts["info"].as_u64(),
        Some(info_count as u64),
        "info count mismatch"
    );
    assert_eq!(
        counts["warn"].as_u64(),
        Some(warn_count as u64),
        "warn count mismatch"
    );
    assert_eq!(
        counts["error"].as_u64(),
        Some(error_count as u64),
        "error count mismatch"
    );
}
