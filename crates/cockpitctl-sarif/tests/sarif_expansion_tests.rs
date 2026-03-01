//! Expanded SARIF snapshot tests: structural assertions + severity mapping + tool metadata.

use std::collections::BTreeMap;

use cockpitctl_sarif::{cockpit_report_to_sarif, cockpit_report_to_sarif_json};
use cockpitctl_types::*;

// ── Helpers ─────────────────────────────────────────────────────────────

fn base_report() -> CockpitReport {
    CockpitReport {
        schema: "cockpit.report.v1".to_string(),
        tool: ToolInfo {
            name: "cockpitctl".to_string(),
            version: "0.3.0".to_string(),
            commit: None,
        },
        run: RunInfo {
            started_at: "2026-02-01T00:00:00Z".to_string(),
            ended_at: None,
            duration_ms: None,
            host: None,
            git: None,
            ci: None,
            capabilities: BTreeMap::new(),
        },
        verdict: Verdict {
            status: VerdictStatus::Pass,
            counts: VerdictCounts {
                info: 0,
                warn: 0,
                error: 0,
                suppressed: 0,
            },
            reasons: vec![],
        },
        sensors: vec![],
        highlights: vec![],
        policy: PolicySnapshot {
            warn_is_fail: false,
            max_highlights: 7,
            max_per_sensor_findings: 20,
            max_annotations: 25,
            section_order: vec![],
            sensors: vec![],
        },
        data: None,
    }
}

fn make_highlight(
    sensor_id: &str,
    severity: Severity,
    code: &str,
    message: &str,
    path: Option<&str>,
    line: Option<u32>,
) -> Highlight {
    Highlight {
        sensor_id: sensor_id.to_string(),
        finding: Finding {
            severity,
            check_id: None,
            code: code.to_string(),
            message: message.to_string(),
            location: path.map(|p| Location {
                path: Some(p.to_string()),
                line,
                col: None,
            }),
            help: None,
            url: None,
            fingerprint: None,
            data: None,
        },
    }
}

// ── Single sensor with error finding ────────────────────────────────────

#[test]
fn single_sensor_error_finding_produces_valid_sarif() {
    let report = CockpitReport {
        highlights: vec![make_highlight(
            "clippy",
            Severity::Error,
            "clippy::unwrap_used",
            "used `unwrap()` on a Result value",
            Some("src/main.rs"),
            Some(42),
        )],
        verdict: Verdict {
            status: VerdictStatus::Fail,
            counts: VerdictCounts {
                info: 0,
                warn: 0,
                error: 1,
                suppressed: 0,
            },
            reasons: vec![],
        },
        ..base_report()
    };

    let sarif = cockpit_report_to_sarif(&report);
    insta::assert_json_snapshot!("single_sensor_error_finding", sarif);

    // Structural assertions
    assert_eq!(sarif.version, "2.1.0");
    assert_eq!(sarif.runs.len(), 1);
    assert_eq!(sarif.runs[0].results.len(), 1);
    assert_eq!(sarif.runs[0].results[0].level, "error");
    assert_eq!(sarif.runs[0].results[0].rule_id, "clippy::unwrap_used");
}

// ── Multiple sensors → results array contains all ───────────────────────

#[test]
fn multi_sensor_results_contain_all_findings() {
    let report = CockpitReport {
        highlights: vec![
            make_highlight(
                "clippy",
                Severity::Error,
                "clippy::unwrap_used",
                "unwrap in main",
                Some("src/main.rs"),
                Some(10),
            ),
            make_highlight(
                "builddiag",
                Severity::Error,
                "E0308",
                "mismatched types",
                Some("src/utils.rs"),
                Some(5),
            ),
            make_highlight(
                "secaudit",
                Severity::Warn,
                "SEC-001",
                "outdated dependency",
                None,
                None,
            ),
        ],
        verdict: Verdict {
            status: VerdictStatus::Fail,
            counts: VerdictCounts {
                info: 0,
                warn: 1,
                error: 2,
                suppressed: 0,
            },
            reasons: vec![],
        },
        ..base_report()
    };

    let sarif = cockpit_report_to_sarif(&report);
    insta::assert_json_snapshot!("multi_sensor_all_results", sarif);

    assert_eq!(sarif.runs[0].results.len(), 3);
    let rule_ids: Vec<&str> = sarif.runs[0]
        .results
        .iter()
        .map(|r| r.rule_id.as_str())
        .collect();
    assert!(rule_ids.contains(&"clippy::unwrap_used"));
    assert!(rule_ids.contains(&"E0308"));
    assert!(rule_ids.contains(&"SEC-001"));
}

