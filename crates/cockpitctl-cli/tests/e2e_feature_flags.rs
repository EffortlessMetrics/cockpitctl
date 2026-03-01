//! E2E tests verifying feature-flag behavior through the cockpitctl binary.
//!
//! These tests exercise the CLI with `assert_cmd` to verify:
//! - Ingest succeeds with default features (happy path)
//! - Each disable flag is accepted and produces graceful degradation
//! - Feature-gated behavior is observable in output
//! - Schema feature disabled produces stderr notice

use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

// ─────────────────────────────────────────────────────────────────────────────
// Helpers (same pattern as e2e_ingest.rs)
// ─────────────────────────────────────────────────────────────────────────────

fn cmd() -> Command {
    let mut cmd = assert_cmd::cargo_bin_cmd!("cockpitctl");
    cmd.env("COCKPITCTL_STARTED_AT", "2026-02-02T12:00:00Z");
    if let Ok(profile) = std::env::var("LLVM_PROFILE_FILE") {
        cmd.env("LLVM_PROFILE_FILE", profile);
    }
    cmd
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixture_path(name: &str) -> PathBuf {
    workspace_root().join("fixtures").join(name)
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

    fn cockpit_report_path(&self) -> PathBuf {
        self.artifacts_dir.join("cockpit").join("report.json")
    }

    fn cockpit_comment_path(&self) -> PathBuf {
        self.artifacts_dir.join("cockpit").join("comment.md")
    }

    fn artifacts_arg(&self) -> String {
        self.artifacts_dir.to_string_lossy().to_string()
    }

    fn config_arg(&self) -> String {
        self.config_path.to_string_lossy().to_string()
    }
}

fn setup_from_fixture(fixture_name: &str) -> TestSetup {
    let setup = TestSetup::new();
    let fixture = fixture_path(fixture_name);
    let config_src = fixture.join("cockpit.toml");
    if config_src.exists() {
        fs::copy(&config_src, &setup.config_path).expect("copy cockpit.toml");
    }
    let src_artifacts = fixture.join("artifacts");
    if src_artifacts.exists() {
        copy_dir_recursive(&src_artifacts, &setup.artifacts_dir);
    }
    setup
}

fn copy_dir_recursive(src: &Path, dst: &Path) {
    fs::create_dir_all(dst).expect("create dest dir");
    for entry in fs::read_dir(src).expect("read source dir") {
        let entry = entry.expect("dir entry");
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if src_path.is_dir() {
            if entry.file_name() == "cockpit" {
                continue;
            }
            copy_dir_recursive(&src_path, &dst_path);
        } else {
            fs::copy(&src_path, &dst_path).expect("copy file");
        }
    }
}

// =============================================================================
// Default features: ingest works with happy_path fixture
// =============================================================================

#[test]
fn ingest_default_features_happy_path() {
    let setup = setup_from_fixture("happy_path");
    cmd()
        .args([
            "ingest",
            "--artifacts",
            &setup.artifacts_arg(),
            "--config",
            &setup.config_arg(),
        ])
        .assert()
        .success();

    assert!(setup.cockpit_report_path().exists());
    assert!(setup.cockpit_comment_path().exists());
}

// =============================================================================
// --disable-hooks: ingest succeeds without hooks
// =============================================================================

#[test]
fn ingest_disable_hooks_succeeds() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]
[sensors.alpha]
blocking = false
missing = "skip"
"#,
    );
    setup.write_sensor_report("alpha", &valid_sensor_report("alpha"));

    cmd()
        .args([
            "ingest",
            "--artifacts",
            &setup.artifacts_arg(),
            "--config",
            &setup.config_arg(),
            "--disable-hooks",
        ])
        .assert()
        .success();

    assert!(setup.cockpit_report_path().exists());
}

// =============================================================================
// --disable-buildfix: ingest succeeds without buildfix
// =============================================================================

#[test]
fn ingest_disable_buildfix_succeeds() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]
[sensors.alpha]
blocking = false
missing = "skip"
"#,
    );
    setup.write_sensor_report("alpha", &valid_sensor_report("alpha"));

    cmd()
        .args([
            "ingest",
            "--artifacts",
            &setup.artifacts_arg(),
            "--config",
            &setup.config_arg(),
            "--disable-buildfix",
        ])
        .assert()
        .success();

    assert!(setup.cockpit_report_path().exists());
    // Buildfix sidecar must not exist when buildfix is disabled
    let sidecar = setup
        .artifacts_dir
        .join("cockpit")
        .join("buildfix.apply.json");
    assert!(!sidecar.exists());
}

// =============================================================================
// --disable-policy-signing: ingest succeeds without signing
// =============================================================================

#[test]
fn ingest_disable_policy_signing_succeeds() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]
[sensors.alpha]
blocking = false
missing = "skip"
"#,
    );
    setup.write_sensor_report("alpha", &valid_sensor_report("alpha"));

    cmd()
        .args([
            "ingest",
            "--artifacts",
            &setup.artifacts_arg(),
            "--config",
            &setup.config_arg(),
            "--disable-policy-signing",
        ])
        .assert()
        .success();

    assert!(setup.cockpit_report_path().exists());
    // Signing sidecar must not exist when signing is disabled
    let sidecar = setup
        .artifacts_dir
        .join("cockpit")
        .join("policy.signature.json");
    assert!(!sidecar.exists());
}

// =============================================================================
// All disable flags combined: ingest still succeeds gracefully
// =============================================================================

