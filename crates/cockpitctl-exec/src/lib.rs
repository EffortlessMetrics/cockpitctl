use anyhow::{Context, Result};
use cockpitctl_ingest::OutputSink;
use cockpitctl_types::{
    BuildfixActuatorConfig, BuildfixActuatorResult, BuildfixApplyRequest, PolicySigningConfig,
};
use std::fs;

/// Output from a post-processor hook.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct PostProcessOutput {
    #[serde(default)]
    pub comment_sections: Vec<CommentSection>,
    #[serde(default)]
    pub files: Vec<OutputFile>,
}

/// A comment section contributed by a hook.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct CommentSection {
    pub name: String,
    pub content: String,
    #[serde(default)]
    pub order: i32,
}

/// A file contributed by a hook.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct OutputFile {
    pub name: String,
    #[serde(with = "base64_bytes", default)]
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

/// Run post-processor hooks and collect their outputs.
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

pub fn load_policy_signing_key(cfg: &PolicySigningConfig) -> Result<Option<Vec<u8>>> {
    if let Some(path) = cfg.key_path.as_deref() {
        if path.trim().is_empty() {
            anyhow::bail!("policy signing key_path is empty");
        }
        let bytes = fs::read(path).with_context(|| format!("read policy signing key {}", path))?;
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