// ── Zero findings → valid empty results ─────────────────────────────────

#[test]
fn zero_findings_produces_empty_results() {
    let report = base_report();
    let sarif = cockpit_report_to_sarif(&report);
    insta::assert_json_snapshot!("zero_findings_empty_results", sarif);

    assert_eq!(sarif.runs.len(), 1);
    assert!(sarif.runs[0].results.is_empty());
    assert!(sarif.runs[0].tool.driver.rules.is_empty());
}

// ── All severity levels → correct SARIF level mapping ───────────────────

#[test]
fn severity_levels_map_to_correct_sarif_levels() {
    let report = CockpitReport {
        highlights: vec![
            make_highlight(
                "s",
                Severity::Error,
                "ERR",
                "error msg",
                Some("a.rs"),
                Some(1),
            ),
            make_highlight(
                "s",
                Severity::Warn,
                "WARN",
                "warn msg",
                Some("b.rs"),
                Some(2),
            ),
            make_highlight(
                "s",
                Severity::Info,
                "INFO",
                "info msg",
                Some("c.rs"),
                Some(3),
            ),
        ],
        verdict: Verdict {
            status: VerdictStatus::Fail,
            counts: VerdictCounts {
                info: 1,
                warn: 1,
                error: 1,
                suppressed: 0,
            },
            reasons: vec![],
        },
        ..base_report()
    };

    let sarif = cockpit_report_to_sarif(&report);
    insta::assert_json_snapshot!("severity_level_mapping", sarif);

    let levels: Vec<&str> = sarif.runs[0]
        .results
        .iter()
        .map(|r| r.level.as_str())
        .collect();
    assert!(levels.contains(&"error"), "Error → error");
    assert!(levels.contains(&"warning"), "Warn → warning");
    assert!(levels.contains(&"note"), "Info → note");
}

// ── Tool metadata populated correctly ───────────────────────────────────

#[test]
fn tool_metadata_name_and_version_populated() {
    let mut report = base_report();
    report.tool.name = "my-custom-tool".to_string();
    report.tool.version = "1.2.3".to_string();

    let sarif = cockpit_report_to_sarif(&report);

    assert_eq!(sarif.runs[0].tool.driver.name, "my-custom-tool");
    assert_eq!(sarif.runs[0].tool.driver.version, "1.2.3");
    assert_eq!(
        sarif.schema,
        "https://raw.githubusercontent.com/oasis-tcs/sarif-spec/main/sarif-2.1/schema/sarif-schema-2.1.0.json"
    );
}

// ── JSON roundtrip produces valid JSON ──────────────────────────────────

#[test]
fn sarif_json_output_is_valid_json() {
    let report = CockpitReport {
        highlights: vec![
            make_highlight(
                "clippy",
                Severity::Error,
                "E001",
                "err",
                Some("x.rs"),
                Some(1),
            ),
            make_highlight(
                "lint",
                Severity::Info,
                "I001",
                "info",
                Some("y.rs"),
                Some(2),
            ),
        ],
        ..base_report()
    };

    let json_str = cockpit_report_to_sarif_json(&report).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
    insta::assert_json_snapshot!("sarif_json_roundtrip_expansion", parsed);

    assert_eq!(parsed["version"], "2.1.0");
    assert!(parsed["runs"].is_array());
    assert_eq!(parsed["runs"][0]["results"].as_array().unwrap().len(), 2);
}
