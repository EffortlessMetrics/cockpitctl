//! Cross-crate integration tests: full ingest → domain → render → io pipeline
//! exercised through the cockpitctl-core facade with real filesystem adapters.

use std::fs;
use tempfile::TempDir;

use cockpitctl_core::io::{FsLayout, FsOutputSink, FsPolicySource, FsReceiptSource};
use cockpitctl_core::render::render_comment;
use cockpitctl_core::types::{RunInfo, SchemaValidation, ToolInfo, VerdictStatus};
use cockpitctl_core::{IngestRequest, IngestUseCase, NoOpSchemaValidator};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn tool_info() -> ToolInfo {
    ToolInfo {
        name: "integration-test".to_string(),
        version: "0.0.1".to_string(),
        commit: None,
    }
}

fn run_info() -> RunInfo {
    RunInfo {
        started_at: "2026-06-01T00:00:00Z".to_string(),
        ended_at: None,
        duration_ms: None,
        host: None,
        git: None,
        ci: None,
        capabilities: Default::default(),
    }
}

fn default_request() -> IngestRequest {
    IngestRequest {
        labels: vec![],
        tool: tool_info(),
        run: run_info(),
        schema_validation_override: None,
    }
}

fn minimal_receipt(sensor_name: &str, status: &str) -> String {
    serde_json::json!({
        "schema": "sensor.report.v1",
        "tool": { "name": sensor_name, "version": "1.0.0" },
        "run":  { "started_at": "2026-06-01T00:00:00Z" },
        "verdict": {
            "status": status,
            "counts": { "info": 0, "warn": 0, "error": 0 },
            "reasons": []
        },
        "findings": []
    })
    .to_string()
}

fn receipt_with_findings(sensor_name: &str) -> String {
    serde_json::json!({
        "schema": "sensor.report.v1",
        "tool": { "name": sensor_name, "version": "1.0.0" },
        "run":  { "started_at": "2026-06-01T00:00:00Z" },
        "verdict": {
            "status": "warn",
            "counts": { "info": 0, "warn": 2, "error": 0 },
            "reasons": []
        },
        "findings": [
            {
                "severity": "warn",
                "code": "test.warning-a",
                "message": "First warning",
                "location": { "path": "src/main.rs", "line": 10 }
            },
            {
                "severity": "warn",
                "code": "test.warning-b",
                "message": "Second warning",
                "location": { "path": "src/lib.rs", "line": 20 }
            }
        ]
    })
    .to_string()
}

fn create_sensor(artifacts: &std::path::Path, sensor_id: &str, json: &str) {
    let dir = artifacts.join(sensor_id);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("report.json"), json).unwrap();
}

fn write_config(path: &std::path::Path, toml_content: &str) {
    fs::write(path, toml_content).unwrap();
}

/// Wire up a full pipeline with real FS adapters and return the ingest result.
fn run_pipeline(
    artifacts: &std::path::Path,
    config_path: &std::path::Path,
) -> cockpitctl_core::IngestResult {
    let layout = FsLayout::new(artifacts, config_path);
    let receipts = FsReceiptSource::new(layout.clone());
    let policy = FsPolicySource::new(layout.clone());
    let output = FsOutputSink::new(layout);
    let uc = IngestUseCase::new(
        receipts,
        policy,
        output,
        NoOpSchemaValidator,
        render_comment,
    );
    uc.execute(default_request())
        .expect("pipeline should succeed")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn happy_path_two_passing_sensors() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    fs::create_dir_all(&artifacts).unwrap();

    create_sensor(&artifacts, "alpha", &minimal_receipt("alpha", "pass"));
    create_sensor(&artifacts, "beta", &minimal_receipt("beta", "pass"));

    let config_path = tmp.path().join("cockpit.toml");
    write_config(
        &config_path,
        r#"
[policy]
warn_is_fail = false

[sensors.alpha]
blocking = true
missing = "fail"

[sensors.beta]
blocking = true
missing = "fail"
"#,
    );

    let result = run_pipeline(&artifacts, &config_path);

    assert_eq!(result.exit_code, 0, "two passing sensors → exit 0");
    assert_eq!(result.report.verdict.status, VerdictStatus::Pass);
    assert_eq!(result.report.sensors.len(), 2);

    // Sensors should be in lexical order.
    let ids: Vec<&str> = result
        .report
        .sensors
        .iter()
        .map(|s| s.id.as_str())
        .collect();
    assert_eq!(ids, vec!["alpha", "beta"]);

    // Comment should be non-empty and contain key markers.
    assert!(!result.comment_md.is_empty());
    assert!(result.comment_md.contains("Cockpit"));

    // Output files should exist on disk.
    let out_dir = artifacts.join("cockpit");
    assert!(out_dir.join("report.json").exists());
    assert!(out_dir.join("comment.md").exists());

    // Written report.json should be valid JSON matching the schema tag.
    let on_disk: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(out_dir.join("report.json")).unwrap()).unwrap();
    assert_eq!(on_disk["schema"], "cockpit.report.v1");
}

#[test]
fn failing_sensor_causes_exit_code_2() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    fs::create_dir_all(&artifacts).unwrap();

    create_sensor(
        &artifacts,
        "builddiag",
        &minimal_receipt("builddiag", "fail"),
    );

    let config_path = tmp.path().join("cockpit.toml");
    write_config(
        &config_path,
        r#"
[sensors.builddiag]
blocking = true
missing = "fail"
"#,
    );

    let result = run_pipeline(&artifacts, &config_path);

    assert_eq!(result.exit_code, 2, "failing blocking sensor → exit 2");
    assert_eq!(result.report.verdict.status, VerdictStatus::Fail);
}

