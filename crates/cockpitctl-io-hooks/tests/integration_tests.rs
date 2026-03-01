//! Integration tests for cockpitctl-io-hooks.
//!
//! Focuses on filesystem-based hook execution, environment variable
//! propagation, output capture, timeout handling, and deterministic
//! section merging across multiple hooks.

use anyhow::Result;
use cockpitctl_ingest::OutputSink;
use cockpitctl_io_hooks::run_hooks;
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

// ── Hook execution with valid script ───────────────────────────────────

#[test]
fn hook_receives_report_json_on_stdin() {
    let tmp = tempfile::tempdir().unwrap();
    let script = write_script(
        tmp.path(),
        "stdin_echo.py",
        r#"
import sys, json
data = json.load(sys.stdin)
out = {
    "comment_sections": [{"name": "echo", "content": data.get("key", "missing"), "order": 0}],
    "files": []
}
json.dump(out, sys.stdout)
"#,
    );
    let hook = make_hook("echo", &format!("python {}", script.display()));
    let sink = MemorySink::new();
    let sections = run_hooks(&[hook], r#"{"key":"hello"}"#, &sink).unwrap();
    assert_eq!(sections.len(), 1);
    assert_eq!(sections[0].content, "hello");
}

// ── Hook with missing script path → error handling ─────────────────────

#[test]
fn hook_with_nonexistent_script_path_is_handled_gracefully() {
    let tmp = tempfile::tempdir().unwrap();
    let missing = tmp.path().join("does_not_exist.py");
    let hook = make_hook("missing", &format!("python {}", missing.display()));
    let sink = MemorySink::new();
    // run_hooks should not propagate the error — it logs and continues
    let sections = run_hooks(&[hook], "{}", &sink).unwrap();
    assert!(sections.is_empty());
    assert!(sink.files.borrow().is_empty());
}

#[test]
fn empty_command_string_is_handled_gracefully() {
    let hook = make_hook("empty", "");
    let sink = MemorySink::new();
    let sections = run_hooks(&[hook], "{}", &sink).unwrap();
    assert!(sections.is_empty());
}

// ── Hook with invalid exit code → correct reporting ────────────────────

#[test]
fn hook_with_exit_code_2_is_treated_as_failure() {
    let tmp = tempfile::tempdir().unwrap();
    let script = write_script(tmp.path(), "exit2.py", "import sys; sys.exit(2)\n");
    let hook = make_hook("exit2", &format!("python {}", script.display()));
    let sink = MemorySink::new();
    let sections = run_hooks(&[hook], "{}", &sink).unwrap();
    // Non-zero exit is a failure → no sections collected
    assert!(sections.is_empty());
}

#[test]
fn hook_returning_invalid_json_is_treated_as_failure() {
    let tmp = tempfile::tempdir().unwrap();
    let script = write_script(
        tmp.path(),
        "bad_json.py",
        "import sys; sys.stdout.write('NOT JSON'); sys.stdout.flush()\n",
    );
    let hook = make_hook("bad", &format!("python {}", script.display()));
    let sink = MemorySink::new();
    let sections = run_hooks(&[hook], "{}", &sink).unwrap();
    assert!(sections.is_empty());
}

// ── Hook output capture and comment section construction ───────────────

#[test]
fn hook_produces_multiple_files_and_sections() {
    let tmp = tempfile::tempdir().unwrap();
    let script = write_script(
        tmp.path(),
        "multi.py",
        r#"
import sys, json, base64
out = {
    "comment_sections": [
        {"name": "sec-a", "content": "content-a", "order": 1},
        {"name": "sec-b", "content": "content-b", "order": 2}
    ],
    "files": [
        {"name": "file1.txt", "content": base64.b64encode(b"data1").decode()},
        {"name": "file2.bin", "content": base64.b64encode(b"\x00\xff").decode()}
    ]
}
json.dump(out, sys.stdout)
"#,
    );
    let hook = make_hook("multi", &format!("python {}", script.display()));
    let sink = MemorySink::new();
    let sections = run_hooks(&[hook], "{}", &sink).unwrap();
    assert_eq!(sections.len(), 2);
    assert_eq!(sections[0].name, "sec-a");
    assert_eq!(sections[1].name, "sec-b");
    let files = sink.files.borrow();
    assert_eq!(files.get("file1.txt").unwrap(), b"data1");
    assert_eq!(files.get("file2.bin").unwrap(), &[0x00, 0xff]);
}

// ── Multiple hooks in order → deterministic output ─────────────────────

#[test]
fn three_hooks_sections_merged_deterministically() {
    let tmp = tempfile::tempdir().unwrap();
    // Each script contributes a section with a different order value.
    let script_a = write_script(
        tmp.path(),
        "hook_a.py",
        r#"
import sys, json
json.dump({"comment_sections": [{"name": "from-a", "content": "A", "order": 3}], "files": []}, sys.stdout)
"#,
    );
    let script_b = write_script(
        tmp.path(),
        "hook_b.py",
        r#"
import sys, json
json.dump({"comment_sections": [{"name": "from-b", "content": "B", "order": 1}], "files": []}, sys.stdout)
"#,
    );
    let script_c = write_script(
        tmp.path(),
        "hook_c.py",
        r#"
import sys, json
json.dump({"comment_sections": [{"name": "from-c", "content": "C", "order": 2}], "files": []}, sys.stdout)
"#,
    );
    let hooks = vec![
        make_hook("a", &format!("python {}", script_a.display())),
        make_hook("b", &format!("python {}", script_b.display())),
        make_hook("c", &format!("python {}", script_c.display())),
    ];
    let sink = MemorySink::new();
    let sections = run_hooks(&hooks, "{}", &sink).unwrap();
    assert_eq!(sections.len(), 3);
    // Sorted by order: 1, 2, 3
    assert_eq!(sections[0].name, "from-b");
    assert_eq!(sections[1].name, "from-c");
    assert_eq!(sections[2].name, "from-a");
}

#[test]
fn sections_with_same_order_sorted_by_name() {
    let tmp = tempfile::tempdir().unwrap();
    let script = write_script(
        tmp.path(),
        "same_order.py",
        r#"
import sys, json
json.dump({
    "comment_sections": [
        {"name": "zebra", "content": "Z", "order": 0},
        {"name": "apple", "content": "A", "order": 0}
    ],
    "files": []
}, sys.stdout)
"#,
    );
    let hook = make_hook("same", &format!("python {}", script.display()));
    let sink = MemorySink::new();
    let sections = run_hooks(&[hook], "{}", &sink).unwrap();
    assert_eq!(sections.len(), 2);
    assert_eq!(sections[0].name, "apple");
    assert_eq!(sections[1].name, "zebra");
}

// ── Hook with environment variables ────────────────────────────────────

#[test]
fn hook_inherits_process_environment() {
    // Hooks run as child processes and inherit the parent environment.
    // We verify this indirectly: if python is found on PATH, env is inherited.
    let tmp = tempfile::tempdir().unwrap();
    let script = write_script(
        tmp.path(),
        "env_check.py",
        r#"
import sys, os, json
# PATH must be available for this script to even run
path_val = os.environ.get("PATH", "")
has_path = len(path_val) > 0
json.dump({
    "comment_sections": [{"name": "env", "content": str(has_path), "order": 0}],
    "files": []
}, sys.stdout)
"#,
    );
    let hook = make_hook("env", &format!("python {}", script.display()));
    let sink = MemorySink::new();
    let sections = run_hooks(&[hook], "{}", &sink).unwrap();
    assert_eq!(sections.len(), 1);
    assert_eq!(sections[0].content, "True");
}

// ── Hook timeout handling ──────────────────────────────────────────────

#[test]
fn timed_out_hook_does_not_block_subsequent_hooks() {
    let tmp = tempfile::tempdir().unwrap();
    let slow = write_script(tmp.path(), "slow.py", "import time; time.sleep(60)\n");
    let fast = write_script(
        tmp.path(),
        "fast.py",
        r#"
import sys, json
json.dump({"comment_sections": [{"name": "fast", "content": "ok", "order": 0}], "files": []}, sys.stdout)
"#,
    );

    let mut slow_hook = make_hook("slow", &format!("python {}", slow.display()));
    slow_hook.timeout_ms = 200;
    let fast_hook = make_hook("fast", &format!("python {}", fast.display()));

    let sink = MemorySink::new();
    let sections = run_hooks(&[slow_hook, fast_hook], "{}", &sink).unwrap();
    // Slow hook times out and is skipped; fast hook still runs
    assert_eq!(sections.len(), 1);
    assert_eq!(sections[0].name, "fast");
}

// ── Large report JSON payload ──────────────────────────────────────────

#[test]
fn hook_handles_large_stdin_payload() {
    let tmp = tempfile::tempdir().unwrap();
    let script = write_script(
        tmp.path(),
        "large_stdin.py",
        r#"
import sys, json
data = sys.stdin.read()
json.dump({"comment_sections": [{"name": "len", "content": str(len(data)), "order": 0}], "files": []}, sys.stdout)
"#,
    );
    let hook = make_hook("large", &format!("python {}", script.display()));
    let sink = MemorySink::new();
    // Send a 100KB JSON payload
    let big_report = format!(r#"{{"data":"{}"}}"#, "x".repeat(100_000));
    let sections = run_hooks(&[hook], &big_report, &sink).unwrap();
    assert_eq!(sections.len(), 1);
    let reported_len: usize = sections[0].content.parse().unwrap();
    assert!(reported_len > 100_000);
}
