//! Snapshot tests for SARIF output structure edge cases.

use std::collections::BTreeMap;

use cockpitctl_sarif::{cockpit_report_to_sarif, cockpit_report_to_sarif_json};
use cockpitctl_types::*;

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
            counts: VerdictCounts::default(),
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
    col: Option<u32>,
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
                col,
            }),
            help: None,
            url: None,
            fingerprint: None,
            data: None,
        },
    }
}

// ---------------------------------------------------------------------------
// SARIF with unicode in messages and paths
// ---------------------------------------------------------------------------

#[test]
fn snapshot_sarif_unicode_content() {
    let report = CockpitReport {
        highlights: vec![
            make_highlight(
                "lint-日本",
                Severity::Error,
                "E-日本語",
                "日本語のエラーメッセージ",
                Some("ソース/ファイル.rs"),
                Some(42),
                Some(10),
            ),
            make_highlight(
                "lint-émoji",
                Severity::Warn,
                "W-café",
                "c'est un avertissement 🔥",
                Some("café/résumé.rs"),
                Some(7),
                None,
            ),
        ],
        verdict: Verdict {
            status: VerdictStatus::Fail,
            counts: VerdictCounts {
                info: 0,
                warn: 1,
                error: 1,
                suppressed: 0,
            },
            reasons: vec![],
        },
        ..base_report()
    };

    let sarif = cockpit_report_to_sarif(&report);
    insta::assert_json_snapshot!("sarif_unicode_content", sarif);
}

// ---------------------------------------------------------------------------
// SARIF with many sensors (5+)
// ---------------------------------------------------------------------------

#[test]
fn snapshot_sarif_many_sensors() {
    let sensors = ["alpha", "beta", "gamma", "delta", "epsilon"];
    let highlights: Vec<Highlight> = sensors
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let sev = match i % 3 {
                0 => Severity::Error,
                1 => Severity::Warn,
                _ => Severity::Info,
            };
            make_highlight(
                s,
                sev,
                &format!("{}-001", s),
                &format!("Finding from {}", s),
                Some(&format!("src/{}.rs", s)),
                Some((i as u32 + 1) * 10),
                None,
            )
        })
        .collect();

    let report = CockpitReport {
        highlights,
        verdict: Verdict {
            status: VerdictStatus::Fail,
            counts: VerdictCounts {
                info: 1,
                warn: 2,
                error: 2,
                suppressed: 0,
            },
            reasons: vec![],
        },
        ..base_report()
    };

    let sarif = cockpit_report_to_sarif(&report);
    insta::assert_json_snapshot!("sarif_many_sensors", sarif);
}

// ---------------------------------------------------------------------------
// SARIF with findings with help and URL fields
// ---------------------------------------------------------------------------

#[test]
fn snapshot_sarif_with_help_and_url() {
    let report = CockpitReport {
        highlights: vec![Highlight {
            sensor_id: "scanner".to_string(),
            finding: Finding {
                severity: Severity::Error,
                check_id: Some("SEC-001".to_string()),
                code: "sec/vuln".to_string(),
                message: "Critical vulnerability found".to_string(),
                location: Some(Location {
                    path: Some("Cargo.lock".to_string()),
                    line: Some(100),
                    col: Some(1),
                }),
                help: Some("Upgrade the affected dependency".to_string()),
                url: Some("https://example.com/cve-2024-001".to_string()),
                fingerprint: Some("fp_vuln_001".to_string()),
                data: None,
            },
        }],
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
    insta::assert_json_snapshot!("sarif_with_help_and_url", sarif);
}

// ---------------------------------------------------------------------------
// SARIF JSON string roundtrip with complex report
// ---------------------------------------------------------------------------

#[test]
fn snapshot_sarif_json_complex_roundtrip() {
    let report = CockpitReport {
        highlights: vec![
            make_highlight(
                "build",
                Severity::Error,
                "E0308",
                "type mismatch",
                Some("src/main.rs"),
                Some(10),
                Some(5),
            ),
            make_highlight(
                "build",
                Severity::Error,
                "E0599",
                "no method named `foo`",
                Some("src/lib.rs"),
                Some(20),
                None,
            ),
            make_highlight(
                "lint",
                Severity::Warn,
                "clippy::todo",
                "TODO found",
                Some("src/utils.rs"),
                Some(5),
                Some(1),
            ),
            make_highlight(
                "security",
                Severity::Info,
                "advisory",
                "outdated dep",
                None,
                None,
                None,
            ),
        ],
        verdict: Verdict {
            status: VerdictStatus::Fail,
            counts: VerdictCounts {
                info: 1,
                warn: 1,
                error: 2,
                suppressed: 0,
            },
            reasons: vec![],
        },
        ..base_report()
    };

    let json_str = cockpit_report_to_sarif_json(&report).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
    insta::assert_json_snapshot!("sarif_json_complex_roundtrip", parsed);
}
