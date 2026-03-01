//! Integration tests for the buildfix actuator adapter.
//!
//! These exercise the public API (`run_buildfix_actuator`) from the outside,
//! verifying error handling, timeout behaviour, and result parsing.

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

// ── Missing / invalid command ──────────────────────────────────────────

#[test]
fn missing_command_returns_error() {
    let cfg = BuildfixActuatorConfig {
        command: "totally_nonexistent_binary_xyz_42".into(),
        timeout_ms: 5_000,
    };
    let err = run_buildfix_actuator(&cfg, &make_request()).unwrap_err();
    assert!(
        format!("{err:#}").contains("spawn buildfix actuator"),
        "unexpected: {err:#}"
    );
}

#[test]
fn empty_command_returns_error() {
    let cfg = BuildfixActuatorConfig {
        command: String::new(),
        timeout_ms: 5_000,
    };
    let err = run_buildfix_actuator(&cfg, &make_request()).unwrap_err();
    assert!(
        format!("{err:#}").contains("command is empty"),
        "unexpected: {err:#}"
    );
}

// ── Timeout behaviour ──────────────────────────────────────────────────

#[test]
fn actuator_timeout_produces_error() {
    let tmp = tempdir().unwrap();
    #[cfg(windows)]
    let script = {
        let p = tmp.path().join("slow.cmd");
        fs::write(&p, "@echo off\r\nping -n 10 127.0.0.1 > nul\r\n").unwrap();
        p.to_string_lossy().to_string()
    };
    #[cfg(unix)]
    let script = {
        use std::os::unix::fs::PermissionsExt;
        let p = tmp.path().join("slow.sh");
        fs::write(&p, "#!/bin/sh\nsleep 60\n").unwrap();
        let mut perms = fs::metadata(&p).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&p, perms).unwrap();
        p.to_string_lossy().to_string()
    };
    let cfg = BuildfixActuatorConfig {
        command: platform_command(&script),
        timeout_ms: 200,
    };
    let err = run_buildfix_actuator(&cfg, &make_request()).unwrap_err();
    assert!(
        format!("{err:#}").contains("timed out"),
        "unexpected: {err:#}"
    );
}

// ── Successful run (valid exit code) ───────────────────────────────────

#[test]
fn valid_exit_code_success() {
    let tmp = tempdir().unwrap();
    let response =
        r#"{"applied_fix_ids":["fix_b","fix_a"],"skipped_fix_ids":["fix_c"],"errors":[]}"#;
    let script = write_actuator_script(tmp.path(), response, 0);
    let cfg = BuildfixActuatorConfig {
        command: platform_command(&script),
        timeout_ms: 5_000,
    };
    let out = run_buildfix_actuator(&cfg, &make_request()).unwrap();
    // Results are sorted and deduped
    assert_eq!(out.applied_fix_ids, vec!["fix_a", "fix_b"]);
    assert_eq!(out.skipped_fix_ids, vec!["fix_c"]);
    assert!(out.errors.is_empty());
}

#[test]
fn nonzero_exit_code_is_error() {
    let tmp = tempdir().unwrap();
    let script = write_actuator_script(tmp.path(), "{}", 9);
    let cfg = BuildfixActuatorConfig {
        command: platform_command(&script),
        timeout_ms: 5_000,
    };
    let err = run_buildfix_actuator(&cfg, &make_request()).unwrap_err();
    assert!(format!("{err:#}").contains("buildfix actuator exited"));
}

// ── Buildfix plan parsing from JSON ────────────────────────────────────

#[test]
fn request_with_fixes_roundtrips_through_actuator() {
    let tmp = tempdir().unwrap();
    let response = r#"{"applied_fix_ids":["fix1"],"skipped_fix_ids":[],"errors":[]}"#;
    let script = write_actuator_script(tmp.path(), response, 0);
    let cfg = BuildfixActuatorConfig {
        command: platform_command(&script),
        timeout_ms: 5_000,
    };
    let request = BuildfixApplyRequest {
        schema: BUILDFIX_APPLY_REQUEST_SCHEMA_ID.to_string(),
        max_auto_apply_safety: SafetyLevel::Guarded,
        require_matched_finding: false,
        fixes: vec![FixSummary {
            fix_id: "fix1".into(),
            sensor_id: "lint".into(),
            safety: SafetyLevel::Safe,
            description: "remove unused import".into(),
            matched_findings: vec![MatchedFinding {
                sensor_id: "lint".into(),
                code: "W001".into(),
                fingerprint: Some("abc123".into()),
            }],
            unmatched: false,
        }],
    };
    let out = run_buildfix_actuator(&cfg, &request).unwrap();
    assert_eq!(out.applied_fix_ids, vec!["fix1"]);
}

#[test]
fn empty_stdout_returns_default() {
    let tmp = tempdir().unwrap();
    #[cfg(windows)]
    let script = {
        let p = tmp.path().join("empty.cmd");
        fs::write(&p, "@echo off\r\nexit /b 0\r\n").unwrap();
        p.to_string_lossy().to_string()
    };
    #[cfg(unix)]
    let script = {
        use std::os::unix::fs::PermissionsExt;
        let p = tmp.path().join("empty.sh");
        fs::write(&p, "#!/bin/sh\ncat >/dev/null\n").unwrap();
        let mut perms = fs::metadata(&p).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&p, perms).unwrap();
        p.to_string_lossy().to_string()
    };
    let cfg = BuildfixActuatorConfig {
        command: platform_command(&script),
        timeout_ms: 5_000,
    };
    let out = run_buildfix_actuator(&cfg, &make_request()).unwrap();
    assert_eq!(out, BuildfixActuatorResult::default());
}

#[test]
fn invalid_json_stdout_is_error() {
    let tmp = tempdir().unwrap();
    let script = write_actuator_script(tmp.path(), "NOT-JSON!!!", 0);
    let cfg = BuildfixActuatorConfig {
        command: platform_command(&script),
        timeout_ms: 5_000,
    };
    let err = run_buildfix_actuator(&cfg, &make_request()).unwrap_err();
    assert!(
        format!("{err:#}").contains("parse buildfix actuator output JSON"),
        "unexpected: {err:#}"
    );
}

#[test]
fn result_snapshot_dedup_and_sort() {
    let tmp = tempdir().unwrap();
    let response = r#"{"applied_fix_ids":["z","a","z","m"],"skipped_fix_ids":["b","b","a"],"errors":["err2","err1","err2"]}"#;
    let script = write_actuator_script(tmp.path(), response, 0);
    let cfg = BuildfixActuatorConfig {
        command: platform_command(&script),
        timeout_ms: 5_000,
    };
    let out = run_buildfix_actuator(&cfg, &make_request()).unwrap();
    insta::assert_json_snapshot!(
        "dedup_and_sort",
        serde_json::json!({
            "applied_fix_ids": out.applied_fix_ids,
            "skipped_fix_ids": out.skipped_fix_ids,
            "errors": out.errors,
        })
    );
}
