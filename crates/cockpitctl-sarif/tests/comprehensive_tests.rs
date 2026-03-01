//! Comprehensive SARIF tests: edge cases, format correctness, determinism,
//! multi-sensor scenarios, location/rule/severity mapping, and error paths.

use std::collections::BTreeMap;

use cockpitctl_sarif::{cockpit_report_to_sarif, cockpit_report_to_sarif_json};
use cockpitctl_types::*;
use pretty_assertions::assert_eq;

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

// ============================================================================
// 1) Edge cases
// ============================================================================

#[test]
fn edge_single_finding_minimal() {
    let report = CockpitReport {
        highlights: vec![make_highlight(
            "s",
            Severity::Info,
            "C1",
            "one finding",
            None,
            None,
            None,
            None,
        )],
        ..base_report()
    };
    let sarif = cockpit_report_to_sarif(&report);
    assert_eq!(sarif.runs.len(), 1);
    assert_eq!(sarif.runs[0].results.len(), 1);
    assert_eq!(sarif.runs[0].tool.driver.rules.len(), 1);
}

#[test]
fn edge_many_findings_stress() {
    let highlights: Vec<Highlight> = (0..200)
        .map(|i| {
            make_highlight(
                &format!("sensor-{}", i % 5),
                match i % 3 {
                    0 => Severity::Error,
                    1 => Severity::Warn,
                    _ => Severity::Info,
                },
                &format!("CODE-{i:04}"),
                &format!("Finding number {i}"),
                Some(&format!("src/file_{}.rs", i % 10)),
                Some(i as u32 + 1),
                None,
                None,
            )
        })
        .collect();
    let report = CockpitReport {
        highlights,
        ..base_report()
    };
    let sarif = cockpit_report_to_sarif(&report);
    assert_eq!(sarif.runs[0].results.len(), 200);
    // Each finding has a unique code → 200 rules.
    assert_eq!(sarif.runs[0].tool.driver.rules.len(), 200);
}

#[test]
fn edge_finding_all_optional_fields_populated() {
    let h = Highlight {
        sensor_id: "full-sensor".to_string(),
        finding: Finding {
            severity: Severity::Error,
            check_id: Some("CHK-42".to_string()),
            code: "FULL-001".to_string(),
            message: "fully populated finding".to_string(),
            location: Some(Location {
                path: Some("src/deep/nested/file.rs".to_string()),
                line: Some(999),
                col: Some(42),
            }),
            help: Some("Consider refactoring".to_string()),
            url: Some("https://example.com/rule/FULL-001".to_string()),
            fingerprint: Some("abcdef1234567890".to_string()),
            data: Some(serde_json::json!({"extra": "payload"})),
        },
    };
    let report = CockpitReport {
        highlights: vec![h],
        ..base_report()
    };
    let sarif = cockpit_report_to_sarif(&report);
    let r = &sarif.runs[0].results[0];
    assert_eq!(r.rule_id, "FULL-001");
    assert_eq!(r.level, "error");
    assert_eq!(r.locations.len(), 1);
    assert_eq!(
        r.locations[0].physical_location.artifact_location.uri,
        "src/deep/nested/file.rs"
    );
    assert_eq!(
        r.locations[0]
            .physical_location
            .region
            .as_ref()
            .unwrap()
            .start_line,
        Some(999)
    );
    assert_eq!(
        r.locations[0]
            .physical_location
            .region
            .as_ref()
            .unwrap()
            .start_column,
        Some(42)
    );
    assert_eq!(
        r.fingerprints.get("cockpitctl/v1"),
        Some(&"abcdef1234567890".to_string())
    );
}

#[test]
fn edge_location_with_no_path_field() {
    let h = Highlight {
        sensor_id: "s".to_string(),
        finding: Finding {
            severity: Severity::Warn,
            check_id: None,
            code: "NP".to_string(),
            message: "location but no path".to_string(),
            location: Some(Location {
                path: None,
                line: Some(10),
                col: Some(5),
            }),
            help: None,
            url: None,
            fingerprint: None,
            data: None,
        },
    };
    let report = CockpitReport {
        highlights: vec![h],
        ..base_report()
    };
    let sarif = cockpit_report_to_sarif(&report);
    // No path means no SARIF location is emitted.
    assert!(sarif.runs[0].results[0].locations.is_empty());
}

