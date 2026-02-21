//! Hook adapter boundary extracted from `cockpitctl-io`.

use anyhow::{Context, Result};
use cockpitctl_ingest::OutputSink;
use cockpitctl_types::HookConfig;
use serde::Deserialize;
use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::time::Duration;
use wait_timeout::ChildExt;

/// Output from a post-processor hook.
#[derive(Debug, Clone, Deserialize)]
pub struct PostProcessOutput {
    #[serde(default)]
    pub comment_sections: Vec<CommentSection>,
    #[serde(default)]
    pub files: Vec<OutputFile>,
}

/// A comment section contributed by a hook.
#[derive(Debug, Clone, Deserialize)]
pub struct CommentSection {
    pub name: String,
    pub content: String,
    #[serde(default)]
    pub order: i32,
}

/// A file contributed by a hook.
#[derive(Debug, Clone, Deserialize)]
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
        // Accept either base64-encoded string or raw bytes.
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
    hooks: &[HookConfig],
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

    // Sort sections by (order, name) for determinism.
    all_sections.sort_by(|a, b| (a.order, &a.name).cmp(&(b.order, &b.name)));
    Ok(all_sections)
}

fn run_single_hook(hook: &HookConfig, report_json: &str) -> Result<PostProcessOutput> {
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

#[cfg(test)]
mod tests {
    use super::*;

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
