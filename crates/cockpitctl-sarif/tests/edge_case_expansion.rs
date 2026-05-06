//! Edge-case and stress tests for cockpitctl-sarif.
//!
//! Covers:
//! - Empty results / no highlights
//! - Huge finding sets
//! - Unicode in messages
//! - Missing locations, line 0, no path
//! - Fingerprint handling
//! - Rule deduplication
//! - JSON round-trip stability

use std::collections::BTreeMap;

use cockpitctl_sarif::{cockpit_report_to_sarif, cockpit_report_to_sarif_json};
use cockpitctl_types::{
    CockpitReport, Finding, Highlight, Location, PolicySnapshot, RunInfo, Severity, ToolInfo,
    Verdict, VerdictCounts, VerdictStatus,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn minimal_report(highlights: Vec<Highlight>) -> CockpitReport {
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
        highlights,
        policy: PolicySnapshot {
            warn_is_fail: false,
            max_highlights: 10,
            max_per_sensor_findings: 20,
            max_annotations: 10,
            section_order: vec![],
            sensors: vec![],
        },
        data: None,
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "CLI and test helpers mirror stable input surfaces."
)]
fn make_highlight(
    sensor_id: &str,
    severity: Severity,
    code: &str,
    message: &str,
    path: Option<&str>,
    line: Option<u32>,
    col: Option<u32>,
    fingerprint: Option<&str>,
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
            fingerprint: fingerprint.map(|f| f.to_string()),
            data: None,
        },
    }
}

// ===========================================================================
// 1. Empty results
// ===========================================================================

#[test]
fn sarif_empty_highlights_produces_empty_results() {
    let report = minimal_report(vec![]);
    let sarif = cockpit_report_to_sarif(&report);
    assert_eq!(sarif.runs.len(), 1);
    assert!(sarif.runs[0].results.is_empty());
    assert!(sarif.runs[0].tool.driver.rules.is_empty());
}

#[test]
fn sarif_empty_highlights_json_is_valid() {
    let report = minimal_report(vec![]);
    let json = cockpit_report_to_sarif_json(&report).unwrap();
    assert!(json.contains("\"version\": \"2.1.0\""));
    assert!(json.contains("\"results\": []"));
}

// ===========================================================================
// 2. Huge finding sets
// ===========================================================================

#[test]
fn sarif_many_findings_all_converted() {
    let highlights: Vec<Highlight> = (0..500)
        .map(|i| {
            make_highlight(
                &format!("sensor_{}", i % 5),
                if i % 3 == 0 {
                    Severity::Error
                } else if i % 3 == 1 {
                    Severity::Warn
                } else {
                    Severity::Info
                },
                &format!("CODE_{:04}", i),
                &format!("Finding message number {}", i),
                Some(&format!("src/file_{}.rs", i)),
                Some(i as u32),
                None,
                None,
            )
        })
        .collect();
    let report = minimal_report(highlights);
    let sarif = cockpit_report_to_sarif(&report);
    assert_eq!(sarif.runs[0].results.len(), 500);
    // Each unique code should produce a rule
    assert_eq!(sarif.runs[0].tool.driver.rules.len(), 500);
}

#[test]
fn sarif_many_findings_json_round_trips() {
    let highlights: Vec<Highlight> = (0..100)
        .map(|i| {
            make_highlight(
                "sensor",
                Severity::Warn,
                &format!("C{:04}", i),
                &format!("msg {}", i),
                Some("f.rs"),
                Some(i as u32),
                None,
                None,
            )
        })
        .collect();
    let report = minimal_report(highlights);
    let json = cockpit_report_to_sarif_json(&report).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    let results = parsed["runs"][0]["results"].as_array().unwrap();
    assert_eq!(results.len(), 100);
}

// ===========================================================================
// 3. Unicode in messages
// ===========================================================================

