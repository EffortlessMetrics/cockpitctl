//! Edge-case tests for the buildfix actuator adapter.
//!
//! Covers stderr passthrough, large payloads, and boundary conditions
//! not exercised by the primary adapter or integration test suites.

use cockpitctl_io_buildfix::run_buildfix_actuator;
use cockpitctl_types::{
    BUILDFIX_APPLY_REQUEST_SCHEMA_ID, BuildfixActuatorConfig, BuildfixApplyRequest, SafetyLevel,
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

fn write_script(dir: &std::path::Path, name: &str, body: &str) -> String {
    let script = dir.join(name);
    fs::write(&script, body).expect("write script");
    script.to_string_lossy().to_string()
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

// ── Stderr output does not prevent success ─────────────────────────────

#[test]
fn stderr_output_does_not_prevent_success() {
    let tmp = tempdir().unwrap();
    #[cfg(windows)]
    let script = {
        let body = "@echo off\r\necho warning on stderr >&2\r\necho {\"applied_fix_ids\":[\"fix1\"],\"skipped_fix_ids\":[],\"errors\":[]}\r\nexit /b 0\r\n";
        write_script(tmp.path(), "stderr_ok.cmd", body)
    };
    #[cfg(unix)]
    let script = {
        use std::os::unix::fs::PermissionsExt;
        let body = "#!/bin/sh\ncat >/dev/null\necho 'warning on stderr' >&2\nprintf '{\"applied_fix_ids\":[\"fix1\"],\"skipped_fix_ids\":[],\"errors\":[]}\\n'\n";
        let p = write_script(tmp.path(), "stderr_ok.sh", body);
        let mut perms = fs::metadata(tmp.path().join("stderr_ok.sh"))
            .unwrap()
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(tmp.path().join("stderr_ok.sh"), perms).unwrap();
        p
    };
    let cfg = BuildfixActuatorConfig {
        command: platform_command(&script),
        timeout_ms: 5_000,
    };
    let out = run_buildfix_actuator(&cfg, &make_request()).unwrap();
    assert_eq!(out.applied_fix_ids, vec!["fix1"]);
}

// ── Large response payload parsed correctly ────────────────────────────

#[test]
fn large_response_payload_parsed_correctly() {
    let tmp = tempdir().unwrap();
    // Generate a response with many fix IDs to exercise parsing of larger payloads.
    let ids: Vec<String> = (0..100).map(|i| format!("fix_{:04}", i)).collect();
    let ids_json: Vec<String> = ids.iter().map(|id| format!("\"{}\"", id)).collect();
    let response = format!(
        "{{\"applied_fix_ids\":[{}],\"skipped_fix_ids\":[],\"errors\":[]}}",
        ids_json.join(",")
    );

    #[cfg(windows)]
    let script = {
        let body = format!("@echo off\r\necho {}\r\nexit /b 0\r\n", response);
        write_script(tmp.path(), "large.cmd", &body)
    };
    #[cfg(unix)]
    let script = {
        use std::os::unix::fs::PermissionsExt;
        let body = format!("#!/bin/sh\ncat >/dev/null\nprintf '%s\\n' '{}'\n", response);
        let p = write_script(tmp.path(), "large.sh", &body);
        let mut perms = fs::metadata(tmp.path().join("large.sh"))
            .unwrap()
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(tmp.path().join("large.sh"), perms).unwrap();
        p
    };
    let cfg = BuildfixActuatorConfig {
        command: platform_command(&script),
        timeout_ms: 10_000,
    };
    let out = run_buildfix_actuator(&cfg, &make_request()).unwrap();
    // All 100 IDs should be present and sorted
    assert_eq!(out.applied_fix_ids.len(), 100);
    assert_eq!(out.applied_fix_ids[0], "fix_0000");
    assert_eq!(out.applied_fix_ids[99], "fix_0099");
}

// ── Timeout just barely sufficient succeeds ────────────────────────────

#[test]
fn fast_script_with_generous_timeout_succeeds() {
    let tmp = tempdir().unwrap();
    let response = r#"{"applied_fix_ids":["ok"],"skipped_fix_ids":[],"errors":[]}"#;
    #[cfg(windows)]
    let script = {
        let body = format!("@echo off\r\necho {}\r\nexit /b 0\r\n", response);
        write_script(tmp.path(), "fast.cmd", &body)
    };
    #[cfg(unix)]
    let script = {
        use std::os::unix::fs::PermissionsExt;
        let body = format!("#!/bin/sh\ncat >/dev/null\nprintf '%s\\n' '{}'\n", response);
        let p = write_script(tmp.path(), "fast.sh", &body);
        let mut perms = fs::metadata(tmp.path().join("fast.sh"))
            .unwrap()
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(tmp.path().join("fast.sh"), perms).unwrap();
        p
    };
    // Script is instant, timeout is 1 second — should succeed
    let cfg = BuildfixActuatorConfig {
        command: platform_command(&script),
        timeout_ms: 1_000,
    };
    let out = run_buildfix_actuator(&cfg, &make_request()).unwrap();
    assert_eq!(out.applied_fix_ids, vec!["ok"]);
}

// ── Extra JSON fields are tolerated (serde default) ────────────────────

#[test]
fn extra_json_fields_in_response_tolerated() {
    let tmp = tempdir().unwrap();
    let response =
        r#"{"applied_fix_ids":["a"],"skipped_fix_ids":[],"errors":[],"extra_field":"ignored"}"#;
    #[cfg(windows)]
    let script = {
        let body = format!("@echo off\r\necho {}\r\nexit /b 0\r\n", response);
        write_script(tmp.path(), "extra.cmd", &body)
    };
    #[cfg(unix)]
    let script = {
        use std::os::unix::fs::PermissionsExt;
        let body = format!("#!/bin/sh\ncat >/dev/null\nprintf '%s\\n' '{}'\n", response);
        let p = write_script(tmp.path(), "extra.sh", &body);
        let mut perms = fs::metadata(tmp.path().join("extra.sh"))
            .unwrap()
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(tmp.path().join("extra.sh"), perms).unwrap();
        p
    };
    let cfg = BuildfixActuatorConfig {
        command: platform_command(&script),
        timeout_ms: 5_000,
    };
    // Extra fields in response JSON should not cause failure (serde deny_unknown_fields not set)
    let out = run_buildfix_actuator(&cfg, &make_request()).unwrap();
    assert_eq!(out.applied_fix_ids, vec!["a"]);
}
