//! SARIF v2.1.0 export for cockpitctl reports.
//!
//! Converts a `CockpitReport` into the Static Analysis Results Interchange Format
//! for consumption by GitHub Code Scanning, VS Code SARIF Viewer, etc.

use std::collections::BTreeMap;

use serde::Serialize;

use cockpitctl_types::{CockpitReport, Highlight, Severity};

// ============================================================================
// SARIF v2.1.0 types (hand-rolled, minimal subset)
// ============================================================================

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SarifLog {
    #[serde(rename = "$schema")]
    pub schema: String,
    pub version: String,
    pub runs: Vec<SarifRun>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SarifRun {
    pub tool: SarifTool,
    pub results: Vec<SarifResult>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SarifTool {
    pub driver: SarifToolComponent,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SarifToolComponent {
    pub name: String,
    pub version: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub rules: Vec<SarifRule>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SarifRule {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub short_description: Option<SarifMessage>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SarifMessage {
    pub text: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SarifResult {
    pub rule_id: String,
    pub level: String,
    pub message: SarifMessage,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub locations: Vec<SarifLocation>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub fingerprints: BTreeMap<String, String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SarifLocation {
    pub physical_location: SarifPhysicalLocation,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SarifPhysicalLocation {
    pub artifact_location: SarifArtifactLocation,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<SarifRegion>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SarifArtifactLocation {
    pub uri: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SarifRegion {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_line: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_column: Option<u32>,
}

// ============================================================================
// Conversion
// ============================================================================

fn severity_to_sarif_level(s: &Severity) -> &'static str {
    match s {
        Severity::Error => "error",
        Severity::Warn => "warning",
        Severity::Info => "note",
    }
}

fn highlight_to_sarif_result(h: &Highlight) -> SarifResult {
    let f = &h.finding;

    let mut locations = Vec::new();
    if let Some(loc) = &f.location
        && let Some(path) = &loc.path
    {
        let region = if loc.line.is_some() || loc.col.is_some() {
            Some(SarifRegion {
                start_line: loc.line,
                start_column: loc.col,
            })
        } else {
            None
        };
        locations.push(SarifLocation {
            physical_location: SarifPhysicalLocation {
                artifact_location: SarifArtifactLocation { uri: path.clone() },
                region,
            },
        });
    }

    let mut fingerprints = BTreeMap::new();
    if let Some(fp) = &f.fingerprint {
        fingerprints.insert("cockpitctl/v1".to_string(), fp.clone());
    }

    SarifResult {
        rule_id: f.code.clone(),
        level: severity_to_sarif_level(&f.severity).to_string(),
        message: SarifMessage {
            text: f.message.clone(),
        },
        locations,
        fingerprints,
    }
}

/// Convert a cockpit report to a SARIF v2.1.0 log.
pub fn cockpit_report_to_sarif(report: &CockpitReport) -> SarifLog {
    // Collect unique rules by code (BTreeMap for determinism).
    let mut rules_map: BTreeMap<String, SarifRule> = BTreeMap::new();
    for h in &report.highlights {
        rules_map
            .entry(h.finding.code.clone())
            .or_insert_with(|| SarifRule {
                id: h.finding.code.clone(),
                short_description: Some(SarifMessage {
                    text: format!("[{}] {}", h.sensor_id, h.finding.code),
                }),
            });
    }

    let rules: Vec<SarifRule> = rules_map.into_values().collect();
    let results: Vec<SarifResult> = report
        .highlights
        .iter()
        .map(highlight_to_sarif_result)
        .collect();

    SarifLog {
        schema: "https://raw.githubusercontent.com/oasis-tcs/sarif-spec/main/sarif-2.1/schema/sarif-schema-2.1.0.json".to_string(),
        version: "2.1.0".to_string(),
        runs: vec![SarifRun {
            tool: SarifTool {
                driver: SarifToolComponent {
                    name: report.tool.name.clone(),
                    version: report.tool.version.clone(),
                    rules,
                },
            },
            results,
        }],
    }
}

/// Convert a cockpit report to a pretty-printed SARIF JSON string.
pub fn cockpit_report_to_sarif_json(report: &CockpitReport) -> Result<String, serde_json::Error> {
    let sarif = cockpit_report_to_sarif(report);
    let mut json = serde_json::to_string_pretty(&sarif)?;
    json.push('\n');
    Ok(json)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cockpitctl_types::*;
    use pretty_assertions::assert_eq;

    fn minimal_report_with_highlights() -> CockpitReport {
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
                status: VerdictStatus::Warn,
                counts: VerdictCounts {
                    info: 0,
                    warn: 1,
                    error: 1,
                    suppressed: 0,
                },
                reasons: vec![],
            },
            sensors: vec![],
            highlights: vec![
                Highlight {
                    sensor_id: "clippy".to_string(),
                    finding: Finding {
                        severity: Severity::Error,
                        check_id: None,
                        code: "clippy::unwrap_used".to_string(),
                        message: "used `unwrap()` on a Result value".to_string(),
                        location: Some(Location {
                            path: Some("src/main.rs".to_string()),
                            line: Some(42),
                            col: Some(10),
                        }),
                        help: None,
                        url: None,
                        fingerprint: Some("fp_abc123".to_string()),
                        data: None,
                    },
                },
                Highlight {
                    sensor_id: "clippy".to_string(),
                    finding: Finding {
                        severity: Severity::Warn,
                        check_id: None,
                        code: "clippy::todo".to_string(),
                        message: "TODO found".to_string(),
                        location: Some(Location {
                            path: Some("src/lib.rs".to_string()),
                            line: Some(10),
                            col: None,
                        }),
                        help: None,
                        url: None,
                        fingerprint: None,
                        data: None,
                    },
                },
            ],
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

    fn empty_report() -> CockpitReport {
        CockpitReport {
            highlights: vec![],
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
            ..minimal_report_with_highlights()
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

    // ── Schema & version ────────────────────────────────────────────────

    #[test]
    fn sarif_has_correct_schema_and_version() {
        let report = minimal_report_with_highlights();
        let sarif = cockpit_report_to_sarif(&report);
        assert_eq!(sarif.version, "2.1.0");
        assert!(sarif.schema.contains("sarif-schema-2.1.0"));
    }

    // ── Severity mapping ────────────────────────────────────────────────

    #[test]
    fn sarif_severity_mapping() {
        assert_eq!(severity_to_sarif_level(&Severity::Error), "error");
        assert_eq!(severity_to_sarif_level(&Severity::Warn), "warning");
        assert_eq!(severity_to_sarif_level(&Severity::Info), "note");
    }

    #[test]
    fn sarif_severity_carried_into_result_level() {
        for (sev, expected) in [
            (Severity::Error, "error"),
            (Severity::Warn, "warning"),
            (Severity::Info, "note"),
        ] {
            let report = CockpitReport {
                highlights: vec![make_highlight(
                    "sensor",
                    sev,
                    "code",
                    "msg",
                    Some("f.rs"),
                    None,
                    None,
                    None,
                )],
                ..empty_report()
            };
            let sarif = cockpit_report_to_sarif(&report);
            assert_eq!(
                sarif.runs[0].results[0].level, expected,
                "severity mismatch for {expected}"
            );
        }
    }

    // ── Finding-to-result mapping ───────────────────────────────────────

    #[test]
    fn sarif_maps_results_from_highlights() {
        let report = minimal_report_with_highlights();
        let sarif = cockpit_report_to_sarif(&report);
        assert_eq!(sarif.runs.len(), 1);
        assert_eq!(sarif.runs[0].results.len(), 2);

        let r0 = &sarif.runs[0].results[0];
        assert_eq!(r0.rule_id, "clippy::unwrap_used");
        assert_eq!(r0.level, "error");
        assert_eq!(r0.message.text, "used `unwrap()` on a Result value");
        assert_eq!(r0.locations.len(), 1);
        assert_eq!(
            r0.locations[0].physical_location.artifact_location.uri,
            "src/main.rs"
        );
        assert_eq!(
            r0.locations[0]
                .physical_location
                .region
                .as_ref()
                .unwrap()
                .start_line,
            Some(42)
        );
        assert_eq!(
            r0.locations[0]
                .physical_location
                .region
                .as_ref()
                .unwrap()
                .start_column,
            Some(10)
        );
        assert_eq!(
            r0.fingerprints.get("cockpitctl/v1"),
            Some(&"fp_abc123".to_string())
        );

        let r1 = &sarif.runs[0].results[1];
        assert_eq!(r1.rule_id, "clippy::todo");
        assert_eq!(r1.level, "warning");
        assert_eq!(r1.message.text, "TODO found");
        assert_eq!(
            r1.locations[0].physical_location.artifact_location.uri,
            "src/lib.rs"
        );
        assert_eq!(
            r1.locations[0]
                .physical_location
                .region
                .as_ref()
                .unwrap()
                .start_line,
            Some(10)
        );
        assert!(
            r1.locations[0]
                .physical_location
                .region
                .as_ref()
                .unwrap()
                .start_column
                .is_none()
        );
        assert!(r1.fingerprints.is_empty());
    }

    #[test]
    fn sarif_result_location_with_line_only() {
        let report = CockpitReport {
            highlights: vec![make_highlight(
                "s",
                Severity::Info,
                "c",
                "m",
                Some("a.rs"),
                Some(7),
                None,
                None,
            )],
            ..empty_report()
        };
        let sarif = cockpit_report_to_sarif(&report);
        let region = sarif.runs[0].results[0].locations[0]
            .physical_location
            .region
            .as_ref()
            .unwrap();
        assert_eq!(region.start_line, Some(7));
        assert_eq!(region.start_column, None);
    }

    #[test]
    fn sarif_result_location_with_col_only() {
        let report = CockpitReport {
            highlights: vec![make_highlight(
                "s",
                Severity::Info,
                "c",
                "m",
                Some("a.rs"),
                None,
                Some(5),
                None,
            )],
            ..empty_report()
        };
        let sarif = cockpit_report_to_sarif(&report);
        let region = sarif.runs[0].results[0].locations[0]
            .physical_location
            .region
            .as_ref()
            .unwrap();
        assert_eq!(region.start_line, None);
        assert_eq!(region.start_column, Some(5));
    }

    #[test]
    fn sarif_result_path_without_line_or_col_has_no_region() {
        let report = CockpitReport {
            highlights: vec![make_highlight(
                "s",
                Severity::Warn,
                "c",
                "m",
                Some("b.rs"),
                None,
                None,
                None,
            )],
            ..empty_report()
        };
        let sarif = cockpit_report_to_sarif(&report);
        let loc = &sarif.runs[0].results[0].locations[0];
        assert_eq!(loc.physical_location.artifact_location.uri, "b.rs");
        assert!(loc.physical_location.region.is_none());
    }

    // ── Empty report / empty highlights ─────────────────────────────────

    #[test]
    fn sarif_empty_highlights_produces_empty_results() {
        let report = empty_report();
        let sarif = cockpit_report_to_sarif(&report);
        assert_eq!(sarif.runs.len(), 1);
        assert!(sarif.runs[0].results.is_empty());
        assert!(sarif.runs[0].tool.driver.rules.is_empty());
    }

    #[test]
    fn sarif_empty_report_json_is_valid() {
        let report = empty_report();
        let json = cockpit_report_to_sarif_json(&report).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["version"], "2.1.0");
        assert_eq!(parsed["runs"][0]["results"].as_array().unwrap().len(), 0);
    }

    // ── No location / missing fields ────────────────────────────────────

    #[test]
    fn sarif_no_location_omits_locations() {
        let report = CockpitReport {
            highlights: vec![Highlight {
                sensor_id: "test".to_string(),
                finding: Finding {
                    severity: Severity::Info,
                    check_id: None,
                    code: "test.code".to_string(),
                    message: "no location".to_string(),
                    location: None,
                    help: None,
                    url: None,
                    fingerprint: None,
                    data: None,
                },
            }],
            ..minimal_report_with_highlights()
        };
        let sarif = cockpit_report_to_sarif(&report);
        assert!(sarif.runs[0].results[0].locations.is_empty());
        assert!(sarif.runs[0].results[0].fingerprints.is_empty());
    }

    #[test]
    fn sarif_location_with_no_path_omits_locations() {
        let report = CockpitReport {
            highlights: vec![Highlight {
                sensor_id: "x".to_string(),
                finding: Finding {
                    severity: Severity::Warn,
                    check_id: None,
                    code: "x.y".to_string(),
                    message: "msg".to_string(),
                    location: Some(Location {
                        path: None,
                        line: Some(1),
                        col: Some(2),
                    }),
                    help: None,
                    url: None,
                    fingerprint: None,
                    data: None,
                },
            }],
            ..empty_report()
        };
        let sarif = cockpit_report_to_sarif(&report);
        assert!(sarif.runs[0].results[0].locations.is_empty());
    }

    // ── Multiple findings across different files ────────────────────────

    #[test]
    fn sarif_multiple_findings_across_files() {
        let report = CockpitReport {
            highlights: vec![
                make_highlight(
                    "clippy",
                    Severity::Error,
                    "clippy::unwrap_used",
                    "unwrap in main",
                    Some("src/main.rs"),
                    Some(10),
                    None,
                    None,
                ),
                make_highlight(
                    "clippy",
                    Severity::Warn,
                    "clippy::todo",
                    "todo in lib",
                    Some("src/lib.rs"),
                    Some(20),
                    None,
                    None,
                ),
                make_highlight(
                    "builddiag",
                    Severity::Error,
                    "E0308",
                    "mismatched types",
                    Some("src/utils.rs"),
                    Some(5),
                    Some(12),
                    Some("fp_build"),
                ),
                make_highlight(
                    "builddiag",
                    Severity::Info,
                    "E0308",
                    "expected bool, found i32",
                    Some("src/utils.rs"),
                    Some(6),
                    None,
                    None,
                ),
            ],
            ..empty_report()
        };
        let sarif = cockpit_report_to_sarif(&report);
        let run = &sarif.runs[0];

        assert_eq!(run.results.len(), 4);

        // Each unique file path appears as a location URI.
        let uris: Vec<&str> = run
            .results
            .iter()
            .filter_map(|r| r.locations.first())
            .map(|l| l.physical_location.artifact_location.uri.as_str())
            .collect();
        assert!(uris.contains(&"src/main.rs"));
        assert!(uris.contains(&"src/lib.rs"));
        assert!(uris.contains(&"src/utils.rs"));

        // Unique rule codes: clippy::unwrap_used, clippy::todo, E0308 → 3 rules.
        assert_eq!(run.tool.driver.rules.len(), 3);
    }

    #[test]
    fn sarif_multiple_findings_from_different_sensors() {
        let report = CockpitReport {
            highlights: vec![
                make_highlight(
                    "sensor-a",
                    Severity::Warn,
                    "SA01",
                    "msg1",
                    None,
                    None,
                    None,
                    None,
                ),
                make_highlight(
                    "sensor-b",
                    Severity::Error,
                    "SB01",
                    "msg2",
                    None,
                    None,
                    None,
                    None,
                ),
            ],
            ..empty_report()
        };
        let sarif = cockpit_report_to_sarif(&report);
        assert_eq!(sarif.runs[0].results.len(), 2);
        assert_eq!(sarif.runs[0].results[0].rule_id, "SA01");
        assert_eq!(sarif.runs[0].results[1].rule_id, "SB01");
    }

    // ── Rule deduplication ──────────────────────────────────────────────

    #[test]
    fn sarif_dedupes_rules_by_code() {
        let report = minimal_report_with_highlights();
        let sarif = cockpit_report_to_sarif(&report);
        // Two highlights with different codes → two rules (in driver only).
        assert_eq!(sarif.runs[0].tool.driver.rules.len(), 2);
    }

    #[test]
    fn sarif_duplicate_codes_yield_one_rule() {
        let report = CockpitReport {
            highlights: vec![
                make_highlight("s", Severity::Warn, "dup", "a", None, None, None, None),
                make_highlight("s", Severity::Error, "dup", "b", None, None, None, None),
                make_highlight("s", Severity::Info, "dup", "c", None, None, None, None),
            ],
            ..empty_report()
        };
        let sarif = cockpit_report_to_sarif(&report);
        assert_eq!(sarif.runs[0].tool.driver.rules.len(), 1);
        assert_eq!(sarif.runs[0].tool.driver.rules[0].id, "dup");
        // But results are preserved individually.
        assert_eq!(sarif.runs[0].results.len(), 3);
    }

    #[test]
    fn sarif_rules_sorted_deterministically() {
        let report = CockpitReport {
            highlights: vec![
                make_highlight("s", Severity::Info, "z-rule", "m", None, None, None, None),
                make_highlight("s", Severity::Info, "a-rule", "m", None, None, None, None),
                make_highlight("s", Severity::Info, "m-rule", "m", None, None, None, None),
            ],
            ..empty_report()
        };
        let sarif = cockpit_report_to_sarif(&report);
        let rule_ids: Vec<&str> = sarif.runs[0]
            .tool
            .driver
            .rules
            .iter()
            .map(|r| r.id.as_str())
            .collect();
        // BTreeMap iteration is lexical.
        assert_eq!(rule_ids, vec!["a-rule", "m-rule", "z-rule"]);
    }

    // ── Tool information ────────────────────────────────────────────────

    #[test]
    fn sarif_tool_info_propagated() {
        let report = CockpitReport {
            tool: ToolInfo {
                name: "my-tool".to_string(),
                version: "1.2.3".to_string(),
                commit: Some("abc123".to_string()),
            },
            ..empty_report()
        };
        let sarif = cockpit_report_to_sarif(&report);
        let driver = &sarif.runs[0].tool.driver;
        assert_eq!(driver.name, "my-tool");
        assert_eq!(driver.version, "1.2.3");
    }

    #[test]
    fn sarif_rule_short_description_includes_sensor() {
        let report = CockpitReport {
            highlights: vec![make_highlight(
                "builddiag",
                Severity::Error,
                "E0308",
                "type mismatch",
                None,
                None,
                None,
                None,
            )],
            ..empty_report()
        };
        let sarif = cockpit_report_to_sarif(&report);
        let rule = &sarif.runs[0].tool.driver.rules[0];
        assert_eq!(rule.id, "E0308");
        let desc = rule.short_description.as_ref().unwrap();
        assert_eq!(desc.text, "[builddiag] E0308");
    }

    // ── Fingerprint ─────────────────────────────────────────────────────

    #[test]
    fn sarif_fingerprint_present_when_set() {
        let report = CockpitReport {
            highlights: vec![make_highlight(
                "s",
                Severity::Error,
                "c",
                "m",
                None,
                None,
                None,
                Some("sha256-deadbeef"),
            )],
            ..empty_report()
        };
        let sarif = cockpit_report_to_sarif(&report);
        assert_eq!(
            sarif.runs[0].results[0].fingerprints.get("cockpitctl/v1"),
            Some(&"sha256-deadbeef".to_string())
        );
    }

    #[test]
    fn sarif_fingerprint_absent_when_none() {
        let report = CockpitReport {
            highlights: vec![make_highlight(
                "s",
                Severity::Warn,
                "c",
                "m",
                None,
                None,
                None,
                None,
            )],
            ..empty_report()
        };
        let sarif = cockpit_report_to_sarif(&report);
        assert!(sarif.runs[0].results[0].fingerprints.is_empty());
    }

    // ── JSON round-trip ─────────────────────────────────────────────────

    #[test]
    fn sarif_json_round_trips() {
        let report = minimal_report_with_highlights();
        let json = cockpit_report_to_sarif_json(&report).unwrap();
        assert!(json.contains("\"version\": \"2.1.0\""));
        // Verify it's valid JSON.
        let _: serde_json::Value = serde_json::from_str(&json).unwrap();
    }

    #[test]
    fn sarif_json_ends_with_newline() {
        let report = empty_report();
        let json = cockpit_report_to_sarif_json(&report).unwrap();
        assert!(json.ends_with('\n'));
    }

    #[test]
    fn sarif_json_contains_expected_keys() {
        let report = minimal_report_with_highlights();
        let json = cockpit_report_to_sarif_json(&report).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(v.get("$schema").is_some());
        assert!(v.get("version").is_some());
        assert!(v.get("runs").is_some());
        let run = &v["runs"][0];
        assert!(run.get("tool").is_some());
        assert!(run.get("results").is_some());
    }

    // ── Edge cases ──────────────────────────────────────────────────────

    #[test]
    fn sarif_special_characters_in_message() {
        let msg = r#"expected `&str`, found `"hello <world> & 'friends'"` at line 42"#;
        let report = CockpitReport {
            highlights: vec![make_highlight(
                "s",
                Severity::Error,
                "code",
                msg,
                None,
                None,
                None,
                None,
            )],
            ..empty_report()
        };
        let json = cockpit_report_to_sarif_json(&report).unwrap();
        // Must be parseable despite quotes, angle brackets, ampersand.
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(
            parsed["runs"][0]["results"][0]["message"]["text"]
                .as_str()
                .unwrap(),
            msg
        );
    }

    #[test]
    fn sarif_unicode_in_fields() {
        let report = CockpitReport {
            highlights: vec![make_highlight(
                "sensor-日本語",
                Severity::Warn,
                "règle",
                "变量名 should be camelCase 🐫",
                Some("src/données.rs"),
                Some(1),
                None,
                None,
            )],
            ..empty_report()
        };
        let json = cockpit_report_to_sarif_json(&report).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(
            parsed["runs"][0]["results"][0]["ruleId"].as_str().unwrap(),
            "règle"
        );
        assert!(
            parsed["runs"][0]["results"][0]["message"]["text"]
                .as_str()
                .unwrap()
                .contains('🐫')
        );
    }

    #[test]
    fn sarif_very_long_message() {
        let long_msg = "x".repeat(10_000);
        let report = CockpitReport {
            highlights: vec![make_highlight(
                "s",
                Severity::Info,
                "long",
                &long_msg,
                None,
                None,
                None,
                None,
            )],
            ..empty_report()
        };
        let sarif = cockpit_report_to_sarif(&report);
        assert_eq!(sarif.runs[0].results[0].message.text.len(), 10_000);
    }

    #[test]
    fn sarif_empty_code_and_message() {
        let report = CockpitReport {
            highlights: vec![make_highlight(
                "s",
                Severity::Info,
                "",
                "",
                None,
                None,
                None,
                None,
            )],
            ..empty_report()
        };
        let sarif = cockpit_report_to_sarif(&report);
        assert_eq!(sarif.runs[0].results[0].rule_id, "");
        assert_eq!(sarif.runs[0].results[0].message.text, "");
        // Rule is still registered with empty id.
        assert_eq!(sarif.runs[0].tool.driver.rules.len(), 1);
    }

    #[test]
    fn sarif_json_skips_empty_locations_and_fingerprints() {
        let report = CockpitReport {
            highlights: vec![make_highlight(
                "s",
                Severity::Info,
                "c",
                "m",
                None,
                None,
                None,
                None,
            )],
            ..empty_report()
        };
        let json = cockpit_report_to_sarif_json(&report).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        let result = &parsed["runs"][0]["results"][0];
        // skip_serializing_if = "Vec::is_empty" / "BTreeMap::is_empty"
        assert!(result.get("locations").is_none());
        assert!(result.get("fingerprints").is_none());
    }

    #[test]
    fn sarif_json_skips_empty_rules() {
        let report = empty_report();
        let json = cockpit_report_to_sarif_json(&report).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        // skip_serializing_if = "Vec::is_empty" on rules
        assert!(parsed["runs"][0]["tool"]["driver"].get("rules").is_none());
    }

    #[test]
    fn sarif_single_run_always() {
        let report = minimal_report_with_highlights();
        let sarif = cockpit_report_to_sarif(&report);
        assert_eq!(sarif.runs.len(), 1);
    }
}
