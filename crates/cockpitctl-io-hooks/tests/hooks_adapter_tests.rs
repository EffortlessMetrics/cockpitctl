//! Integration tests for the hooks adapter.
//!
//! Exercises the public API (`run_hooks`) verifying ordering determinism,
//! missing-script handling, output capture, and no-op behaviour.

use anyhow::Result;
use cockpitctl_ingest::OutputSink;
use cockpitctl_io_hooks::{CommentSection, PostProcessOutput, run_hooks};
use cockpitctl_types::HookConfig;
use std::cell::RefCell;
use std::collections::HashMap;

// ── Helpers ────────────────────────────────────────────────────────────

fn make_hook(name: &str, command: &str) -> HookConfig {
    HookConfig {
        name: name.to_string(),
        command: command.to_string(),
        when: Default::default(),
        timeout_ms: 30_000,
    }
}

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

fn write_sections_script(dir: &std::path::Path) -> std::path::PathBuf {
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

fn write_files_script(dir: &std::path::Path) -> std::path::PathBuf {
    let script = dir.join("files_hook.py");
    std::fs::write(
        &script,
        r#"
import sys, json, base64
out = {
    "comment_sections": [{"name": "file-hook", "content": "wrote file", "order": 0}],
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

fn write_fail_script(dir: &std::path::Path) -> std::path::PathBuf {
    let script = dir.join("fail_hook.py");
    std::fs::write(&script, "import sys; sys.exit(1)\n").unwrap();
    script
}

// ── Empty hooks list → no-op ───────────────────────────────────────────

#[test]
fn empty_hooks_list_is_noop() {
    let sink = MemorySink::new();
    let sections = run_hooks(&[], "{}", &sink).unwrap();
    assert!(sections.is_empty());
    assert!(sink.files.borrow().is_empty());
}

// ── Hook execution ordering (deterministic) ────────────────────────────

#[test]
fn sections_are_sorted_by_order_then_name() {
    let tmp = tempfile::tempdir().unwrap();
    let script = write_sections_script(tmp.path());
    let hook = make_hook("sec", &format!("python {}", script.display()));
    let sink = MemorySink::new();
    let sections = run_hooks(&[hook], "{}", &sink).unwrap();
    assert_eq!(sections.len(), 2);
    // order=1 before order=2
    assert_eq!(sections[0].name, "alpha");
    assert_eq!(sections[1].name, "beta");
    insta::assert_json_snapshot!(
        "section_ordering",
        serde_json::json!(
            sections
                .iter()
                .map(|s| serde_json::json!({
                    "name": s.name,
                    "order": s.order,
                }))
                .collect::<Vec<_>>()
        )
    );
}

#[test]
fn multiple_hooks_merge_and_sort_sections() {
    let tmp = tempfile::tempdir().unwrap();
    let script = write_sections_script(tmp.path());
    let hooks = vec![
        make_hook("h1", &format!("python {}", script.display())),
        make_hook("h2", &format!("python {}", script.display())),
    ];
    let sink = MemorySink::new();
    let sections = run_hooks(&hooks, "{}", &sink).unwrap();
    // 2 hooks × 2 sections each = 4 total, still sorted
    assert_eq!(sections.len(), 4);
    assert_eq!(sections[0].order, 1);
    assert_eq!(sections[3].order, 2);
}

// ── Hook with missing script → graceful error ──────────────────────────

#[test]
fn missing_script_does_not_crash_run_hooks() {
    let hooks = vec![make_hook("missing", "nonexistent_binary_xyz_42")];
    let sink = MemorySink::new();
    // run_hooks handles individual hook failures gracefully (logs, continues)
    let sections = run_hooks(&hooks, "{}", &sink).unwrap();
    assert!(sections.is_empty());
}

#[test]
fn failed_hook_does_not_block_subsequent_hooks() {
    let tmp = tempfile::tempdir().unwrap();
    let fail = write_fail_script(tmp.path());
    let ok = write_sections_script(tmp.path());
    let hooks = vec![
        make_hook("fail", &format!("python {}", fail.display())),
        make_hook("ok", &format!("python {}", ok.display())),
    ];
    let sink = MemorySink::new();
    let sections = run_hooks(&hooks, "{}", &sink).unwrap();
    // Only sections from the successful hook
    assert_eq!(sections.len(), 2);
}

// ── Hook output capture → CommentSection + files ───────────────────────

#[test]
fn hook_output_captures_files_and_sections() {
    let tmp = tempfile::tempdir().unwrap();
    let script = write_files_script(tmp.path());
    let hook = make_hook("files", &format!("python {}", script.display()));
    let sink = MemorySink::new();
    let sections = run_hooks(&[hook], "{}", &sink).unwrap();
    assert_eq!(sections.len(), 1);
    assert_eq!(sections[0].name, "file-hook");
    let files = sink.files.borrow();
    assert_eq!(files.get("out.txt").unwrap(), b"hello");
}

// ── Deserialization edge cases ─────────────────────────────────────────

#[test]
fn post_process_output_defaults_missing_fields() {
    let out: PostProcessOutput = serde_json::from_str("{}").unwrap();
    assert!(out.comment_sections.is_empty());
    assert!(out.files.is_empty());
}

#[test]
fn comment_section_order_defaults_to_zero() {
    let sec: CommentSection = serde_json::from_str(r#"{"name":"s","content":"c"}"#).unwrap();
    assert_eq!(sec.order, 0);
}
