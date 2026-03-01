//! Hook adapter boundary extracted from `cockpitctl-io`.
//!
//! Runs post-processing hook commands after ingest, allowing external
//! tools to contribute extra comment sections and sidecar files.

#![warn(missing_docs)]

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
    /// Comment sections contributed by the hook.
    #[serde(default)]
    pub comment_sections: Vec<CommentSection>,
    /// Files contributed by the hook.
    #[serde(default)]
    pub files: Vec<OutputFile>,
}

/// A comment section contributed by a hook.
#[derive(Debug, Clone, Deserialize)]
pub struct CommentSection {
    /// Section name.
    pub name: String,
    /// Markdown content.
    pub content: String,
    /// Sort order (lower values come first).
    #[serde(default)]
    pub order: i32,
}

/// A file contributed by a hook.
#[derive(Debug, Clone, Deserialize)]
pub struct OutputFile {
    /// Output file name.
    pub name: String,
    /// File content bytes (base64-decoded if applicable).
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
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::path::PathBuf;

    // --- helpers ---

    fn decode_output_file_content(content: &str) -> Vec<u8> {
        let json = format!(r#"{{"name":"test.txt","content":"{}"}}"#, content);
        let file: OutputFile = serde_json::from_str(&json).unwrap();
        file.content
    }

    fn make_hook(name: &str, command: &str) -> HookConfig {
        HookConfig {
            name: name.to_string(),
            command: command.to_string(),
            when: Default::default(),
            timeout_ms: 30_000,
        }
    }

    /// In-memory `OutputSink` that records extra files written by hooks.
    struct MemorySink {
        files: RefCell<HashMap<String, Vec<u8>>>,
    }

    impl MemorySink {
        fn new() -> Self {
            Self {
                files: RefCell::new(HashMap::new()),
            }
        }
    }

    impl OutputSink for MemorySink {
        fn write_cockpit_report(&self, _json: &str) -> Result<()> {
            Ok(())
        }
        fn write_cockpit_comment(&self, _md: &str) -> Result<()> {
            Ok(())
        }
        fn write_extra_file(&self, name: &str, content: &[u8]) -> Result<()> {
            self.files
                .borrow_mut()
                .insert(name.to_string(), content.to_vec());
            Ok(())
        }
    }

    /// Return a helper script path that echoes its stdin-derived JSON to stdout
    /// after wrapping it. We use a tiny Python script for cross-platform compat.
    fn write_echo_script(dir: &std::path::Path) -> PathBuf {
        let script = dir.join("echo_hook.py");
        std::fs::write(
            &script,
            r#"
import sys, json
data = sys.stdin.read()
out = {"comment_sections": [], "files": []}
json.dump(out, sys.stdout)
"#,
        )
        .unwrap();
        script
    }

    fn write_sections_script(dir: &std::path::Path) -> PathBuf {
        let script = dir.join("sections_hook.py");
        std::fs::write(
            &script,
            r#"
import sys, json
out = {
    "comment_sections": [
        {"name": "beta", "content": "B", "order": 2},
        {"name": "alpha", "content": "A", "order": 1}
    ],
    "files": []
}
json.dump(out, sys.stdout)
"#,
        )
        .unwrap();
        script
    }

    fn write_files_script(dir: &std::path::Path) -> PathBuf {
        let script = dir.join("files_hook.py");
        std::fs::write(
            &script,
            r#"
import sys, json, base64
out = {
    "comment_sections": [],
    "files": [
        {"name": "out.txt", "content": base64.b64encode(b"hello").decode()}
    ]
}
json.dump(out, sys.stdout)
"#,
        )
        .unwrap();
        script
    }

    fn write_stderr_script(dir: &std::path::Path) -> PathBuf {
        let script = dir.join("stderr_hook.py");
        std::fs::write(
            &script,
            r#"
import sys, json
print("debug info", file=sys.stderr)
json.dump({"comment_sections": [], "files": []}, sys.stdout)
"#,
        )
        .unwrap();
        script
    }

    fn write_fail_script(dir: &std::path::Path) -> PathBuf {
        let script = dir.join("fail_hook.py");
        std::fs::write(&script, "import sys; sys.exit(1)\n").unwrap();
        script
    }

    fn write_bad_json_script(dir: &std::path::Path) -> PathBuf {
        let script = dir.join("bad_json_hook.py");
        std::fs::write(
            &script,
            "import sys; sys.stdout.write('NOT JSON'); sys.stdout.flush()\n",
        )
        .unwrap();
        script
    }

    fn write_slow_script(dir: &std::path::Path) -> PathBuf {
        let script = dir.join("slow_hook.py");
        std::fs::write(&script, "import time; time.sleep(60)\n").unwrap();
        script
    }

    // --- base64 / deserialization tests (existing + new) ---

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

    // --- PostProcessOutput deserialization ---

    #[test]
    fn post_process_output_deserializes_empty_sections_and_files() {
        let json = r#"{"comment_sections":[],"files":[]}"#;
        let out: PostProcessOutput = serde_json::from_str(json).unwrap();
        assert!(out.comment_sections.is_empty());
        assert!(out.files.is_empty());
    }

    #[test]
    fn post_process_output_defaults_missing_fields() {
        let json = r#"{}"#;
        let out: PostProcessOutput = serde_json::from_str(json).unwrap();
        assert!(out.comment_sections.is_empty());
        assert!(out.files.is_empty());
    }

    #[test]
    fn comment_section_order_defaults_to_zero() {
        let json = r#"{"name":"s","content":"c"}"#;
        let sec: CommentSection = serde_json::from_str(json).unwrap();
        assert_eq!(sec.order, 0);
        assert_eq!(sec.name, "s");
        assert_eq!(sec.content, "c");
    }

    #[test]
    fn output_file_default_content_is_empty() {
        let json = r#"{"name":"f.txt"}"#;
        let f: OutputFile = serde_json::from_str(json).unwrap();
        assert_eq!(f.name, "f.txt");
        assert!(f.content.is_empty());
    }

    // --- run_hooks with empty list ---

    #[test]
    fn run_hooks_with_no_hooks_returns_empty() {
        let sink = MemorySink::new();
        let sections = run_hooks(&[], "{}", &sink).unwrap();
        assert!(sections.is_empty());
    }

    // --- run_single_hook: empty command ---

    #[test]
    fn run_single_hook_empty_command_errors() {
        let hook = make_hook("empty", "");
        let err = run_single_hook(&hook, "{}").unwrap_err();
        assert!(
            format!("{err:#}").contains("empty command"),
            "unexpected error: {err:#}"
        );
    }

    // --- run_single_hook: command not found ---

    #[test]
    fn run_single_hook_command_not_found_errors() {
        let hook = make_hook("missing", "nonexistent_binary_xyz_42");
        let err = run_single_hook(&hook, "{}").unwrap_err();
        assert!(
            format!("{err:#}").contains("spawn hook"),
            "unexpected error: {err:#}"
        );
    }

    // --- integration tests using tiny Python scripts ---

    #[test]
    fn run_single_hook_captures_valid_json_stdout() {
        let tmp = tempfile::tempdir().unwrap();
        let script = write_echo_script(tmp.path());
        let hook = make_hook("echo", &format!("python {}", script.display()));
        let out = run_single_hook(&hook, r#"{"hello":"world"}"#).unwrap();
        assert!(out.comment_sections.is_empty());
        assert!(out.files.is_empty());
    }

    #[test]
    fn run_single_hook_returns_comment_sections() {
        let tmp = tempfile::tempdir().unwrap();
        let script = write_sections_script(tmp.path());
        let hook = make_hook("sec", &format!("python {}", script.display()));
        let out = run_single_hook(&hook, "{}").unwrap();
        assert_eq!(out.comment_sections.len(), 2);
        assert_eq!(out.comment_sections[0].name, "beta");
        assert_eq!(out.comment_sections[1].name, "alpha");
    }

    #[test]
    fn run_hooks_sorts_sections_by_order_then_name() {
        let tmp = tempfile::tempdir().unwrap();
        let script = write_sections_script(tmp.path());
        let hook = make_hook("sec", &format!("python {}", script.display()));
        let sink = MemorySink::new();
        let sections = run_hooks(&[hook], "{}", &sink).unwrap();
        assert_eq!(sections.len(), 2);
        // order=1 comes before order=2
        assert_eq!(sections[0].name, "alpha");
        assert_eq!(sections[1].name, "beta");
    }

    #[test]
    fn run_hooks_writes_extra_files_to_sink() {
        let tmp = tempfile::tempdir().unwrap();
        let script = write_files_script(tmp.path());
        let hook = make_hook("files", &format!("python {}", script.display()));
        let sink = MemorySink::new();
        run_hooks(&[hook], "{}", &sink).unwrap();
        let files = sink.files.borrow();
        assert_eq!(files.get("out.txt").unwrap(), b"hello");
    }

    #[test]
    fn run_single_hook_non_zero_exit_errors() {
        let tmp = tempfile::tempdir().unwrap();
        let script = write_fail_script(tmp.path());
        let hook = make_hook("fail", &format!("python {}", script.display()));
        let err = run_single_hook(&hook, "{}").unwrap_err();
        assert!(
            format!("{err:#}").contains("exited with status"),
            "unexpected error: {err:#}"
        );
    }

    #[test]
    fn run_single_hook_invalid_json_output_errors() {
        let tmp = tempfile::tempdir().unwrap();
        let script = write_bad_json_script(tmp.path());
        let hook = make_hook("bad", &format!("python {}", script.display()));
        let err = run_single_hook(&hook, "{}").unwrap_err();
        assert!(
            format!("{err:#}").contains("parse hook"),
            "unexpected error: {err:#}"
        );
    }

    #[test]
    fn run_single_hook_stderr_does_not_fail() {
        let tmp = tempfile::tempdir().unwrap();
        let script = write_stderr_script(tmp.path());
        let hook = make_hook("noisy", &format!("python {}", script.display()));
        // Should succeed even when stderr is non-empty.
        let out = run_single_hook(&hook, "{}").unwrap();
        assert!(out.comment_sections.is_empty());
    }

    #[test]
    fn run_single_hook_timeout_kills_process() {
        let tmp = tempfile::tempdir().unwrap();
        let script = write_slow_script(tmp.path());
        let mut hook = make_hook("slow", &format!("python {}", script.display()));
        hook.timeout_ms = 200; // very short timeout
        let err = run_single_hook(&hook, "{}").unwrap_err();
        assert!(
            format!("{err:#}").contains("timed out"),
            "unexpected error: {err:#}"
        );
    }

    #[test]
    fn run_hooks_continues_after_failed_hook() {
        let tmp = tempfile::tempdir().unwrap();
        let fail_script = write_fail_script(tmp.path());
        let ok_script = write_sections_script(tmp.path());

        let hooks = vec![
            make_hook("fail", &format!("python {}", fail_script.display())),
            make_hook("ok", &format!("python {}", ok_script.display())),
        ];
        let sink = MemorySink::new();
        let sections = run_hooks(&hooks, "{}", &sink).unwrap();
        // The failing hook is skipped; sections from the OK hook are returned.
        assert_eq!(sections.len(), 2);
    }

    #[test]
    fn run_hooks_multiple_hooks_merge_sections() {
        let tmp = tempfile::tempdir().unwrap();
        let script = write_sections_script(tmp.path());

        let hooks = vec![
            make_hook("h1", &format!("python {}", script.display())),
            make_hook("h2", &format!("python {}", script.display())),
        ];
        let sink = MemorySink::new();
        let sections = run_hooks(&hooks, "{}", &sink).unwrap();
        // 2 sections per hook × 2 hooks = 4 sections total
        assert_eq!(sections.len(), 4);
        // Still sorted by (order, name)
        assert_eq!(sections[0].order, 1);
        assert_eq!(sections[3].order, 2);
    }
}
