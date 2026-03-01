//! Safety integration tests: path traversal, oversize receipts, symlinks.
//!
//! These tests exercise the full pipeline through cockpitctl-core with real
//! filesystem adapters, verifying that untrusted inputs are handled safely.

use std::fs;
use tempfile::TempDir;

use cockpitctl_core::io::{FsLayout, FsOutputSink, FsPolicySource, FsReceiptSource};
use cockpitctl_core::render::render_comment;
use cockpitctl_core::types::{RunInfo, ToolInfo};
use cockpitctl_core::{IngestRequest, IngestUseCase, NoOpSchemaValidator};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn tool_info() -> ToolInfo {
    ToolInfo {
        name: "safety-test".to_string(),
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

fn minimal_receipt(sensor_name: &str) -> String {
    serde_json::json!({
        "schema": "sensor.report.v1",
        "tool": { "name": sensor_name, "version": "1.0.0" },
        "run":  { "started_at": "2026-06-01T00:00:00Z" },
        "verdict": {
            "status": "pass",
            "counts": { "info": 0, "warn": 0, "error": 0 },
            "reasons": []
        },
        "findings": []
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

fn run_pipeline_with_layout(
    layout: FsLayout,
    _config_path: &std::path::Path,
) -> cockpitctl_core::IngestResult {
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
// Path traversal tests
// ---------------------------------------------------------------------------

#[test]
fn path_traversal_in_configured_sensor_id_rejected() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    fs::create_dir_all(&artifacts).unwrap();

    let config_path = tmp.path().join("cockpit.toml");
    write_config(
        &config_path,
        r#"
[sensors."bad..id"]
blocking = true
missing = "fail"
"#,
    );

    let result = run_pipeline(&artifacts, &config_path);

    assert_eq!(result.exit_code, 2, "path traversal sensor → exit 2");
    assert!(
        result
            .report
            .highlights
            .iter()
            .any(|h| h.finding.code == "cockpit.path_traversal"),
        "should emit path_traversal highlight"
    );
}

#[test]
fn dotdot_sensor_id_in_config_fails() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    fs::create_dir_all(&artifacts).unwrap();

    let config_path = tmp.path().join("cockpit.toml");
    write_config(
        &config_path,
        r#"
[sensors."../escape"]
blocking = true
missing = "fail"
"#,
    );

    let result = run_pipeline(&artifacts, &config_path);

    assert_eq!(result.exit_code, 2);
    assert!(
        result
            .report
            .highlights
            .iter()
            .any(|h| h.finding.code == "cockpit.path_traversal"),
        "should reject ../ in sensor id"
    );
}

// ---------------------------------------------------------------------------
// Oversize receipt tests
// ---------------------------------------------------------------------------

#[test]
fn oversized_receipt_emits_highlight_and_continues() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    fs::create_dir_all(&artifacts).unwrap();

    // Create a receipt that exceeds a small cap.
    let sensor_dir = artifacts.join("big-sensor");
    fs::create_dir_all(&sensor_dir).unwrap();
    let oversized = "x".repeat(2048);
    fs::write(sensor_dir.join("report.json"), &oversized).unwrap();

    // Also create a normal sensor so we verify the pipeline continues.
    create_sensor(&artifacts, "good-sensor", &minimal_receipt("good-sensor"));

    let config_path = tmp.path().join("cockpit.toml");
    write_config(
        &config_path,
        r#"
[sensors.big-sensor]
blocking = true
missing = "fail"

[sensors.good-sensor]
blocking = true
missing = "fail"
"#,
    );

    let layout = FsLayout::new(&artifacts, &config_path).with_max_receipt_bytes(1024);
    let result = run_pipeline_with_layout(layout, &config_path);

    assert_eq!(result.exit_code, 2, "oversized blocking sensor → exit 2");
    assert!(
        result
            .report
            .highlights
            .iter()
            .any(|h| h.finding.code == "cockpit.receipt_oversized"),
        "should emit receipt_oversized highlight"
    );
    // Good sensor should still be present.
    assert!(
        result.report.sensors.iter().any(|s| s.id == "good-sensor"),
        "good sensor should still be processed"
    );
    // Output files should still be written (even on exit 2).
    assert!(artifacts.join("cockpit").join("report.json").exists());
    assert!(artifacts.join("cockpit").join("comment.md").exists());
}

// ---------------------------------------------------------------------------
// Max receipts cap
// ---------------------------------------------------------------------------

#[test]
fn max_receipts_cap_truncates_and_emits_highlight() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    fs::create_dir_all(&artifacts).unwrap();

    for i in 0..5 {
        create_sensor(
            &artifacts,
            &format!("sensor-{i:02}"),
            &minimal_receipt(&format!("sensor-{i:02}")),
        );
    }

    let config_path = tmp.path().join("cockpit.toml");
    // No config — all discovered.

    let layout = FsLayout::new(&artifacts, &config_path).with_max_receipts(2);
    let result = run_pipeline_with_layout(layout, &config_path);

    // Only 2 sensors should be processed.
    assert_eq!(result.report.sensors.len(), 2);
    assert!(
        result
            .report
            .highlights
            .iter()
            .any(|h| h.finding.code == "cockpit.sensors_truncated"),
        "should emit sensors_truncated highlight"
    );
}

// ---------------------------------------------------------------------------
// Invalid JSON receipt
// ---------------------------------------------------------------------------

#[test]
fn invalid_json_receipt_emits_invalid_receipt_highlight() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    fs::create_dir_all(&artifacts).unwrap();

    let sensor_dir = artifacts.join("broken");
    fs::create_dir_all(&sensor_dir).unwrap();
    fs::write(sensor_dir.join("report.json"), "{ not valid json").unwrap();

    let config_path = tmp.path().join("cockpit.toml");
    write_config(
        &config_path,
        r#"
[sensors.broken]
blocking = true
missing = "fail"
"#,
    );

    let result = run_pipeline(&artifacts, &config_path);

    assert_eq!(result.exit_code, 2, "invalid receipt on blocking → exit 2");
    assert!(
        result
            .report
            .highlights
            .iter()
            .any(|h| h.finding.code == "cockpit.invalid_receipt"),
        "should emit invalid_receipt highlight"
    );
}

// ---------------------------------------------------------------------------
// Safety: cockpit dir is not discovered as a sensor
// ---------------------------------------------------------------------------

#[test]
fn cockpit_output_dir_not_treated_as_sensor() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    fs::create_dir_all(&artifacts).unwrap();

    create_sensor(&artifacts, "real-sensor", &minimal_receipt("real-sensor"));
    // Pre-create the cockpit output dir with a report.json (as if from a previous run).
    let cockpit_dir = artifacts.join("cockpit");
    fs::create_dir_all(&cockpit_dir).unwrap();
    fs::write(cockpit_dir.join("report.json"), "{}").unwrap();

    let config_path = tmp.path().join("cockpit.toml");
    let result = run_pipeline(&artifacts, &config_path);

    // Only the real sensor should appear.
    assert_eq!(result.report.sensors.len(), 1);
    assert_eq!(result.report.sensors[0].id, "real-sensor");
}

