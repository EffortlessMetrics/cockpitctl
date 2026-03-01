//! Edge-case tests for the hooks adapter.
//!
//! Covers negative sort order, unicode content in sections,
//! empty files array handling, and stdin content verification.

use anyhow::Result;
use cockpitctl_ingest::OutputSink;
use cockpitctl_io_hooks::{CommentSection, run_hooks};
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

fn write_script(dir: &std::path::Path, name: &str, body: &str) -> std::path::PathBuf {
    let script = dir.join(name);
    std::fs::write(&script, body).unwrap();
    script
}

// ── Negative order sorts before zero ───────────────────────────────────

#[test]
fn negative_order_sorts_before_zero() {
    let tmp = tempfile::tempdir().unwrap();
    let script = write_script(
        tmp.path(),
        "neg_order.py",
        r#"
import sys, json
out = {
    "comment_sections": [
        {"name": "positive", "content": "P", "order": 1},
        {"name": "zero", "content": "Z", "order": 0},
        {"name": "negative", "content": "N", "order": -5}
    ],
    "files": []
}
json.dump(out, sys.stdout)
"#,
    );
    let hook = make_hook("neg", &format!("python {}", script.display()));
    let sink = MemorySink::new();
    let sections = run_hooks(&[hook], "{}", &sink).unwrap();
    assert_eq!(sections.len(), 3);
    assert_eq!(sections[0].name, "negative");
    assert_eq!(sections[0].order, -5);
    assert_eq!(sections[1].name, "zero");
    assert_eq!(sections[2].name, "positive");
}

// ── Unicode content in sections preserved ──────────────────────────────

#[test]
fn unicode_content_in_sections_preserved() {
    let tmp = tempfile::tempdir().unwrap();
    let script = write_script(
        tmp.path(),
        "unicode.py",
        r#"
import sys, json, io
# Force UTF-8 stdout for cross-platform compatibility
sys.stdout = io.TextIOWrapper(sys.stdout.buffer, encoding='utf-8')
out = {
    "comment_sections": [
        {"name": "emoji", "content": "Status: \u2705 passed \u274c failed \U0001f680 deployed", "order": 0}
    ],
    "files": []
}
json.dump(out, sys.stdout, ensure_ascii=False)
"#,
    );
    let hook = make_hook("unicode", &format!("python {}", script.display()));
    let sink = MemorySink::new();
    let sections = run_hooks(&[hook], "{}", &sink).unwrap();
    assert_eq!(sections.len(), 1);
    assert!(sections[0].content.contains('\u{2705}'));
    assert!(sections[0].content.contains('\u{274c}'));
    assert!(sections[0].content.contains('\u{1f680}'));
}

// ── Empty files array present but produces no files ────────────────────

#[test]
fn empty_files_array_produces_no_extra_files() {
    let tmp = tempfile::tempdir().unwrap();
    let script = write_script(
        tmp.path(),
        "empty_files.py",
        r#"
import sys, json
out = {
    "comment_sections": [{"name": "s", "content": "c", "order": 0}],
    "files": []
}
json.dump(out, sys.stdout)
"#,
    );
    let hook = make_hook("nofiles", &format!("python {}", script.display()));
    let sink = MemorySink::new();
    let sections = run_hooks(&[hook], "{}", &sink).unwrap();
    assert_eq!(sections.len(), 1);
    assert!(sink.files.borrow().is_empty());
}

// ── Output file with plain text content (not base64) ───────────────────

#[test]
fn output_file_plain_text_fallback() {
    // OutputFile content that isn't valid base64 falls back to raw bytes
    let json = r#"{"name":"readme.md","content":"plain text here"}"#;
    let file: cockpitctl_io_hooks::OutputFile = serde_json::from_str(json).unwrap();
    assert_eq!(file.name, "readme.md");
    assert_eq!(file.content, b"plain text here");
}

// ── CommentSection with very long content ──────────────────────────────

#[test]
fn comment_section_with_long_content() {
    let long_content = "x".repeat(50_000);
    let json = format!(
        r#"{{"name":"long","content":"{}","order":0}}"#,
        long_content
    );
    let sec: CommentSection = serde_json::from_str(&json).unwrap();
    assert_eq!(sec.content.len(), 50_000);
}

// ── Multiple hooks where first succeeds and second fails ───────────────

#[test]
fn successful_hook_sections_preserved_when_later_hook_fails() {
    let tmp = tempfile::tempdir().unwrap();
    let ok_script = write_script(
        tmp.path(),
        "ok_first.py",
        r#"
import sys, json
json.dump({
    "comment_sections": [{"name": "from-ok", "content": "success", "order": 0}],
    "files": [{"name": "ok.txt", "content": ""}]
}, sys.stdout)
"#,
    );
    let fail_script = write_script(tmp.path(), "fail_second.py", "import sys; sys.exit(42)\n");
    let hooks = vec![
        make_hook("ok", &format!("python {}", ok_script.display())),
        make_hook("fail", &format!("python {}", fail_script.display())),
    ];
    let sink = MemorySink::new();
    let sections = run_hooks(&hooks, "{}", &sink).unwrap();
    // First hook's sections preserved, second hook's failure doesn't remove them
    assert_eq!(sections.len(), 1);
    assert_eq!(sections[0].name, "from-ok");
    assert!(sink.files.borrow().contains_key("ok.txt"));
}
