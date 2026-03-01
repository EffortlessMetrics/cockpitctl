//! Example: construct a sensor receipt from scratch and serialize it.
//!
//! Shows how to build a `SensorReport` (the `sensor.report.v1` envelope),
//! populate it with findings, and produce JSON suitable for writing to
//! `artifacts/<sensor_id>/report.json`.
//!
//! Run with:
//! ```sh
//! cargo run -p cockpitctl-types --example create_receipt
//! ```

use cockpitctl_types::{
    ArtifactPointer, Finding, Location, RunInfo, SensorReport, Severity, ToolInfo, Verdict,
    VerdictCounts, VerdictStatus,
};

fn main() {
    // --- 1. Build tool and run metadata ---
    let tool = ToolInfo {
        name: "my-custom-sensor".to_string(),
        version: "0.1.0".to_string(),
        commit: Some("abc1234".to_string()),
    };

    let run = RunInfo {
        started_at: "2026-06-15T10:30:00Z".to_string(),
        ended_at: Some("2026-06-15T10:30:05Z".to_string()),
        duration_ms: Some(5000),
        host: None,
        git: None,
        ci: None,
        capabilities: Default::default(),
    };

    // --- 2. Create findings ---
    let findings = vec![
        Finding {
            severity: Severity::Error,
            check_id: Some("SEC-001".to_string()),
            code: "security.hardcoded-secret".to_string(),
            message: "Hardcoded API key detected in source".to_string(),
            location: Some(Location {
                path: Some("src/config.rs".to_string()),
                line: Some(42),
                col: Some(5),
            }),
            help: Some("Use environment variables for secrets".to_string()),
            url: Some("https://example.com/docs/secrets".to_string()),
            fingerprint: Some("sha256:abc123".to_string()),
            data: None,
        },
        Finding {
            severity: Severity::Warn,
            check_id: None,
            code: "style.long-function".to_string(),
            message: "Function `process_data` exceeds 200 lines".to_string(),
            location: Some(Location {
                path: Some("src/lib.rs".to_string()),
                line: Some(100),
                col: None,
            }),
            help: None,
            url: None,
            fingerprint: None,
            data: None,
        },
        Finding {
            severity: Severity::Info,
            check_id: None,
            code: "info.coverage".to_string(),
            message: "Line coverage: 87.3%".to_string(),
            location: None,
            help: None,
            url: None,
            fingerprint: None,
            data: None,
        },
    ];

    // --- 3. Compute verdict from findings ---
    let counts = VerdictCounts {
        error: findings
            .iter()
            .filter(|f| f.severity == Severity::Error)
            .count() as u64,
        warn: findings
            .iter()
            .filter(|f| f.severity == Severity::Warn)
            .count() as u64,
        info: findings
            .iter()
            .filter(|f| f.severity == Severity::Info)
            .count() as u64,
        suppressed: 0,
    };

    let status = if counts.error > 0 {
        VerdictStatus::Fail
    } else if counts.warn > 0 {
        VerdictStatus::Warn
    } else {
        VerdictStatus::Pass
    };

    // --- 4. Assemble the full receipt ---
    let report = SensorReport {
        schema: "sensor.report.v1".to_string(),
        tool,
        run,
        verdict: Verdict {
            status,
            counts,
            reasons: vec!["1 error, 1 warning found".to_string()],
        },
        findings,
        artifacts: vec![ArtifactPointer {
            id: "coverage-report".to_string(),
            path: "artifacts/my-custom-sensor/coverage.html".to_string(),
            mime: "text/html".to_string(),
            schema: None,
        }],
        data: None,
    };

    // --- 5. Serialize and print ---
    let json = serde_json::to_string_pretty(&report).expect("serialize receipt");
    println!("{json}");

    // --- 6. Verify round-trip ---
    let parsed: SensorReport = serde_json::from_str(&json).expect("deserialize receipt");
    assert_eq!(parsed.schema, "sensor.report.v1");
    assert_eq!(parsed.verdict.status, VerdictStatus::Fail);
    assert_eq!(parsed.findings.len(), 3);
    println!(
        "\n✓ Round-trip OK: {} findings, verdict = {:?}",
        parsed.findings.len(),
        parsed.verdict.status
    );
}