#[test]
fn edge_empty_sensor_id() {
    let report = CockpitReport {
        highlights: vec![make_highlight(
            "",
            Severity::Warn,
            "C",
            "empty sensor id",
            Some("f.rs"),
            Some(1),
            None,
            None,
        )],
        ..base_report()
    };
    let sarif = cockpit_report_to_sarif(&report);
    // Conversion still succeeds; rule description includes empty sensor bracket.
    let rule = &sarif.runs[0].tool.driver.rules[0];
    assert_eq!(rule.short_description.as_ref().unwrap().text, "[] C");
}

#[test]
fn edge_empty_path_string() {
    let h = Highlight {
        sensor_id: "s".to_string(),
        finding: Finding {
            severity: Severity::Info,
            check_id: None,
            code: "EP".to_string(),
            message: "empty path string".to_string(),
            location: Some(Location {
                path: Some(String::new()),
                line: Some(1),
                col: None,
            }),
            help: None,
            url: None,
            fingerprint: None,
            data: None,
        },
    };
    let report = CockpitReport {
        highlights: vec![h],
        ..base_report()
    };
    let sarif = cockpit_report_to_sarif(&report);
    // Empty string is still a valid path for SARIF.
    assert_eq!(sarif.runs[0].results[0].locations.len(), 1);
    assert_eq!(
        sarif.runs[0].results[0].locations[0]
            .physical_location
            .artifact_location
            .uri,
        ""
    );
}

// ============================================================================
// 2) SARIF format correctness
// ============================================================================

#[test]
fn format_schema_url_exact() {
    let sarif = cockpit_report_to_sarif(&base_report());
    assert_eq!(
        sarif.schema,
        "https://raw.githubusercontent.com/oasis-tcs/sarif-spec/main/sarif-2.1/schema/sarif-schema-2.1.0.json"
    );
}

#[test]
fn format_version_is_2_1_0() {
    let sarif = cockpit_report_to_sarif(&base_report());
    assert_eq!(sarif.version, "2.1.0");
}

#[test]
fn format_json_camel_case_keys() {
    let report = CockpitReport {
        highlights: vec![make_highlight(
            "s",
            Severity::Error,
            "C",
            "msg",
            Some("f.rs"),
            Some(1),
            Some(2),
            Some("fp"),
        )],
        ..base_report()
    };
    let json = cockpit_report_to_sarif_json(&report).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    // Verify camelCase: ruleId not rule_id, shortDescription not short_description, etc.
    let result = &parsed["runs"][0]["results"][0];
    assert!(
        result.get("ruleId").is_some(),
        "should use ruleId (camelCase)"
    );
    assert!(
        result.get("rule_id").is_none(),
        "should not use rule_id (snake_case)"
    );

    let rule = &parsed["runs"][0]["tool"]["driver"]["rules"][0];
    assert!(
        rule.get("shortDescription").is_some(),
        "should use shortDescription (camelCase)"
    );

    let loc = &result["locations"][0]["physicalLocation"];
    assert!(loc.get("artifactLocation").is_some());
    assert!(loc.get("region").is_some());
    let region = &loc["region"];
    assert!(region.get("startLine").is_some());
    assert!(region.get("startColumn").is_some());
}

#[test]
fn format_top_level_keys_present() {
    let json = cockpit_report_to_sarif_json(&base_report()).unwrap();
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert!(v.get("$schema").is_some());
    assert!(v.get("version").is_some());
    assert!(v.get("runs").is_some());
    assert!(v["runs"].is_array());
    assert_eq!(v["runs"].as_array().unwrap().len(), 1);
}

#[test]
fn format_run_has_tool_and_results() {
    let json = cockpit_report_to_sarif_json(&base_report()).unwrap();
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    let run = &v["runs"][0];
    assert!(run.get("tool").is_some());
    assert!(run.get("results").is_some());
    assert!(run["tool"].get("driver").is_some());
    assert!(run["tool"]["driver"].get("name").is_some());
    assert!(run["tool"]["driver"].get("version").is_some());
}

#[test]
fn format_json_ends_with_newline() {
    let json = cockpit_report_to_sarif_json(&base_report()).unwrap();
    assert!(json.ends_with('\n'));
    // Exactly one trailing newline.
    assert!(!json.ends_with("\n\n"));
}

// ============================================================================
// 3) Severity mapping
// ============================================================================

