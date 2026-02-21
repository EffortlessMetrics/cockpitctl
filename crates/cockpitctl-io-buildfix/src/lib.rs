//! Buildfix actuator adapter extracted from `cockpitctl-io`.

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
    use std::fs;
    use std::path::Path;
    use tempfile::tempdir;

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

    #[test]
    fn run_buildfix_actuator_parses_json_response() {
        let temp = tempdir().expect("tempdir");
        let response =
            r#"{"applied_fix_ids":["fix_b","fix_a"],"skipped_fix_ids":["fix_c"],"errors":[]}"#;
        let script = write_actuator_script(temp.path(), response, 0);
        let command = {
            #[cfg(unix)]
            {
                format!("sh {}", script)
            }
            #[cfg(windows)]
            {
                format!("cmd /c {}", script)
            }
        };

        let actuator = BuildfixActuatorConfig {
            command,
            timeout_ms: 5_000,
        };
        let request = BuildfixApplyRequest {
            schema: cockpitctl_types::BUILDFIX_APPLY_REQUEST_SCHEMA_ID.to_string(),
            max_auto_apply_safety: cockpitctl_types::SafetyLevel::Safe,
            require_matched_finding: true,
            fixes: vec![],
        };
        let out = run_buildfix_actuator(&actuator, &request).expect("run actuator");
        assert_eq!(out.applied_fix_ids, vec!["fix_a", "fix_b"]);
        assert_eq!(out.skipped_fix_ids, vec!["fix_c"]);
        assert!(out.errors.is_empty());
    }

    #[test]
    fn run_buildfix_actuator_nonzero_exit_is_error() {
        let temp = tempdir().expect("tempdir");
        let script = write_actuator_script(temp.path(), "{}", 9);
        let command = {
            #[cfg(unix)]
            {
                format!("sh {}", script)
            }
            #[cfg(windows)]
            {
                format!("cmd /c {}", script)
            }
        };

        let actuator = BuildfixActuatorConfig {
            command,
            timeout_ms: 5_000,
        };
        let request = BuildfixApplyRequest {
            schema: cockpitctl_types::BUILDFIX_APPLY_REQUEST_SCHEMA_ID.to_string(),
            max_auto_apply_safety: cockpitctl_types::SafetyLevel::Safe,
            require_matched_finding: true,
            fixes: vec![],
        };

        let err = run_buildfix_actuator(&actuator, &request).expect_err("expected nonzero exit");
        assert!(format!("{:#}", err).contains("buildfix actuator exited"));
    }
}