// ---------------------------------------------------------------------------
// Safety: outputs are always written even on failure
// ---------------------------------------------------------------------------

#[test]
fn outputs_written_even_on_policy_failure() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    fs::create_dir_all(&artifacts).unwrap();

    create_sensor(
        &artifacts,
        "failing",
        &serde_json::json!({
            "schema": "sensor.report.v1",
            "tool": { "name": "failing", "version": "1.0.0" },
            "run":  { "started_at": "2026-06-01T00:00:00Z" },
            "verdict": {
                "status": "fail",
                "counts": { "info": 0, "warn": 0, "error": 1 },
                "reasons": ["hard failure"]
            },
            "findings": [
                { "severity": "error", "code": "fail.hard", "message": "fatal error" }
            ]
        })
        .to_string(),
    );

    let config_path = tmp.path().join("cockpit.toml");
    write_config(
        &config_path,
        r#"
[sensors.failing]
blocking = true
missing = "fail"
"#,
    );

    let result = run_pipeline(&artifacts, &config_path);
    assert_eq!(result.exit_code, 2);

    let out_dir = artifacts.join("cockpit");
    assert!(
        out_dir.join("report.json").exists(),
        "report.json must be written even on exit 2"
    );
    assert!(
        out_dir.join("comment.md").exists(),
        "comment.md must be written even on exit 2"
    );

    // Verify the on-disk report has the correct verdict.
    let on_disk: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(out_dir.join("report.json")).unwrap()).unwrap();
    assert_eq!(on_disk["verdict"]["status"], "fail");
}

// ---------------------------------------------------------------------------
// Symlink safety (Unix only — requires no elevated privileges)
// ---------------------------------------------------------------------------

#[cfg(unix)]
#[test]
fn symlink_outside_artifacts_is_rejected_in_full_pipeline() {
    use std::os::unix::fs as unix_fs;

    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    fs::create_dir_all(&artifacts).unwrap();

    // Create a legitimate sensor.
    create_sensor(&artifacts, "legit", &minimal_receipt("legit"));

    // Create an external directory with a receipt.
    let external = tmp.path().join("external");
    fs::create_dir_all(&external).unwrap();
    fs::write(external.join("report.json"), minimal_receipt("evil")).unwrap();

    // Symlink artifacts/evil -> ../external
    unix_fs::symlink(&external, artifacts.join("evil")).unwrap();

    let config_path = tmp.path().join("cockpit.toml");
    write_config(
        &config_path,
        r#"
[sensors.legit]
blocking = true
missing = "fail"

[sensors.evil]
blocking = true
missing = "fail"
"#,
    );

    let result = run_pipeline(&artifacts, &config_path);

    // The evil sensor should be flagged with path_traversal.
    assert!(
        result.report.highlights.iter().any(|h| {
            h.finding.code == "cockpit.path_traversal"
                || h.finding.code == "cockpit.missing_receipt"
        }),
        "symlinked sensor outside artifacts should be rejected or treated as missing"
    );
    // Legit sensor should still be present.
    assert!(
        result
            .report
            .sensors
            .iter()
            .any(|s| s.id == "legit" && s.verdict.status == cockpitctl_core::VerdictStatus::Pass),
        "legitimate sensor should pass"
    );
}

// ---------------------------------------------------------------------------
// Empty artifacts dir produces valid output
// ---------------------------------------------------------------------------

#[test]
fn empty_artifacts_produces_valid_output() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    fs::create_dir_all(&artifacts).unwrap();

    let config_path = tmp.path().join("cockpit.toml");
    let result = run_pipeline(&artifacts, &config_path);

    assert_eq!(result.exit_code, 0);
    assert_eq!(result.report.sensors.len(), 0);
    assert_eq!(result.report.schema, "cockpit.report.v1");
    assert!(!result.comment_md.is_empty());
}
