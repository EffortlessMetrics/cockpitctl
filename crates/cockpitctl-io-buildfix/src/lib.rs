//! Buildfix actuator adapter extracted from `cockpitctl-io`.
//!
//! Runs an external command to apply selected fixes, passing the request
//! as JSON on stdin and reading the result from stdout.

use anyhow::{Context, Result};
use cockpitctl_types::{BuildfixActuatorConfig, BuildfixActuatorResult, BuildfixApplyRequest};
use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::time::Duration;
use wait_timeout::ChildExt;

/// Run the configured buildfix actuator command.
///
/// The request is written as JSON to stdin. The command may emit JSON on stdout
/// matching `BuildfixActuatorResult`. Empty stdout is treated as a successful
/// no-op with empty result arrays.
pub fn run_buildfix_actuator(
    actuator: &BuildfixActuatorConfig,
    request: &BuildfixApplyRequest,
) -> Result<BuildfixActuatorResult> {
    let parts: Vec<&str> = actuator.command.split_whitespace().collect();
    if parts.is_empty() {
        anyhow::bail!("buildfix actuator command is empty");
    }

    let payload =
        serde_json::to_vec(request).context("serialize buildfix apply request for actuator")?;

    let mut child = Command::new(parts[0])
        .args(&parts[1..])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("spawn buildfix actuator `{}`", actuator.command))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(&payload)
            .context("write buildfix apply request to actuator stdin")?;
    }

    let mut stdout_handle = child.stdout.take();
    let mut stderr_handle = child.stderr.take();

    let stdout_thread = std::thread::spawn(move || -> Vec<u8> {
        let mut buf = Vec::new();
        if let Some(ref mut h) = stdout_handle {
            let _ = h.read_to_end(&mut buf);
        }
        buf
    });

    let stderr_thread = std::thread::spawn(move || -> Vec<u8> {
        let mut buf = Vec::new();
        if let Some(ref mut h) = stderr_handle {
            let _ = h.read_to_end(&mut buf);
        }
        buf
    });

    let timeout = Duration::from_millis(actuator.timeout_ms);
    let status = match child
        .wait_timeout(timeout)
        .context("wait for buildfix actuator")?
    {
        Some(status) => status,
        None => {
            let _ = child.kill();
            let _ = child.wait();
            anyhow::bail!(
                "buildfix actuator timed out after {}ms",
                actuator.timeout_ms
            );
        }
    };

    let stdout_bytes = stdout_thread.join().unwrap_or_default();
    let stderr_bytes = stderr_thread.join().unwrap_or_default();
    let stderr = String::from_utf8_lossy(&stderr_bytes);
    if !stderr.is_empty() {
        eprintln!("cockpitctl: buildfix actuator stderr: {}", stderr.trim());
    }

    if !status.success() {
        anyhow::bail!("buildfix actuator exited with status {}", status);
    }

    if stdout_bytes.is_empty() {
        return Ok(BuildfixActuatorResult::default());
    }

    let mut result: BuildfixActuatorResult =
        serde_json::from_slice(&stdout_bytes).context("parse buildfix actuator output JSON")?;

    result.applied_fix_ids.sort();
    result.applied_fix_ids.dedup();
    result.skipped_fix_ids.sort();
    result.skipped_fix_ids.dedup();
    result.errors.sort();
    result.errors.dedup();

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cockpitctl_types::{FixSummary, MatchedFinding};
    use std::fs;
    use std::path::Path;
    use tempfile::tempdir;

    fn make_request() -> BuildfixApplyRequest {
        BuildfixApplyRequest {
            schema: cockpitctl_types::BUILDFIX_APPLY_REQUEST_SCHEMA_ID.to_string(),
            max_auto_apply_safety: cockpitctl_types::SafetyLevel::Safe,
            require_matched_finding: true,
            fixes: vec![],
        }
    }

    fn write_actuator_script(temp: &Path, response_json: &str, exit_code: i32) -> String {
        #[cfg(windows)]
        {
            let script = temp.join("actuator.cmd");
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
            let script = temp.join("actuator.sh");
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

    /// Helper: build platform command string to run a script file.
    fn platform_command(script: &str) -> String {
        #[cfg(unix)]
        {
            format!("sh {}", script)
        }
        #[cfg(windows)]
        {
            format!("cmd /c {}", script)
        }
    }

    // ── empty command ──────────────────────────────────────────────

    #[test]
    fn empty_command_is_error() {
        let actuator = BuildfixActuatorConfig {
            command: String::new(),
            timeout_ms: 5_000,
        };
        let err =
            run_buildfix_actuator(&actuator, &make_request()).expect_err("expected empty error");
        assert!(
            format!("{:#}", err).contains("command is empty"),
            "unexpected: {err:#}"
        );
    }

    // ── successful parse ───────────────────────────────────────────

    #[test]
    fn run_buildfix_actuator_parses_json_response() {
        let temp = tempdir().expect("tempdir");
        let response =
            r#"{"applied_fix_ids":["fix_b","fix_a"],"skipped_fix_ids":["fix_c"],"errors":[]}"#;
        let script = write_actuator_script(temp.path(), response, 0);
        let command = platform_command(&script);

        let actuator = BuildfixActuatorConfig {
            command,
            timeout_ms: 5_000,
        };
        let out = run_buildfix_actuator(&actuator, &make_request()).expect("run actuator");
        assert_eq!(out.applied_fix_ids, vec!["fix_a", "fix_b"]);
        assert_eq!(out.skipped_fix_ids, vec!["fix_c"]);
        assert!(out.errors.is_empty());
    }

    // ── non-zero exit ──────────────────────────────────────────────

    #[test]
    fn run_buildfix_actuator_nonzero_exit_is_error() {
        let temp = tempdir().expect("tempdir");
        let script = write_actuator_script(temp.path(), "{}", 9);
        let command = platform_command(&script);

        let actuator = BuildfixActuatorConfig {
            command,
            timeout_ms: 5_000,
        };

        let err =
            run_buildfix_actuator(&actuator, &make_request()).expect_err("expected nonzero exit");
        assert!(format!("{:#}", err).contains("buildfix actuator exited"));
    }

    // ── empty stdout returns default ───────────────────────────────

    #[test]
    fn empty_stdout_returns_default_result() {
        let temp = tempdir().expect("tempdir");
        // Script that produces no stdout.
        #[cfg(windows)]
        let script = {
            let p = temp.path().join("empty.cmd");
            fs::write(&p, "@echo off\r\nexit /b 0\r\n").unwrap();
            p.to_string_lossy().to_string()
        };
        #[cfg(unix)]
        let script = {
            use std::os::unix::fs::PermissionsExt;
            let p = temp.path().join("empty.sh");
            fs::write(&p, "#!/bin/sh\ncat >/dev/null\n").unwrap();
            let mut perms = fs::metadata(&p).unwrap().permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&p, perms).unwrap();
            p.to_string_lossy().to_string()
        };
        let command = platform_command(&script);

        let actuator = BuildfixActuatorConfig {
            command,
            timeout_ms: 5_000,
        };
        let out = run_buildfix_actuator(&actuator, &make_request()).expect("run actuator");
        assert_eq!(out, BuildfixActuatorResult::default());
        assert!(out.applied_fix_ids.is_empty());
        assert!(out.skipped_fix_ids.is_empty());
        assert!(out.errors.is_empty());
    }

    // ── invalid JSON stdout ────────────────────────────────────────

    #[test]
    fn invalid_json_stdout_is_error() {
        let temp = tempdir().expect("tempdir");
        let script = write_actuator_script(temp.path(), "NOT-JSON!!!", 0);
        let command = platform_command(&script);

        let actuator = BuildfixActuatorConfig {
            command,
            timeout_ms: 5_000,
        };
        let err = run_buildfix_actuator(&actuator, &make_request())
            .expect_err("expected JSON parse error");
        assert!(
            format!("{:#}", err).contains("parse buildfix actuator output JSON"),
            "unexpected: {err:#}"
        );
    }

    // ── dedup and sort ─────────────────────────────────────────────

    #[test]
    fn result_dedup_and_sort() {
        let temp = tempdir().expect("tempdir");
        let response = r#"{"applied_fix_ids":["z","a","z","m"],"skipped_fix_ids":["b","b","a"],"errors":["err2","err1","err2"]}"#;
        let script = write_actuator_script(temp.path(), response, 0);
        let command = platform_command(&script);

        let actuator = BuildfixActuatorConfig {
            command,
            timeout_ms: 5_000,
        };
        let out = run_buildfix_actuator(&actuator, &make_request()).expect("run actuator");
        assert_eq!(out.applied_fix_ids, vec!["a", "m", "z"]);
        assert_eq!(out.skipped_fix_ids, vec!["a", "b"]);
        assert_eq!(out.errors, vec!["err1", "err2"]);
    }

    // ── missing/non-existent command ───────────────────────────────

    #[test]
    fn nonexistent_command_is_error() {
        let actuator = BuildfixActuatorConfig {
            command: "totally_nonexistent_binary_12345".to_string(),
            timeout_ms: 5_000,
        };
        let err =
            run_buildfix_actuator(&actuator, &make_request()).expect_err("expected spawn error");
        assert!(
            format!("{:#}", err).contains("spawn buildfix actuator"),
            "unexpected: {err:#}"
        );
    }

    // ── timeout ────────────────────────────────────────────────────

    #[test]
    fn actuator_timeout_is_error() {
        let temp = tempdir().expect("tempdir");
        // Script that sleeps longer than the timeout.
        #[cfg(windows)]
        let script = {
            let p = temp.path().join("slow.cmd");
            // ping localhost 10 times ≈ 10 seconds
            fs::write(&p, "@echo off\r\nping -n 10 127.0.0.1 > nul\r\n").unwrap();
            p.to_string_lossy().to_string()
        };
        #[cfg(unix)]
        let script = {
            use std::os::unix::fs::PermissionsExt;
            let p = temp.path().join("slow.sh");
            fs::write(&p, "#!/bin/sh\nsleep 60\n").unwrap();
            let mut perms = fs::metadata(&p).unwrap().permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&p, perms).unwrap();
            p.to_string_lossy().to_string()
        };
        let command = platform_command(&script);

        let actuator = BuildfixActuatorConfig {
            command,
            timeout_ms: 200, // very short timeout
        };
        let err =
            run_buildfix_actuator(&actuator, &make_request()).expect_err("expected timeout error");
        assert!(
            format!("{:#}", err).contains("timed out"),
            "unexpected: {err:#}"
        );
    }

    // ── request with fixes is serialized correctly ─────────────────

    #[test]
    fn request_with_fixes_serializes_to_actuator() {
        let temp = tempdir().expect("tempdir");
        // Script that echoes empty result; we just verify the call succeeds.
        let response = r#"{"applied_fix_ids":["fix1"],"skipped_fix_ids":[],"errors":[]}"#;
        let script = write_actuator_script(temp.path(), response, 0);
        let command = platform_command(&script);

        let actuator = BuildfixActuatorConfig {
            command,
            timeout_ms: 5_000,
        };
        let request = BuildfixApplyRequest {
            schema: cockpitctl_types::BUILDFIX_APPLY_REQUEST_SCHEMA_ID.to_string(),
            max_auto_apply_safety: cockpitctl_types::SafetyLevel::Guarded,
            require_matched_finding: false,
            fixes: vec![FixSummary {
                fix_id: "fix1".into(),
                sensor_id: "lint".into(),
                safety: cockpitctl_types::SafetyLevel::Safe,
                description: "remove unused import".into(),
                matched_findings: vec![MatchedFinding {
                    sensor_id: "lint".into(),
                    code: "W001".into(),
                    fingerprint: Some("abc123".into()),
                }],
                unmatched: false,
            }],
        };

        let out = run_buildfix_actuator(&actuator, &request).expect("run actuator");
        assert_eq!(out.applied_fix_ids, vec!["fix1"]);
    }

    // ── only errors in result ──────────────────────────────────────

    #[test]
    fn result_with_only_errors() {
        let temp = tempdir().expect("tempdir");
        let response = r#"{"applied_fix_ids":[],"skipped_fix_ids":[],"errors":["disk full","permission denied"]}"#;
        let script = write_actuator_script(temp.path(), response, 0);
        let command = platform_command(&script);

        let actuator = BuildfixActuatorConfig {
            command,
            timeout_ms: 5_000,
        };
        let out = run_buildfix_actuator(&actuator, &make_request()).expect("run actuator");
        assert!(out.applied_fix_ids.is_empty());
        assert!(out.skipped_fix_ids.is_empty());
        assert_eq!(out.errors, vec!["disk full", "permission denied"]);
    }

    // ── minimal JSON (missing optional fields) ─────────────────────

    #[test]
    fn minimal_json_uses_serde_defaults() {
        let temp = tempdir().expect("tempdir");
        // Only `{}` — all fields have serde(default).
        let script = write_actuator_script(temp.path(), "{}", 0);
        let command = platform_command(&script);

        let actuator = BuildfixActuatorConfig {
            command,
            timeout_ms: 5_000,
        };
        let out = run_buildfix_actuator(&actuator, &make_request()).expect("run actuator");
        assert_eq!(out, BuildfixActuatorResult::default());
    }

    // ── whitespace-only command ────────────────────────────────────

    #[test]
    fn whitespace_only_command_is_error() {
        let actuator = BuildfixActuatorConfig {
            command: "   ".to_string(),
            timeout_ms: 5_000,
        };
        let err =
            run_buildfix_actuator(&actuator, &make_request()).expect_err("expected empty error");
        assert!(
            format!("{:#}", err).contains("command is empty"),
            "unexpected: {err:#}"
        );
    }

    // ── command with multiple args ─────────────────────────────────

    #[test]
    fn command_string_is_split_on_whitespace() {
        let temp = tempdir().expect("tempdir");
        let response = r#"{"applied_fix_ids":["ok"],"skipped_fix_ids":[],"errors":[]}"#;
        let script = write_actuator_script(temp.path(), response, 0);

        // On Windows: "cmd /c <script>" already has 3 parts.
        // On Unix: "sh <script>" has 2 parts.
        // Either way the split_whitespace logic must handle multi-arg commands.
        let command = platform_command(&script);
        assert!(
            command.split_whitespace().count() >= 2,
            "test assumes multi-part command"
        );

        let actuator = BuildfixActuatorConfig {
            command,
            timeout_ms: 5_000,
        };
        let out = run_buildfix_actuator(&actuator, &make_request()).expect("run actuator");
        assert_eq!(out.applied_fix_ids, vec!["ok"]);
    }
}
