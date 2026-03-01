//! End-to-end tests for the `cockpitctl ingest --format sarif` CLI output.
//!
//! Verifies that SARIF v2.1.0 files are created, contain valid JSON with the
//! correct schema version, include findings when present, and produce empty
//! results for clean reports.

use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use tempfile::TempDir;

// ─────────────────────────────────────────────────────────────────────────────
// Helpers (mirrors e2e_ingest conventions)
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

    fn sarif_path(&self) -> PathBuf {
        self.artifacts_dir.join("cockpit").join("sarif.json")
    }

    fn read_sarif(&self) -> String {
        let path = self.sarif_path();
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("read SARIF at {:?}: {}", path, e))
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

/// Run ingest with `--format sarif` and return the parsed SARIF JSON.
fn ingest_sarif(setup: &TestSetup) -> assert_cmd::assert::Assert {
    cmd()
        .args([
            "ingest",
            "--artifacts",
            &setup.artifacts_arg(),
            "--config",
            &setup.config_arg(),
            "--format",
            "sarif",
        ])
        .assert()
}

// ─────────────────────────────────────────────────────────────────────────────
// 1) SARIF output file is created
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn sarif_output_file_is_created() {
    let setup = setup_from_fixture("happy_path");

    ingest_sarif(&setup).success();

    assert!(
        setup.sarif_path().exists(),
        "artifacts/cockpit/sarif.json should be created when --format sarif is used"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// 2) SARIF output is valid JSON with correct schema version
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn sarif_output_is_valid_json_with_schema_version() {
    let setup = setup_from_fixture("happy_path");

    ingest_sarif(&setup).success();

    let sarif: serde_json::Value =
        serde_json::from_str(&setup.read_sarif()).expect("sarif.json must be valid JSON");

    assert_eq!(
        sarif["version"].as_str(),
        Some("2.1.0"),
        "SARIF version must be 2.1.0"
    );
    assert!(
        sarif["$schema"].as_str().unwrap_or("").contains("sarif"),
        "$schema should reference the SARIF schema"
    );
    assert!(sarif["runs"].is_array(), "runs must be an array");
}

// ─────────────────────────────────────────────────────────────────────────────
// 3) SARIF contains findings from sensor highlights
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn sarif_contains_findings() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]
warn_is_fail = false

[sensors.checker]
blocking = false
missing = "skip"
"#,
    );
    setup.write_sensor_report(
        "checker",
        r#"{
  "schema": "checker.report.v1",
  "tool": { "name": "checker", "version": "1.0.0" },
  "run": { "started_at": "2026-02-02T11:00:00Z" },
  "verdict": { "status": "warn", "counts": { "info": 0, "warn": 1, "error": 0 } },
  "findings": [
    {
      "severity": "warn",
      "code": "checker.unused_import",
      "message": "Unused import detected",
      "location": { "path": "src/main.rs", "line": 10 }
    }
  ]
}"#,
    );

    ingest_sarif(&setup).success();

    let sarif: serde_json::Value = serde_json::from_str(&setup.read_sarif()).expect("parse sarif");

    let runs = sarif["runs"].as_array().expect("runs array");
    assert_eq!(runs.len(), 1, "should have exactly one run");

    let results = runs[0]["results"].as_array().expect("results array");
    assert!(!results.is_empty(), "results should contain findings");

    let first = &results[0];
    assert_eq!(first["ruleId"].as_str(), Some("checker.unused_import"));
    assert_eq!(first["level"].as_str(), Some("warning"));
    assert_eq!(
        first["message"]["text"].as_str(),
        Some("Unused import detected")
    );

    // Verify location
    let locations = first["locations"].as_array().expect("locations array");
    assert!(!locations.is_empty(), "result should have a location");
    let phys = &locations[0]["physicalLocation"];
    assert_eq!(
        phys["artifactLocation"]["uri"].as_str(),
        Some("src/main.rs")
    );
    assert_eq!(phys["region"]["startLine"].as_u64(), Some(10));
}

// ─────────────────────────────────────────────────────────────────────────────
// 4) SARIF on empty report: no findings → empty results
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn sarif_empty_report_has_empty_results() {
    let setup = setup_from_fixture("empty_findings");

    ingest_sarif(&setup).success();

    let sarif: serde_json::Value = serde_json::from_str(&setup.read_sarif()).expect("parse sarif");

    let runs = sarif["runs"].as_array().expect("runs array");
    assert_eq!(runs.len(), 1, "should have exactly one run");

    let results = runs[0]["results"].as_array().expect("results array");
    assert!(
        results.is_empty(),
        "results should be empty when report has no findings"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// 5) SARIF file is written to the expected output directory
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn sarif_file_is_in_cockpit_output_directory() {
    let setup = setup_from_fixture("happy_path");

    ingest_sarif(&setup).success();

    let sarif_path = setup.sarif_path();
    assert!(sarif_path.exists(), "sarif.json should exist");

    // Verify it's inside artifacts/cockpit/
    let parent = sarif_path.parent().expect("parent dir");
    assert!(
        parent.ends_with("cockpit"),
        "sarif.json should be inside the cockpit output directory, got {:?}",
        parent
    );

    // Standard cockpit outputs should also still be present
    assert!(
        setup
            .artifacts_dir
            .join("cockpit")
            .join("report.json")
            .exists(),
        "report.json should still be created alongside sarif.json"
    );
    assert!(
        setup
            .artifacts_dir
            .join("cockpit")
            .join("comment.md")
            .exists(),
        "comment.md should still be created alongside sarif.json"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// 6) SARIF tool driver has correct name and rules
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn sarif_tool_driver_metadata() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]
warn_is_fail = false

[sensors.lintcheck]
blocking = false
missing = "skip"
"#,
    );
    setup.write_sensor_report(
        "lintcheck",
        r#"{
  "schema": "lintcheck.report.v1",
  "tool": { "name": "lintcheck", "version": "2.0.0" },
  "run": { "started_at": "2026-02-02T11:00:00Z" },
  "verdict": { "status": "fail", "counts": { "info": 0, "warn": 0, "error": 2 } },
  "findings": [
    {
      "severity": "error",
      "code": "lintcheck.null_deref",
      "message": "Null pointer dereference",
      "location": { "path": "src/foo.rs", "line": 5 }
    },
    {
      "severity": "error",
      "code": "lintcheck.oob_access",
      "message": "Out of bounds array access",
      "location": { "path": "src/bar.rs", "line": 20 }
    }
  ]
}"#,
    );

    ingest_sarif(&setup).success();

    let sarif: serde_json::Value = serde_json::from_str(&setup.read_sarif()).expect("parse sarif");

    let driver = &sarif["runs"][0]["tool"]["driver"];
    assert_eq!(
        driver["name"].as_str(),
        Some("cockpitctl"),
        "tool driver name should be cockpitctl"
    );
    assert!(
        driver["version"].as_str().is_some(),
        "tool driver should have a version"
    );

    // Rules should contain entries for the distinct codes
    let rules = driver["rules"].as_array().expect("rules array");
    let rule_ids: Vec<&str> = rules.iter().filter_map(|r| r["id"].as_str()).collect();
    assert!(
        rule_ids.contains(&"lintcheck.null_deref"),
        "rules should include lintcheck.null_deref"
    );
    assert!(
        rule_ids.contains(&"lintcheck.oob_access"),
        "rules should include lintcheck.oob_access"
    );

    // Results should have two entries
    let results = sarif["runs"][0]["results"].as_array().expect("results");
    assert_eq!(results.len(), 2, "should have two SARIF results");
}
