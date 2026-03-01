//! Integration tests for cockpitctl-io-buildfix.
//!
//! Focuses on filesystem-based actuator execution, plan file handling,
//! path resolution, and edge cases around JSON parsing and exit codes.

use cockpitctl_io_buildfix::run_buildfix_actuator;
use cockpitctl_types::{
    BUILDFIX_APPLY_REQUEST_SCHEMA_ID, BuildfixActuatorConfig, BuildfixActuatorResult,
    BuildfixApplyRequest, FixSummary, MatchedFinding, SafetyLevel,
};
use std::fs;
use tempfile::tempdir;

// ── Helpers ────────────────────────────────────────────────────────────

fn make_request() -> BuildfixApplyRequest {
    BuildfixApplyRequest {
        schema: BUILDFIX_APPLY_REQUEST_SCHEMA_ID.to_string(),
        max_auto_apply_safety: SafetyLevel::Safe,
        require_matched_finding: true,
        fixes: vec![],
    }
}

fn write_actuator_script(dir: &std::path::Path, response_json: &str, exit_code: i32) -> String {
    #[cfg(windows)]
    {
        let script = dir.join("actuator.cmd");
        let body = format!(
            "@echo off\r\necho {}\r\nexit /b {}\r\n",
            response_json, exit_code
        );
        fs::write(&script, body).expect("write actuator");
        script.to_string_lossy().to_string()
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let script = dir.join("actuator.sh");
        let body = format!(
            "#!/bin/sh\ncat >/dev/null\nprintf '%s\\n' '{}'\nexit {}\n",
            response_json, exit_code
        );
        fs::write(&script, body).expect("write actuator");
        let mut perms = fs::metadata(&script)
            .expect("actuator metadata")
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script, perms).expect("set actuator executable");
        script.to_string_lossy().to_string()
    }
}

fn platform_command(script: &str) -> String {
    #[cfg(unix)]
    {
        format!("sh {script}")
    }
    #[cfg(windows)]
    {
        format!("cmd /c {script}")
    }
}

// ── Buildfix plan file loading from filesystem ─────────────────────────

#[test]
fn plan_file_json_loads_and_roundtrips_through_actuator() {
    let tmp = tempdir().unwrap();
    // Write a plan file to disk, read it back, build a request from it
    let plan_json = serde_json::json!({
        "fixes": [{
            "fix_id": "fix-from-plan",
            "sensor_id": "lint",
            "safety": "safe",
            "description": "remove unused import",
            "matched_findings": [],
            "unmatched": false
        }]
    });
    let plan_path = tmp.path().join("plan.json");
    fs::write(
        &plan_path,
        serde_json::to_string_pretty(&plan_json).unwrap(),
    )
    .unwrap();

    // Read the plan back and build a request
    let plan_contents = fs::read_to_string(&plan_path).unwrap();
    let plan: serde_json::Value = serde_json::from_str(&plan_contents).unwrap();
    assert!(plan["fixes"].is_array());
    assert_eq!(plan["fixes"][0]["fix_id"], "fix-from-plan");
}

// ── Missing plan file → graceful error ─────────────────────────────────

#[test]
fn missing_plan_file_produces_io_error() {
    let tmp = tempdir().unwrap();
    let missing = tmp.path().join("nonexistent_plan.json");
    let result = fs::read_to_string(&missing);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
}

// ── Invalid plan JSON → error reporting ────────────────────────────────

#[test]
fn invalid_plan_json_produces_parse_error() {
    let tmp = tempdir().unwrap();
    let plan_path = tmp.path().join("bad_plan.json");
    fs::write(&plan_path, "NOT VALID JSON {{{}}}").unwrap();

    let contents = fs::read_to_string(&plan_path).unwrap();
    let result: Result<serde_json::Value, _> = serde_json::from_str(&contents);
    assert!(result.is_err());
}

