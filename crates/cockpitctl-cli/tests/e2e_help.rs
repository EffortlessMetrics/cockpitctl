//! Expanded end-to-end tests for CLI help text and error messages.
//!
//! Covers: top-level help, version, subcommand help, unknown commands,
//! missing required arguments, nonexistent paths, short flags, and
//! invalid argument values.

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

/// Build a `cockpitctl` command, forwarding the LLVM coverage env if set.
fn cmd() -> Command {
    let mut cmd = assert_cmd::cargo_bin_cmd!("cockpitctl");
    if let Ok(profile) = std::env::var("LLVM_PROFILE_FILE") {
        cmd.env("LLVM_PROFILE_FILE", profile);
    }
    cmd
}

// =============================================================================
// 1. cockpitctl --help → exits 0, shows "cockpitctl"
// =============================================================================

#[test]
fn top_level_help_exits_zero_and_shows_name() {
    cmd()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("cockpitctl"));
}

// =============================================================================
// 2. cockpitctl --version → exits 0, shows version number
// =============================================================================

#[test]
fn version_flag_exits_zero_and_shows_version() {
    cmd()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("0.3.0"));
}

// =============================================================================
// 3. cockpitctl ingest --help → exits 0, shows "artifacts"
// =============================================================================

#[test]
fn ingest_help_mentions_artifacts() {
    cmd()
        .args(["ingest", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("artifacts"));
}

// =============================================================================
// 4. cockpitctl init --help → exits 0, shows "path"
// =============================================================================

#[test]
fn init_help_mentions_path() {
    cmd()
        .args(["init", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("path"));
}

// =============================================================================
// 5. cockpitctl validate --help → exits 0, shows "input"
// =============================================================================

#[test]
fn validate_help_mentions_input() {
    cmd()
        .args(["validate", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("input"));
}

// =============================================================================
// 6. cockpitctl unknown-command → exits non-zero
// =============================================================================

#[test]
fn unknown_command_exits_nonzero() {
    cmd().arg("unknown-command").assert().failure();
}

// =============================================================================
// 7. cockpitctl ingest without required args → useful error message
// =============================================================================

#[test]
fn ingest_without_args_uses_defaults_or_errors_usefully() {
    // `ingest` has defaults for --artifacts and --config, so it will run.
    // Without a real artifacts dir it may succeed (pass-through) or fail
    // with a useful exit code — never a panic.
    let assert = cmd().arg("ingest").assert();
    let code = assert.get_output().status.code().unwrap_or(-1);
    assert!(
        code == 0 || code == 1 || code == 2,
        "expected exit 0, 1, or 2 but got {code}"
    );
}

// =============================================================================
// 8. cockpitctl ingest --artifacts nonexistent → error with path info
// =============================================================================

#[test]
fn ingest_nonexistent_artifacts_dir_exits_cleanly() {
    let tmp = TempDir::new().unwrap();
    let config = tmp.path().join("cockpit.toml");
    fs::write(
        &config,
        "[policy]\n[sensors.required]\nblocking = true\nmissing = \"fail\"\n",
    )
    .unwrap();

    // Nonexistent artifacts with a blocking sensor → exit 2 (policy fail)
    cmd()
        .args([
            "ingest",
            "--artifacts",
            tmp.path().join("nonexistent").to_string_lossy().as_ref(),
            "--config",
            config.to_string_lossy().as_ref(),
        ])
        .assert()
        .code(2);
}

// =============================================================================
// 9. cockpitctl validate --input nonexistent.json → error about missing file
// =============================================================================

#[test]
fn validate_nonexistent_file_errors_with_path() {
    cmd()
        .args(["validate", "--input", "nonexistent_file_abc.json", "--lax"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("nonexistent_file_abc.json"));
}

// =============================================================================
// 10. cockpitctl init --path → creates config or errors usefully
// =============================================================================

#[test]
fn init_creates_config_or_errors_usefully() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("cockpit.toml");

    let assert = cmd()
        .args(["init", "--path", path.to_string_lossy().as_ref()])
        .assert();

    let code = assert.get_output().status.code().unwrap_or(-1);
    if code == 0 {
        assert!(
            path.exists(),
            "init should create the config file on success"
        );
    }
    // Either success or a useful exit code (not a panic)
    assert!(code == 0 || code == 1, "expected exit 0 or 1, got {code}");
}

// =============================================================================
// 11. Help text contains all subcommands (ingest, init, validate, explain)
// =============================================================================

#[test]
fn help_text_lists_all_subcommands() {
    cmd().arg("--help").assert().success().stdout(
        predicate::str::contains("ingest")
            .and(predicate::str::contains("init"))
            .and(predicate::str::contains("validate"))
            .and(predicate::str::contains("explain")),
    );
}

// =============================================================================
// 12. No deprecated flags in help output
// =============================================================================

#[test]
fn help_text_has_no_deprecated_flags() {
    let output = cmd().arg("--help").output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        !stdout.contains("deprecated"),
        "top-level help should not mention deprecated flags: {stdout}"
    );
    assert!(
        !stdout.contains("DEPRECATED"),
        "top-level help should not mention DEPRECATED flags: {stdout}"
    );
}

// =============================================================================
// 13. cockpitctl ingest --schema-validation invalid → useful error
// =============================================================================

#[test]
fn ingest_invalid_schema_validation_value_errors() {
    cmd()
        .args(["ingest", "--schema-validation", "invalid_value"])
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("invalid value")
                .or(predicate::str::contains("error"))
                .or(predicate::str::contains("possible values")),
        );
}

// =============================================================================
// 14. cockpitctl ingest --config nonexistent.toml → defaults or errors
// =============================================================================

#[test]
fn ingest_nonexistent_config_uses_defaults() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    fs::create_dir_all(&artifacts).unwrap();

    // Write a sensor so ingest has something to process
    let sensor_dir = artifacts.join("testsensor");
    fs::create_dir_all(&sensor_dir).unwrap();
    fs::write(
        sensor_dir.join("report.json"),
        r#"{
  "schema": "sensor.report.v1",
  "tool": { "name": "testsensor", "version": "1.0.0" },
  "run": { "started_at": "2026-01-01T00:00:00Z" },
  "verdict": { "status": "pass", "counts": { "info": 0, "warn": 0, "error": 0 } },
  "findings": []
}"#,
    )
    .unwrap();

    // Nonexistent config → should continue with defaults
    cmd()
        .args([
            "ingest",
            "--artifacts",
            artifacts.to_string_lossy().as_ref(),
            "--config",
            tmp.path()
                .join("does_not_exist.toml")
                .to_string_lossy()
                .as_ref(),
        ])
        .assert()
        .success();
}

// =============================================================================
// 15. Short flags work: -h and -V
// =============================================================================

#[test]
fn short_help_flag_works() {
    cmd()
        .arg("-h")
        .assert()
        .success()
        .stdout(predicate::str::contains("cockpitctl"));
}

#[test]
fn short_version_flag_works() {
    cmd()
        .arg("-V")
        .assert()
        .success()
        .stdout(predicate::str::contains("cockpitctl"));
}

// =============================================================================
// Bonus: ingest subcommand help shows schema-validation options
// =============================================================================

#[test]
fn ingest_help_shows_schema_validation_options() {
    cmd().args(["ingest", "--help"]).assert().success().stdout(
        predicate::str::contains("--schema-validation").and(predicate::str::contains("--config")),
    );
}

// =============================================================================
// Bonus: explain --help shows usage
// =============================================================================

#[test]
fn explain_help_mentions_code() {
    cmd()
        .args(["explain", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("code"));
}

// =============================================================================
// Bonus: validate missing --input → clap error with --input hint
// =============================================================================

#[test]
fn validate_missing_required_input_shows_error() {
    cmd()
        .arg("validate")
        .assert()
        .failure()
        .stderr(predicate::str::contains("--input"));
}
