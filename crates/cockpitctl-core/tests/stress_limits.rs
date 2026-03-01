//! Stress tests for memory limits, edge-case inputs, and safety boundaries.
//!
//! These tests exercise cockpitctl-core through the full ingest pipeline using
//! filesystem adapters, verifying that the system handles untrusted input at
//! safety boundaries without panics or OOM.

use std::collections::BTreeMap;
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
        name: "stress-test".to_string(),
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
        capabilities: BTreeMap::new(),
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

fn receipt_with_findings(sensor_name: &str, findings: &[serde_json::Value]) -> String {
    let error_count = findings.iter().filter(|f| f["severity"] == "error").count();
    let warn_count = findings.iter().filter(|f| f["severity"] == "warn").count();
    let info_count = findings.iter().filter(|f| f["severity"] == "info").count();
    let status = if error_count > 0 {
        "fail"
    } else if warn_count > 0 {
        "warn"
    } else {
        "pass"
    };
    serde_json::json!({
        "schema": "sensor.report.v1",
        "tool": { "name": sensor_name, "version": "1.0.0" },
        "run":  { "started_at": "2026-06-01T00:00:00Z" },
        "verdict": {
            "status": status,
            "counts": {
                "info": info_count,
                "warn": warn_count,
                "error": error_count,
            },
            "reasons": []
        },
        "findings": findings
    })
    .to_string()
}

fn create_sensor(artifacts: &std::path::Path, sensor_id: &str, json: &str) {
    let dir = artifacts.join(sensor_id);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("report.json"), json).unwrap();
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
        .expect("pipeline should not panic")
}

fn run_pipeline_with_layout(layout: FsLayout) -> cockpitctl_core::IngestResult {
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
        .expect("pipeline should not panic")
}

fn write_config(path: &std::path::Path, toml_content: &str) {
    fs::write(path, toml_content).unwrap();
}

// ---------------------------------------------------------------------------
// 1. Max sensor count (100) → processes all without crash
// ---------------------------------------------------------------------------

#[test]
fn stress_max_sensor_count_100_all_processed() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    fs::create_dir_all(&artifacts).unwrap();

    for i in 0..100 {
        create_sensor(
            &artifacts,
            &format!("sensor-{i:03}"),
            &minimal_receipt(&format!("sensor-{i:03}")),
        );
    }

    let config_path = tmp.path().join("cockpit.toml");
    let result = run_pipeline(&artifacts, &config_path);

    assert_eq!(result.report.sensors.len(), 100);
    assert_eq!(result.exit_code, 0);
    assert!(!result.comment_md.is_empty());
}

// ---------------------------------------------------------------------------
// 2. 101 sensors → capped with truncation highlight
// ---------------------------------------------------------------------------

#[test]
fn stress_101_sensors_capped_or_truncated() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    fs::create_dir_all(&artifacts).unwrap();

    for i in 0..101 {
        create_sensor(
            &artifacts,
            &format!("sensor-{i:03}"),
            &minimal_receipt(&format!("sensor-{i:03}")),
        );
    }

    let config_path = tmp.path().join("cockpit.toml");
    // Default max_receipts is 100, so 101 sensors should trigger truncation.
    let result = run_pipeline(&artifacts, &config_path);

    assert_eq!(
        result.report.sensors.len(),
        100,
        "should cap at 100 sensors"
    );
    assert!(
        result
            .report
            .highlights
            .iter()
            .any(|h| h.finding.code.contains("sensors_truncated")),
        "should emit sensors_truncated highlight"
    );
}

// ---------------------------------------------------------------------------
// 3. 100 findings per sensor × 100 sensors → 10k findings processed
// ---------------------------------------------------------------------------