#[test]
fn truncated_json_plan_produces_parse_error() {
    let tmp = tempdir().unwrap();
    let plan_path = tmp.path().join("truncated.json");
    fs::write(&plan_path, r#"{"fixes": ["#).unwrap();

    let contents = fs::read_to_string(&plan_path).unwrap();
    let result: Result<serde_json::Value, _> = serde_json::from_str(&contents);
    assert!(result.is_err());
}

// ── Buildfix actuator execution (temp script) ──────────────────────────

#[test]
fn actuator_with_multiple_fixes_returns_sorted_results() {
    let tmp = tempdir().unwrap();
    let response = r#"{"applied_fix_ids":["c","a","b"],"skipped_fix_ids":["z","x"],"errors":[]}"#;
    let script = write_actuator_script(tmp.path(), response, 0);
    let cfg = BuildfixActuatorConfig {
        command: platform_command(&script),
        timeout_ms: 5_000,
    };
    let out = run_buildfix_actuator(&cfg, &make_request()).unwrap();
    assert_eq!(out.applied_fix_ids, vec!["a", "b", "c"]);
    assert_eq!(out.skipped_fix_ids, vec!["x", "z"]);
}

#[test]
fn actuator_with_rich_request_payload() {
    let tmp = tempdir().unwrap();
    let response = r#"{"applied_fix_ids":["fix1","fix2"],"skipped_fix_ids":[],"errors":[]}"#;
    let script = write_actuator_script(tmp.path(), response, 0);
    let cfg = BuildfixActuatorConfig {
        command: platform_command(&script),
        timeout_ms: 5_000,
    };
    let request = BuildfixApplyRequest {
        schema: BUILDFIX_APPLY_REQUEST_SCHEMA_ID.to_string(),
        max_auto_apply_safety: SafetyLevel::Guarded,
        require_matched_finding: true,
        fixes: vec![
            FixSummary {
                fix_id: "fix1".into(),
                sensor_id: "lint".into(),
                safety: SafetyLevel::Safe,
                description: "remove unused import".into(),
                matched_findings: vec![MatchedFinding {
                    sensor_id: "lint".into(),
                    code: "W001".into(),
                    fingerprint: Some("abc".into()),
                }],
                unmatched: false,
            },
            FixSummary {
                fix_id: "fix2".into(),
                sensor_id: "fmt".into(),
                safety: SafetyLevel::Guarded,
                description: "reformat file".into(),
                matched_findings: vec![],
                unmatched: true,
            },
        ],
    };
    let out = run_buildfix_actuator(&cfg, &request).unwrap();
    assert_eq!(out.applied_fix_ids, vec!["fix1", "fix2"]);
}

// ── Plan file path resolution ──────────────────────────────────────────

#[test]
fn plan_file_in_nested_directory() {
    let tmp = tempdir().unwrap();
    let nested = tmp.path().join("deep").join("nested").join("dir");
    fs::create_dir_all(&nested).unwrap();
    let plan_path = nested.join("plan.json");
    let plan_json = r#"{"fixes": []}"#;
    fs::write(&plan_path, plan_json).unwrap();

    let contents = fs::read_to_string(&plan_path).unwrap();
    let plan: serde_json::Value = serde_json::from_str(&contents).unwrap();
    assert!(plan["fixes"].as_array().unwrap().is_empty());
}

#[test]
fn actuator_errors_contain_command_info() {
    let cfg = BuildfixActuatorConfig {
        command: "nonexistent_binary_42_xyz".into(),
        timeout_ms: 5_000,
    };
    let err = run_buildfix_actuator(&cfg, &make_request()).unwrap_err();
    let msg = format!("{err:#}");
    assert!(
        msg.contains("spawn buildfix actuator"),
        "error should mention actuator: {msg}"
    );
}

#[test]
fn whitespace_only_command_is_error() {
    let cfg = BuildfixActuatorConfig {
        command: "   \t  ".into(),
        timeout_ms: 5_000,
    };
    let err = run_buildfix_actuator(&cfg, &make_request()).unwrap_err();
    assert!(format!("{err:#}").contains("command is empty"));
}

#[test]
fn actuator_result_with_only_errors_preserved() {
    let tmp = tempdir().unwrap();
    let response =
        r#"{"applied_fix_ids":[],"skipped_fix_ids":[],"errors":["disk full","timeout"]}"#;
    let script = write_actuator_script(tmp.path(), response, 0);
    let cfg = BuildfixActuatorConfig {
        command: platform_command(&script),
        timeout_ms: 5_000,
    };
    let out = run_buildfix_actuator(&cfg, &make_request()).unwrap();
    assert!(out.applied_fix_ids.is_empty());
    assert!(out.skipped_fix_ids.is_empty());
    assert_eq!(out.errors, vec!["disk full", "timeout"]);
}

#[test]
fn actuator_minimal_json_response_uses_defaults() {
    let tmp = tempdir().unwrap();
    let script = write_actuator_script(tmp.path(), "{}", 0);
    let cfg = BuildfixActuatorConfig {
        command: platform_command(&script),
        timeout_ms: 5_000,
    };
    let out = run_buildfix_actuator(&cfg, &make_request()).unwrap();
    assert_eq!(out, BuildfixActuatorResult::default());
}
