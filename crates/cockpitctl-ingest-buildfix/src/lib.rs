//! Buildfix-specific ingest helpers extracted from `cockpitctl-ingest`.

#![warn(missing_docs)]

use anyhow::Result;
use cockpitctl_domain_buildfix::match_buildfix_plan;
use cockpitctl_types::{BuildfixSummary, CockpitReport, Highlight};

/// Aggregate buildfix summaries from discovered sensor IDs and optional plan bytes.
///
/// Invalid plan JSON is ignored because plan ingestion is additive and must not
/// break cockpit generation.
pub fn aggregate_buildfix_summary<F>(
    discovered: &[String],
    highlights: &[Highlight],
    mut read_plan_bytes: F,
) -> Result<Option<BuildfixSummary>>
where
    F: FnMut(&str) -> Result<Option<Vec<u8>>>,
{
    let mut all_fixes = Vec::new();

    for sensor_id in discovered {
        if let Some(bytes) = read_plan_bytes(sensor_id)?
            && let Ok(plan) = serde_json::from_slice::<cockpitctl_types::BuildfixPlan>(&bytes)
        {
            let summary = match_buildfix_plan(sensor_id, &plan, highlights);
            all_fixes.extend(summary.fixes);
        }
    }

    if all_fixes.is_empty() {
        return Ok(None);
    }

    let total_fixes = all_fixes.len();
    let unmatched_count = all_fixes.iter().filter(|f| f.unmatched).count();
    let matched_count = total_fixes - unmatched_count;

    Ok(Some(BuildfixSummary {
        fixes: all_fixes,
        total_fixes,
        matched_count,
        unmatched_count,
    }))
}

/// Insert buildfix summary in `report.data["_buildfix"]` when present.
pub fn attach_buildfix_data(report: &mut CockpitReport, buildfix: Option<&BuildfixSummary>) {
    let Some(buildfix) = buildfix else {
        return;
    };

    if let Ok(value) = serde_json::to_value(buildfix) {
        let data = report
            .data
            .get_or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
        if let Some(obj) = data.as_object_mut() {
            obj.insert("_buildfix".to_string(), value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cockpitctl_types::{
        CockpitReport, Finding, FixSummary, MatchedFinding, PolicySnapshot, RunInfo, Severity,
        ToolInfo, Verdict, VerdictCounts, VerdictStatus,
    };
    use std::collections::BTreeMap;

    fn valid_plan_bytes(fix_id: &str) -> Vec<u8> {
        format!(
            r#"{{"schema":"buildfix.plan.v1","tool":{{"name":"test","version":"1.0.0"}},"fixes":[{{"id":"{}","safety":"safe","description":"Fix issue","finding_refs":[{{"sensor_id":"sensor-a","code":"test.code"}}]}}]}}"#,
            fix_id
        )
        .into_bytes()
    }

    fn sample_highlight() -> cockpitctl_types::Highlight {
        cockpitctl_types::Highlight {
            sensor_id: "sensor-a".to_string(),
            finding: Finding {
                severity: Severity::Error,
                check_id: None,
                code: "test.code".to_string(),
                message: "msg".to_string(),
                location: None,
                help: None,
                url: None,
                fingerprint: None,
                data: None,
            },
        }
    }

    #[test]
    fn aggregate_buildfix_summary_skips_invalid_plans() {
        let discovered = vec!["sensor-a".to_string(), "sensor-b".to_string()];

        let summary = aggregate_buildfix_summary(&discovered, &[sample_highlight()], |id| {
            Ok(match id {
                "sensor-a" => Some(valid_plan_bytes("fix-1")),
                "sensor-b" => Some(b"{not-json".to_vec()),
                _ => None,
            })
        })
        .expect("aggregate")
        .expect("summary");

        assert_eq!(summary.total_fixes, 1);
        assert_eq!(summary.matched_count, 1);
        assert_eq!(summary.unmatched_count, 0);
        assert_eq!(summary.fixes[0].fix_id, "fix-1");
    }

    #[test]
    fn attach_buildfix_data_writes_data_key() {
        let mut report = CockpitReport {
            schema: "cockpit.report.v1".to_string(),
            tool: ToolInfo {
                name: "cockpitctl".to_string(),
                version: "0.3.0".to_string(),
                commit: None,
            },
            run: RunInfo {
                started_at: "2026-01-01T00:00:00Z".to_string(),
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
                max_annotations: 50,
                section_order: vec![],
                sensors: vec![],
            },
            data: None,
        };

        let summary = BuildfixSummary {
            fixes: vec![FixSummary {
                fix_id: "fix-1".to_string(),
                sensor_id: "sensor-a".to_string(),
                safety: cockpitctl_types::SafetyLevel::Safe,
                description: "Fix issue".to_string(),
                matched_findings: vec![MatchedFinding {
                    sensor_id: "sensor-a".to_string(),
                    code: "test.code".to_string(),
                    fingerprint: None,
                }],
                unmatched: false,
            }],
            total_fixes: 1,
            matched_count: 1,
            unmatched_count: 0,
        };

        attach_buildfix_data(&mut report, Some(&summary));

        let data = report.data.expect("data");
        assert!(data.get("_buildfix").is_some());
    }
}