#[test]
fn missing_receipt_with_warn_policy() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    fs::create_dir_all(&artifacts).unwrap();

    let config_path = tmp.path().join("cockpit.toml");
    write_config(
        &config_path,
        r#"
[sensors.expected]
blocking = false
missing = "warn"
"#,
    );

    let result = run_pipeline(&artifacts, &config_path);

    assert_eq!(result.exit_code, 0);
    assert_eq!(result.report.sensors.len(), 1);
    assert_eq!(result.report.sensors[0].id, "expected");
    assert!(
        result
            .report
            .highlights
            .iter()
            .any(|h| h.finding.code == "cockpit.missing_receipt"),
        "missing receipt should be highlighted"
    );
}

#[test]
fn missing_receipt_with_fail_policy_exits_2() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    fs::create_dir_all(&artifacts).unwrap();

    let config_path = tmp.path().join("cockpit.toml");
    write_config(
        &config_path,
        r#"
[sensors.required]
blocking = true
missing = "fail"
"#,
    );

    let result = run_pipeline(&artifacts, &config_path);
    assert_eq!(result.exit_code, 2, "missing blocking sensor → exit 2");
}

#[test]
fn findings_appear_in_report_and_highlights() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    fs::create_dir_all(&artifacts).unwrap();

    create_sensor(&artifacts, "linter", &receipt_with_findings("linter"));

    let config_path = tmp.path().join("cockpit.toml");
    write_config(
        &config_path,
        r#"
[policy]
max_highlights = 10

[sensors.linter]
blocking = true
missing = "fail"
"#,
    );

    let result = run_pipeline(&artifacts, &config_path);

    // Findings from the sensor should be promoted to highlights.
    assert!(
        !result.report.highlights.is_empty(),
        "findings should produce highlights"
    );
    // The comment should reference findings.
    assert!(
        result.comment_md.contains("warn") || result.comment_md.contains("⚠"),
        "comment should surface warnings"
    );
}

#[test]
fn no_config_uses_discovered_sensors() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    fs::create_dir_all(&artifacts).unwrap();

    create_sensor(&artifacts, "sensor-a", &minimal_receipt("sensor-a", "pass"));
    create_sensor(&artifacts, "sensor-b", &minimal_receipt("sensor-b", "pass"));

    // No cockpit.toml — pipeline should discover sensors automatically.
    let config_path = tmp.path().join("cockpit.toml");
    let result = run_pipeline(&artifacts, &config_path);

    assert_eq!(result.exit_code, 0);
    assert_eq!(result.report.sensors.len(), 2);
    let ids: Vec<&str> = result
        .report
        .sensors
        .iter()
        .map(|s| s.id.as_str())
        .collect();
    assert_eq!(ids, vec!["sensor-a", "sensor-b"]);
}

#[test]
fn schema_validation_override_to_lax() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    fs::create_dir_all(&artifacts).unwrap();

    create_sensor(&artifacts, "sensor", &minimal_receipt("sensor", "pass"));

    let config_path = tmp.path().join("cockpit.toml");
    write_config(
        &config_path,
        r#"
[policy]
schema_validation = "strict"

[sensors.sensor]
blocking = true
missing = "fail"
"#,
    );

    let layout = FsLayout::new(&artifacts, &config_path);
    let receipts = FsReceiptSource::new(layout.clone());
    let policy = FsPolicySource::new(layout.clone());
    let output = FsOutputSink::new(layout);
    let uc = IngestUseCase::new(
        receipts,
        policy,
        output,
        NoOpSchemaValidator,
        render_comment,
    );

    let mut req = default_request();
    req.schema_validation_override = Some(SchemaValidation::Lax);
    let result = uc.execute(req).expect("pipeline should succeed");

    assert_eq!(result.exit_code, 0, "lax override should skip validation");
}

#[test]
fn comment_md_written_to_disk_matches_result() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    fs::create_dir_all(&artifacts).unwrap();

    create_sensor(&artifacts, "sensor", &minimal_receipt("sensor", "pass"));
    let config_path = tmp.path().join("cockpit.toml");

    let result = run_pipeline(&artifacts, &config_path);

    let on_disk_comment = fs::read_to_string(artifacts.join("cockpit").join("comment.md")).unwrap();
    assert_eq!(on_disk_comment, result.comment_md);
}

#[test]
fn report_json_on_disk_roundtrips() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    fs::create_dir_all(&artifacts).unwrap();

    create_sensor(&artifacts, "sensor", &minimal_receipt("sensor", "pass"));
    let config_path = tmp.path().join("cockpit.toml");

    let result = run_pipeline(&artifacts, &config_path);

    let on_disk_str = fs::read_to_string(artifacts.join("cockpit").join("report.json")).unwrap();
    let on_disk: serde_json::Value = serde_json::from_str(&on_disk_str).unwrap();

    // The on-disk report should have the same verdict as the in-memory report.
    assert_eq!(on_disk["schema"], "cockpit.report.v1");
    assert_eq!(
        on_disk["verdict"]["status"],
        serde_json::to_value(&result.report.verdict.status).unwrap()
    );
    assert_eq!(
        on_disk["sensors"].as_array().unwrap().len(),
        result.report.sensors.len()
    );
}
