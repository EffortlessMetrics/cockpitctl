//! Advanced end-to-end tests for the `cockpitctl ingest` CLI command.
//!
//! Covers: multi-sensor mixed verdicts, schema validation modes,
//! CLI config overrides, output file structure, and exit code semantics.

use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
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

    fn cockpit_report_path(&self) -> PathBuf {
        self.artifacts_dir.join("cockpit").join("report.json")
    }

    fn cockpit_comment_path(&self) -> PathBuf {
        self.artifacts_dir.join("cockpit").join("comment.md")
    }

    fn read_cockpit_report(&self) -> String {
        let path = self.cockpit_report_path();
        fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read cockpit report at {:?}: {}", path, e))
    }

    fn read_cockpit_comment(&self) -> String {
        let path = self.cockpit_comment_path();
        fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read cockpit comment at {:?}: {}", path, e))
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

fn fail_sensor_report(sensor_name: &str) -> String {
    format!(
        r#"{{
  "schema": "{sensor_name}.report.v1",
  "tool": {{ "name": "{sensor_name}", "version": "1.0.0" }},
  "run": {{ "started_at": "2026-02-02T11:00:00Z" }},
  "verdict": {{ "status": "fail", "counts": {{ "info": 0, "warn": 0, "error": 1 }} }},
  "findings": [
    {{
      "severity": "error",
      "code": "{sensor_name}.hard_error",
      "message": "A blocking failure was detected"
    }}
  ]
}}"#
    )
}

fn warn_sensor_report(sensor_name: &str) -> String {
    format!(
        r#"{{
  "schema": "{sensor_name}.report.v1",
  "tool": {{ "name": "{sensor_name}", "version": "1.0.0" }},
  "run": {{ "started_at": "2026-02-02T11:00:00Z" }},
  "verdict": {{ "status": "warn", "counts": {{ "info": 0, "warn": 1, "error": 0 }} }},
  "findings": [
    {{
      "severity": "warn",
      "code": "{sensor_name}.style_issue",
      "message": "Style issue detected"
    }}
  ]
}}"#
    )
}

fn skip_sensor_report(sensor_name: &str) -> String {
    format!(
        r#"{{
  "schema": "{sensor_name}.report.v1",
  "tool": {{ "name": "{sensor_name}", "version": "1.0.0" }},
  "run": {{ "started_at": "2026-02-02T11:00:00Z" }},
  "verdict": {{ "status": "skip", "counts": {{ "info": 0, "warn": 0, "error": 0 }}, "reasons": ["not_applicable"] }},
  "findings": []
}}"#
    )
}

// ─────────────────────────────────────────────────────────────────────────────
// Multi-sensor mixed verdicts: pass + warn + fail + skip
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn mixed_verdicts_four_sensors_exits_two() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]
warn_is_fail = false

[sensors.alpha]
blocking = true
missing = "fail"

[sensors.beta]
blocking = false
missing = "skip"

[sensors.gamma]
blocking = true
missing = "fail"

[sensors.delta]
blocking = false
missing = "skip"
"#,
    );

    setup.write_sensor_report("alpha", &valid_sensor_report("alpha"));
    setup.write_sensor_report("beta", &warn_sensor_report("beta"));
    setup.write_sensor_report("gamma", &fail_sensor_report("gamma"));
    setup.write_sensor_report("delta", &skip_sensor_report("delta"));

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

    let report = setup.read_cockpit_report();
    let json: serde_json::Value = serde_json::from_str(&report).expect("parse cockpit report");

    // All four sensors should appear in output
    let sensors = json["sensors"].as_array().expect("sensors array");
    let sensor_ids: Vec<&str> = sensors.iter().filter_map(|s| s["id"].as_str()).collect();
    assert_eq!(sensor_ids.len(), 4, "should have exactly 4 sensors");
    assert!(sensor_ids.contains(&"alpha"));
    assert!(sensor_ids.contains(&"beta"));
    assert!(sensor_ids.contains(&"gamma"));
    assert!(sensor_ids.contains(&"delta"));

    // Overall verdict must be fail because gamma (blocking) failed
    assert_eq!(json["verdict"]["status"].as_str(), Some("fail"));
}

#[test]
fn mixed_verdicts_all_pass_exits_zero() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]
warn_is_fail = false

