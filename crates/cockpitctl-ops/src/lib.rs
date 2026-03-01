use anyhow::{Context, Result};
use cockpitctl_ingest::OutputSink;
use cockpitctl_types::{
    BuildfixActuatorConfig, BuildfixActuatorResult, BuildfixApplyRequest, PolicySigningConfig,
};
use std::fs;

#[derive(Debug, Clone, serde::Deserialize)]
pub struct PostProcessOutput {
    #[serde(default)]
    pub comment_sections: Vec<CommentSection>,
    #[serde(default)]
    pub files: Vec<OutputFile>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct CommentSection {
    pub name: String,
    #[serde(default)]
    pub order: i64,
    #[serde(default, alias = "markdown")]
    pub content: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct OutputFile {
    pub name: String,
    #[serde(deserialize_with = "base64_bytes::deserialize")]
    pub content: Vec<u8>,
}

mod base64_bytes {
    use base64::Engine;
    use serde::{Deserialize, Deserializer};

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        match base64_decode(&s) {
            Some(bytes) => Ok(bytes),
            None => Ok(s.into_bytes()),
        }
    }

    fn base64_decode(s: &str) -> Option<Vec<u8>> {
        let s = s.trim();
        if s.is_empty() {
            return Some(Vec::new());
        }
        if s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'+' || b == b'/' || b == b'=')
        {
            base64::engine::general_purpose::STANDARD.decode(s).ok()
        } else {
            None
        }
    }
}

pub fn run_hooks(
    hooks: &[cockpitctl_types::HookConfig],
    report_json: &str,
    output_sink: &impl OutputSink,
) -> Result<Vec<CommentSection>> {
    let mut all_sections = Vec::new();

    for hook in hooks {
        match run_single_hook(hook, report_json) {
            Ok(output) => {
                for file in &output.files {
                    output_sink.write_extra_file(&file.name, &file.content)?;
                }
                all_sections.extend(output.comment_sections);
            }
            Err(e) => {
                eprintln!("cockpitctl: hook `{}` failed: {:#}", hook.name, e);
            }
        }
    }

    all_sections.sort_by(|a, b| (a.order, &a.name).cmp(&(b.order, &b.name)));
    Ok(all_sections)
}

fn run_single_hook(
    hook: &cockpitctl_types::HookConfig,
    report_json: &str,
) -> Result<PostProcessOutput> {
    use std::io::{Read, Write};
    use std::process::{Command, Stdio};
    use std::time::Duration;
    use wait_timeout::ChildExt;

    let parts: Vec<&str> = hook.command.split_whitespace().collect();
    if parts.is_empty() {
        anyhow::bail!("hook `{}` has empty command", hook.name);
    }

    let mut child = Command::new(parts[0])
        .args(&parts[1..])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("spawn hook `{}`", hook.name))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(report_json.as_bytes())
            .with_context(|| format!("write stdin for hook `{}`", hook.name))?;
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

    let timeout = Duration::from_millis(hook.timeout_ms);
    let status = match child
        .wait_timeout(timeout)
        .with_context(|| format!("wait for hook `{}`", hook.name))?
    {
        Some(status) => status,
        None => {
            let _ = child.kill();
            let _ = child.wait();
            anyhow::bail!("hook `{}` timed out after {}ms", hook.name, hook.timeout_ms);
        }
    };

    let stdout_bytes = stdout_thread.join().unwrap_or_default();
    let stderr_bytes = stderr_thread.join().unwrap_or_default();

    let stderr = String::from_utf8_lossy(&stderr_bytes);
    if !stderr.is_empty() {
        eprintln!("cockpitctl: hook `{}` stderr: {}", hook.name, stderr.trim());
    }

    if !status.success() {
        anyhow::bail!("hook `{}` exited with status {}", hook.name, status);
    }

    let result: PostProcessOutput = serde_json::from_slice(&stdout_bytes)
        .with_context(|| format!("parse hook `{}` output", hook.name))?;

    Ok(result)
}

