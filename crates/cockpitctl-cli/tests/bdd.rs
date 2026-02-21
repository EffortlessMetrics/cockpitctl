//! BDD tests using cucumber-rs for cockpitctl.
//!
//! Runs feature files from `features/` directory using proper Gherkin syntax.

use cockpitctl_feature_grid::{feature_runtime_present, parse_feature_state};
use cockpitctl_feature_state::Feature;
use cucumber::{World, given, then, when};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

fn cockpitctl_cmd() -> assert_cmd::Command {
    let mut cmd = assert_cmd::cargo_bin_cmd!("cockpitctl");
    if let Ok(profile) = std::env::var("LLVM_PROFILE_FILE") {
        cmd.env("LLVM_PROFILE_FILE", profile);
    }
    cmd
}

/// World state shared across steps within a scenario.
#[derive(Debug, World)]
#[world(init = Self::new)]
pub struct IngestWorld {
    /// Workspace root path.
    workspace: PathBuf,
    /// Temporary directory for this scenario.
    #[allow(dead_code)]
    temp_dir: Option<TempDir>,
    /// Path to the copied fixture.
    fixture_path: Option<PathBuf>,
    /// Name of the current fixture.
    fixture_name: String,
    /// Last exit code from running the command.
    exit_code: Option<i32>,
    /// Path to the generated report.json.
    report_path: Option<PathBuf>,
    /// Path to the generated comment.md.
    comment_path: Option<PathBuf>,
    /// Captured report content for determinism checks.
    captured_report: Option<String>,
    /// Extra CLI arguments to pass.
    extra_args: Vec<String>,
    /// Captured stdout from the last command execution.
    stdout: String,
    /// Captured stderr from the last command execution.
    stderr: String,
    /// Path to a baseline report used for trend comparisons.
    baseline_report_path: Option<PathBuf>,
    /// Path to a generated hook script for the current scenario.
    hook_script_path: Option<PathBuf>,
    /// Path to a generated buildfix actuator script for the current scenario.
    actuator_script_path: Option<PathBuf>,
    /// Path to generated policy-signing key material.
    policy_sign_key_path: Option<PathBuf>,
}

impl IngestWorld {
    fn new() -> Self {
        // CARGO_MANIFEST_DIR points at crates/cockpitctl-cli
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string());
        let workspace = Path::new(&manifest_dir)
            .join("../..")
            .canonicalize()
            .unwrap_or_else(|_| PathBuf::from("."));

