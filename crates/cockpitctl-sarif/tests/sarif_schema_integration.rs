//! SARIF v2.1.0 schema-compliance and integration tests.
//!
//! Covers four areas requested for wave-31 expansion:
//!   1. Schema compliance — top-level fields, run structure, result types, rules
//!   2. Finding-to-SARIF mapping — severity→level, location, ruleId, message
//!   3. Edge cases — empty findings, missing location, huge messages, special chars
//!   4. Determinism — repeated runs produce identical output

use std::collections::BTreeMap;

use cockpitctl_sarif::{cockpit_report_to_sarif, cockpit_report_to_sarif_json};
use cockpitctl_types::*;
use pretty_assertions::assert_eq;

// ── Helpers ──────────────────────────────────────────────────────────────

fn base_report() -> CockpitReport {
    CockpitReport {
        schema: "cockpit.report.v1".to_string(),
        tool: ToolInfo {
            name: "cockpitctl".to_string(),
            version: "0.3.0".to_string(),
            commit: None,
        },
        run: RunInfo {
            started_at: "2026-03-01T00:00:00Z".to_string(),
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

#[allow(clippy::too_many_arguments)]
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

// =========================================================================
// 1) Schema compliance — top-level structure
// =========================================================================

#[test]
fn schema_compliance_top_level_fields() {
    let sarif = cockpit_report_to_sarif(&base_report());
    assert_eq!(sarif.version, "2.1.0");
    assert_eq!(
        sarif.schema,
        "https://raw.githubusercontent.com/oasis-tcs/sarif-spec/main/sarif-2.1/schema/sarif-schema-2.1.0.json"
    );
    assert_eq!(sarif.runs.len(), 1);
}

#[test]
fn schema_compliance_run_structure() {
    let sarif = cockpit_report_to_sarif(&base_report());
    let run = &sarif.runs[0];
    assert_eq!(run.tool.driver.name, "cockpitctl");
    assert_eq!(run.tool.driver.version, "0.3.0");
    assert!(run.tool.driver.rules.is_empty());
    assert!(run.results.is_empty());
}

#[test]
fn schema_compliance_result_field_types() {
    let mut report = base_report();
    report.highlights.push(make_highlight(
        "clippy",
        Severity::Warn,
        "W001",
        "unused variable",
        Some("src/main.rs"),
        Some(10),
        Some(5),
        Some("abc123"),
    ));
    let sarif = cockpit_report_to_sarif(&report);
    let r = &sarif.runs[0].results[0];

    assert_eq!(r.rule_id, "W001");
    assert_eq!(r.level, "warning");
    assert_eq!(r.message.text, "unused variable");
    assert_eq!(r.locations.len(), 1);
    let phys = &r.locations[0].physical_location;
    assert_eq!(phys.artifact_location.uri, "src/main.rs");
    let region = phys.region.as_ref().unwrap();
    assert_eq!(region.start_line, Some(10));
    assert_eq!(region.start_column, Some(5));
    assert_eq!(r.fingerprints.get("cockpitctl/v1").unwrap(), "abc123");
}

#[test]
fn schema_compliance_rules_array() {
    let mut report = base_report();
    report.highlights.push(make_highlight(
        "lint",
        Severity::Error,
        "E001",
        "msg1",
        None,
        None,
        None,
        None,
    ));
    report.highlights.push(make_highlight(
        "lint",
        Severity::Warn,
        "E001",
        "msg2",
        None,
        None,
        None,
        None,
    ));
    report.highlights.push(make_highlight(
        "sec",
        Severity::Info,
        "S100",
        "msg3",
        None,
        None,
        None,
        None,
    ));
    let sarif = cockpit_report_to_sarif(&report);
    let rules = &sarif.runs[0].tool.driver.rules;
    // Deduplicated by code, sorted via BTreeMap
    assert_eq!(rules.len(), 2);
    assert_eq!(rules[0].id, "E001");
    assert_eq!(rules[1].id, "S100");
}

#[test]
fn schema_compliance_json_no_unknown_top_keys() {
    let json_str = cockpit_report_to_sarif_json(&base_report()).unwrap();
    let v: serde_json::Value = serde_json::from_str(&json_str).unwrap();
    let obj = v.as_object().unwrap();
    let keys: Vec<&String> = obj.keys().collect();
    // Only $schema, version, runs at top level
    assert!(keys.contains(&&"$schema".to_string()));
    assert!(keys.contains(&&"version".to_string()));
    assert!(keys.contains(&&"runs".to_string()));
    assert_eq!(keys.len(), 3, "unexpected top-level keys: {keys:?}");
}

// =========================================================================
// 2) Finding-to-SARIF mapping
// =========================================================================

#[test]
fn mapping_severity_error_to_level() {
    let mut report = base_report();
    report.highlights.push(make_highlight(
        "s",
        Severity::Error,
        "E1",
        "err",
        None,
        None,
        None,
        None,
    ));
    let sarif = cockpit_report_to_sarif(&report);
    assert_eq!(sarif.runs[0].results[0].level, "error");
}

#[test]
fn mapping_severity_warn_to_level() {
    let mut report = base_report();
    report.highlights.push(make_highlight(
        "s",
        Severity::Warn,
        "W1",
        "wrn",
        None,
        None,
        None,
        None,
    ));
    let sarif = cockpit_report_to_sarif(&report);
    assert_eq!(sarif.runs[0].results[0].level, "warning");
}

#[test]
fn mapping_severity_info_to_level() {
    let mut report = base_report();
    report.highlights.push(make_highlight(
        "s",
        Severity::Info,
        "I1",
        "inf",
        None,
        None,
        None,
        None,
    ));
    let sarif = cockpit_report_to_sarif(&report);
    assert_eq!(sarif.runs[0].results[0].level, "note");
}

#[test]
fn mapping_severity_json_values() {
    let mut report = base_report();
    report.highlights.push(make_highlight(
        "s",
        Severity::Error,
        "E",
        "e",
        None,
        None,
        None,
        None,
    ));
    report.highlights.push(make_highlight(
        "s",
        Severity::Warn,
        "W",
        "w",
        None,
        None,
        None,
        None,
    ));
    report.highlights.push(make_highlight(
        "s",
        Severity::Info,
        "I",
        "i",
        None,
        None,
        None,
        None,
    ));
    let json_str = cockpit_report_to_sarif_json(&report).unwrap();
    let v: serde_json::Value = serde_json::from_str(&json_str).unwrap();
    let results = v["runs"][0]["results"].as_array().unwrap();
    assert_eq!(results[0]["level"], "error");
    assert_eq!(results[1]["level"], "warning");
    assert_eq!(results[2]["level"], "note");
}

#[test]
fn mapping_path_line_to_physical_location() {
    let mut report = base_report();
    report.highlights.push(make_highlight(
        "s",
        Severity::Warn,
        "C1",
        "msg",
        Some("lib/foo.rs"),
        Some(42),
        Some(8),
        None,
    ));
    let sarif = cockpit_report_to_sarif(&report);
    let loc = &sarif.runs[0].results[0].locations[0].physical_location;
    assert_eq!(loc.artifact_location.uri, "lib/foo.rs");
    assert_eq!(loc.region.as_ref().unwrap().start_line, Some(42));
    assert_eq!(loc.region.as_ref().unwrap().start_column, Some(8));
}

#[test]
fn mapping_code_to_rule_id() {
    let mut report = base_report();
    report.highlights.push(make_highlight(
        "sec",
        Severity::Error,
        "SEC-XSS-01",
        "xss found",
        None,
        None,
        None,
        None,
    ));
    let sarif = cockpit_report_to_sarif(&report);
    assert_eq!(sarif.runs[0].results[0].rule_id, "SEC-XSS-01");
    assert_eq!(sarif.runs[0].tool.driver.rules[0].id, "SEC-XSS-01");
}

#[test]
fn mapping_message_preserved() {
    let msg = "Buffer overflow detected in parse_input at offset 0xFF";
    let mut report = base_report();
    report.highlights.push(make_highlight(
        "s",
        Severity::Error,
        "E",
        msg,
        None,
        None,
        None,
        None,
    ));
    let sarif = cockpit_report_to_sarif(&report);
    assert_eq!(sarif.runs[0].results[0].message.text, msg);
}

#[test]
fn mapping_sensor_id_in_rule_description() {
    let mut report = base_report();
    report.highlights.push(make_highlight(
        "builddiag",
        Severity::Warn,
        "BD01",
        "warning msg",
        None,
        None,
        None,
        None,
    ));
    let sarif = cockpit_report_to_sarif(&report);
    let desc = sarif.runs[0].tool.driver.rules[0]
        .short_description
        .as_ref()
        .unwrap();
    assert_eq!(desc.text, "[builddiag] BD01");
}

// =========================================================================
// 3) Edge cases
// =========================================================================

#[test]
fn edge_empty_highlights() {
    let report = base_report();
    let sarif = cockpit_report_to_sarif(&report);
    assert!(sarif.runs[0].results.is_empty());
    assert!(sarif.runs[0].tool.driver.rules.is_empty());
    // JSON round-trips cleanly
    let json = cockpit_report_to_sarif_json(&report).unwrap();
    let _: serde_json::Value = serde_json::from_str(&json).unwrap();
}

#[test]
fn edge_no_location() {
    let mut report = base_report();
    report.highlights.push(make_highlight(
        "s",
        Severity::Error,
        "E1",
        "no loc",
        None,
        None,
        None,
        None,
    ));
    let sarif = cockpit_report_to_sarif(&report);
    assert!(sarif.runs[0].results[0].locations.is_empty());
}

#[test]
fn edge_huge_message() {
    let big = "X".repeat(50_000);
    let mut report = base_report();
    report.highlights.push(make_highlight(
        "s",
        Severity::Info,
        "BIG",
        &big,
        None,
        None,
        None,
        None,
    ));
    let sarif = cockpit_report_to_sarif(&report);
    assert_eq!(sarif.runs[0].results[0].message.text.len(), 50_000);
    // Must still produce valid JSON
    let json = cockpit_report_to_sarif_json(&report).unwrap();
    assert!(json.contains(&big));
}

#[test]
fn edge_special_characters_in_message() {
    let msg = r#"found "x < y" && 'a > b' in file.c"#;
    let mut report = base_report();
    report.highlights.push(make_highlight(
        "s",
        Severity::Warn,
        "SP",
        msg,
        None,
        None,
        None,
        None,
    ));
    let json = cockpit_report_to_sarif_json(&report).unwrap();
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["runs"][0]["results"][0]["message"]["text"], msg);
}

#[test]
fn edge_html_entities_in_message() {
    let msg = "&lt;script&gt;alert(1)&lt;/script&gt;";
    let mut report = base_report();
    report.highlights.push(make_highlight(
        "s",
        Severity::Error,
        "HTML",
        msg,
        None,
        None,
        None,
        None,
    ));
    let sarif = cockpit_report_to_sarif(&report);
    assert_eq!(sarif.runs[0].results[0].message.text, msg);
}

#[test]
fn edge_newlines_and_tabs_in_message() {
    let msg = "line1\nline2\ttabbed";
    let mut report = base_report();
    report.highlights.push(make_highlight(
        "s",
        Severity::Info,
        "NL",
        msg,
        None,
        None,
        None,
        None,
    ));
    let json = cockpit_report_to_sarif_json(&report).unwrap();
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["runs"][0]["results"][0]["message"]["text"], msg);
}

#[test]
fn edge_unicode_emoji_cjk() {
    let msg = format!(
        "emoji {} CJK {} math {}",
        "\u{1F680}",        // rocket emoji
        "\u{4E16}\u{754C}", // CJK characters
        "\u{221A}"          // square root symbol
    );
    let mut report = base_report();
    report.highlights.push(make_highlight(
        "s",
        Severity::Info,
        "UNI",
        &msg,
        None,
        None,
        None,
        None,
    ));
    let sarif = cockpit_report_to_sarif(&report);
    assert_eq!(sarif.runs[0].results[0].message.text, msg);
    let json = cockpit_report_to_sarif_json(&report).unwrap();
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["runs"][0]["results"][0]["message"]["text"], msg);
}