#[test]
fn severity_error_maps_to_error() {
    let report = CockpitReport {
        highlights: vec![make_highlight(
            "s",
            Severity::Error,
            "E",
            "e",
            Some("a.rs"),
            Some(1),
            None,
            None,
        )],
        ..base_report()
    };
    let sarif = cockpit_report_to_sarif(&report);
    assert_eq!(sarif.runs[0].results[0].level, "error");
}

#[test]
fn severity_warn_maps_to_warning() {
    let report = CockpitReport {
        highlights: vec![make_highlight(
            "s",
            Severity::Warn,
            "W",
            "w",
            Some("a.rs"),
            Some(1),
            None,
            None,
        )],
        ..base_report()
    };
    let sarif = cockpit_report_to_sarif(&report);
    assert_eq!(sarif.runs[0].results[0].level, "warning");
}

#[test]
fn severity_info_maps_to_note() {
    let report = CockpitReport {
        highlights: vec![make_highlight(
            "s",
            Severity::Info,
            "I",
            "i",
            Some("a.rs"),
            Some(1),
            None,
            None,
        )],
        ..base_report()
    };
    let sarif = cockpit_report_to_sarif(&report);
    assert_eq!(sarif.runs[0].results[0].level, "note");
}

#[test]
fn severity_mixed_in_single_report() {
    let report = CockpitReport {
        highlights: vec![
            make_highlight("a", Severity::Error, "E1", "e", None, None, None, None),
            make_highlight("b", Severity::Warn, "W1", "w", None, None, None, None),
            make_highlight("c", Severity::Info, "I1", "i", None, None, None, None),
            make_highlight("d", Severity::Error, "E2", "e2", None, None, None, None),
        ],
        ..base_report()
    };
    let sarif = cockpit_report_to_sarif(&report);
    let levels: Vec<&str> = sarif.runs[0]
        .results
        .iter()
        .map(|r| r.level.as_str())
        .collect();
    assert_eq!(levels, vec!["error", "warning", "note", "error"]);
}

// ============================================================================
// 4) Location mapping
// ============================================================================

#[test]
fn location_full_path_line_col() {
    let report = CockpitReport {
        highlights: vec![make_highlight(
            "s",
            Severity::Error,
            "C",
            "m",
            Some("src/main.rs"),
            Some(42),
            Some(10),
            None,
        )],
        ..base_report()
    };
    let sarif = cockpit_report_to_sarif(&report);
    let loc = &sarif.runs[0].results[0].locations[0];
    assert_eq!(loc.physical_location.artifact_location.uri, "src/main.rs");
    let region = loc.physical_location.region.as_ref().unwrap();
    assert_eq!(region.start_line, Some(42));
    assert_eq!(region.start_column, Some(10));
}

#[test]
fn location_path_and_line_no_col() {
    let report = CockpitReport {
        highlights: vec![make_highlight(
            "s",
            Severity::Warn,
            "C",
            "m",
            Some("lib.rs"),
            Some(7),
            None,
            None,
        )],
        ..base_report()
    };
    let sarif = cockpit_report_to_sarif(&report);
    let loc = &sarif.runs[0].results[0].locations[0];
    assert_eq!(loc.physical_location.artifact_location.uri, "lib.rs");
    let region = loc.physical_location.region.as_ref().unwrap();
    assert_eq!(region.start_line, Some(7));
    assert_eq!(region.start_column, None);
}

#[test]
fn location_path_and_col_no_line() {
    let report = CockpitReport {
        highlights: vec![make_highlight(
            "s",
            Severity::Info,
            "C",
            "m",
            Some("x.rs"),
            None,
            Some(15),
            None,
        )],
        ..base_report()
    };
    let sarif = cockpit_report_to_sarif(&report);
    let loc = &sarif.runs[0].results[0].locations[0];
    assert_eq!(loc.physical_location.artifact_location.uri, "x.rs");
    let region = loc.physical_location.region.as_ref().unwrap();
    assert_eq!(region.start_line, None);
    assert_eq!(region.start_column, Some(15));
}

#[test]
fn location_path_only_no_region() {
    let report = CockpitReport {
        highlights: vec![make_highlight(
            "s",
            Severity::Warn,
            "C",
            "m",
            Some("only_path.rs"),
            None,
            None,
            None,
        )],
        ..base_report()
    };
    let sarif = cockpit_report_to_sarif(&report);
    let loc = &sarif.runs[0].results[0].locations[0];
    assert_eq!(loc.physical_location.artifact_location.uri, "only_path.rs");
    assert!(loc.physical_location.region.is_none());
}