pub fn run_buildfix_actuator(
    actuator: &BuildfixActuatorConfig,
    request: &BuildfixApplyRequest,
) -> Result<BuildfixActuatorResult> {
    use std::io::{Read, Write};
    use std::process::{Command, Stdio};
    use std::time::Duration;
    use wait_timeout::ChildExt;

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
    if !stderr.trim().is_empty() {
        eprintln!("cockpitctl: buildfix actuator stderr: {}", stderr.trim());
    }

    if !status.success() {
        anyhow::bail!("buildfix actuator exited with status {}", status);
    }

    if stdout_bytes.iter().all(|b| b.is_ascii_whitespace()) {
        return Ok(BuildfixActuatorResult::default());
    }

    let mut out: BuildfixActuatorResult =
        serde_json::from_slice(&stdout_bytes).context("parse buildfix actuator JSON response")?;

    out.applied_fix_ids.sort();
    out.applied_fix_ids.dedup();
    out.skipped_fix_ids.sort();
    out.skipped_fix_ids.dedup();

    Ok(out)
}

pub fn load_policy_signing_key(cfg: &PolicySigningConfig) -> Result<Option<Vec<u8>>> {
    if let Some(path) = cfg.key_path.as_deref() {
        let path = path.trim();
        if path.is_empty() {
            anyhow::bail!("policy signing key_path is empty");
        }
        let bytes = fs::read(path)
            .with_context(|| format!("read policy signing key from path {}", path))?;
        return Ok(Some(normalize_signing_key_bytes(bytes)?));
    }

    if let Some(env_name) = cfg.key_env.as_deref() {
        if env_name.trim().is_empty() {
            anyhow::bail!("policy signing key_env is empty");
        }
        let value = std::env::var(env_name)
            .with_context(|| format!("read policy signing key from env {}", env_name))?;
        return Ok(Some(normalize_signing_key_bytes(value.into_bytes())?));
    }

    Ok(None)
}