        Self {
            workspace,
            temp_dir: None,
            fixture_path: None,
            fixture_name: String::new(),
            exit_code: None,
            report_path: None,
            comment_path: None,
            captured_report: None,
            extra_args: Vec::new(),
            stdout: String::new(),
            stderr: String::new(),
            baseline_report_path: None,
            hook_script_path: None,
            actuator_script_path: None,
            policy_sign_key_path: None,
        }
    }

    fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
        fs::create_dir_all(dst)?;
        for entry in fs::read_dir(src)? {
            let entry = entry?;
            let file_type = entry.file_type()?;
            let from = entry.path();
            let to = dst.join(entry.file_name());
            if file_type.is_dir() {
                Self::copy_dir_all(&from, &to)?;
            } else if file_type.is_file() {
                fs::copy(&from, &to)?;
            }
        }
        Ok(())
    }

    fn read_report(&self) -> Value {
        let path = self.report_path.as_ref().expect("report path not set");
        let content = fs::read_to_string(path).expect("failed to read report");
        serde_json::from_str(&content).expect("failed to parse report JSON")
    }

    fn read_comment(&self) -> String {
        let path = self.comment_path.as_ref().expect("comment path not set");
        fs::read_to_string(path).expect("failed to read comment")
    }

    fn resolve_arg_placeholder(&self, arg: &str) -> String {
        match arg {
            "{baseline_report}" => self
                .baseline_report_path
                .as_ref()
                .expect("baseline report not set")
                .to_string_lossy()
                .to_string(),
            "{actuator_script}" => self
                .actuator_script_path
                .as_ref()
                .expect("actuator script not set")
                .to_string_lossy()
                .to_string(),
            "{hook_script}" => self
                .hook_script_path
                .as_ref()
                .expect("hook script not set")
                .to_string_lossy()
                .to_string(),
            "{policy_sign_key}" => self
                .policy_sign_key_path
                .as_ref()
                .expect("policy signing key path not set")
                .to_string_lossy()
                .to_string(),
            _ => arg.to_string(),
        }
    }

    fn fixture_file_path(&self, rel: &str) -> PathBuf {
        self.fixture_path
            .as_ref()
            .expect("fixture path not set")
            .join(rel)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Given steps
// ─────────────────────────────────────────────────────────────────────────────

#[given("a clean output directory")]
fn clean_output_directory(_world: &mut IngestWorld) {
    // Background step - actual cleanup happens when fixture is set up
}

#[given(expr = "a fixture {string}")]
fn given_fixture(world: &mut IngestWorld, fixture_name: String) {
    world.fixture_name = fixture_name.clone();
    world.extra_args.clear();
    world.stdout.clear();
    world.stderr.clear();
    world.baseline_report_path = None;
    world.hook_script_path = None;
    world.actuator_script_path = None;
    world.policy_sign_key_path = None;

    let fixture_src = world.workspace.join("fixtures").join(&fixture_name);
    assert!(
        fixture_src.exists(),
        "fixture {} does not exist at {:?}",
        fixture_name,
        fixture_src
    );

    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let fixture_dst = temp_dir.path().join(&fixture_name);

    IngestWorld::copy_dir_all(&fixture_src, &fixture_dst).expect("failed to copy fixture");

    // Ensure clean outputs
    let out_dir = fixture_dst.join("artifacts").join("cockpit");
    let _ = fs::remove_dir_all(&out_dir);

    world.fixture_path = Some(fixture_dst);
    world.temp_dir = Some(temp_dir);
}

fn toml_escape_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "\\\\")
}

#[cfg(windows)]
fn write_hook_script(dir: &Path) -> PathBuf {
    let script_path = dir.join("hook.cmd");
    let script = "@echo off\r\nmore > nul\r\necho {\"comment_sections\":[{\"name\":\"Hook Notes\",\"content\":\"From hook\",\"order\":1}]}\r\n";
    fs::write(&script_path, script).expect("failed to write hook script");
    script_path
}

#[cfg(unix)]
fn write_hook_script(dir: &Path) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;

    let script_path = dir.join("hook.sh");
    let script = "#!/bin/sh\ncat >/dev/null\nprintf '%s\\n' '{\"comment_sections\":[{\"name\":\"Hook Notes\",\"content\":\"From hook\",\"order\":1}]}'\n";
    fs::write(&script_path, script).expect("failed to write hook script");
    let mut perms = fs::metadata(&script_path)
        .expect("hook script metadata")
        .permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&script_path, perms).expect("set hook script executable");
    script_path
}

#[cfg(windows)]
fn write_actuator_script(dir: &Path) -> PathBuf {
    let script_path = dir.join("actuator.cmd");
    let script = "@echo off\r\nmore > nul\r\necho {\"applied_fix_ids\":[\"remove_unused_import\"],\"skipped_fix_ids\":[],\"errors\":[]}\r\n";
    fs::write(&script_path, script).expect("failed to write actuator script");
    script_path
}

#[cfg(unix)]
fn write_actuator_script(dir: &Path) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;

    let script_path = dir.join("actuator.sh");
    let script = "#!/bin/sh\ncat >/dev/null\nprintf '%s\\n' '{\"applied_fix_ids\":[\"remove_unused_import\"],\"skipped_fix_ids\":[],\"errors\":[]}'\n";
    fs::write(&script_path, script).expect("failed to write actuator script");
    let mut perms = fs::metadata(&script_path)
        .expect("actuator script metadata")
        .permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&script_path, perms).expect("set actuator script executable");
    script_path
}