#[test]
fn sarif_unicode_in_message_preserved() {
    let highlights = vec![make_highlight(
        "sensor",
        Severity::Error,
        "U001",
        "变量未使用 • émoji 🎉 • αβγδ",
        Some("src/main.rs"),
        Some(1),
        None,
        None,
    )];
    let report = minimal_report(highlights);
    let sarif = cockpit_report_to_sarif(&report);
    assert_eq!(
        sarif.runs[0].results[0].message.text,
        "变量未使用 • émoji 🎉 • αβγδ"
    );
}

#[test]
fn sarif_unicode_in_message_json_valid() {
    let highlights = vec![make_highlight(
        "sensor",
        Severity::Error,
        "U001",
        "日本語テスト → success ✓",
        Some("src/main.rs"),
        Some(1),
        None,
        None,
    )];
    let report = minimal_report(highlights);
    let json = cockpit_report_to_sarif_json(&report).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(
        parsed["runs"][0]["results"][0]["message"]["text"]
            .as_str()
            .unwrap(),
        "日本語テスト → success ✓"
    );
}

#[test]
fn sarif_unicode_in_code_and_sensor() {
    let highlights = vec![make_highlight(
        "传感器",
        Severity::Warn,
        "规则_001",
        "message",
        Some("src/main.rs"),
        Some(1),
        None,
        None,
    )];
    let report = minimal_report(highlights);
    let sarif = cockpit_report_to_sarif(&report);
    assert_eq!(sarif.runs[0].results[0].rule_id, "规则_001");
    assert!(
        sarif.runs[0].tool.driver.rules[0]
            .short_description
            .as_ref()
            .unwrap()
            .text
            .contains("传感器")
    );
}

// ===========================================================================
// 4. Missing locations, line 0, no path
// ===========================================================================

#[test]
fn sarif_no_location_produces_empty_locations() {
    let h = Highlight {
        sensor_id: "sensor".to_string(),
        finding: Finding {
            severity: Severity::Error,
            check_id: None,
            code: "E001".to_string(),
            message: "no location".to_string(),
            location: None,
            help: None,
            url: None,
            fingerprint: None,
            data: None,
        },
    };
    let report = minimal_report(vec![h]);
    let sarif = cockpit_report_to_sarif(&report);
    assert!(sarif.runs[0].results[0].locations.is_empty());
}

#[test]
fn sarif_line_zero_preserved() {
    let highlights = vec![make_highlight(
        "sensor",
        Severity::Error,
        "E001",
        "line zero",
        Some("src/lib.rs"),
        Some(0),
        None,
        None,
    )];
    let report = minimal_report(highlights);
    let sarif = cockpit_report_to_sarif(&report);
    let region = sarif.runs[0].results[0].locations[0]
        .physical_location
        .region
        .as_ref()
        .unwrap();
    assert_eq!(region.start_line, Some(0));
}

#[test]
fn sarif_path_only_no_region() {
    let highlights = vec![make_highlight(
        "sensor",
        Severity::Warn,
        "W001",
        "path only",
        Some("README.md"),
        None,
        None,
        None,
    )];
    let report = minimal_report(highlights);
    let sarif = cockpit_report_to_sarif(&report);
    let loc = &sarif.runs[0].results[0].locations[0];
    assert_eq!(loc.physical_location.artifact_location.uri, "README.md");
    assert!(loc.physical_location.region.is_none());
}

// ===========================================================================
// 5. Fingerprint handling
// ===========================================================================

#[test]
fn sarif_fingerprint_included_when_present() {
    let highlights = vec![make_highlight(
        "sensor",
        Severity::Error,
        "E001",
        "msg",
        Some("f.rs"),
        Some(1),
        None,
        Some("fp_abc123"),
    )];
    let report = minimal_report(highlights);
    let sarif = cockpit_report_to_sarif(&report);
    let fps = &sarif.runs[0].results[0].fingerprints;
    assert_eq!(fps.get("cockpitctl/v1").unwrap(), "fp_abc123");
}

#[test]
fn sarif_no_fingerprint_empty_map() {
    let highlights = vec![make_highlight(
        "sensor",
        Severity::Error,
        "E001",
        "msg",
        Some("f.rs"),
        Some(1),
        None,
        None,
    )];
    let report = minimal_report(highlights);
    let sarif = cockpit_report_to_sarif(&report);
    assert!(sarif.runs[0].results[0].fingerprints.is_empty());
}