#[test]
fn stress_100_findings_per_sensor_times_100_sensors() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    fs::create_dir_all(&artifacts).unwrap();

    for i in 0..100 {
        let sensor_id = format!("sensor-{i:03}");
        let findings: Vec<serde_json::Value> = (0..100)
            .map(|j| {
                serde_json::json!({
                    "severity": "warn",
                    "code": format!("W{j:04}"),
                    "message": format!("warning {j} from sensor {i}")
                })
            })
            .collect();
        create_sensor(
            &artifacts,
            &sensor_id,
            &receipt_with_findings(&sensor_id, &findings),
        );
    }

    let config_path = tmp.path().join("cockpit.toml");
    let result = run_pipeline(&artifacts, &config_path);

    assert_eq!(result.report.sensors.len(), 100);
    // Highlights are capped at max_highlights (default 7).
    assert!(result.report.highlights.len() <= 7);
    // Each sensor should have truncated findings (100 > max_per_sensor_findings=20).
    for s in &result.report.sensors {
        assert!(
            s.truncated,
            "sensor {} should have truncated findings",
            s.id
        );
    }
    assert_eq!(result.exit_code, 0);
}

// ---------------------------------------------------------------------------
// 4. Single sensor with 10000 findings → handled within limits
// ---------------------------------------------------------------------------

#[test]
fn stress_single_sensor_10000_findings() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    fs::create_dir_all(&artifacts).unwrap();

    let findings: Vec<serde_json::Value> = (0..10_000)
        .map(|j| {
            serde_json::json!({
                "severity": "info",
                "code": format!("I{j:05}"),
                "message": format!("info finding number {j}")
            })
        })
        .collect();
    create_sensor(
        &artifacts,
        "mega-sensor",
        &receipt_with_findings("mega-sensor", &findings),
    );

    let config_path = tmp.path().join("cockpit.toml");
    let result = run_pipeline(&artifacts, &config_path);

    assert_eq!(result.report.sensors.len(), 1);
    assert_eq!(result.report.sensors[0].id, "mega-sensor");
    assert!(result.report.sensors[0].truncated);
    assert_eq!(result.exit_code, 0);
}

// ---------------------------------------------------------------------------
// 5. Receipt at exactly 2MB → accepted
// ---------------------------------------------------------------------------

#[test]
fn stress_receipt_exactly_2mb_accepted() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    fs::create_dir_all(&artifacts).unwrap();

    let target_size: usize = 2 * 1024 * 1024; // 2MB
    // Build a receipt and pad the message field so the total is exactly 2MB.
    let template = serde_json::json!({
        "schema": "sensor.report.v1",
        "tool": { "name": "big-sensor", "version": "1.0.0" },
        "run":  { "started_at": "2026-06-01T00:00:00Z" },
        "verdict": {
            "status": "pass",
            "counts": { "info": 0, "warn": 0, "error": 0 },
            "reasons": []
        },
        "findings": [{
            "severity": "info",
            "code": "pad",
            "message": ""
        }]
    })
    .to_string();
    // Calculate how many chars the message needs to fill.
    let overhead = template.len();
    let padding_needed = target_size.saturating_sub(overhead);
    let padded_message = "x".repeat(padding_needed);
    let mut receipt = template.replacen(
        r#""message":"""#,
        &format!(r#""message":"{}""#, padded_message),
        1,
    );
    // Trim if overshooting, or pad with trailing whitespace.
    if receipt.len() > target_size {
        receipt.truncate(target_size);
    } else {
        while receipt.len() < target_size {
            receipt.push(' ');
        }
    }
    assert_eq!(receipt.len(), target_size);

    let sensor_dir = artifacts.join("big-sensor");
    fs::create_dir_all(&sensor_dir).unwrap();
    fs::write(sensor_dir.join("report.json"), &receipt).unwrap();

    let config_path = tmp.path().join("cockpit.toml");
    let layout = FsLayout::new(&artifacts, &config_path);
    let result = run_pipeline_with_layout(layout);

    // Should not be rejected as oversized (exactly at the cap).
    assert!(
        !result
            .report
            .highlights
            .iter()
            .any(|h| h.finding.code == "cockpit.receipt_oversized"),
        "receipt at exactly 2MB should not be rejected as oversized"
    );
}

// ---------------------------------------------------------------------------
// 6. Receipt at 2MB + 1 byte → rejected
// ---------------------------------------------------------------------------

