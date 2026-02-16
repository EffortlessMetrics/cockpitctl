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

    fn minimal_report_with_highlights() -> CockpitReport {
        CockpitReport {
            schema: "cockpit.report.v1".to_string(),
            tool: ToolInfo {
                name: "cockpitctl".to_string(),
                version: "0.2.1".to_string(),
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

    #[test]
    fn sarif_has_correct_schema_and_version() {
        let report = minimal_report_with_highlights();
        let sarif = cockpit_report_to_sarif(&report);
        assert_eq!(sarif.version, "2.1.0");
        assert!(sarif.schema.contains("sarif-schema-2.1.0"));
    }

    #[test]
    fn sarif_maps_results_from_highlights() {
        let report = minimal_report_with_highlights();
        let sarif = cockpit_report_to_sarif(&report);
        assert_eq!(sarif.runs.len(), 1);
        assert_eq!(sarif.runs[0].results.len(), 2);

        let r0 = &sarif.runs[0].results[0];
        assert_eq!(r0.rule_id, "clippy::unwrap_used");
        assert_eq!(r0.level, "error");
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
            r0.fingerprints.get("cockpitctl/v1"),
            Some(&"fp_abc123".to_string())
        );
    }

    #[test]
    fn sarif_dedupes_rules_by_code() {
        let report = minimal_report_with_highlights();
        let sarif = cockpit_report_to_sarif(&report);
        // Two highlights with different codes → two rules (in driver only).
        assert_eq!(sarif.runs[0].tool.driver.rules.len(), 2);
    }

    #[test]
    fn sarif_severity_mapping() {
        assert_eq!(severity_to_sarif_level(&Severity::Error), "error");
        assert_eq!(severity_to_sarif_level(&Severity::Warn), "warning");
        assert_eq!(severity_to_sarif_level(&Severity::Info), "note");
    }

    #[test]
    fn sarif_json_round_trips() {
        let report = minimal_report_with_highlights();
        let json = cockpit_report_to_sarif_json(&report).unwrap();
        assert!(json.contains("\"version\": \"2.1.0\""));
        // Verify it's valid JSON.
        let _: serde_json::Value = serde_json::from_str(&json).unwrap();
    }

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
}
