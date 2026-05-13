//! Integration tests for the `xtask` binary.
//!
//! Exercises every subcommand through `assert_cmd`, verifying exit codes
//! and stderr/stdout content for both success and failure paths.

use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

fn cmd() -> Command {
    let mut cmd = assert_cmd::cargo_bin_cmd!("xtask");
    if let Ok(profile) = std::env::var("LLVM_PROFILE_FILE") {
        cmd.env("LLVM_PROFILE_FILE", profile);
    }
    cmd
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn valid_sensor_report(sensor_name: &str) -> String {
    format!(
        r#"{{
  "schema": "{sensor_name}.report.v1",
  "tool": {{ "name": "{sensor_name}", "version": "1.0.0" }},
  "run": {{ "started_at": "2026-02-02T11:00:00Z" }},
  "verdict": {{ "status": "pass", "counts": {{ "info": 0, "warn": 0, "error": 0 }} }},
  "findings": []
}}"#
    )
}

fn write_file(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create dirs");
    }
    fs::write(path, content).expect("write file");
}

// ─────────────────────────────────────────────────────────────────────────────
// schema-sync-check
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn schema_sync_check_succeeds() {
    cmd()
        .current_dir(workspace_root())
        .arg("schema-sync-check")
        .assert()
        .success()
        .stderr(predicate::str::contains("all").and(predicate::str::contains("in sync")));
}

#[test]
fn schema_sync_check_detects_mismatch() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path();

    let schemas = [
        "sensor.report.v1.json",
        "cockpit.report.v1.json",
        "buildfix.plan.v1.json",
        "cockpit.promote.v1.json",
    ];

    for name in &schemas {
        let content = format!(r#"{{"name":"{}"}}"#, name);
        write_file(&root.join("contracts/schemas").join(name), &content);
        write_file(
            &root.join("crates/cockpitctl-types/schemas").join(name),
            &content,
        );
    }

    // Tamper with one schema copy to create a mismatch.
    write_file(
        &root.join("crates/cockpitctl-types/schemas/sensor.report.v1.json"),
        r#"{"tampered":true}"#,
    );

    cmd()
        .current_dir(root)
        .arg("schema-sync-check")
        .assert()
        .failure()
        .stderr(predicate::str::contains("out of sync"));
}

// ─────────────────────────────────────────────────────────────────────────────
// schema-sync-fix
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn schema_sync_fix_repairs_mismatch() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path();

    let schemas = [
        "sensor.report.v1.json",
        "cockpit.report.v1.json",
        "buildfix.plan.v1.json",
        "cockpit.promote.v1.json",
    ];

    for name in &schemas {
        let content = format!(r#"{{"name":"{}"}}"#, name);
        write_file(&root.join("contracts/schemas").join(name), &content);
        write_file(
            &root.join("crates/cockpitctl-types/schemas").join(name),
            "old",
        );
    }

    cmd()
        .current_dir(root)
        .arg("schema-sync-fix")
        .assert()
        .success()
        .stderr(predicate::str::contains("synced"));

    // Verify sync-check now passes.
    cmd()
        .current_dir(root)
        .arg("schema-sync-check")
        .assert()
        .success();
}

