//! Example: custom policy configuration and evaluation.
//!
//! Demonstrates how to configure sensors with different policies
//! (blocking vs. informational, missing behavior, warn-is-fail),
//! and shows how policy decisions affect the overall verdict.
//!
//! Run with:
//! ```sh
//! cargo run -p cockpitctl-core --example custom_policy
//! ```

use std::fs;

use cockpitctl_core::io::{FsLayout, FsOutputSink, FsPolicySource, FsReceiptSource};
use cockpitctl_core::render::render_comment;
use cockpitctl_core::types::RunInfo;
use cockpitctl_core::{IngestRequest, IngestUseCase, NoOpSchemaValidator, ToolInfo};

fn main() {
    let tmp = tempfile::TempDir::new().expect("create temp dir");
    let artifacts = tmp.path().join("artifacts");
    fs::create_dir_all(&artifacts).expect("create artifacts dir");

    // Sensor "build" passes, but "lint" reports warnings.
    let pass_receipt = serde_json::json!({
        "schema": "sensor.report.v1",
        "tool": { "name": "build", "version": "1.0.0" },
        "run":  { "started_at": "2026-01-01T00:00:00Z" },
        "verdict": {
            "status": "pass",
            "counts": { "info": 0, "warn": 0, "error": 0 },
            "reasons": []
        },
        "findings": []
    });

    let warn_receipt = serde_json::json!({
        "schema": "sensor.report.v1",
        "tool": { "name": "lint", "version": "2.0.0" },
        "run":  { "started_at": "2026-01-01T00:00:00Z" },
        "verdict": {
            "status": "warn",
            "counts": { "info": 0, "warn": 1, "error": 0 },
            "reasons": ["unused import detected"]
        },
        "findings": [
            {
                "severity": "warn",
                "code": "lint.unused-import",
                "message": "Unused import `std::io`",
                "location": { "path": "src/main.rs", "line": 3 }
            }
        ]
    });

    // Write sensor receipts to disk.
    for (id, receipt) in [("build", &pass_receipt), ("lint", &warn_receipt)] {
        let dir = artifacts.join(id);
        fs::create_dir_all(&dir).expect("create sensor dir");
        fs::write(dir.join("report.json"), receipt.to_string()).expect("write receipt");
    }

    // --- Scenario A: warn_is_fail = false (default) ---
    println!("=== Scenario A: warn_is_fail = false ===");
    run_with_policy(
        &artifacts,
        tmp.path(),
        r#"
[policy]
warn_is_fail = false

[sensors.build]
blocking = true
missing = "fail"

[sensors.lint]
blocking = true
missing = "warn"

# "security" sensor is expected but absent — missing = "skip" ignores it.
[sensors.security]
blocking = false
missing = "skip"
"#,
    );

    // --- Scenario B: warn_is_fail = true (strict) ---
    println!("\n=== Scenario B: warn_is_fail = true ===");
    run_with_policy(
        &artifacts,
        tmp.path(),
        r#"
[policy]
warn_is_fail = true

[sensors.build]
blocking = true
missing = "fail"

[sensors.lint]
blocking = true
missing = "warn"
"#,
    );
}

fn run_with_policy(artifacts: &std::path::Path, base: &std::path::Path, toml_content: &str) {
    let config_path = base.join("cockpit.toml");
    fs::write(&config_path, toml_content).expect("write config");

    let layout = FsLayout::new(artifacts, &config_path);
    let receipts = FsReceiptSource::new(layout.clone());
    let policy = FsPolicySource::new(layout.clone());
    let output = FsOutputSink::new(layout);

    let use_case = IngestUseCase::new(
        receipts,
        policy,
        output,
        NoOpSchemaValidator,
        render_comment,
    );

    let result = use_case
        .execute(IngestRequest {
            labels: vec![],
            tool: ToolInfo {
                name: "example".to_string(),
                version: "0.0.1".to_string(),
                commit: None,
            },
            run: RunInfo {
                started_at: "2026-01-01T00:00:00Z".to_string(),
                ended_at: None,
                duration_ms: None,
                host: None,
                git: None,
                ci: None,
                capabilities: Default::default(),
            },
            schema_validation_override: None,
        })
        .expect("ingest should succeed");

    println!("  Exit code : {}", result.exit_code);
    println!("  Verdict   : {:?}", result.report.verdict.status);
    for s in &result.report.sensors {
        println!(
            "  Sensor {:12} → {:?}  (blocking={}, presence={:?})",
            s.id, s.verdict.status, s.blocking, s.presence
        );
    }
    if !result.report.highlights.is_empty() {
        println!("  Highlights:");
        for h in &result.report.highlights {
            println!(
                "    [{:?}] {} — {}",
                h.finding.severity, h.finding.code, h.finding.message
            );
        }
    }
}