#[test]
fn location_none_produces_empty_locations_array() {
    let report = CockpitReport {
        highlights: vec![make_highlight(
            "s",
            Severity::Error,
            "C",
            "m",
            None,
            None,
            None,
            None,
        )],
        ..base_report()
    };
    let sarif = cockpit_report_to_sarif(&report);
    assert!(sarif.runs[0].results[0].locations.is_empty());
}

#[test]
fn location_line_zero_is_preserved() {
    let report = CockpitReport {
        highlights: vec![make_highlight(
            "s",
            Severity::Info,
            "C",
            "m",
            Some("f.rs"),
            Some(0),
            None,
            None,
        )],
        ..base_report()
    };
    let sarif = cockpit_report_to_sarif(&report);
    let region = sarif.runs[0].results[0].locations[0]
        .physical_location
        .region
        .as_ref()
        .unwrap();
    assert_eq!(region.start_line, Some(0));
}

#[test]
fn location_large_line_col_numbers() {
    let report = CockpitReport {
        highlights: vec![make_highlight(
            "s",
            Severity::Warn,
            "C",
            "m",
            Some("huge.rs"),
            Some(u32::MAX),
            Some(u32::MAX),
            None,
        )],
        ..base_report()
    };
    let sarif = cockpit_report_to_sarif(&report);
    let region = sarif.runs[0].results[0].locations[0]
        .physical_location
        .region
        .as_ref()
        .unwrap();
    assert_eq!(region.start_line, Some(u32::MAX));
    assert_eq!(region.start_column, Some(u32::MAX));
}

// ============================================================================
// 5) Rule mapping
// ============================================================================

#[test]
fn rules_deduplicated_across_sensors() {
    // Two sensors report findings with the same code → one rule.
    let report = CockpitReport {
        highlights: vec![
            make_highlight(
                "sensor-a",
                Severity::Error,
                "SHARED-CODE",
                "msg a",
                None,
                None,
                None,
                None,
            ),
            make_highlight(
                "sensor-b",
                Severity::Warn,
                "SHARED-CODE",
                "msg b",
                None,
                None,
                None,
                None,
            ),
        ],
        ..base_report()
    };
    let sarif = cockpit_report_to_sarif(&report);
    assert_eq!(sarif.runs[0].tool.driver.rules.len(), 1);
    assert_eq!(sarif.runs[0].tool.driver.rules[0].id, "SHARED-CODE");
    // The first sensor seen wins for the short description.
    assert_eq!(
        sarif.runs[0].tool.driver.rules[0]
            .short_description
            .as_ref()
            .unwrap()
            .text,
        "[sensor-a] SHARED-CODE"
    );
}

#[test]
fn rules_ordered_lexicographically() {
    let report = CockpitReport {
        highlights: vec![
            make_highlight("s", Severity::Info, "zebra", "m", None, None, None, None),
            make_highlight("s", Severity::Info, "alpha", "m", None, None, None, None),
            make_highlight("s", Severity::Info, "middle", "m", None, None, None, None),
        ],
        ..base_report()
    };
    let sarif = cockpit_report_to_sarif(&report);
    let ids: Vec<&str> = sarif.runs[0]
        .tool
        .driver
        .rules
        .iter()
        .map(|r| r.id.as_str())
        .collect();
    assert_eq!(ids, vec!["alpha", "middle", "zebra"]);
}

#[test]
fn rule_short_description_format() {
    let report = CockpitReport {
        highlights: vec![make_highlight(
            "my-sensor",
            Severity::Error,
            "MY-RULE",
            "msg",
            None,
            None,
            None,
            None,
        )],
        ..base_report()
    };
    let sarif = cockpit_report_to_sarif(&report);
    let desc = &sarif.runs[0].tool.driver.rules[0]
        .short_description
        .as_ref()
        .unwrap()
        .text;
    assert_eq!(desc, "[my-sensor] MY-RULE");
}