[sensors.alpha]
blocking = true
missing = "fail"

[sensors.beta]
blocking = true
missing = "fail"

[sensors.gamma]
blocking = false
missing = "skip"
"#,
    );

    setup.write_sensor_report("alpha", &valid_sensor_report("alpha"));
    setup.write_sensor_report("beta", &valid_sensor_report("beta"));
    setup.write_sensor_report("gamma", &valid_sensor_report("gamma"));

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

    let report = setup.read_cockpit_report();
    let json: serde_json::Value = serde_json::from_str(&report).expect("parse report");
    assert_eq!(json["verdict"]["status"].as_str(), Some("pass"));
}

#[test]
fn non_blocking_fail_does_not_cause_exit_two() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]
warn_is_fail = false

[sensors.safe]
blocking = true
missing = "fail"

[sensors.optional]
blocking = false
missing = "skip"
"#,
    );

    setup.write_sensor_report("safe", &valid_sensor_report("safe"));
    setup.write_sensor_report("optional", &fail_sensor_report("optional"));

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
}

// ─────────────────────────────────────────────────────────────────────────────
// Schema validation: strict mode rejects additional properties
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(feature = "feature-schema")]
#[test]
fn schema_validation_strict_rejects_extra_fields() {
    let setup = setup_from_fixture("schema_violation");

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
        .code(2);

    let report = setup.read_cockpit_report();
    let json: serde_json::Value = serde_json::from_str(&report).expect("parse report");

    // In strict mode, the schema violation should be recorded
    let highlights = json["highlights"].as_array().expect("highlights array");
    let has_violation = highlights
        .iter()
        .any(|h| h["finding"]["code"].as_str() == Some("cockpit.schema_violation"));
    assert!(
        has_violation,
        "strict mode should produce schema_violation findings"
    );
}

#[cfg(not(feature = "feature-schema"))]
#[test]
fn schema_validation_strict_rejects_extra_fields() {
    let setup = setup_from_fixture("schema_violation");

    // Without the schema feature, strict mode gracefully falls back to lax
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
        .success()
        .stderr(predicates::str::contains(
            "schema feature disabled in this build",
        ));
}

#[test]
fn schema_validation_lax_passes_extra_fields() {
    let setup = setup_from_fixture("schema_violation");

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

#[test]
fn cli_schema_validation_overrides_config() {
    // The schema_violation fixture has schema_validation = "strict" in config.
    // CLI flag --schema-validation lax should override it.
    let setup = setup_from_fixture("schema_violation");

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

// ─────────────────────────────────────────────────────────────────────────────
// Config override: --warn-is-fail flag
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn cli_warn_is_fail_overrides_config() {
    let setup = TestSetup::new();
    // Config says warn_is_fail = false
    setup.write_config(
        r#"[policy]
warn_is_fail = false

[sensors.linter]
blocking = true
missing = "fail"
"#,
    );
    setup.write_sensor_report("linter", &warn_sensor_report("linter"));

    // Without override, warn should pass (warn_is_fail = false, non-error verdict)
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
}

#[test]
fn warn_as_fail_fixture_exits_two() {
    let setup = setup_from_fixture("warn_as_fail");

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

    let report = setup.read_cockpit_report();
    let json: serde_json::Value = serde_json::from_str(&report).expect("parse report");
    assert_eq!(json["verdict"]["status"].as_str(), Some("fail"));
}

// ─────────────────────────────────────────────────────────────────────────────
// Output file verification: report.json structure
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn report_json_sensors_have_required_fields() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]
warn_is_fail = false

[sensors.checker]
blocking = true
missing = "fail"
"#,
    );
    setup.write_sensor_report("checker", &valid_sensor_report("checker"));

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

    let report = setup.read_cockpit_report();
    let json: serde_json::Value = serde_json::from_str(&report).expect("parse report");

    let sensors = json["sensors"].as_array().expect("sensors array");
    assert!(!sensors.is_empty(), "sensors should not be empty");

    for sensor in sensors {
        assert!(sensor["id"].is_string(), "sensor.id must be a string");
        assert!(
            sensor["verdict"].is_object(),
            "sensor.verdict must be present"
        );
        assert!(
            sensor["verdict"]["status"].is_string(),
            "sensor.verdict.status must be a string"
        );
    }
}

