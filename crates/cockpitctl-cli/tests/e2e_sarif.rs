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

// ─────────────────────────────────────────────────────────────────────────────
// 7) SARIF output with valid multi-sensor report
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn sarif_multi_sensor_report() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]
warn_is_fail = false

[sensors.alpha]
blocking = false
missing = "skip"

[sensors.beta]
blocking = false
missing = "skip"
"#,
    );
    setup.write_sensor_report(
        "alpha",
        r#"{
  "schema": "alpha.report.v1",
  "tool": { "name": "alpha", "version": "1.0.0" },
  "run": { "started_at": "2026-02-02T11:00:00Z" },
  "verdict": { "status": "warn", "counts": { "info": 0, "warn": 1, "error": 0 } },
  "findings": [
    {
      "severity": "warn",
      "code": "alpha.lint",
      "message": "Alpha lint warning",
      "location": { "path": "src/a.rs", "line": 5 }
    }
  ]
}"#,
    );
    setup.write_sensor_report(
        "beta",
        r#"{
  "schema": "beta.report.v1",
  "tool": { "name": "beta", "version": "2.0.0" },
  "run": { "started_at": "2026-02-02T11:00:00Z" },
  "verdict": { "status": "fail", "counts": { "info": 0, "warn": 0, "error": 1 } },
  "findings": [
    {
      "severity": "error",
      "code": "beta.crash",
      "message": "Beta crash detected",
      "location": { "path": "src/b.rs", "line": 42 }
    }
  ]
}"#,
    );

    ingest_sarif(&setup).success();

    let sarif: serde_json::Value = serde_json::from_str(&setup.read_sarif()).expect("parse sarif");

    let runs = sarif["runs"].as_array().expect("runs array");
    assert_eq!(runs.len(), 1, "should have exactly one run");

    let results = runs[0]["results"].as_array().expect("results array");
    assert!(
        results.len() >= 2,
        "multi-sensor should produce at least 2 results, got {}",
        results.len()
    );

    let rule_ids: Vec<&str> = results
        .iter()
        .filter_map(|r| r["ruleId"].as_str())
        .collect();
    assert!(
        rule_ids.contains(&"alpha.lint"),
        "results should include alpha.lint"
    );
    assert!(
        rule_ids.contains(&"beta.crash"),
        "results should include beta.crash"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// 8) SARIF with no findings → valid but empty results
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn sarif_no_findings_produces_valid_empty_results() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]
warn_is_fail = false