#[test]
fn stress_receipt_2mb_plus_1_rejected() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    fs::create_dir_all(&artifacts).unwrap();

    let oversized_size = 2 * 1024 * 1024 + 1;
    let data = "x".repeat(oversized_size);

    let sensor_dir = artifacts.join("oversized");
    fs::create_dir_all(&sensor_dir).unwrap();
    fs::write(sensor_dir.join("report.json"), &data).unwrap();

    let config_path = tmp.path().join("cockpit.toml");
    write_config(
        &config_path,
        r#"
[sensors.oversized]
blocking = true
missing = "fail"
"#,
    );

    let result = run_pipeline(&artifacts, &config_path);

    assert_eq!(
        result.exit_code, 2,
        "oversized receipt on blocking sensor → exit 2"
    );
    assert!(
        result
            .report
            .highlights
            .iter()
            .any(|h| h.finding.code == "cockpit.receipt_oversized"),
        "should emit receipt_oversized highlight"
    );
}

// ---------------------------------------------------------------------------
// 7. Very long sensor names (255 chars) → handled
// ---------------------------------------------------------------------------

#[test]
fn stress_long_sensor_names_255_chars() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    fs::create_dir_all(&artifacts).unwrap();

    // Use a long but valid sensor name (alphanumeric, no path traversal).
    let long_name = "a".repeat(255);
    create_sensor(&artifacts, &long_name, &minimal_receipt(&long_name));

    let config_path = tmp.path().join("cockpit.toml");
    let result = run_pipeline(&artifacts, &config_path);

    // Should process without panic. The sensor may or may not appear depending
    // on OS path limits, but the pipeline must not crash.
    assert!(!result.comment_md.is_empty());
    // If the sensor was processed, verify it's present.
    if result.report.sensors.iter().any(|s| s.id == long_name) {
        assert_eq!(result.exit_code, 0);
    }
}

// ---------------------------------------------------------------------------
// 8. Very long finding messages (10KB) → handled
// ---------------------------------------------------------------------------

#[test]
fn stress_long_finding_messages_10kb() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    fs::create_dir_all(&artifacts).unwrap();

    let long_message = "M".repeat(10 * 1024); // 10KB message
    let findings = vec![serde_json::json!({
        "severity": "warn",
        "code": "long-msg",
        "message": long_message,
    })];
    create_sensor(
        &artifacts,
        "long-msg-sensor",
        &receipt_with_findings("long-msg-sensor", &findings),
    );

    let config_path = tmp.path().join("cockpit.toml");
    let result = run_pipeline(&artifacts, &config_path);

    assert_eq!(result.report.sensors.len(), 1);
    assert_eq!(result.exit_code, 0);
    assert!(!result.comment_md.is_empty());
}

// ---------------------------------------------------------------------------
// 9. Very long file paths (1024 chars) in findings → handled
// ---------------------------------------------------------------------------

#[test]
fn stress_long_file_paths_1024_chars() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    fs::create_dir_all(&artifacts).unwrap();

    let long_path = format!("src/{}", "a/".repeat(500));
    let findings = vec![serde_json::json!({
        "severity": "error",
        "code": "deep-path",
        "message": "finding with deep path",
        "location": {
            "path": long_path,
            "line": 1
        }
    })];
    create_sensor(
        &artifacts,
        "deep-path-sensor",
        &receipt_with_findings("deep-path-sensor", &findings),
    );

    let config_path = tmp.path().join("cockpit.toml");
    let result = run_pipeline(&artifacts, &config_path);

    assert_eq!(result.report.sensors.len(), 1);
    assert!(!result.comment_md.is_empty());
}

// ---------------------------------------------------------------------------
// 10. Deeply nested JSON structures → no stack overflow
// ---------------------------------------------------------------------------

#[test]
fn stress_deeply_nested_json_no_stack_overflow() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    fs::create_dir_all(&artifacts).unwrap();

    // Build a receipt with deeply nested `data` field (128 levels).
    let mut nested = serde_json::json!("leaf");
    for _ in 0..128 {
        nested = serde_json::json!({ "inner": nested });
    }
    let receipt = serde_json::json!({
        "schema": "sensor.report.v1",
        "tool": { "name": "nested-sensor", "version": "1.0.0" },
        "run":  { "started_at": "2026-06-01T00:00:00Z" },
        "verdict": {
            "status": "pass",
            "counts": { "info": 0, "warn": 0, "error": 0 },
            "reasons": []
        },
        "findings": [{
            "severity": "info",
            "code": "nested",
            "message": "deeply nested data",
            "data": nested
        }]
    })
    .to_string();

    create_sensor(&artifacts, "nested-sensor", &receipt);

    let config_path = tmp.path().join("cockpit.toml");
    let result = run_pipeline(&artifacts, &config_path);

    assert_eq!(result.report.sensors.len(), 1);
    assert_eq!(result.exit_code, 0);
}