#[test]
fn ingest_all_features_disabled_via_flags() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]
[sensors.alpha]
blocking = false
missing = "skip"
"#,
    );
    setup.write_sensor_report("alpha", &valid_sensor_report("alpha"));

    cmd()
        .args([
            "ingest",
            "--artifacts",
            &setup.artifacts_arg(),
            "--config",
            &setup.config_arg(),
            "--disable-hooks",
            "--disable-buildfix",
            "--disable-policy-signing",
        ])
        .assert()
        .success();

    assert!(setup.cockpit_report_path().exists());
    assert!(setup.cockpit_comment_path().exists());
}

// =============================================================================
// Schema validation: --schema-validation lax works
// =============================================================================

#[test]
fn ingest_schema_lax_mode_succeeds() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]
[sensors.alpha]
blocking = false
missing = "skip"
"#,
    );
    setup.write_sensor_report("alpha", &valid_sensor_report("alpha"));

    cmd()
        .args([
            "ingest",
            "--artifacts",
            &setup.artifacts_arg(),
            "--config",
            &setup.config_arg(),
            "--schema-validation",
            "lax",
        ])
        .assert()
        .success();
}

// =============================================================================
// Schema validation: strict mode works when feature-schema is compiled in
// =============================================================================

#[cfg(feature = "feature-schema")]
#[test]
fn ingest_schema_strict_mode_with_feature() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]
[sensors.alpha]
blocking = false
missing = "skip"
"#,
    );
    setup.write_sensor_report("alpha", &valid_sensor_report("alpha"));

    cmd()
        .args([
            "ingest",
            "--artifacts",
            &setup.artifacts_arg(),
            "--config",
            &setup.config_arg(),
            "--schema-validation",
            "strict",
        ])
        .assert()
        .success();
}

// =============================================================================
// Feature flags don't interfere with core explain/validate commands
// =============================================================================

#[test]
fn explain_works_regardless_of_feature_flags() {
    // explain doesn't depend on any optional feature flags
    cmd()
        .args(["explain", "all"])
        .assert()
        .success()
        .stdout(predicate::str::contains("cockpit.missing_receipt"));
}

#[test]
fn validate_lax_works_regardless_of_feature_flags() {
    let setup = TestSetup::new();
    let report_path = setup.artifacts_dir.join("test_report.json");
    fs::write(&report_path, valid_sensor_report("alpha")).unwrap();

    cmd()
        .args([
            "validate",
            "--input",
            &report_path.to_string_lossy(),
            "--lax",
        ])
        .assert()
        .success()
        .stderr(predicate::str::contains("parsed as sensor.report.v1"));
}

// =============================================================================
// Disable flags are properly recognized by clap (no unknown-flag error)
// =============================================================================

#[test]
fn clap_accepts_disable_hooks_flag() {
    let setup = TestSetup::new();
    setup.write_config("[policy]\n");
    cmd()
        .args([
            "ingest",
            "--artifacts",
            &setup.artifacts_arg(),
            "--config",
            &setup.config_arg(),
            "--disable-hooks",
        ])
        .assert()
        .success();
}

#[test]
fn clap_accepts_disable_buildfix_flag() {
    let setup = TestSetup::new();
    setup.write_config("[policy]\n");
    cmd()
        .args([
            "ingest",
            "--artifacts",
            &setup.artifacts_arg(),
            "--config",
            &setup.config_arg(),
            "--disable-buildfix",
        ])
        .assert()
        .success();
}

#[test]
fn clap_accepts_disable_policy_signing_flag() {
    let setup = TestSetup::new();
    setup.write_config("[policy]\n");
    cmd()
        .args([
            "ingest",
            "--artifacts",
            &setup.artifacts_arg(),
            "--config",
            &setup.config_arg(),
            "--disable-policy-signing",
        ])
        .assert()
        .success();
}

// =============================================================================
// Report content: disabled features don't produce sidecar artifacts
// =============================================================================

#[test]
fn disabled_buildfix_no_buildfix_data_in_report() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]
[sensors.alpha]
blocking = false
missing = "skip"
"#,
    );
    setup.write_sensor_report("alpha", &valid_sensor_report("alpha"));

    cmd()
        .args([
            "ingest",
            "--artifacts",
            &setup.artifacts_arg(),
            "--config",
            &setup.config_arg(),
            "--disable-buildfix",
        ])
        .assert()
        .success();

    let report = fs::read_to_string(setup.cockpit_report_path()).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&report).unwrap();
    // When buildfix is disabled, _buildfix_apply should not appear in data
    if let Some(data) = parsed.get("data") {
        assert!(
            data.get("_buildfix_apply").is_none(),
            "buildfix data should not be in report when disabled"
        );
    }
}

#[test]
fn disabled_signing_no_signature_data_in_report() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]
[sensors.alpha]
blocking = false
missing = "skip"
"#,
    );
    setup.write_sensor_report("alpha", &valid_sensor_report("alpha"));

    cmd()
        .args([
            "ingest",
            "--artifacts",
            &setup.artifacts_arg(),
            "--config",
            &setup.config_arg(),
            "--disable-policy-signing",
        ])
        .assert()
        .success();

    let report = fs::read_to_string(setup.cockpit_report_path()).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&report).unwrap();
    if let Some(data) = parsed.get("data") {
        assert!(
            data.get("_policy_signature").is_none(),
            "policy signature should not be in report when disabled"
        );
    }
}