fn normalize_signing_key_bytes(mut bytes: Vec<u8>) -> Result<Vec<u8>> {
    while matches!(bytes.last(), Some(b'\n' | b'\r')) {
        bytes.pop();
    }
    if bytes.is_empty() {
        anyhow::bail!("policy signing key is empty");
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn write_actuator_script(
        temp: &TempDir,
        response_json: &str,
        exit_code: i32,
    ) -> std::path::PathBuf {
        #[cfg(windows)]
        {
            let script_path = temp.path().join("actuator.cmd");
            let script = format!(
                "@echo off\r\necho {}\r\nexit /b {}\r\n",
                response_json, exit_code
            );
            std::fs::write(&script_path, script).expect("write actuator script");
            script_path
        }

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            let script_path = temp.path().join("actuator.sh");
            let script = format!(
                "#!/bin/sh\ncat >/dev/null\nprintf '%s\\n' '{}'\nexit {}\n",
                response_json, exit_code
            );
            std::fs::write(&script_path, script).expect("write actuator script");
            let mut perms = std::fs::metadata(&script_path)
                .expect("script metadata")
                .permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&script_path, perms).expect("set executable bit");
            script_path
        }
    }

    #[test]
    fn run_buildfix_actuator_parses_json_response() {
        let temp = TempDir::new().expect("tempdir");
        let response =
            r#"{"applied_fix_ids":["fix_b","fix_a"],"skipped_fix_ids":["fix_c"],"errors":[]}"#;
        let script = write_actuator_script(&temp, response, 0);
        let command = {
            #[cfg(unix)]
            {
                format!("sh {}", script.display())
            }

            #[cfg(windows)]
            {
                format!("cmd /c {}", script.display())
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
        let temp = TempDir::new().expect("tempdir");
        let script = write_actuator_script(&temp, "{}", 9);
        let command = {
            #[cfg(unix)]
            {
                format!("sh {}", script.display())
            }

            #[cfg(windows)]
            {
                format!("cmd /c {}", script.display())
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

        let err =
            run_buildfix_actuator(&actuator, &request).expect_err("expected nonzero exit error");
        assert!(format!("{:#}", err).contains("buildfix actuator exited"));
    }

    #[test]
    fn load_policy_signing_key_reads_path_bytes() {
        let temp = TempDir::new().expect("tempdir");
        let key_path = temp.path().join("policy.key");
        std::fs::write(&key_path, b"file-secret\n").expect("write key");

        let cfg = PolicySigningConfig {
            enabled: true,
            algorithm: cockpitctl_types::PolicySignatureAlgorithm::HmacSha256,
            key_path: Some(key_path.to_string_lossy().to_string()),
            key_env: None,
            key_id: None,
        };

        let key = load_policy_signing_key(&cfg)
            .expect("load key")
            .expect("key present");
        assert_eq!(key, b"file-secret");
    }

    #[test]
    fn load_policy_signing_key_reads_env_when_path_unset() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        unsafe {
            std::env::set_var("COCKPITCTL_POLICY_KEY_TEST", "env-secret\r\n");
        }
        let cfg = PolicySigningConfig {
            enabled: true,
            algorithm: cockpitctl_types::PolicySignatureAlgorithm::HmacSha256,
            key_path: None,
            key_env: Some("COCKPITCTL_POLICY_KEY_TEST".to_string()),
            key_id: None,
        };

        let key = load_policy_signing_key(&cfg)
            .expect("load key")
            .expect("key present");
        unsafe {
            std::env::remove_var("COCKPITCTL_POLICY_KEY_TEST");
        }
        assert_eq!(key, b"env-secret");
    }

    #[test]
    fn load_policy_signing_key_returns_none_when_unconfigured() {
        let cfg = PolicySigningConfig::default();
        let key = load_policy_signing_key(&cfg).expect("load key");
        assert!(key.is_none());
    }

    #[test]
    fn load_policy_signing_key_rejects_empty_key_material() {
        let temp = TempDir::new().expect("tempdir");
        let key_path = temp.path().join("policy.key");
        std::fs::write(&key_path, b"\n").expect("write key");

        let cfg = PolicySigningConfig {
            enabled: true,
            algorithm: cockpitctl_types::PolicySignatureAlgorithm::HmacSha256,
            key_path: Some(key_path.to_string_lossy().to_string()),
            key_env: None,
            key_id: None,
        };

        let err = load_policy_signing_key(&cfg).expect_err("expected empty key error");
        assert!(format!("{:#}", err).contains("policy signing key is empty"));
    }

    fn decode_output_file_content(content: &str) -> Vec<u8> {
        let json = format!(r#"{{"name":"test.txt","content":"{}"}}"#, content);
        let file: OutputFile = serde_json::from_str(&json).unwrap();
        file.content
    }

    #[test]
    fn output_file_base64_content_is_decoded() {
        let bytes = decode_output_file_content("SGVsbG8gV29ybGQ=");
        assert_eq!(bytes, b"Hello World");
    }

    #[test]
    fn output_file_plain_text_content_is_preserved() {
        let bytes = decode_output_file_content("Hello World");
        assert_eq!(bytes, b"Hello World");
    }

    #[test]
    fn output_file_empty_content_is_empty_vec() {
        let bytes = decode_output_file_content("");
        assert!(bytes.is_empty());
    }

    #[test]
    fn output_file_invalid_base64_falls_back_to_raw() {
        let bytes = decode_output_file_content("NOT===VALID");
        assert_eq!(bytes, b"NOT===VALID");
    }

    #[test]
    fn output_file_binary_base64_roundtrip() {
        let bytes = decode_output_file_content("AP+Afw==");
        assert_eq!(bytes, vec![0x00, 0xFF, 0x80, 0x7F]);
    }
}