#[test]
fn rule_id_matches_finding_code() {
    let report = CockpitReport {
        highlights: vec![
            make_highlight(
                "s",
                Severity::Error,
                "clippy::unwrap_used",
                "m",
                None,
                None,
                None,
                None,
            ),
            make_highlight("s", Severity::Warn, "E0308", "m", None, None, None, None),
        ],
        ..base_report()
    };
    let sarif = cockpit_report_to_sarif(&report);
    let rule_ids: Vec<&str> = sarif.runs[0]
        .tool
        .driver
        .rules
        .iter()
        .map(|r| r.id.as_str())
        .collect();
    // BTreeMap → sorted.
    assert_eq!(rule_ids, vec!["E0308", "clippy::unwrap_used"]);
    // Each result's rule_id matches the finding code.
    assert_eq!(sarif.runs[0].results[0].rule_id, "clippy::unwrap_used");
    assert_eq!(sarif.runs[0].results[1].rule_id, "E0308");
}

#[test]
fn many_results_same_rule_single_rule_entry() {
    let highlights: Vec<Highlight> = (0..10)
        .map(|i| {
            make_highlight(
                "s",
                Severity::Warn,
                "SAME-RULE",
                &format!("instance {i}"),
                Some(&format!("file_{i}.rs")),
                Some(i),
                None,
                None,
            )
        })
        .collect();
    let report = CockpitReport {
        highlights,
        ..base_report()
    };
    let sarif = cockpit_report_to_sarif(&report);
    assert_eq!(sarif.runs[0].tool.driver.rules.len(), 1);
    assert_eq!(sarif.runs[0].results.len(), 10);
}

// ============================================================================
// 6) Snapshot tests
// ============================================================================