fn json_value_at_path<'a>(value: &'a Value, dotted_path: &str) -> Option<&'a Value> {
    let mut current = value;
    for segment in dotted_path.split('.') {
        if segment.is_empty() {
            continue;
        }
        if let Ok(idx) = segment.parse::<usize>() {
            current = current.as_array()?.get(idx)?;
        } else {
            current = current.get(segment)?;
        }
    }
    Some(current)
}

#[given(expr = "a baseline report from fixture {string}")]
fn given_baseline_report_from_fixture(world: &mut IngestWorld, fixture_name: String) {
    let dst = world
        .fixture_path
        .as_ref()
        .expect("fixture path not set; load a fixture first")
        .join("baseline.report.json");
    let src_root = world.workspace.join("fixtures").join(&fixture_name);
    let expected = src_root.join("expected").join("report.json");
    let fallback = src_root
        .join("artifacts")
        .join("cockpit")
        .join("report.json");

    let src = if expected.exists() {
        expected
    } else if fallback.exists() {
        fallback
    } else {
        panic!(
            "baseline fixture '{}' has no expected/report.json or artifacts/cockpit/report.json",
            fixture_name
        );
    };

    fs::copy(&src, &dst).unwrap_or_else(|e| {
        panic!(
            "failed to copy baseline report from {:?} to {:?}: {}",
            src, dst, e
        )
    });
    world.baseline_report_path = Some(dst);
}

#[given("a hook script is configured")]
fn given_hook_script_configured(world: &mut IngestWorld) {
    let fixture_root = world
        .fixture_path
        .as_ref()
        .expect("fixture path not set; load a fixture first");
    let script_path = write_hook_script(fixture_root);
    let escaped = toml_escape_path(&script_path);

    let config_path = fixture_root.join("cockpit.toml");
    let mut config = fs::read_to_string(&config_path).expect("failed to read cockpit.toml");
    if !config.ends_with('\n') {
        config.push('\n');
    }
    config.push_str(&format!(
        "\n[[hooks]]\nname = \"hook-notes\"\ncommand = \"{}\"\ntimeout_ms = 5000\n",
        escaped
    ));
    fs::write(&config_path, config).expect("failed to write cockpit.toml hook config");
    world.hook_script_path = Some(script_path);
}

#[given("a successful buildfix actuator script")]
fn given_successful_buildfix_actuator_script(world: &mut IngestWorld) {
    let fixture_root = world
        .fixture_path
        .as_ref()
        .expect("fixture path not set; load a fixture first");
    world.actuator_script_path = Some(write_actuator_script(fixture_root));
}

#[given("a policy signing key file")]
fn given_policy_signing_key_file(world: &mut IngestWorld) {
    let fixture_root = world
        .fixture_path
        .as_ref()
        .expect("fixture path not set; load a fixture first");
    let key_path = fixture_root.join("policy-signing.key");
    fs::write(&key_path, b"shared-signing-secret\n").expect("failed to write policy key file");
    world.policy_sign_key_path = Some(key_path);
}

// ─────────────────────────────────────────────────────────────────────────────
// When steps
// ─────────────────────────────────────────────────────────────────────────────

#[when(expr = "I run {string} on the fixture")]
fn run_ingest(world: &mut IngestWorld, _command: String) {
    run_ingest_impl(world);
}

#[when(expr = "I run {string} on the fixture with {string}")]
fn run_ingest_with_args(world: &mut IngestWorld, _command: String, args: String) {
    world.extra_args = args
        .split_whitespace()
        .map(|arg| world.resolve_arg_placeholder(arg))
        .collect();
    run_ingest_impl(world);
}

#[when(expr = "I run {string} on the fixture again")]
fn run_ingest_again(world: &mut IngestWorld, _command: String) {
    // Clean outputs before re-run
    if let Some(ref fixture_path) = world.fixture_path {
        let out_dir = fixture_path.join("artifacts").join("cockpit");
        let _ = fs::remove_dir_all(&out_dir);
    }
    run_ingest_impl(world);
}

