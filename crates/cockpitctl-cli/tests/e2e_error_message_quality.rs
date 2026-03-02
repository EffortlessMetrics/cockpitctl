//! Error message quality and actionability tests.
//!
//! These tests verify that user-facing errors are:
//! - **Clear**: include the file/path/sensor involved
//! - **Actionable**: suggest what to do when applicable
//! - **Safe**: never expose internal stack traces or panics

use assert_cmd::Command;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

fn cmd() -> Command {
    let mut cmd = assert_cmd::cargo_bin_cmd!("cockpitctl");
    cmd.env("COCKPITCTL_STARTED_AT", "2026-02-02T12:00:00Z");
    if let Ok(profile) = std::env::var("LLVM_PROFILE_FILE") {
        cmd.env("LLVM_PROFILE_FILE", profile);
    }
    cmd
}

fn valid_sensor_report(name: &str) -> String {
    format!(
        r#"{{
  "schema": "sensor.report.v1",
  "tool": {{ "name": "{name}", "version": "1.0.0" }},
  "run": {{ "started_at": "2026-02-02T11:00:00Z" }},
  "verdict": {{ "status": "pass", "counts": {{ "info": 0, "warn": 0, "error": 0 }} }},
  "findings": []
}}"#
    )
}

struct TestSetup {
    _temp_dir: TempDir,
    artifacts_dir: PathBuf,
    config_path: PathBuf,
}

impl TestSetup {
    fn new() -> Self {
        let temp_dir = TempDir::new().expect("create temp dir");
        let artifacts_dir = temp_dir.path().join("artifacts");
        let config_path = temp_dir.path().join("cockpit.toml");
        fs::create_dir_all(&artifacts_dir).expect("create artifacts dir");
        Self {
            _temp_dir: temp_dir,
            artifacts_dir,
            config_path,
        }
    }

    fn write_config(&self, content: &str) {
        fs::write(&self.config_path, content).expect("write config");
    }

    fn write_sensor_report(&self, sensor_id: &str, content: &str) {
        let sensor_dir = self.artifacts_dir.join(sensor_id);
        fs::create_dir_all(&sensor_dir).expect("create sensor dir");
        fs::write(sensor_dir.join("report.json"), content).expect("write report");
    }

    fn artifacts_arg(&self) -> String {
        self.artifacts_dir.to_string_lossy().to_string()
    }

    fn config_arg(&self) -> String {
        self.config_path.to_string_lossy().to_string()
    }

    fn cockpit_report_json(&self) -> serde_json::Value {
        let path = self.artifacts_dir.join("cockpit").join("report.json");
        let raw = fs::read_to_string(&path).expect("read cockpit report");
        serde_json::from_str(&raw).expect("parse cockpit report")
    }
}

/// Assert that output does not look like an internal panic or stack trace.
fn assert_no_panic_or_stacktrace(stderr: &str) {
    assert!(
        !stderr.contains("thread 'main' panicked"),
        "stderr should not contain a panic: {stderr}"
    );
    assert!(
        !stderr.contains("stack backtrace:"),
        "stderr should not contain a stack backtrace: {stderr}"
    );
    assert!(
        !stderr.contains("RUST_BACKTRACE"),
        "stderr should not suggest RUST_BACKTRACE: {stderr}"
    );
}

// ═════════════════════════════════════════════════════════════════════════════
// 1. CLI argument error messages
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn ingest_unknown_flag_mentions_flag_name() {
    let output = cmd()
        .args(["ingest", "--nonexistent-flag"])
        .output()
        .expect("run");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("--nonexistent-flag"),
        "error should echo the unknown flag back to the user: {stderr}"
    );
    assert_no_panic_or_stacktrace(&stderr);
}

#[test]
fn schema_validation_bad_value_shows_valid_options() {
    let setup = TestSetup::new();
    setup.write_config("[policy]\n");
    setup.write_sensor_report("s", &valid_sensor_report("s"));

    let output = cmd()
        .args([
            "ingest",
            "--artifacts",
            &setup.artifacts_arg(),
            "--config",
            &setup.config_arg(),
            "--schema-validation",
            "bogus",
        ])
        .output()
        .expect("run");
    let stderr = String::from_utf8_lossy(&output.stderr);
    // clap should mention the valid values
    assert!(
        stderr.contains("lax") || stderr.contains("strict"),
        "error should list valid schema-validation values: {stderr}"
    );
    assert_no_panic_or_stacktrace(&stderr);
}