// ---------------------------------------------------------------------------
// 11. 10 rapid sequential ingests → all produce valid output
// ---------------------------------------------------------------------------

#[test]
fn stress_10_rapid_sequential_ingests() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    fs::create_dir_all(&artifacts).unwrap();

    create_sensor(&artifacts, "sensor-a", &minimal_receipt("sensor-a"));
    create_sensor(&artifacts, "sensor-b", &minimal_receipt("sensor-b"));

    let config_path = tmp.path().join("cockpit.toml");

    for iteration in 0..10 {
        let result = run_pipeline(&artifacts, &config_path);
        assert_eq!(
            result.report.sensors.len(),
            2,
            "iteration {iteration}: should always have 2 sensors"
        );
        assert_eq!(
            result.exit_code, 0,
            "iteration {iteration}: should always pass"
        );
        assert!(
            !result.comment_md.is_empty(),
            "iteration {iteration}: comment should not be empty"
        );
        assert_eq!(
            result.report.schema, "cockpit.report.v1",
            "iteration {iteration}: schema must be correct"
        );
    }
}

// ---------------------------------------------------------------------------
// 12. Report with 1000 highlights budget → respects budget
// ---------------------------------------------------------------------------

#[test]
fn stress_highlights_budget_1000_respected() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    fs::create_dir_all(&artifacts).unwrap();

    // Create 50 sensors each with 50 error findings → 2500 total findings.
    for i in 0..50 {
        let sensor_id = format!("sensor-{i:03}");
        let findings: Vec<serde_json::Value> = (0..50)
            .map(|j| {
                serde_json::json!({
                    "severity": "error",
                    "code": format!("E{j:04}"),
                    "message": format!("error {j} from sensor {i}")
                })
            })
            .collect();
        create_sensor(
            &artifacts,
            &sensor_id,
            &receipt_with_findings(&sensor_id, &findings),
        );
    }

    let config_path = tmp.path().join("cockpit.toml");
    write_config(
        &config_path,
        r#"
[policy]
max_highlights = 1000
"#,
    );

    let result = run_pipeline(&artifacts, &config_path);

    assert!(
        result.report.highlights.len() <= 1000,
        "highlights should be capped at configured budget of 1000, got {}",
        result.report.highlights.len()
    );
    assert_eq!(result.report.policy.max_highlights, 1000);
}

// ---------------------------------------------------------------------------
// 13. Comment with all budgets at 0 → produces minimal output
// ---------------------------------------------------------------------------

#[test]
fn stress_zero_budgets_produce_minimal_output() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    fs::create_dir_all(&artifacts).unwrap();

    create_sensor(&artifacts, "some-sensor", &minimal_receipt("some-sensor"));

    let config_path = tmp.path().join("cockpit.toml");
    write_config(
        &config_path,
        r#"
[policy]
max_highlights = 0
max_per_sensor_findings = 0
max_annotations = 0
"#,
    );

    let result = run_pipeline(&artifacts, &config_path);

    assert_eq!(
        result.report.highlights.len(),
        0,
        "0 highlights budget → 0 highlights"
    );
    assert!(
        !result.comment_md.is_empty(),
        "comment should still be produced"
    );
    assert_eq!(result.exit_code, 0);
}

// ---------------------------------------------------------------------------
// 14. Empty strings everywhere → doesn't crash
// ---------------------------------------------------------------------------