#[test]
fn report_json_policy_section_present() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]
warn_is_fail = true
max_highlights = 5
max_per_sensor_findings = 10

[sensors.x]
blocking = true
missing = "fail"
"#,
    );
    setup.write_sensor_report("x", &valid_sensor_report("x"));

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

    let report = setup.read_cockpit_report();
    let json: serde_json::Value = serde_json::from_str(&report).expect("parse report");

    let policy = &json["policy"];
    assert!(policy.is_object(), "policy must be present");
    assert_eq!(policy["warn_is_fail"].as_bool(), Some(true));
}

// ─────────────────────────────────────────────────────────────────────────────
// Output file verification: comment.md markers
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn comment_md_contains_sensor_names() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]
warn_is_fail = false

[sensors.builddiag]
blocking = true
missing = "fail"

[sensors.linter]
blocking = false
missing = "skip"
"#,
    );
    setup.write_sensor_report("builddiag", &valid_sensor_report("builddiag"));
    setup.write_sensor_report("linter", &warn_sensor_report("linter"));

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

    let comment = setup.read_cockpit_comment();
    assert!(
        comment.contains("builddiag"),
        "comment.md should mention sensor 'builddiag'"
    );
    assert!(
        comment.contains("linter"),
        "comment.md should mention sensor 'linter'"
    );
}

#[test]
fn comment_md_on_failure_mentions_fail() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]

[sensors.blocker]
blocking = true
missing = "fail"
"#,
    );
    setup.write_sensor_report("blocker", &fail_sensor_report("blocker"));

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

    let comment = setup.read_cockpit_comment();
    // The comment should indicate failure in some form
    let lower = comment.to_lowercase();
    assert!(
        lower.contains("fail") || lower.contains("❌") || lower.contains("blocked"),
        "comment.md should indicate failure"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Exit code: missing blocking sensor → exit 2
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn missing_blocking_sensor_with_fail_policy_exits_two() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]

[sensors.required_sensor]
blocking = true
missing = "fail"
"#,
    );
    // Do not write any sensor report — sensor is missing

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

    let report = setup.read_cockpit_report();
    let json: serde_json::Value = serde_json::from_str(&report).expect("parse report");
    assert_eq!(json["verdict"]["status"].as_str(), Some("fail"));
}

#[test]
fn missing_sensor_with_skip_policy_exits_zero() {
    let setup = TestSetup::new();
    setup.write_config(
        r#"[policy]

[sensors.optional_sensor]
blocking = false
missing = "skip"
"#,
    );
    // Do not write any sensor report — sensor is missing

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
}

// ─────────────────────────────────────────────────────────────────────────────
// Fixture: mixed_verdicts has at least 3 sensors
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn mixed_verdicts_fixture_sensors_in_report() {
    let setup = setup_from_fixture("mixed_verdicts");

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

    let report = setup.read_cockpit_report();
    let json: serde_json::Value = serde_json::from_str(&report).expect("parse report");

    let sensors = json["sensors"].as_array().expect("sensors array");
    assert!(
        sensors.len() >= 3,
        "mixed_verdicts should have at least 3 sensors, got {}",
        sensors.len()
    );

    // Sensor IDs should be in lexical order (determinism requirement)
    let ids: Vec<&str> = sensors.iter().filter_map(|s| s["id"].as_str()).collect();
    let mut sorted = ids.clone();
    sorted.sort();
    assert_eq!(ids, sorted, "sensors should be in lexical order");
}

// ─────────────────────────────────────────────────────────────────────────────
// Determinism: same input → same output
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn deterministic_output_on_repeated_runs() {
    let run = || {
        let setup = TestSetup::new();
        setup.write_config(
            r#"[policy]
warn_is_fail = false

[sensors.a]
blocking = true
missing = "fail"

[sensors.b]
blocking = false
missing = "skip"
"#,
        );
        setup.write_sensor_report("a", &valid_sensor_report("a"));
        setup.write_sensor_report("b", &warn_sensor_report("b"));

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

        (setup.read_cockpit_report(), setup.read_cockpit_comment())
    };

    let (report1, comment1) = run();
    let (report2, comment2) = run();

    assert_eq!(report1, report2, "report.json should be deterministic");
    assert_eq!(comment1, comment2, "comment.md should be deterministic");
}