#[when("I capture the report")]
fn capture_report(world: &mut IngestWorld) {
    let report = world.read_report();
    world.captured_report = Some(serde_json::to_string_pretty(&report).unwrap());
}

fn run_ingest_impl(world: &mut IngestWorld) {
    let fixture_path = world.fixture_path.as_ref().expect("fixture not set up");
    let artifacts = fixture_path.join("artifacts");
    let config = fixture_path.join("cockpit.toml");

    let mut cmd = cockpitctl_cmd();
    cmd.env("COCKPITCTL_STARTED_AT", "2026-02-02T12:00:00Z");
    cmd.args([
        "ingest",
        "--artifacts",
        artifacts.to_string_lossy().as_ref(),
        "--config",
        config.to_string_lossy().as_ref(),
    ]);

    for arg in &world.extra_args {
        cmd.arg(arg);
    }

    let output = cmd.output().expect("failed to execute command");
    world.exit_code = Some(output.status.code().unwrap_or(-1));
    world.stdout = String::from_utf8_lossy(&output.stdout).to_string();
    world.stderr = String::from_utf8_lossy(&output.stderr).to_string();

    let out_dir = artifacts.join("cockpit");
    world.report_path = Some(out_dir.join("report.json"));
    world.comment_path = Some(out_dir.join("comment.md"));
}

// ─────────────────────────────────────────────────────────────────────────────
// Then steps - Exit codes
// ─────────────────────────────────────────────────────────────────────────────

#[then(expr = "the exit code is {int}")]
fn check_exit_code(world: &mut IngestWorld, expected: i32) {
    let actual = world.exit_code.expect("command not run");
    assert_eq!(actual, expected, "exit code mismatch");
}

#[then(expr = "stdout contains {string}")]
fn stdout_contains(world: &mut IngestWorld, expected: String) {
    assert!(
        world.stdout.contains(&expected),
        "stdout does not contain '{}'\n\nActual stdout:\n{}",
        expected,
        world.stdout
    );
}

#[then(expr = "stderr contains {string}")]
fn stderr_contains(world: &mut IngestWorld, expected: String) {
    assert!(
        world.stderr.contains(&expected),
        "stderr does not contain '{}'\n\nActual stderr:\n{}",
        expected,
        world.stderr
    );
}

