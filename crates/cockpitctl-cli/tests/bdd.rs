//! BDD tests using cucumber-rs for cockpitctl.
//!
//! Runs feature files from `features/` directory using proper Gherkin syntax.

use cucumber::{given, then, when, World};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

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

// ─────────────────────────────────────────────────────────────────────────────
// When steps
// ─────────────────────────────────────────────────────────────────────────────

#[when(expr = "I run {string} on the fixture")]
fn run_ingest(world: &mut IngestWorld, _command: String) {
    run_ingest_impl(world);
}

#[when(expr = "I run {string} on the fixture with {string}")]
fn run_ingest_with_args(world: &mut IngestWorld, _command: String, args: String) {
    world.extra_args = args.split_whitespace().map(String::from).collect();
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

    let mut cmd = assert_cmd::cargo_bin_cmd!("cockpitctl");
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

    let mut cmd = assert_cmd::cargo_bin_cmd!("cockpitctl");
    cmd.current_dir(dir);
    cmd.arg(subcommand);

    // Add remaining args
    for arg in &parts[2..] {
        cmd.arg(arg);
    }

    let output = cmd.output().expect("failed to execute command");
    world.exit_code = Some(output.status.code().unwrap_or(-1));
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
