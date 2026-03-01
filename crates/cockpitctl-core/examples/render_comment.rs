//! Example: build a cockpit report and render it to a PR comment.
//!
//! Constructs a `CockpitReport` and `CockpitConfig` in memory, then
//! calls the renderer to produce the deterministic markdown comment.
//!
//! Run with:
//! ```sh
//! cargo run -p cockpitctl-core --example render_comment
//! ```

use std::collections::BTreeMap;

use cockpitctl_core::render::render_comment;
use cockpitctl_core::types::{
    CockpitConfig, CockpitReport, Finding, Highlight, Location, MissingPolicy, Policy,
    PolicySensorSnapshot, PolicySnapshot, Presence, RunInfo, SchemaValidation, SensorPolicy,
    SensorSummary, Severity, ToolInfo, Verdict, VerdictCounts, VerdictStatus,
};

fn main() {
    // --- 1. Build a CockpitConfig ---
    let mut sensors = BTreeMap::new();
    sensors.insert(
        "builddiag".to_string(),
        SensorPolicy {
            blocking: true,
            missing: MissingPolicy::Fail,
            section: Some("Build".to_string()),
            require_label: None,
            repro: Some("cargo build 2>&1".to_string()),
        },
    );
    sensors.insert(
        "clippy".to_string(),
        SensorPolicy {
            blocking: false,
            missing: MissingPolicy::Warn,
            section: Some("Lint".to_string()),
            require_label: None,
            repro: None,
        },
    );

    let cfg = CockpitConfig {
        policy: Policy {
            warn_is_fail: false,
            max_highlights: 7,
            max_per_sensor_findings: 50,
            max_annotations: 25,
            section_order: vec!["Build".into(), "Lint".into()],
            schema_validation: SchemaValidation::Strict,
            max_receipt_size_bytes: 2 * 1024 * 1024,
        },
        buildfix: Default::default(),
        policy_signing: Default::default(),
        sensors,
        hooks: vec![],
    };

    // --- 2. Build a CockpitReport ---
    let report = CockpitReport {
        schema: "cockpit.report.v1".to_string(),
        tool: ToolInfo {
            name: "cockpitctl".to_string(),
            version: "0.3.0".to_string(),
            commit: None,
        },
        run: RunInfo {
            started_at: "2026-06-01T12:00:00Z".to_string(),
            ended_at: Some("2026-06-01T12:00:05Z".to_string()),
            duration_ms: Some(5000),
            host: None,
            git: None,
            ci: None,
            capabilities: BTreeMap::new(),
        },
        verdict: Verdict {
            status: VerdictStatus::Warn,
            counts: VerdictCounts {
                info: 0,
                warn: 1,
                error: 0,
                suppressed: 0,
            },
            reasons: vec![],
        },
        sensors: vec![
            SensorSummary {
                id: "builddiag".to_string(),
                blocking: true,
                missing: MissingPolicy::Fail,
                presence: Presence::Present,
                report_path: "artifacts/builddiag/report.json".to_string(),
                comment_path: None,
                verdict: Verdict {
                    status: VerdictStatus::Pass,
                    counts: VerdictCounts::default(),
                    reasons: vec![],
                },
                truncated: false,
                errors: vec![],
                missing_policy_applied: None,
                policy_outcome: None,
            },
            SensorSummary {
                id: "clippy".to_string(),
                blocking: false,
                missing: MissingPolicy::Warn,
                presence: Presence::Present,
                report_path: "artifacts/clippy/report.json".to_string(),
                comment_path: None,
                verdict: Verdict {
                    status: VerdictStatus::Warn,
                    counts: VerdictCounts {
                        info: 0,
                        warn: 1,
                        error: 0,
                        suppressed: 0,
                    },
                    reasons: vec!["unused_imports".to_string()],
                },
                truncated: false,
                errors: vec![],
                missing_policy_applied: None,
                policy_outcome: None,
            },
        ],
        highlights: vec![Highlight {
            sensor_id: "clippy".to_string(),
            finding: Finding {
                severity: Severity::Warn,
                check_id: None,
                code: "unused_import".to_string(),
                message: "unused import `std::io`".to_string(),
                location: Some(Location {
                    path: Some("src/main.rs".to_string()),
                    line: Some(3),
                    col: None,
                }),
                help: Some("remove the unused import".to_string()),
                url: None,
                fingerprint: Some("abc123".to_string()),
                data: None,
            },
        }],
        policy: PolicySnapshot {
            warn_is_fail: false,
            max_highlights: 7,
            max_per_sensor_findings: 50,
            max_annotations: 25,
            section_order: vec!["Build".into(), "Lint".into()],
            sensors: vec![
                PolicySensorSnapshot {
                    id: "builddiag".to_string(),
                    blocking: true,
                    missing: MissingPolicy::Fail,
                    section: Some("Build".to_string()),
                    require_label: None,
                    repro: Some("cargo build 2>&1".to_string()),
                },
                PolicySensorSnapshot {
                    id: "clippy".to_string(),
                    blocking: false,
                    missing: MissingPolicy::Warn,
                    section: Some("Lint".to_string()),
                    require_label: None,
                    repro: None,
                },
            ],
        },
        data: None,
    };

    // --- 3. Render the markdown comment ---
    let comment = render_comment(&report, &cfg);

    println!("=== Rendered PR Comment ({} bytes) ===\n", comment.len());
    println!("{comment}");
}