#[test]
fn no_subcommand_shows_usage_with_available_commands() {
    let output = cmd().output().expect("run");
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    // Should mention at least the main subcommands
    assert!(
        combined.contains("ingest") && combined.contains("validate"),
        "no-subcommand output should list available commands: {combined}"
    );
    assert_no_panic_or_stacktrace(&String::from_utf8_lossy(&output.stderr));
}

// ═════════════════════════════════════════════════════════════════════════════
// 2. Config / IO error messages
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn malformed_toml_error_includes_parse_details() {
    let setup = TestSetup::new();
    setup.write_config("[policy\nkey = ???");
    setup.write_sensor_report("alpha", &valid_sensor_report("alpha"));

    let output = cmd()
        .args([
            "ingest",
            "--artifacts",
            &setup.artifacts_arg(),
            "--config",
            &setup.config_arg(),
        ])
        .output()
        .expect("run");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "malformed toml should fail");
    // Error should include some indication of where parsing failed (line/col or TOML keyword)
    assert!(
        stderr.contains("parse")
            || stderr.contains("TOML")
            || stderr.contains("toml")
            || stderr.contains("expected"),
        "malformed TOML error should include parse context: {stderr}"
    );
    assert_no_panic_or_stacktrace(&stderr);
}

#[test]
fn init_refuses_overwrite_mentions_path() {
    let setup = TestSetup::new();
    // Create an existing file at the init path
    let init_path = setup._temp_dir.path().join("cockpit.toml");
    fs::write(&init_path, "existing").expect("write");

    let output = cmd()
        .args(["init", "--path", &init_path.to_string_lossy()])
        .output()
        .expect("run");
    let stderr = String::from_utf8_lossy(&output.stderr);
    // Should mention the path and refusal
    assert!(
        stderr.contains("cockpit.toml"),
        "init overwrite refusal should mention the file path: {stderr}"
    );
    assert!(
        stderr.contains("refusing") || stderr.contains("overwrite") || stderr.contains("exists"),
        "init overwrite should explain the reason: {stderr}"
    );
    assert_no_panic_or_stacktrace(&stderr);
}

// ═════════════════════════════════════════════════════════════════════════════
// 3. Validate subcommand error messages
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn validate_nonexistent_file_includes_filename_in_error() {
    let output = cmd()
        .args(["validate", "--input", "does_not_exist_42.json", "--lax"])
        .output()
        .expect("run");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("does_not_exist_42.json"),
        "validate error for missing file should include the filename: {stderr}"
    );
    assert_no_panic_or_stacktrace(&stderr);
}

#[test]
fn validate_invalid_json_error_is_not_empty() {
    let dir = TempDir::new().expect("tmpdir");
    let path = dir.path().join("bad.json");
    fs::write(&path, "<<<not json>>>").expect("write");

    let output = cmd()
        .args(["validate", "--input", &path.to_string_lossy(), "--lax"])
        .output()
        .expect("run");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success());
    assert!(
        !stderr.trim().is_empty(),
        "validate should produce a non-empty error for invalid JSON"
    );
    assert_no_panic_or_stacktrace(&stderr);
}

#[test]
fn validate_valid_json_wrong_shape_mentions_what_was_tried() {
    let dir = TempDir::new().expect("tmpdir");
    let path = dir.path().join("wrong.json");
    fs::write(&path, r#"{"foo": "bar"}"#).expect("write");

    let output = cmd()
        .args(["validate", "--input", &path.to_string_lossy(), "--lax"])
        .output()
        .expect("run");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success());
    // Should mention which report types were tried
    assert!(
        stderr.contains("SensorReport")
            || stderr.contains("CockpitReport")
            || stderr.contains("parse"),
        "validate should mention which report shapes were tried: {stderr}"
    );
    assert_no_panic_or_stacktrace(&stderr);
}