#[test]
fn edge_max_line_col_u32() {
    let mut report = base_report();
    report.highlights.push(make_highlight(
        "s",
        Severity::Warn,
        "MAX",
        "max line/col",
        Some("big.rs"),
        Some(u32::MAX),
        Some(u32::MAX),
        None,
    ));
    let sarif = cockpit_report_to_sarif(&report);
    let region = sarif.runs[0].results[0].locations[0]
        .physical_location
        .region
        .as_ref()
        .unwrap();
    assert_eq!(region.start_line, Some(u32::MAX));
    assert_eq!(region.start_column, Some(u32::MAX));
}

#[test]
fn edge_path_only_no_line_col() {
    let mut report = base_report();
    report.highlights.push(make_highlight(
        "s",
        Severity::Warn,
        "P1",
        "path only",
        Some("src/lib.rs"),
        None,
        None,
        None,
    ));
    let sarif = cockpit_report_to_sarif(&report);
    let loc = &sarif.runs[0].results[0].locations[0].physical_location;
    assert_eq!(loc.artifact_location.uri, "src/lib.rs");
    assert!(loc.region.is_none(), "no region when line/col absent");
}

// =========================================================================
// 4) Determinism
// =========================================================================

fn multi_sensor_report() -> CockpitReport {
    let mut report = base_report();
    report.highlights = vec![
        make_highlight(
            "clippy",
            Severity::Warn,
            "W001",
            "warn1",
            Some("a.rs"),
            Some(1),
            None,
            None,
        ),
        make_highlight(
            "sec",
            Severity::Error,
            "E100",
            "err1",
            Some("b.rs"),
            Some(5),
            Some(3),
            Some("fp1"),
        ),
        make_highlight(
            "lint",
            Severity::Info,
            "I050",
            "info1",
            None,
            None,
            None,
            None,
        ),
        make_highlight(
            "clippy",
            Severity::Error,
            "E200",
            "err2",
            Some("c.rs"),
            Some(99),
            None,
            Some("fp2"),
        ),
        make_highlight(
            "sec",
            Severity::Warn,
            "W001",
            "warn2",
            Some("d.rs"),
            Some(7),
            None,
            None,
        ),
    ];
    report
}