#[test]
fn sarif_fingerprint_not_in_json_when_absent() {
    let highlights = vec![make_highlight(
        "sensor",
        Severity::Error,
        "E001",
        "msg",
        Some("f.rs"),
        Some(1),
        None,
        None,
    )];
    let report = minimal_report(highlights);
    let json = cockpit_report_to_sarif_json(&report).unwrap();
    // fingerprints is skip_serializing_if = "BTreeMap::is_empty"
    assert!(!json.contains("fingerprints"));
}

// ===========================================================================
// 6. Rule deduplication
// ===========================================================================

#[test]
fn sarif_duplicate_codes_produce_single_rule() {
    let highlights = vec![
        make_highlight(
            "sensor",
            Severity::Error,
            "E001",
            "first instance",
            Some("a.rs"),
            Some(1),
            None,
            None,
        ),
        make_highlight(
            "sensor",
            Severity::Error,
            "E001",
            "second instance",
            Some("b.rs"),
            Some(2),
            None,
            None,
        ),
    ];
    let report = minimal_report(highlights);
    let sarif = cockpit_report_to_sarif(&report);
    // Two results but only one rule
    assert_eq!(sarif.runs[0].results.len(), 2);
    assert_eq!(sarif.runs[0].tool.driver.rules.len(), 1);
    assert_eq!(sarif.runs[0].tool.driver.rules[0].id, "E001");
}

#[test]
fn sarif_different_codes_produce_separate_rules() {
    let highlights = vec![
        make_highlight(
            "sensor",
            Severity::Error,
            "E001",
            "error",
            Some("a.rs"),
            Some(1),
            None,
            None,
        ),
        make_highlight(
            "sensor",
            Severity::Warn,
            "W001",
            "warning",
            Some("b.rs"),
            Some(2),
            None,
            None,
        ),
    ];
    let report = minimal_report(highlights);
    let sarif = cockpit_report_to_sarif(&report);
    assert_eq!(sarif.runs[0].tool.driver.rules.len(), 2);
}

// ===========================================================================
// 7. Tool metadata
// ===========================================================================

#[test]
fn sarif_tool_name_and_version_from_report() {
    let report = minimal_report(vec![]);
    let sarif = cockpit_report_to_sarif(&report);
    assert_eq!(sarif.runs[0].tool.driver.name, "cockpitctl");
    assert_eq!(sarif.runs[0].tool.driver.version, "0.3.0");
}

// ===========================================================================
// 8. Column handling
// ===========================================================================

#[test]
fn sarif_column_included_in_region() {
    let highlights = vec![make_highlight(
        "sensor",
        Severity::Error,
        "E001",
        "msg",
        Some("f.rs"),
        Some(10),
        Some(5),
        None,
    )];
    let report = minimal_report(highlights);
    let sarif = cockpit_report_to_sarif(&report);
    let region = sarif.runs[0].results[0].locations[0]
        .physical_location
        .region
        .as_ref()
        .unwrap();
    assert_eq!(region.start_line, Some(10));
    assert_eq!(region.start_column, Some(5));
}

#[test]
fn sarif_column_only_no_line_still_produces_region() {
    let h = Highlight {
        sensor_id: "sensor".to_string(),
        finding: Finding {
            severity: Severity::Error,
            check_id: None,
            code: "E001".to_string(),
            message: "col only".to_string(),
            location: Some(Location {
                path: Some("f.rs".to_string()),
                line: None,
                col: Some(5),
            }),
            help: None,
            url: None,
            fingerprint: None,
            data: None,
        },
    };
    let report = minimal_report(vec![h]);
    let sarif = cockpit_report_to_sarif(&report);
    let region = sarif.runs[0].results[0].locations[0]
        .physical_location
        .region
        .as_ref()
        .unwrap();
    assert_eq!(region.start_line, None);
    assert_eq!(region.start_column, Some(5));
}
