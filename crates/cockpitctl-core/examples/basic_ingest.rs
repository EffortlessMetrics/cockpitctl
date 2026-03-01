//! Minimal example: run the ingest pipeline programmatically.
//!
//! This sets up a temporary artifacts directory with two sensor receipts,
//! writes a `cockpit.toml` policy, wires up the real filesystem adapters,
//! and executes the full ingest → render pipeline.
//!
//! Run with:
//! ```sh
//! cargo run -p cockpitctl-core --example basic_ingest
//! ```

use std::fs;

use cockpitctl_core::io::{FsLayout, FsOutputSink, FsPolicySource, FsReceiptSource};
use cockpitctl_core::render::render_comment;
use cockpitctl_core::types::RunInfo;
use cockpitctl_core::{IngestRequest, IngestUseCase, NoOpSchemaValidator, ToolInfo};

fn main() {
    // --- 1. Set up a temporary workspace ---
    let tmp = tempfile::TempDir::new().expect("create temp dir");
    let artifacts = tmp.path().join("artifacts");
    fs::create_dir_all(&artifacts).expect("create artifacts dir");

    // --- 2. Write two sensor receipts ---
    let receipt = |name: &str, status: &str| -> String {
        serde_json::json!({
            "schema": "sensor.report.v1",
            "tool": { "name": name, "version": "1.0.0" },
            "run":  { "started_at": "2026-01-01T00:00:00Z" },
            "verdict": {
                "status": status,
                "counts": { "info": 0, "warn": 0, "error": 0 },
                "reasons": []
            },
            "findings": []
        })
        .to_string()
    };

    for (sensor_id, status) in [("builddiag", "pass"), ("clippy", "pass")] {
        let dir = artifacts.join(sensor_id);
        fs::create_dir_all(&dir).expect("create sensor dir");
        fs::write(dir.join("report.json"), receipt(sensor_id, status)).expect("write receipt");
    }

    // --- 3. Write a cockpit.toml policy ---
    let config_path = tmp.path().join("cockpit.toml");
    fs::write(
        &config_path,
        r#"
[policy]
warn_is_fail = false

[sensors.builddiag]
blocking = true
missing = "fail"

[sensors.clippy]
blocking = false
missing = "warn"
"#,
    )
    .expect("write config");

    // --- 4. Wire up the adapters and run ingest ---
    let layout = FsLayout::new(&artifacts, &config_path);
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

    // --- 5. Inspect results ---
    println!("Exit code : {}", result.exit_code);
    println!("Verdict   : {:?}", result.report.verdict.status);
    println!("Sensors   : {}", result.report.sensors.len());
    for s in &result.report.sensors {
        println!("  - {} → {:?}", s.id, s.verdict.status);
    }
    println!("Highlights: {}", result.report.highlights.len());
    println!();
    println!("--- Comment (first 400 chars) ---");
    println!("{}", &result.comment_md[..result.comment_md.len().min(400)]);
}