[sensors.clean]
blocking = false
missing = "skip"
"#,
    );
    setup.write_sensor_report(
        "clean",
        r#"{
  "schema": "clean.report.v1",
  "tool": { "name": "clean", "version": "1.0.0" },
  "run": { "started_at": "2026-02-02T11:00:00Z" },
  "verdict": { "status": "pass", "counts": { "info": 0, "warn": 0, "error": 0 } },
  "findings": []
}"#,
    );

    ingest_sarif(&setup).success();

    let sarif: serde_json::Value = serde_json::from_str(&setup.read_sarif()).expect("parse sarif");

    // Valid structure
    assert_eq!(sarif["version"].as_str(), Some("2.1.0"));
    assert!(sarif["runs"].is_array());

    let runs = sarif["runs"].as_array().unwrap();
    assert_eq!(runs.len(), 1);

    let results = runs[0]["results"].as_array().expect("results array");
    assert!(
        results.is_empty(),
        "no findings should produce empty results, got {} results",
        results.len()
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// 9) SARIF output format validation (check required SARIF fields)
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn sarif_required_fields_present() {
    let setup = setup_from_fixture("happy_path");

    ingest_sarif(&setup).success();

    let sarif: serde_json::Value = serde_json::from_str(&setup.read_sarif()).expect("parse sarif");

    // Top-level required fields
    assert!(sarif["$schema"].is_string(), "$schema must be a string");
    assert!(sarif["version"].is_string(), "version must be a string");
    assert!(sarif["runs"].is_array(), "runs must be an array");

    let runs = sarif["runs"].as_array().unwrap();
    for (i, run) in runs.iter().enumerate() {
        // Each run must have tool and results
        assert!(run["tool"].is_object(), "run[{i}].tool must be an object");
        assert!(
            run["tool"]["driver"].is_object(),
            "run[{i}].tool.driver must be an object"
        );
        assert!(
            run["tool"]["driver"]["name"].is_string(),
            "run[{i}].tool.driver.name must be a string"
        );
        assert!(
            run["tool"]["driver"]["version"].is_string(),
            "run[{i}].tool.driver.version must be a string"
        );
        assert!(
            run["results"].is_array(),
            "run[{i}].results must be an array"
        );

        // Each result must have ruleId, level, and message
        let results = run["results"].as_array().unwrap();
        for (j, result) in results.iter().enumerate() {
            assert!(
                result["ruleId"].is_string(),
                "run[{i}].results[{j}].ruleId must be a string"
            );
            assert!(
                result["level"].is_string(),
                "run[{i}].results[{j}].level must be a string"
            );
            assert!(
                result["message"]["text"].is_string(),
                "run[{i}].results[{j}].message.text must be a string"
            );
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// 10) SARIF with different severity levels → correct SARIF level mapping
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn sarif_severity_level_mapping() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]
warn_is_fail = false

[sensors.sev]
blocking = false
missing = "skip"
"#,
    );
    setup.write_sensor_report(
        "sev",
        r#"{
  "schema": "sev.report.v1",
  "tool": { "name": "sev", "version": "1.0.0" },
  "run": { "started_at": "2026-02-02T11:00:00Z" },
  "verdict": { "status": "fail", "counts": { "info": 1, "warn": 1, "error": 1 } },
  "findings": [
    {
      "severity": "error",
      "code": "sev.err",
      "message": "An error finding",
      "location": { "path": "src/e.rs", "line": 1 }
    },
    {
      "severity": "warn",
      "code": "sev.wrn",
      "message": "A warning finding",
      "location": { "path": "src/w.rs", "line": 2 }
    },
    {
      "severity": "info",
      "code": "sev.inf",
      "message": "An info finding",
      "location": { "path": "src/i.rs", "line": 3 }
    }
  ]
}"#,
    );

    ingest_sarif(&setup).success();

    let sarif: serde_json::Value = serde_json::from_str(&setup.read_sarif()).expect("parse sarif");
    let results = sarif["runs"][0]["results"].as_array().expect("results");

    // Collect level mappings by ruleId
    let mut levels: std::collections::HashMap<&str, &str> = std::collections::HashMap::new();
    for r in results {
        if let (Some(id), Some(level)) = (r["ruleId"].as_str(), r["level"].as_str()) {
            levels.insert(id, level);
        }
    }

    // SARIF level mapping: error→error, warn→warning, info→note
    if let Some(&level) = levels.get("sev.err") {
        assert_eq!(level, "error", "error severity should map to SARIF 'error'");
    }
    if let Some(&level) = levels.get("sev.wrn") {
        assert_eq!(
            level, "warning",
            "warn severity should map to SARIF 'warning'"
        );
    }
    if let Some(&level) = levels.get("sev.inf") {
        assert_eq!(level, "note", "info severity should map to SARIF 'note'");
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// 11) SARIF output is deterministic (run twice, compare)
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn sarif_output_is_deterministic() {
    // Run 1 — multi_error fixture exits with code 2 (policy fail), but SARIF is still written
    let setup1 = setup_from_fixture("multi_error");
    ingest_sarif(&setup1).code(2);
    let sarif1 = setup1.read_sarif();

    // Run 2
    let setup2 = setup_from_fixture("multi_error");
    ingest_sarif(&setup2).code(2);
    let sarif2 = setup2.read_sarif();

    assert_eq!(
        sarif1, sarif2,
        "SARIF output must be byte-identical across runs with identical inputs"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// 12) SARIF with special characters in findings → valid JSON
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn sarif_special_characters_produce_valid_json() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]
warn_is_fail = false

[sensors.special]
blocking = false
missing = "skip"
"#,
    );
    setup.write_sensor_report(
        "special",
        r#"{
  "schema": "special.report.v1",
  "tool": { "name": "special", "version": "1.0.0" },
  "run": { "started_at": "2026-02-02T11:00:00Z" },
  "verdict": { "status": "warn", "counts": { "info": 0, "warn": 1, "error": 0 } },
  "findings": [
    {
      "severity": "warn",
      "code": "special.chars",
      "message": "Found issue with \"quotes\" and <angle> & ampersand \\ backslash",
      "location": { "path": "src/spécial/naïve.rs", "line": 1 }
    }
  ]
}"#,
    );

    ingest_sarif(&setup).success();

    // Must parse as valid JSON
    let sarif_str = setup.read_sarif();
    let sarif: serde_json::Value =
        serde_json::from_str(&sarif_str).expect("SARIF with special chars must be valid JSON");

    let results = sarif["runs"][0]["results"].as_array().expect("results");
    assert!(!results.is_empty(), "should have at least one result");

    // The message should preserve the special characters
    let msg = results[0]["message"]["text"]
        .as_str()
        .expect("message text");
    assert!(
        msg.contains("quotes") && msg.contains("ampersand"),
        "special characters should be preserved in SARIF message, got: {msg}"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// 13) SARIF snapshot test for output stability (structure check)
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn sarif_snapshot_structural_stability() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]
warn_is_fail = false

[sensors.snap]
blocking = false
missing = "skip"
"#,
    );
    setup.write_sensor_report(
        "snap",
        r#"{
  "schema": "snap.report.v1",
  "tool": { "name": "snap", "version": "1.0.0" },
  "run": { "started_at": "2026-02-02T11:00:00Z" },
  "verdict": { "status": "warn", "counts": { "info": 0, "warn": 1, "error": 0 } },
  "findings": [
    {
      "severity": "warn",
      "code": "snap.test_rule",
      "message": "Snapshot test finding",
      "location": { "path": "src/snap.rs", "line": 42 }
    }
  ]
}"#,
    );

    ingest_sarif(&setup).success();

    let sarif: serde_json::Value = serde_json::from_str(&setup.read_sarif()).expect("parse sarif");

    // Verify structural invariants that should never change
    assert_eq!(sarif["version"].as_str(), Some("2.1.0"));
    assert!(
        sarif["$schema"]
            .as_str()
            .unwrap_or("")
            .contains("sarif-schema-2.1.0"),
        "$schema should reference sarif-schema-2.1.0"
    );

    let run = &sarif["runs"][0];
    assert_eq!(
        run["tool"]["driver"]["name"].as_str(),
        Some("cockpitctl"),
        "driver name should be cockpitctl"
    );

    let results = run["results"].as_array().expect("results");
    assert_eq!(results.len(), 1);

    let result = &results[0];
    assert_eq!(result["ruleId"].as_str(), Some("snap.test_rule"));
    assert_eq!(result["level"].as_str(), Some("warning"));
    assert_eq!(
        result["message"]["text"].as_str(),
        Some("Snapshot test finding")
    );

    let loc = &result["locations"][0]["physicalLocation"];
    assert_eq!(loc["artifactLocation"]["uri"].as_str(), Some("src/snap.rs"));
    assert_eq!(loc["region"]["startLine"].as_u64(), Some(42));
}