#[test]
fn stress_empty_strings_everywhere_no_crash() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    fs::create_dir_all(&artifacts).unwrap();

    let receipt = serde_json::json!({
        "schema": "sensor.report.v1",
        "tool": { "name": "", "version": "" },
        "run":  { "started_at": "" },
        "verdict": {
            "status": "pass",
            "counts": { "info": 0, "warn": 0, "error": 0 },
            "reasons": []
        },
        "findings": [{
            "severity": "info",
            "code": "",
            "message": "",
            "location": { "path": "", "line": 0 },
            "help": "",
            "url": ""
        }]
    })
    .to_string();

    create_sensor(&artifacts, "empty-strings", &receipt);

    let config_path = tmp.path().join("cockpit.toml");
    let result = run_pipeline(&artifacts, &config_path);

    assert_eq!(result.report.sensors.len(), 1);
    assert!(!result.comment_md.is_empty());
}

// ---------------------------------------------------------------------------
// 15. All None optionals → produces valid output
// ---------------------------------------------------------------------------

#[test]
fn stress_all_none_optionals_valid_output() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    fs::create_dir_all(&artifacts).unwrap();

    // Minimal receipt with no optional fields at all.
    let receipt = serde_json::json!({
        "schema": "sensor.report.v1",
        "tool": { "name": "bare", "version": "0.1.0" },
        "run":  { "started_at": "2026-06-01T00:00:00Z" },
        "verdict": {
            "status": "pass",
            "counts": { "info": 0, "warn": 0, "error": 0 },
            "reasons": []
        },
        "findings": [{
            "severity": "info",
            "code": "bare.check",
            "message": "no optionals"
        }]
    })
    .to_string();

    create_sensor(&artifacts, "bare-sensor", &receipt);

    // Also run with no config at all (None policy).
    let config_path = tmp.path().join("cockpit.toml");
    let result = run_pipeline(&artifacts, &config_path);

    assert_eq!(result.report.sensors.len(), 1);
    assert_eq!(result.report.schema, "cockpit.report.v1");
    assert_eq!(result.exit_code, 0);

    // Verify the report can round-trip through JSON.
    let json = serde_json::to_string(&result.report).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["schema"], "cockpit.report.v1");
}

// ---------------------------------------------------------------------------
// 16. Unicode stress: sensor names with multibyte characters
// ---------------------------------------------------------------------------

#[test]
fn stress_unicode_sensor_content() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    fs::create_dir_all(&artifacts).unwrap();

    // Use ASCII sensor name but embed unicode in findings.
    let receipt = serde_json::json!({
        "schema": "sensor.report.v1",
        "tool": { "name": "unicode-sensor", "version": "1.0.0" },
        "run":  { "started_at": "2026-06-01T00:00:00Z" },
        "verdict": {
            "status": "warn",
            "counts": { "info": 0, "warn": 1, "error": 0 },
            "reasons": []
        },
        "findings": [{
            "severity": "warn",
            "code": "unicode.test",
            "message": "变量未使用 • αβγδ • 🚀🔥 • файл не найден",
            "location": { "path": "src/日本語/テスト.rs", "line": 42 }
        }]
    })
    .to_string();

    create_sensor(&artifacts, "unicode-sensor", &receipt);

    let config_path = tmp.path().join("cockpit.toml");
    let result = run_pipeline(&artifacts, &config_path);

    assert_eq!(result.report.sensors.len(), 1);
    assert!(!result.comment_md.is_empty());
}

// ---------------------------------------------------------------------------
// 17. Concurrent-safety: multiple independent pipeline runs on separate dirs
// ---------------------------------------------------------------------------

#[test]
fn stress_multiple_independent_pipeline_runs() {
    // Run 5 independent pipelines in separate temp dirs to verify no global state
    // corruption when multiple instances would run sequentially.
    let results: Vec<_> = (0..5)
        .map(|i| {
            let tmp = TempDir::new().unwrap();
            let artifacts = tmp.path().join("artifacts");
            fs::create_dir_all(&artifacts).unwrap();

            for j in 0..10 {
                create_sensor(
                    &artifacts,
                    &format!("s-{j}"),
                    &minimal_receipt(&format!("s-{j}")),
                );
            }

            let config_path = tmp.path().join("cockpit.toml");
            let result = run_pipeline(&artifacts, &config_path);
            (i, result, tmp) // keep tmp alive
        })
        .collect();

    for (i, result, _tmp) in &results {
        assert_eq!(
            result.report.sensors.len(),
            10,
            "run {i}: should have 10 sensors"
        );
        assert_eq!(result.exit_code, 0, "run {i}: should pass");
    }
}