// ─────────────────────────────────────────────────────────────────────────────
// fixtures-help
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn fixtures_help_prints_instructions() {
    cmd().arg("fixtures-help").assert().success().stderr(
        predicate::str::contains("Golden fixtures")
            .and(predicate::str::contains("cargo run -p cockpitctl")),
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// schema-check
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn schema_check_succeeds_on_contracts() {
    cmd()
        .current_dir(workspace_root())
        .args(["schema-check", "--dir", "contracts/schemas"])
        .assert()
        .success()
        .stderr(predicate::str::contains("ok:"));
}

#[test]
fn schema_check_fails_on_missing_fields() {
    let temp = TempDir::new().expect("tempdir");
    let dir = temp.path();

    // sensor schema with missing $id
    write_file(
        &dir.join("sensor.report.v1.json"),
        r#"{"title":"Missing id"}"#,
    );
    write_file(
        &dir.join("cockpit.report.v1.json"),
        r#"{"$id":"cockpit.report.v1","title":"Cockpit Report"}"#,
    );

    cmd()
        .args(["schema-check", "--dir"])
        .arg(dir)
        .assert()
        .failure()
        .stderr(predicate::str::contains("schema missing"));
}

// ─────────────────────────────────────────────────────────────────────────────
// validate-schemas
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn validate_schemas_succeeds_on_contracts() {
    cmd()
        .current_dir(workspace_root())
        .args(["validate-schemas", "--dir", "contracts/schemas"])
        .assert()
        .success()
        .stderr(predicate::str::contains("0 error(s)"));
}

#[test]
fn validate_schemas_fails_on_invalid_json() {
    let temp = TempDir::new().expect("tempdir");
    write_file(&temp.path().join("bad.json"), "{");

    cmd()
        .args(["validate-schemas", "--dir"])
        .arg(temp.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("schema validation failed"));
}

#[test]
fn validate_schemas_fails_on_nonexistent_dir() {
    cmd()
        .args(["validate-schemas", "--dir", "nonexistent_dir_12345"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("does not exist"));
}

// ─────────────────────────────────────────────────────────────────────────────
// example-sync-check
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn example_sync_check_succeeds() {
    cmd()
        .current_dir(workspace_root())
        .arg("example-sync-check")
        .assert()
        .success()
        .stderr(predicate::str::contains("in sync"));
}

// ─────────────────────────────────────────────────────────────────────────────
// conform (single report)
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn conform_valid_receipt_passes() {
    let temp = TempDir::new().expect("tempdir");
    let report = temp.path().join("report.json");
    write_file(&report, &valid_sensor_report("testsensor"));

    cmd()
        .args(["conform", "--report"])
        .arg(&report)
        .args(["--sensor-id", "testsensor", "--all"])
        .assert()
        .success()
        .stderr(predicate::str::contains("PASS"));
}

#[test]
fn conform_ordering_requires_sensor_id() {
    let temp = TempDir::new().expect("tempdir");
    let report = temp.path().join("report.json");
    write_file(&report, &valid_sensor_report("testsensor"));

    cmd()
        .args(["conform", "--report"])
        .arg(&report)
        .arg("--ordering")
        .assert()
        .failure()
        .stderr(predicate::str::contains("requires --sensor-id"));
}

#[test]
fn conform_fails_on_invalid_json() {
    let temp = TempDir::new().expect("tempdir");
    let report = temp.path().join("report.json");
    write_file(&report, "not json at all");

    cmd()
        .args(["conform", "--report"])
        .arg(&report)
        .assert()
        .failure();
}

#[test]
fn conform_golden_mismatch_fails() {
    let temp = TempDir::new().expect("tempdir");
    let report = temp.path().join("report.json");
    let golden = temp.path().join("golden.json");
    write_file(&report, &valid_sensor_report("testsensor"));
    write_file(&golden, r#"{"different":"content"}"#);

    cmd()
        .args(["conform", "--report"])
        .arg(&report)
        .args(["--golden"])
        .arg(&golden)
        .assert()
        .failure()
        .stderr(predicate::str::contains("determinism check failed"));
}

// ─────────────────────────────────────────────────────────────────────────────
// conform-dir
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn conform_dir_with_valid_sensors_passes() {
    let temp = TempDir::new().expect("tempdir");
    let artifacts = temp.path();

    write_file(
        &artifacts.join("sensor_a/report.json"),
        &valid_sensor_report("sensor_a"),
    );
    write_file(
        &artifacts.join("sensor_b/report.json"),
        &valid_sensor_report("sensor_b"),
    );

    cmd()
        .args(["conform-dir", "--dir"])
        .arg(artifacts)
        .arg("--all")
        .assert()
        .success()
        .stderr(
            predicate::str::contains("PASS").and(predicate::str::contains("2 sensor(s) checked")),
        );
}

#[test]
fn conform_dir_fails_on_missing_report() {
    let temp = TempDir::new().expect("tempdir");
    let artifacts = temp.path();

    // Sensor directory with no report.json
    fs::create_dir_all(artifacts.join("empty_sensor")).expect("create dir");

    cmd()
        .args(["conform-dir", "--dir"])
        .arg(artifacts)
        .assert()
        .failure()
        .stderr(predicate::str::contains("no report.json"));
}

#[test]
fn conform_dir_allow_missing_report_skips() {
    let temp = TempDir::new().expect("tempdir");
    let artifacts = temp.path();

    fs::create_dir_all(artifacts.join("empty_sensor")).expect("create dir");
    write_file(
        &artifacts.join("ok_sensor/report.json"),
        &valid_sensor_report("ok_sensor"),
    );

    cmd()
        .args(["conform-dir", "--dir"])
        .arg(artifacts)
        .arg("--allow-missing-report")
        .assert()
        .success()
        .stderr(predicate::str::contains("skip: no report.json"));
}

#[test]
fn conform_dir_fails_on_nonexistent_dir() {
    cmd()
        .args(["conform-dir", "--dir", "nonexistent_dir_12345"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("does not exist"));
}

#[test]
fn conform_dir_validates_cockpit_report() {
    let temp = TempDir::new().expect("tempdir");
    let artifacts = temp.path();

    write_file(
        &artifacts.join("sensor_a/report.json"),
        &valid_sensor_report("sensor_a"),
    );
    // Invalid cockpit report
    write_file(&artifacts.join("cockpit/report.json"), "{}");

    cmd()
        .args(["conform-dir", "--dir"])
        .arg(artifacts)
        .arg("--validate-cockpit")
        .assert()
        .failure()
        .stderr(predicate::str::contains("cockpit report schema validation"));
}

// ─────────────────────────────────────────────────────────────────────────────
// CLI argument parsing / help / unknown commands
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn no_args_shows_help_and_fails() {
    cmd()
        .assert()
        .failure()
        .stderr(predicate::str::contains("Usage"));
}

#[test]
fn unknown_subcommand_fails() {
    cmd().arg("nonexistent-command").assert().failure().stderr(
        predicate::str::contains("unrecognized subcommand").or(predicate::str::contains("invalid")),
    );
}

#[test]
fn help_flag_succeeds() {
    cmd().arg("--help").assert().success().stdout(
        predicate::str::contains("schema-sync-check")
            .and(predicate::str::contains("fixtures-help"))
            .and(predicate::str::contains("conform")),
    );
}

#[test]
fn conform_missing_required_report_arg_fails() {
    cmd()
        .arg("conform")
        .assert()
        .failure()
        .stderr(predicate::str::contains("--report"));
}

#[test]
fn conform_dir_missing_required_dir_arg_fails() {
    cmd()
        .arg("conform-dir")
        .assert()
        .failure()
        .stderr(predicate::str::contains("--dir"));
}

// ─────────────────────────────────────────────────────────────────────────────
// badge endpoints
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(unix)]
fn fake_ripr_script(dir: &Path) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;

    let path = dir.join("ripr");
    write_file(
        &path,
        r#"#!/usr/bin/env sh
set -eu
if [ "${1:-}" = "check" ] && [ "${5:-}" = "repo-badge-plus-shields" ]; then
  printf '{"schemaVersion":1,"label":"ripr+","message":"0","color":"brightgreen"}\n'
  exit 0
fi
if [ "${1:-}" = "check" ]; then
  printf '{"findings":[]}\n'
  exit 0
fi
if [ "${1:-}" = "review-comments" ]; then
  out=""
  while [ "$#" -gt 0 ]; do
    if [ "$1" = "--out" ]; then
      shift
      out="$1"
    fi
    shift || true
  done
  mkdir -p "$(dirname "$out")"
  printf '{"comments":[],"summary_only":[],"suppressed":[],"warnings":[]}\n' > "$out"
  printf '# RIPR Review Guidance\n\nNo line-placeable guidance.\n' > "$(dirname "$out")/comments.md"
  exit 0
fi
echo "unexpected ripr args: $*" >&2
exit 2
"#,
    );
    let mut perms = fs::metadata(&path)
        .expect("fake ripr metadata")
        .permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&path, perms).expect("fake ripr executable");
    path
}

#[test]
#[cfg(unix)]
fn badges_check_accepts_generated_endpoint_shape() {
    let temp = TempDir::new().expect("tempdir");
    let fake_ripr = fake_ripr_script(temp.path());

    cmd()
        .current_dir(workspace_root())
        .env("RIPR_BIN", fake_ripr)
        .args(["badges", "--check"])
        .assert()
        .success()
        .stderr(predicate::str::contains("committed endpoints are current"));
}

#[test]
#[cfg(unix)]
fn ripr_pr_check_validates_generated_contract() {
    let temp = TempDir::new().expect("tempdir");
    let fake_ripr = fake_ripr_script(temp.path());

    cmd()
        .current_dir(workspace_root())
        .env("RIPR_BIN", fake_ripr)
        .arg("ripr-pr")
        .assert()
        .success()
        .stderr(predicate::str::contains("wrote PR evidence"));

    cmd()
        .current_dir(workspace_root())
        .args(["ripr-pr", "--check"])
        .assert()
        .success()
        .stderr(predicate::str::contains("output contract is intact"));
}

#[test]
#[cfg(unix)]
fn ripr_review_comments_check_validates_generated_contract() {
    let temp = TempDir::new().expect("tempdir");
    let fake_ripr = fake_ripr_script(temp.path());

    cmd()
        .current_dir(workspace_root())
        .env("RIPR_BIN", fake_ripr)
        .arg("ripr-review-comments")
        .assert()
        .success()
        .stderr(predicate::str::contains("wrote review guidance"));

    cmd()
        .current_dir(workspace_root())
        .args(["ripr-review-comments", "--check"])
        .assert()
        .success()
        .stderr(predicate::str::contains("output contract is intact"));
}