#[test]
fn snapshot_five_sensor_report() {
    let report = CockpitReport {
        highlights: vec![
            make_highlight(
                "clippy",
                Severity::Error,
                "clippy::unwrap",
                "unwrap",
                Some("src/a.rs"),
                Some(10),
                Some(5),
                Some("fp1"),
            ),
            make_highlight(
                "builddiag",
                Severity::Error,
                "E0308",
                "type mismatch",
                Some("src/b.rs"),
                Some(20),
                None,
                None,
            ),
            make_highlight(
                "secaudit",
                Severity::Warn,
                "SEC-001",
                "vuln found",
                None,
                None,
                None,
                None,
            ),
            make_highlight(
                "coverage",
                Severity::Info,
                "COV-LOW",
                "low coverage",
                Some("src/c.rs"),
                Some(1),
                None,
                None,
            ),
            make_highlight(
                "fmt",
                Severity::Warn,
                "FMT-001",
                "formatting issue",
                Some("src/d.rs"),
                Some(5),
                Some(1),
                Some("fp2"),
            ),
        ],
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
    insta::assert_json_snapshot!("comprehensive__five_sensor_report", sarif);
}

#[test]
fn snapshot_fingerprints_mixed() {
    let report = CockpitReport {
        highlights: vec![
            make_highlight(
                "s",
                Severity::Error,
                "A",
                "with fp",
                Some("a.rs"),
                Some(1),
                None,
                Some("deadbeef"),
            ),
            make_highlight(
                "s",
                Severity::Warn,
                "B",
                "no fp",
                Some("b.rs"),
                Some(2),
                None,
                None,
            ),
            make_highlight(
                "s",
                Severity::Info,
                "C",
                "with fp",
                Some("c.rs"),
                Some(3),
                None,
                Some("cafebabe"),
            ),
        ],
        ..base_report()
    };
    let sarif = cockpit_report_to_sarif(&report);
    insta::assert_json_snapshot!("comprehensive__fingerprints_mixed", sarif);
}

#[test]
fn snapshot_no_locations_report() {
    let report = CockpitReport {
        highlights: vec![
            make_highlight(
                "dep-check",
                Severity::Warn,
                "DEP-001",
                "outdated dep X",
                None,
                None,
                None,
                None,
            ),
            make_highlight(
                "dep-check",
                Severity::Warn,
                "DEP-002",
                "outdated dep Y",
                None,
                None,
                None,
                None,
            ),
            make_highlight(
                "license",
                Severity::Error,
                "LIC-001",
                "GPL violation",
                None,
                None,
                None,
                None,
            ),
        ],
        ..base_report()
    };
    let sarif = cockpit_report_to_sarif(&report);
    insta::assert_json_snapshot!("comprehensive__no_locations_report", sarif);
}

#[test]
fn snapshot_skip_verdict_report() {
    let report = CockpitReport {
        verdict: Verdict {
            status: VerdictStatus::Skip,
            counts: VerdictCounts {
                info: 0,
                warn: 0,
                error: 0,
                suppressed: 0,
            },
            reasons: vec![],
        },
        ..base_report()
    };
    let sarif = cockpit_report_to_sarif(&report);
    insta::assert_json_snapshot!("comprehensive__skip_verdict_empty", sarif);
}

// ============================================================================
// 7) Error paths / edge-case handling
// ============================================================================

#[test]
fn error_path_whitespace_in_code() {
    let report = CockpitReport {
        highlights: vec![make_highlight(
            "s",
            Severity::Warn,
            "code with spaces",
            "msg",
            None,
            None,
            None,
            None,
        )],
        ..base_report()
    };
    let sarif = cockpit_report_to_sarif(&report);
    assert_eq!(sarif.runs[0].results[0].rule_id, "code with spaces");
    let json = cockpit_report_to_sarif_json(&report).unwrap();
    let _: serde_json::Value = serde_json::from_str(&json).unwrap();
}

#[test]
fn error_path_special_chars_in_path() {
    let report = CockpitReport {
        highlights: vec![make_highlight(
            "s",
            Severity::Error,
            "C",
            "m",
            Some("src/path with spaces/file (copy).rs"),
            Some(1),
            None,
            None,
        )],
        ..base_report()
    };
    let sarif = cockpit_report_to_sarif(&report);
    assert_eq!(
        sarif.runs[0].results[0].locations[0]
            .physical_location
            .artifact_location
            .uri,
        "src/path with spaces/file (copy).rs"
    );
}

#[test]
fn error_path_backslash_in_path() {
    let report = CockpitReport {
        highlights: vec![make_highlight(
            "s",
            Severity::Error,
            "C",
            "m",
            Some(r"src\windows\style\path.rs"),
            Some(1),
            None,
            None,
        )],
        ..base_report()
    };
    let json = cockpit_report_to_sarif_json(&report).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(
        parsed["runs"][0]["results"][0]["locations"][0]["physicalLocation"]["artifactLocation"]["uri"]
            .as_str()
            .unwrap(),
        r"src\windows\style\path.rs"
    );
}

#[test]
fn error_path_newlines_in_message() {
    let report = CockpitReport {
        highlights: vec![make_highlight(
            "s",
            Severity::Error,
            "C",
            "line1\nline2\nline3",
            None,
            None,
            None,
            None,
        )],
        ..base_report()
    };
    let json = cockpit_report_to_sarif_json(&report).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(
        parsed["runs"][0]["results"][0]["message"]["text"]
            .as_str()
            .unwrap(),
        "line1\nline2\nline3"
    );
}

#[test]
fn error_path_json_injection_in_fields() {
    // Ensure JSON special characters don't break serialization.
    let msg = r#"found "double quotes" and \backslashes\ and null \u0000"#;
    let report = CockpitReport {
        highlights: vec![make_highlight(
            "s",
            Severity::Error,
            "C",
            msg,
            None,
            None,
            None,
            None,
        )],
        ..base_report()
    };
    let json = cockpit_report_to_sarif_json(&report).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(
        parsed["runs"][0]["results"][0]["message"]["text"]
            .as_str()
            .unwrap(),
        msg
    );
}

// ============================================================================
// 8) Multi-sensor scenarios
// ============================================================================

#[test]
fn multi_sensor_five_sensors_all_present() {
    let sensors = ["clippy", "builddiag", "secaudit", "coverage", "fmt"];
    let highlights: Vec<Highlight> = sensors
        .iter()
        .enumerate()
        .map(|(i, &sensor)| {
            make_highlight(
                sensor,
                match i % 3 {
                    0 => Severity::Error,
                    1 => Severity::Warn,
                    _ => Severity::Info,
                },
                &format!("{}-001", sensor.to_uppercase()),
                &format!("Finding from {sensor}"),
                Some(&format!("src/{sensor}.rs")),
                Some(i as u32 + 1),
                None,
                None,
            )
        })
        .collect();
    let report = CockpitReport {
        highlights,
        ..base_report()
    };
    let sarif = cockpit_report_to_sarif(&report);
    assert_eq!(sarif.runs[0].results.len(), 5);
    assert_eq!(sarif.runs[0].tool.driver.rules.len(), 5);

    // Verify all sensor findings are present.
    let rule_ids: Vec<&str> = sarif.runs[0]
        .results
        .iter()
        .map(|r| r.rule_id.as_str())
        .collect();
    assert!(rule_ids.contains(&"CLIPPY-001"));
    assert!(rule_ids.contains(&"BUILDDIAG-001"));
    assert!(rule_ids.contains(&"SECAUDIT-001"));
    assert!(rule_ids.contains(&"COVERAGE-001"));
    assert!(rule_ids.contains(&"FMT-001"));
}

#[test]
fn multi_sensor_same_code_different_sensors() {
    let report = CockpitReport {
        highlights: vec![
            make_highlight(
                "sensor-a",
                Severity::Error,
                "E0308",
                "from a",
                Some("a.rs"),
                Some(1),
                None,
                None,
            ),
            make_highlight(
                "sensor-b",
                Severity::Warn,
                "E0308",
                "from b",
                Some("b.rs"),
                Some(2),
                None,
                None,
            ),
            make_highlight(
                "sensor-c",
                Severity::Info,
                "E0308",
                "from c",
                Some("c.rs"),
                Some(3),
                None,
                None,
            ),
        ],
        ..base_report()
    };
    let sarif = cockpit_report_to_sarif(&report);
    // Three results, but only one rule (same code).
    assert_eq!(sarif.runs[0].results.len(), 3);
    assert_eq!(sarif.runs[0].tool.driver.rules.len(), 1);
    // The first sensor to use the code wins for the rule description.
    assert_eq!(
        sarif.runs[0].tool.driver.rules[0]
            .short_description
            .as_ref()
            .unwrap()
            .text,
        "[sensor-a] E0308"
    );
}

#[test]
fn multi_sensor_results_preserve_input_order() {
    let report = CockpitReport {
        highlights: vec![
            make_highlight(
                "z-sensor",
                Severity::Info,
                "Z1",
                "last sensor first",
                None,
                None,
                None,
                None,
            ),
            make_highlight(
                "a-sensor",
                Severity::Error,
                "A1",
                "first sensor second",
                None,
                None,
                None,
                None,
            ),
            make_highlight(
                "m-sensor",
                Severity::Warn,
                "M1",
                "middle sensor third",
                None,
                None,
                None,
                None,
            ),
        ],
        ..base_report()
    };
    let sarif = cockpit_report_to_sarif(&report);
    // Results preserve input order (not sorted by sensor or severity).
    assert_eq!(sarif.runs[0].results[0].rule_id, "Z1");
    assert_eq!(sarif.runs[0].results[1].rule_id, "A1");
    assert_eq!(sarif.runs[0].results[2].rule_id, "M1");
}

// ============================================================================
// 9) Determinism
// ============================================================================

#[test]
fn determinism_identical_json_output() {
    let report = CockpitReport {
        highlights: vec![
            make_highlight(
                "s1",
                Severity::Error,
                "E1",
                "msg1",
                Some("a.rs"),
                Some(1),
                Some(2),
                Some("fp1"),
            ),
            make_highlight(
                "s2",
                Severity::Warn,
                "W1",
                "msg2",
                Some("b.rs"),
                Some(3),
                None,
                None,
            ),
            make_highlight("s3", Severity::Info, "I1", "msg3", None, None, None, None),
        ],
        ..base_report()
    };
    let json1 = cockpit_report_to_sarif_json(&report).unwrap();
    let json2 = cockpit_report_to_sarif_json(&report).unwrap();
    let json3 = cockpit_report_to_sarif_json(&report).unwrap();
    assert_eq!(json1, json2);
    assert_eq!(json2, json3);
}

#[test]
fn determinism_struct_equality() {
    let report = CockpitReport {
        highlights: vec![
            make_highlight(
                "a",
                Severity::Error,
                "C1",
                "m1",
                Some("f.rs"),
                Some(1),
                None,
                None,
            ),
            make_highlight("b", Severity::Warn, "C2", "m2", None, None, None, None),
        ],
        ..base_report()
    };
    let s1 = cockpit_report_to_sarif(&report);
    let s2 = cockpit_report_to_sarif(&report);
    // Compare via JSON serialization since SarifLog doesn't derive PartialEq.
    let j1 = serde_json::to_string(&s1).unwrap();
    let j2 = serde_json::to_string(&s2).unwrap();
    assert_eq!(j1, j2);
}

#[test]
fn determinism_rules_order_stable() {
    let report = CockpitReport {
        highlights: vec![
            make_highlight("s", Severity::Info, "z-code", "m", None, None, None, None),
            make_highlight("s", Severity::Info, "a-code", "m", None, None, None, None),
            make_highlight("s", Severity::Info, "m-code", "m", None, None, None, None),
        ],
        ..base_report()
    };
    for _ in 0..10 {
        let sarif = cockpit_report_to_sarif(&report);
        let ids: Vec<&str> = sarif.runs[0]
            .tool
            .driver
            .rules
            .iter()
            .map(|r| r.id.as_str())
            .collect();
        assert_eq!(ids, vec!["a-code", "m-code", "z-code"]);
    }
}

#[test]
fn determinism_results_order_matches_input() {
    let report = CockpitReport {
        highlights: vec![
            make_highlight("c", Severity::Info, "C3", "third", None, None, None, None),
            make_highlight("a", Severity::Error, "C1", "first", None, None, None, None),
            make_highlight("b", Severity::Warn, "C2", "second", None, None, None, None),
        ],
        ..base_report()
    };
    for _ in 0..10 {
        let sarif = cockpit_report_to_sarif(&report);
        assert_eq!(sarif.runs[0].results[0].rule_id, "C3");
        assert_eq!(sarif.runs[0].results[1].rule_id, "C1");
        assert_eq!(sarif.runs[0].results[2].rule_id, "C2");
    }
}

// ============================================================================
// Additional coverage: tool metadata, fingerprints, JSON structure
// ============================================================================

#[test]
fn tool_name_and_version_from_report() {
    let report = CockpitReport {
        tool: ToolInfo {
            name: "custom-director".to_string(),
            version: "9.8.7".to_string(),
            commit: Some("abc1234".to_string()),
        },
        ..base_report()
    };
    let sarif = cockpit_report_to_sarif(&report);
    assert_eq!(sarif.runs[0].tool.driver.name, "custom-director");
    assert_eq!(sarif.runs[0].tool.driver.version, "9.8.7");
}

#[test]
fn fingerprint_key_is_cockpitctl_v1() {
    let report = CockpitReport {
        highlights: vec![make_highlight(
            "s",
            Severity::Error,
            "C",
            "m",
            None,
            None,
            None,
            Some("my-fingerprint"),
        )],
        ..base_report()
    };
    let sarif = cockpit_report_to_sarif(&report);
    let fps = &sarif.runs[0].results[0].fingerprints;
    assert_eq!(fps.len(), 1);
    assert!(fps.contains_key("cockpitctl/v1"));
    assert_eq!(fps["cockpitctl/v1"], "my-fingerprint");
}

#[test]
fn json_no_extra_top_level_keys() {
    let json = cockpit_report_to_sarif_json(&base_report()).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    let obj = parsed.as_object().unwrap();
    let keys: Vec<&String> = obj.keys().collect();
    // Exactly 3 top-level keys: $schema, version, runs.
    assert_eq!(keys.len(), 3);
    assert!(obj.contains_key("$schema"));
    assert!(obj.contains_key("version"));
    assert!(obj.contains_key("runs"));
}

#[test]
fn json_region_omitted_when_no_line_no_col() {
    let report = CockpitReport {
        highlights: vec![make_highlight(
            "s",
            Severity::Info,
            "C",
            "m",
            Some("f.rs"),
            None,
            None,
            None,
        )],
        ..base_report()
    };
    let json = cockpit_report_to_sarif_json(&report).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    let phys_loc = &parsed["runs"][0]["results"][0]["locations"][0]["physicalLocation"];
    assert!(phys_loc.get("region").is_none());
}

#[test]
fn json_locations_key_omitted_when_empty() {
    let report = CockpitReport {
        highlights: vec![make_highlight(
            "s",
            Severity::Info,
            "C",
            "m",
            None,
            None,
            None,
            None,
        )],
        ..base_report()
    };
    let json = cockpit_report_to_sarif_json(&report).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert!(parsed["runs"][0]["results"][0].get("locations").is_none());
}

#[test]
fn json_fingerprints_key_omitted_when_empty() {
    let report = CockpitReport {
        highlights: vec![make_highlight(
            "s",
            Severity::Info,
            "C",
            "m",
            None,
            None,
            None,
            None,
        )],
        ..base_report()
    };
    let json = cockpit_report_to_sarif_json(&report).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert!(
        parsed["runs"][0]["results"][0]
            .get("fingerprints")
            .is_none()
    );
}