// ═════════════════════════════════════════════════════════════════════════════
// 4. Pipeline error messages — sensor identification in cockpit report
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn corrupt_receipt_report_identifies_failing_sensor() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]
[sensors.goodsensor]
blocking = true
missing = "fail"
[sensors.badsensor]
blocking = true
missing = "fail"
"#,
    );
    setup.write_sensor_report("goodsensor", &valid_sensor_report("goodsensor"));
    setup.write_sensor_report("badsensor", "<<< NOT JSON >>>");

    cmd()
        .args([
            "ingest",
            "--artifacts",
            &setup.artifacts_arg(),
            "--config",
            &setup.config_arg(),
        ])
        .assert()
        .code(2);

    let report = setup.cockpit_report_json();
    // The failing sensor should be named in the highlights
    let highlights = report["highlights"].as_array().expect("highlights");
    let bad_highlight = highlights
        .iter()
        .find(|h| h["finding"]["code"].as_str() == Some("cockpit.invalid_receipt"));
    assert!(
        bad_highlight.is_some(),
        "cockpit report should have an invalid_receipt finding for the bad sensor"
    );
    let msg = bad_highlight.unwrap()["finding"]["message"]
        .as_str()
        .unwrap_or("");
    assert!(
        msg.contains("badsensor"),
        "invalid_receipt finding message should name the sensor: {msg}"
    );
    assert!(
        msg.contains("report.json"),
        "invalid_receipt finding message should reference the file path: {msg}"
    );
}

#[test]
fn corrupt_receipt_report_includes_parse_reason() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]
[sensors.parsefail]
blocking = true
missing = "fail"
"#,
    );
    setup.write_sensor_report("parsefail", "{ invalid json }");

    cmd()
        .args([
            "ingest",
            "--artifacts",
            &setup.artifacts_arg(),
            "--config",
            &setup.config_arg(),
        ])
        .assert()
        .code(2);

    let report = setup.cockpit_report_json();
    let sensor = report["sensors"]
        .as_array()
        .expect("sensors")
        .iter()
        .find(|s| s["id"] == "parsefail")
        .expect("parsefail sensor in report");
    let errors = sensor["errors"].as_array().expect("errors array");
    assert!(
        !errors.is_empty(),
        "sensor errors should be populated for a parse failure"
    );
    // The error should give some JSON parse context (line/col or "expected")
    let err_text = errors[0].as_str().unwrap_or("");
    assert!(
        err_text.contains("expected")
            || err_text.contains("line")
            || err_text.contains("column")
            || err_text.contains("key"),
        "parse error should include diagnostic detail, not just 'error': {err_text}"
    );
}

#[test]
fn missing_required_sensor_report_names_sensor() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]
[sensors.required-sensor]
blocking = true
missing = "fail"
"#,
    );
    // Do NOT write a report for required-sensor

    cmd()
        .args([
            "ingest",
            "--artifacts",
            &setup.artifacts_arg(),
            "--config",
            &setup.config_arg(),
        ])
        .assert()
        .code(2);

    let report = setup.cockpit_report_json();
    let sensor = report["sensors"]
        .as_array()
        .expect("sensors")
        .iter()
        .find(|s| s["id"] == "required-sensor")
        .expect("required-sensor in report");
    assert_eq!(
        sensor["presence"].as_str(),
        Some("missing"),
        "sensor should be marked as missing"
    );
    // The highlights should reference the missing sensor
    let highlights = report["highlights"].as_array().expect("highlights");
    let missing_highlight = highlights
        .iter()
        .find(|h| h["finding"]["code"].as_str() == Some("cockpit.missing_receipt"));
    assert!(
        missing_highlight.is_some(),
        "there should be a missing_receipt highlight"
    );
    let msg = missing_highlight.unwrap()["finding"]["message"]
        .as_str()
        .unwrap_or("");
    assert!(
        msg.contains("required-sensor"),
        "missing_receipt message should name the sensor: {msg}"
    );
}