#[test]
fn determinism_five_runs_identical() {
    let report = multi_sensor_report();
    let baseline = cockpit_report_to_sarif_json(&report).unwrap();
    for i in 1..=5 {
        let attempt = cockpit_report_to_sarif_json(&report).unwrap();
        assert_eq!(baseline, attempt, "run {i} diverged from baseline");
    }
}

#[test]
fn determinism_results_preserve_input_order() {
    let report = multi_sensor_report();
    let sarif = cockpit_report_to_sarif(&report);
    let ids: Vec<&str> = sarif.runs[0]
        .results
        .iter()
        .map(|r| r.rule_id.as_str())
        .collect();
    // Results follow highlights order, not sorted
    assert_eq!(ids, vec!["W001", "E100", "I050", "E200", "W001"]);
}

#[test]
fn determinism_rules_lexicographic() {
    let report = multi_sensor_report();
    let sarif = cockpit_report_to_sarif(&report);
    let rule_ids: Vec<&str> = sarif.runs[0]
        .tool
        .driver
        .rules
        .iter()
        .map(|r| r.id.as_str())
        .collect();
    // BTreeMap => sorted lexicographically, deduplicated
    assert_eq!(rule_ids, vec!["E100", "E200", "I050", "W001"]);
}

#[test]
fn determinism_struct_and_json_agree() {
    let report = multi_sensor_report();
    let sarif = cockpit_report_to_sarif(&report);
    let json_str = cockpit_report_to_sarif_json(&report).unwrap();
    let from_json: cockpitctl_sarif::SarifLog = serde_json::from_str(&json_str).unwrap();
    assert_eq!(sarif, from_json);
}

// =========================================================================
// 5) Snapshot / golden tests
// =========================================================================

#[test]
fn schema_compliance_full() {
    let report = multi_sensor_report();
    let json = cockpit_report_to_sarif_json(&report).unwrap();
    insta::assert_snapshot!(json);
}

#[test]
fn edge_cases_mixed_locations() {
    let mut report = base_report();
    report.highlights = vec![
        make_highlight(
            "s1",
            Severity::Error,
            "E1",
            "full loc",
            Some("src/a.rs"),
            Some(10),
            Some(5),
            Some("fp1"),
        ),
        make_highlight(
            "s2",
            Severity::Warn,
            "W1",
            "path only",
            Some("src/b.rs"),
            None,
            None,
            None,
        ),
        make_highlight("s3", Severity::Info, "I1", "no loc", None, None, None, None),
    ];
    let json = cockpit_report_to_sarif_json(&report).unwrap();
    insta::assert_snapshot!(json);
}

#[test]
fn schema_integration_empty() {
    let json = cockpit_report_to_sarif_json(&base_report()).unwrap();
    insta::assert_snapshot!(json);
}