#[then(expr = "stdout has exactly {int} lines starting with {string}")]
fn stdout_line_prefix_count(world: &mut IngestWorld, expected: usize, prefix: String) {
    let count = world
        .stdout
        .lines()
        .filter(|line| line.starts_with(&prefix))
        .count();
    assert_eq!(
        count, expected,
        "expected {} stdout lines starting with '{}', got {}\n\nActual stdout:\n{}",
        expected, prefix, count, world.stdout
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Then steps - Golden file comparisons
// ─────────────────────────────────────────────────────────────────────────────

#[then("the cockpit report matches the golden file")]
fn report_matches_golden(world: &mut IngestWorld) {
    let fixture_path = world.fixture_path.as_ref().expect("fixture not set");
    let expected_path = fixture_path.join("expected").join("report.json");

    let got =
        fs::read_to_string(world.report_path.as_ref().unwrap()).expect("failed to read report");
    let exp = fs::read_to_string(&expected_path).expect("failed to read expected report");

    pretty_assertions::assert_eq!(got, exp, "report mismatch");
}

#[then("the cockpit comment matches the golden file")]
fn comment_matches_golden(world: &mut IngestWorld) {
    let fixture_path = world.fixture_path.as_ref().expect("fixture not set");
    let expected_path = fixture_path.join("expected").join("comment.md");

    let got = world.read_comment();
    let exp = fs::read_to_string(&expected_path).expect("failed to read expected comment");

    pretty_assertions::assert_eq!(got, exp, "comment mismatch");
}

// ─────────────────────────────────────────────────────────────────────────────
// Then steps - Verdict assertions
// ─────────────────────────────────────────────────────────────────────────────

#[then(expr = "the verdict status is {string}")]
fn check_verdict_status(world: &mut IngestWorld, expected: String) {
    let report = world.read_report();
    let status = report
        .get("verdict")
        .and_then(|v| v.get("status"))
        .and_then(|s| s.as_str())
        .expect("verdict.status not found");

    assert_eq!(status, expected, "verdict status mismatch");
}

// ─────────────────────────────────────────────────────────────────────────────
// Then steps - Highlight assertions
// ─────────────────────────────────────────────────────────────────────────────

#[then(expr = "the cockpit report contains a highlight {string}")]
fn report_contains_highlight(world: &mut IngestWorld, code: String) {
    let report = world.read_report();
    let highlights = report
        .get("highlights")
        .and_then(|h| h.as_array())
        .expect("highlights array not found");

    let found = highlights.iter().any(|h| {
        h.get("finding")
            .and_then(|f| f.get("code"))
            .and_then(|c| c.as_str())
            == Some(&code)
    });

    assert!(
        found,
        "highlight with code '{}' not found in {:?}",
        code,
        highlights
            .iter()
            .filter_map(|h| h
                .get("finding")
                .and_then(|f| f.get("code"))
                .and_then(|c| c.as_str()))
            .collect::<Vec<_>>()
    );
}

#[then("the highlights array is empty")]
fn highlights_array_empty(world: &mut IngestWorld) {
    let report = world.read_report();
    let highlights = report
        .get("highlights")
        .and_then(|h| h.as_array())
        .expect("highlights array not found");

    assert!(
        highlights.is_empty(),
        "expected empty highlights, got {} items",
        highlights.len()
    );
}

#[then(expr = "the highlights count is exactly {int}")]
fn highlights_count_exactly(world: &mut IngestWorld, expected: usize) {
    let report = world.read_report();
    let highlights = report
        .get("highlights")
        .and_then(|h| h.as_array())
        .expect("highlights array not found");

    assert_eq!(
        highlights.len(),
        expected,
        "expected {} highlights, got {}",
        expected,
        highlights.len()
    );
}

#[then("the highlights are ordered by severity descending")]
fn highlights_ordered_by_severity(world: &mut IngestWorld) {
    let report = world.read_report();
    let highlights = report
        .get("highlights")
        .and_then(|h| h.as_array())
        .expect("highlights array not found");

    let severity_order = |s: &str| -> i32 {
        match s {
            "error" => 0,
            "warn" => 1,
            "info" => 2,
            _ => 3,
        }
    };

    let severities: Vec<&str> = highlights
        .iter()
        .filter_map(|h| {
            h.get("finding")
                .and_then(|f| f.get("severity"))
                .and_then(|s| s.as_str())
        })
        .collect();

    for i in 1..severities.len() {
        assert!(
            severity_order(severities[i - 1]) <= severity_order(severities[i]),
            "highlights not ordered by severity: {:?}",
            severities
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Then steps - Report structure
// ─────────────────────────────────────────────────────────────────────────────

#[then(expr = "the report schema is {string}")]
fn check_report_schema(world: &mut IngestWorld, expected: String) {
    let report = world.read_report();
    let schema = report
        .get("schema")
        .and_then(|s| s.as_str())
        .expect("schema field not found");

    assert_eq!(schema, expected, "schema mismatch");
}

#[then(expr = "the report contains sensors {string} and {string}")]
fn report_contains_sensors(world: &mut IngestWorld, sensor1: String, sensor2: String) {
    let report = world.read_report();
    let sensors = report
        .get("sensors")
        .and_then(|s| s.as_array())
        .expect("sensors array not found");

    let sensor_ids: Vec<&str> = sensors
        .iter()
        .filter_map(|s| s.get("id").and_then(|id| id.as_str()))
        .collect();

    assert!(
        sensor_ids.contains(&sensor1.as_str()),
        "sensor '{}' not found in {:?}",
        sensor1,
        sensor_ids
    );
    assert!(
        sensor_ids.contains(&sensor2.as_str()),
        "sensor '{}' not found in {:?}",
        sensor2,
        sensor_ids
    );
}

#[then(expr = "the sensor {string} has verdict status {string}")]
fn sensor_has_verdict_status(world: &mut IngestWorld, sensor_id: String, expected_status: String) {
    let report = world.read_report();
    let sensors = report
        .get("sensors")
        .and_then(|s| s.as_array())
        .expect("sensors array not found");

    let sensor = sensors
        .iter()
        .find(|s| s.get("id").and_then(|id| id.as_str()) == Some(&sensor_id))
        .unwrap_or_else(|| panic!("sensor '{}' not found", sensor_id));

    let status = sensor
        .get("verdict")
        .and_then(|v| v.get("status"))
        .and_then(|s| s.as_str())
        .expect("verdict.status not found for sensor");

    assert_eq!(
        status, expected_status,
        "sensor '{}' verdict status mismatch: expected '{}', got '{}'",
        sensor_id, expected_status, status
    );
}

#[then("the cockpit report is valid JSON")]
fn report_is_valid_json(world: &mut IngestWorld) {
    // read_report() already parses JSON, so this validates it
    let _ = world.read_report();
}

#[then(expr = "the report data contains key {string}")]
fn report_data_contains_key(world: &mut IngestWorld, key: String) {
    let report = world.read_report();
    let data = report
        .get("data")
        .and_then(|v| v.as_object())
        .expect("report.data object not found");
    assert!(
        data.contains_key(&key),
        "report.data does not contain key '{}'. Keys: {:?}",
        key,
        data.keys().collect::<Vec<_>>()
    );
}

#[then(expr = "the report field {string} equals {string}")]
fn report_field_equals_string(world: &mut IngestWorld, path: String, expected: String) {
    let report = world.read_report();
    let actual = json_value_at_path(&report, &path)
        .unwrap_or_else(|| panic!("report field path '{}' not found", path));
    let actual_str = actual
        .as_str()
        .unwrap_or_else(|| panic!("report field '{}' is not a string: {}", path, actual));
    assert_eq!(actual_str, expected, "report field '{}' mismatch", path);
}

#[then(expr = "the report field {string} equals {int}")]
fn report_field_equals_int(world: &mut IngestWorld, path: String, expected: i32) {
    let report = world.read_report();
    let actual = json_value_at_path(&report, &path)
        .unwrap_or_else(|| panic!("report field path '{}' not found", path));
    let actual_int = actual
        .as_i64()
        .unwrap_or_else(|| panic!("report field '{}' is not an integer: {}", path, actual));
    assert_eq!(
        actual_int,
        i64::from(expected),
        "report field '{}' mismatch",
        path
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Then steps - Comment assertions
// ─────────────────────────────────────────────────────────────────────────────

#[then(expr = "the comment contains {string}")]
fn comment_contains(world: &mut IngestWorld, expected: String) {
    let comment = world.read_comment();
    assert!(
        comment.contains(&expected),
        "comment does not contain '{}'\n\nActual comment:\n{}",
        expected,
        comment
    );
}

#[then(expr = "in the comment {string} appears before {string}")]
fn comment_contains_in_order(world: &mut IngestWorld, first: String, second: String) {
    let comment = world.read_comment();
    let first_idx = comment
        .find(&first)
        .unwrap_or_else(|| panic!("'{}' not found in comment", first));
    let second_idx = comment
        .find(&second)
        .unwrap_or_else(|| panic!("'{}' not found in comment", second));
    assert!(
        first_idx < second_idx,
        "'{}' should appear before '{}' in comment\n\n{}",
        first,
        second,
        comment
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Then steps - Edge case assertions
// ─────────────────────────────────────────────────────────────────────────────

#[then(expr = "all finding messages are under {int} characters")]
fn findings_under_limit(world: &mut IngestWorld, limit: usize) {
    let report = world.read_report();
    let empty = vec![];
    let highlights = report
        .get("highlights")
        .and_then(|h| h.as_array())
        .unwrap_or(&empty);

    for h in highlights {
        if let Some(message) = h
            .get("finding")
            .and_then(|f| f.get("message"))
            .and_then(|m| m.as_str())
        {
            assert!(
                message.len() < limit,
                "finding message too long: {} chars (limit: {})",
                message.len(),
                limit
            );
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Then steps - Determinism
// ─────────────────────────────────────────────────────────────────────────────

#[then("the reports are identical")]
fn reports_are_identical(world: &mut IngestWorld) {
    let current = serde_json::to_string_pretty(&world.read_report()).unwrap();
    let captured = world
        .captured_report
        .as_ref()
        .expect("no captured report - call 'I capture the report' first");

    pretty_assertions::assert_eq!(current, *captured, "reports differ across runs");
}

// ─────────────────────────────────────────────────────────────────────────────
// CLI Command Steps (init, validate)
// ─────────────────────────────────────────────────────────────────────────────

#[given("a temporary directory")]
fn given_temp_directory(world: &mut IngestWorld) {
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    world.fixture_path = Some(temp_dir.path().to_path_buf());
    world.temp_dir = Some(temp_dir);
    world.stdout.clear();
    world.stderr.clear();
    world.baseline_report_path = None;
    world.hook_script_path = None;
    world.actuator_script_path = None;
    world.policy_sign_key_path = None;
}

#[given(expr = "a file {string} with content {string}")]
fn given_file_with_content(world: &mut IngestWorld, filename: String, content: String) {
    let dir = world.fixture_path.as_ref().expect("temp directory not set");
    let path = dir.join(&filename);
    fs::write(&path, &content).expect("failed to write file");
}

#[when(expr = "I run {string}")]
fn run_cli_command(world: &mut IngestWorld, command: String) {
    let parts: Vec<&str> = command.split_whitespace().collect();
    if parts.len() < 2 || parts[0] != "cockpitctl" {
        panic!("command must start with 'cockpitctl': {}", command);
    }

    let subcommand = parts[1];
    let dir = world.fixture_path.as_ref().expect("directory not set");

    let mut cmd = cockpitctl_cmd();
    cmd.current_dir(dir);
    cmd.arg(subcommand);

    // Add remaining args
    for arg in &parts[2..] {
        cmd.arg(world.resolve_arg_placeholder(arg));
    }

    let output = cmd.output().expect("failed to execute command");
    world.exit_code = Some(output.status.code().unwrap_or(-1));
    world.stdout = String::from_utf8_lossy(&output.stdout).to_string();
    world.stderr = String::from_utf8_lossy(&output.stderr).to_string();
}

#[when(expr = "I run {string} with input {string}")]
fn run_cli_with_input(world: &mut IngestWorld, command: String, input_path: String) {
    let parts: Vec<&str> = command.split_whitespace().collect();
    if parts.len() < 2 || parts[0] != "cockpitctl" {
        panic!("command must start with 'cockpitctl': {}", command);
    }

    let subcommand = parts[1];
    let dir = world.fixture_path.as_ref().expect("directory not set");
    let full_input = dir.join(&input_path);

    let mut cmd = assert_cmd::cargo_bin_cmd!("cockpitctl");
    cmd.current_dir(dir);
    cmd.arg(subcommand);
    cmd.args(["--input", full_input.to_string_lossy().as_ref()]);

    let output = cmd.output().expect("failed to execute command");
    world.exit_code = Some(output.status.code().unwrap_or(-1));
    world.stdout = String::from_utf8_lossy(&output.stdout).to_string();
    world.stderr = String::from_utf8_lossy(&output.stderr).to_string();
}

#[then(expr = "the file {string} exists")]
fn file_exists(world: &mut IngestWorld, filename: String) {
    let dir = world.fixture_path.as_ref().expect("directory not set");
    let path = dir.join(&filename);
    assert!(
        path.exists(),
        "file {} does not exist at {:?}",
        filename,
        path
    );
}

#[then(expr = "the feature {string} is {string}")]
fn feature_gate_assertion(world: &mut IngestWorld, feature: String, state: String) {
    let feature =
        Feature::from_name(&feature).unwrap_or_else(|| panic!("unknown feature '{}'", feature));
    let expected_present =
        parse_feature_state(&state).unwrap_or_else(|| panic!("unknown feature state '{}'", state));
    let feature_present = feature_runtime_present(feature, &world.extra_args);
    let comment = world.read_comment();
    let contract = feature.contract();
    let report = world.read_report();
    let report_data = report.get("data").and_then(|v| v.as_object());
    let artifact_dir = world
        .fixture_path
        .as_ref()
        .expect("fixture path not set")
        .join("artifacts")
        .join("cockpit");
    let mut had_assertion = false;
    if let Some(marker) = contract.comment_marker {
        had_assertion = true;
        let seen = comment.contains(marker);
        assert_eq!(
            seen,
            feature_present,
            "{} feature expected comment marker '{}' {} but was {}",
            feature.as_str(),
            marker,
            expected_present,
            seen
        );
    }
    if let Some(data_key) = contract.report_data_key {
        had_assertion = true;
        let seen = report_data
            .map(|data| data.contains_key(data_key))
            .unwrap_or(false);
        assert_eq!(
            seen,
            feature_present,
            "{} feature expected report key '{}' {} but was {}",
            feature.as_str(),
            data_key,
            expected_present,
            seen
        );
    }
    if let Some(sidecar_file) = contract.sidecar_file {
        had_assertion = true;
        let seen = artifact_dir.join(sidecar_file).exists();
        assert_eq!(
            seen,
            feature_present,
            "{} feature expected sidecar '{}' {} but was {}",
            feature.as_str(),
            sidecar_file,
            expected_present,
            seen
        );
    }
    assert!(
        had_assertion,
        "feature '{}' does not define observable contract fields for BDD assertions",
        feature.as_str()
    );
    if feature_present != expected_present {
        panic!(
            "feature '{}' expected state '{}' but was disabled by runtime flags",
            feature.as_str(),
            state
        );
    }
}

#[then(expr = "the file {string} contains {string}")]
fn file_contains(world: &mut IngestWorld, filename: String, expected: String) {
    let dir = world.fixture_path.as_ref().expect("directory not set");
    let path = dir.join(&filename);
    let content = fs::read_to_string(&path).expect("failed to read file");
    assert!(
        content.contains(&expected),
        "file {} does not contain '{}'\n\nActual content:\n{}",
        filename,
        expected,
        content
    );
}

#[then(expr = "the JSON file {string} field {string} equals {string}")]
fn json_file_field_equals_string(
    world: &mut IngestWorld,
    filename: String,
    field_path: String,
    expected: String,
) {
    let path = world.fixture_file_path(&filename);
    let content = fs::read_to_string(&path).expect("failed to read JSON file");
    let json: Value = serde_json::from_str(&content).expect("failed to parse JSON file");
    let actual = json_value_at_path(&json, &field_path)
        .unwrap_or_else(|| panic!("JSON field '{}' not found in {:?}", field_path, path));
    let actual_str = actual.as_str().unwrap_or_else(|| {
        panic!(
            "JSON field '{}' in {:?} is not a string: {}",
            field_path, path, actual
        )
    });
    assert_eq!(
        actual_str, expected,
        "JSON field '{}' mismatch in {:?}",
        field_path, path
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Main entry point
// ─────────────────────────────────────────────────────────────────────────────

fn main() {
    // Run cucumber with the feature files
    let features = std::env::var("CARGO_MANIFEST_DIR")
        .map(|d| Path::new(&d).join("features"))
        .unwrap_or_else(|_| PathBuf::from("features"));

    futures::executor::block_on(IngestWorld::cucumber().with_default_cli().run(features));
}