#[test]
fn oversized_receipt_report_includes_size_context() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]
[sensors.bigsensor]
blocking = true
missing = "fail"
"#,
    );
    // Write a report that exceeds 2MB
    let sensor_dir = setup.artifacts_dir.join("bigsensor");
    fs::create_dir_all(&sensor_dir).expect("create sensor dir");
    let oversized = vec![b'x'; 2 * 1024 * 1024 + 1];
    fs::write(sensor_dir.join("report.json"), &oversized).expect("write oversized");

    cmd()
        .args([
            "ingest",
            "--artifacts",
            &setup.artifacts_arg(),
            "--config",
            &setup.config_arg(),
        ])
        .assert()
        .code(2);

    let report = setup.cockpit_report_json();
    let highlights = report["highlights"].as_array().expect("highlights");
    let oversized_highlight = highlights
        .iter()
        .find(|h| h["finding"]["code"].as_str() == Some("cockpit.receipt_oversized"));
    assert!(
        oversized_highlight.is_some(),
        "should have a receipt_oversized highlight"
    );
    let msg = oversized_highlight.unwrap()["finding"]["message"]
        .as_str()
        .unwrap_or("");
    assert!(
        msg.contains("bigsensor"),
        "oversized message should name the sensor: {msg}"
    );
    assert!(
        msg.contains("bytes"),
        "oversized message should mention byte sizes: {msg}"
    );
    // Should have a help text
    let help = oversized_highlight.unwrap()["finding"]["help"]
        .as_str()
        .unwrap_or("");
    assert!(
        !help.is_empty(),
        "oversized finding should include actionable help text"
    );
}

#[test]
fn invalid_receipt_finding_includes_help_text() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]
[sensors.broken]
blocking = true
missing = "fail"
"#,
    );
    setup.write_sensor_report("broken", "not json");

    cmd()
        .args([
            "ingest",
            "--artifacts",
            &setup.artifacts_arg(),
            "--config",
            &setup.config_arg(),
        ])
        .assert()
        .code(2);

    let report = setup.cockpit_report_json();
    let highlights = report["highlights"].as_array().expect("highlights");
    let finding = highlights
        .iter()
        .find(|h| h["finding"]["code"].as_str() == Some("cockpit.invalid_receipt"))
        .expect("invalid_receipt highlight");
    let help = finding["finding"]["help"].as_str().unwrap_or("");
    assert!(
        !help.is_empty(),
        "invalid_receipt finding should include help text suggesting how to fix it"
    );
    assert!(
        help.contains("sensor.report.v1") || help.contains("Validate") || help.contains("JSON"),
        "help should reference the expected schema or suggest validation: {help}"
    );
}

// ═════════════════════════════════════════════════════════════════════════════
// 5. No panics or stack traces in any error path
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn no_panic_on_completely_empty_receipt_file() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]
[sensors.empty]
blocking = true
missing = "fail"
"#,
    );
    setup.write_sensor_report("empty", "");

    let output = cmd()
        .args([
            "ingest",
            "--artifacts",
            &setup.artifacts_arg(),
            "--config",
            &setup.config_arg(),
        ])
        .output()
        .expect("run");
    assert_no_panic_or_stacktrace(&String::from_utf8_lossy(&output.stderr));
    // Should still produce outputs
    assert!(
        setup
            .artifacts_dir
            .join("cockpit")
            .join("report.json")
            .exists(),
        "report.json should still be written"
    );
}

#[test]
fn no_panic_on_binary_garbage_receipt() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]
[sensors.garbage]
blocking = true
missing = "fail"
"#,
    );
    let sensor_dir = setup.artifacts_dir.join("garbage");
    fs::create_dir_all(&sensor_dir).expect("create dir");
    // Write actual binary content
    fs::write(
        sensor_dir.join("report.json"),
        [0xFF, 0xFE, 0x00, 0x01, 0xAB],
    )
    .expect("write");

    let output = cmd()
        .args([
            "ingest",
            "--artifacts",
            &setup.artifacts_arg(),
            "--config",
            &setup.config_arg(),
        ])
        .output()
        .expect("run");
    assert_no_panic_or_stacktrace(&String::from_utf8_lossy(&output.stderr));
    assert!(
        setup
            .artifacts_dir
            .join("cockpit")
            .join("report.json")
            .exists(),
        "report.json should still be written even with binary garbage"
    );
}

#[test]
fn no_panic_on_deeply_nested_json_receipt() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]
[sensors.nested]
blocking = false
missing = "fail"
"#,
    );
    // Create deeply nested but valid JSON
    let depth = 64;
    let open: String = "{\"a\":".repeat(depth);
    let close: String = "}".repeat(depth);
    let nested = format!("{open}null{close}");
    setup.write_sensor_report("nested", &nested);

    let output = cmd()
        .args([
            "ingest",
            "--artifacts",
            &setup.artifacts_arg(),
            "--config",
            &setup.config_arg(),
        ])
        .output()
        .expect("run");
    assert_no_panic_or_stacktrace(&String::from_utf8_lossy(&output.stderr));
}

