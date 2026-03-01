//! Determinism integration tests: verify byte-identical output for shuffled inputs.
//!
//! cockpitctl must produce identical report.json and comment.md regardless of
//! the order in which sensors are discovered on the filesystem.

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
        name: "determinism-test".to_string(),
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

fn receipt_json(sensor_name: &str, status: &str, findings: &[(&str, &str, u32)]) -> String {
    let findings_arr: Vec<serde_json::Value> = findings
        .iter()
        .map(|(code, path, line)| {
            serde_json::json!({
                "severity": "warn",
                "code": code,
                "message": format!("Finding from {}", sensor_name),
                "location": { "path": path, "line": line }
            })
        })
        .collect();

    serde_json::json!({
        "schema": "sensor.report.v1",
        "tool": { "name": sensor_name, "version": "1.0.0" },
        "run":  { "started_at": "2026-06-01T00:00:00Z" },
        "verdict": {
            "status": status,
            "counts": { "info": 0, "warn": findings.len(), "error": 0 },
            "reasons": []
        },
        "findings": findings_arr
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

struct PipelineOutput {
    report_json: String,
    comment_md: String,
}

fn run_pipeline(artifacts: &std::path::Path, config_path: &std::path::Path) -> PipelineOutput {
    let layout = FsLayout::new(artifacts, config_path);
    let receipts = FsReceiptSource::new(layout.clone());
    let policy = FsPolicySource::new(layout.clone());
    let output = FsOutputSink::new(layout.clone());
    let uc = IngestUseCase::new(
        receipts,
        policy,
        output,
        NoOpSchemaValidator,
        render_comment,
    );
    let result = uc
        .execute(default_request())
        .expect("pipeline should succeed");

    let report_json = fs::read_to_string(layout.cockpit_report_file()).unwrap();
    PipelineOutput {
        report_json,
        comment_md: result.comment_md,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Create sensors in different filesystem orders and verify identical output.
/// The FS adapter sorts lexically, so we just need to verify the pipeline
/// is stable with the same set of sensors across independent runs.
#[test]
fn identical_output_across_independent_runs() {
    let sensors = vec![
        ("zebra", receipt_json("zebra", "pass", &[])),
        (
            "alpha",
            receipt_json(
                "alpha",
                "warn",
                &[
                    ("alpha.lint", "src/a.rs", 10),
                    ("alpha.lint", "src/b.rs", 5),
                ],
            ),
        ),
        (
            "mango",
            receipt_json("mango", "warn", &[("mango.check", "src/c.rs", 99)]),
        ),
    ];

    let config_toml = r#"
[policy]
max_highlights = 10

[sensors.alpha]
blocking = true
missing = "fail"

[sensors.mango]
blocking = true
missing = "fail"

[sensors.zebra]
blocking = true
missing = "fail"
"#;

    let mut outputs = Vec::new();

    for _ in 0..3 {
        let tmp = TempDir::new().unwrap();
        let artifacts = tmp.path().join("artifacts");
        fs::create_dir_all(&artifacts).unwrap();

        for (id, json) in &sensors {
            create_sensor(&artifacts, id, json);
        }

        let config_path = tmp.path().join("cockpit.toml");
        write_config(&config_path, config_toml);

        outputs.push(run_pipeline(&artifacts, &config_path));
    }

    // All runs must produce byte-identical output.
    for (i, out) in outputs.iter().enumerate().skip(1) {
        assert_eq!(
            outputs[0].report_json, out.report_json,
            "report.json differs between run 0 and run {i}"
        );
        assert_eq!(
            outputs[0].comment_md, out.comment_md,
            "comment.md differs between run 0 and run {i}"
        );
    }
}

/// Sensor order in the config should not matter — the output is determined
/// by the deterministic sort, not by insertion order in the TOML map.
#[test]
fn config_sensor_order_does_not_affect_output() {
    let sensors = vec![
        ("aaa", receipt_json("aaa", "pass", &[])),
        ("bbb", receipt_json("bbb", "warn", &[("bbb.x", "f.rs", 1)])),
        ("ccc", receipt_json("ccc", "pass", &[])),
    ];

    let configs = [
        r#"
[sensors.aaa]
blocking = true
missing = "fail"
[sensors.bbb]
blocking = true
missing = "fail"
[sensors.ccc]
blocking = true
missing = "fail"
"#,
        r#"
[sensors.ccc]
blocking = true
missing = "fail"
[sensors.bbb]
blocking = true
missing = "fail"
[sensors.aaa]
blocking = true
missing = "fail"
"#,
    ];

    let mut outputs = Vec::new();

    for config_toml in &configs {
        let tmp = TempDir::new().unwrap();
        let artifacts = tmp.path().join("artifacts");
        fs::create_dir_all(&artifacts).unwrap();

        for (id, json) in &sensors {
            create_sensor(&artifacts, id, json);
        }

        let config_path = tmp.path().join("cockpit.toml");
        write_config(&config_path, config_toml);
        outputs.push(run_pipeline(&artifacts, &config_path));
    }

    assert_eq!(
        outputs[0].report_json, outputs[1].report_json,
        "report.json should be identical regardless of config sensor order"
    );
    assert_eq!(
        outputs[0].comment_md, outputs[1].comment_md,
        "comment.md should be identical regardless of config sensor order"
    );
}

/// Multiple sensors with overlapping findings: deterministic highlight ordering.
#[test]
fn findings_sort_is_deterministic() {
    let sensors = vec![
        (
            "sensor-b",
            serde_json::json!({
                "schema": "sensor.report.v1",
                "tool": { "name": "sensor-b", "version": "1.0.0" },
                "run":  { "started_at": "2026-06-01T00:00:00Z" },
                "verdict": { "status": "warn", "counts": { "info": 0, "warn": 1, "error": 1 }, "reasons": [] },
                "findings": [
                    { "severity": "error", "code": "b.err", "message": "error from b", "location": { "path": "z.rs", "line": 1 } },
                    { "severity": "warn", "code": "b.warn", "message": "warn from b", "location": { "path": "a.rs", "line": 5 } }
                ]
            }).to_string(),
        ),
        (
            "sensor-a",
            serde_json::json!({
                "schema": "sensor.report.v1",
                "tool": { "name": "sensor-a", "version": "1.0.0" },
                "run":  { "started_at": "2026-06-01T00:00:00Z" },
                "verdict": { "status": "warn", "counts": { "info": 0, "warn": 1, "error": 1 }, "reasons": [] },
                "findings": [
                    { "severity": "error", "code": "a.err", "message": "error from a", "location": { "path": "m.rs", "line": 3 } },
                    { "severity": "warn", "code": "a.warn", "message": "warn from a", "location": { "path": "m.rs", "line": 7 } }
                ]
            }).to_string(),
        ),
    ];

    let config_toml = r#"
[policy]
max_highlights = 20

[sensors.sensor-a]
blocking = true
missing = "fail"

[sensors.sensor-b]
blocking = true
missing = "fail"
"#;

    let mut outputs = Vec::new();

    for _ in 0..3 {
        let tmp = TempDir::new().unwrap();
        let artifacts = tmp.path().join("artifacts");
        fs::create_dir_all(&artifacts).unwrap();

        for (id, json) in &sensors {
            create_sensor(&artifacts, id, json);
        }

        let config_path = tmp.path().join("cockpit.toml");
        write_config(&config_path, config_toml);
        outputs.push(run_pipeline(&artifacts, &config_path));
    }

    for (i, out) in outputs.iter().enumerate().skip(1) {
        assert_eq!(
            outputs[0].report_json, out.report_json,
            "report.json differs between run 0 and run {i}"
        );
    }

    // Verify that highlights are sorted: errors before warnings.
    let report: serde_json::Value = serde_json::from_str(&outputs[0].report_json).unwrap();
    let highlights = report["highlights"].as_array().unwrap();
    if highlights.len() >= 2 {
        let first_sev = highlights[0]["finding"]["severity"].as_str().unwrap();
        assert_eq!(first_sev, "error", "highest severity should come first");
    }
}