// ═════════════════════════════════════════════════════════════════════════════
// 6. Explain subcommand error messages
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn explain_unknown_code_suggests_list_command() {
    let output = cmd()
        .args(["explain", "nonexistent.code.xyz"])
        .output()
        .expect("run");
    let stderr = String::from_utf8_lossy(&output.stderr);
    // Should mention the unknown code
    assert!(
        stderr.contains("nonexistent.code.xyz") || stderr.contains("unknown"),
        "explain should report the unknown code: {stderr}"
    );
    // Should suggest how to list all codes
    assert!(
        stderr.contains("explain all") || stderr.contains("list"),
        "explain should suggest running 'explain all': {stderr}"
    );
    assert_no_panic_or_stacktrace(&stderr);
}

#[test]
fn explain_known_code_includes_fix_guidance() {
    let output = cmd()
        .args(["explain", "cockpit.invalid_receipt"])
        .output()
        .expect("run");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success());
    // Should include a Fix: section with actionable guidance
    assert!(
        stdout.contains("Fix:") || stdout.contains("fix"),
        "explain should include fix guidance: {stdout}"
    );
    assert!(
        stdout.contains("cockpit.invalid_receipt"),
        "explain should echo the code: {stdout}"
    );
}

// ═════════════════════════════════════════════════════════════════════════════
// 7. Multiple sensor failures — each individually identified
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn multiple_bad_sensors_each_identified_separately() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]
[sensors.sensor-alpha]
blocking = true
missing = "fail"
[sensors.sensor-beta]
blocking = true
missing = "fail"
[sensors.sensor-gamma]
blocking = true
missing = "fail"
"#,
    );
    setup.write_sensor_report("sensor-alpha", "not json alpha");
    setup.write_sensor_report("sensor-beta", "not json beta");
    setup.write_sensor_report("sensor-gamma", &valid_sensor_report("sensor-gamma"));

    cmd()
        .args([
            "ingest",
            "--artifacts",
            &setup.artifacts_arg(),
            "--config",
            &setup.config_arg(),
        ])
        .assert()
        .code(2);

    let report = setup.cockpit_report_json();
    let highlights = report["highlights"].as_array().expect("highlights");

    // Both bad sensors should be individually called out
    let alpha_mentioned = highlights.iter().any(|h| {
        h["finding"]["message"]
            .as_str()
            .unwrap_or("")
            .contains("sensor-alpha")
    });
    let beta_mentioned = highlights.iter().any(|h| {
        h["finding"]["message"]
            .as_str()
            .unwrap_or("")
            .contains("sensor-beta")
    });
    assert!(
        alpha_mentioned,
        "sensor-alpha should be individually identified in highlights"
    );
    assert!(
        beta_mentioned,
        "sensor-beta should be individually identified in highlights"
    );

    // The valid sensor should NOT have an invalid_receipt finding
    let gamma_invalid = highlights.iter().any(|h| {
        h["finding"]["code"].as_str() == Some("cockpit.invalid_receipt")
            && h["finding"]["message"]
                .as_str()
                .unwrap_or("")
                .contains("sensor-gamma")
    });
    assert!(
        !gamma_invalid,
        "valid sensor-gamma should not be flagged as invalid"
    );
}

// ═════════════════════════════════════════════════════════════════════════════
// 8. Config errors are prefixed with "cockpitctl error:"
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn runtime_errors_use_cockpitctl_prefix() {
    let setup = TestSetup::new();
    setup.write_config("[invalid toml [[[ syntax");
    setup.write_sensor_report("s", &valid_sensor_report("s"));

    let output = cmd()
        .args([
            "ingest",
            "--artifacts",
            &setup.artifacts_arg(),
            "--config",
            &setup.config_arg(),
        ])
        .output()
        .expect("run");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("cockpitctl error:") || stderr.contains("cockpitctl:"),
        "runtime errors should be prefixed with the tool name: {stderr}"
    );
}

#[test]
fn validate_runtime_errors_use_cockpitctl_prefix() {
    let output = cmd()
        .args(["validate", "--input", "nonexistent_xyz.json", "--lax"])
        .output()
        .expect("run");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("cockpitctl error:") || stderr.contains("cockpitctl:"),
        "validate errors should be prefixed with the tool name: {stderr}"
    );
}
