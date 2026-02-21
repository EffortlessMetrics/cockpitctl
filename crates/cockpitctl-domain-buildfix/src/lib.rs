//! Buildfix domain boundary microcrate.

use cockpitctl_types::{
    BuildfixPlan, BuildfixSummary, FixSummary, Highlight, MatchedFinding, SafetyLevel,
    safety_level_rank,
};

/// Match fixes from a buildfix plan to findings in the report.
///
/// A fix matches a finding when:
/// - `FindingRef.sensor_id` matches the sensor
/// - If `fingerprint` is set, it must match
/// - If `code` is set, it must match
pub fn match_buildfix_plan(
    sensor_id: &str,
    plan: &BuildfixPlan,
    highlights: &[Highlight],
) -> BuildfixSummary {
    let mut fixes = Vec::new();

    for fix in &plan.fixes {
        let mut matched_findings = Vec::new();
        let mut any_matched = false;

        for fref in &fix.finding_refs {
            if fref.sensor_id != sensor_id {
                continue;
            }
            for h in highlights {
                if h.sensor_id != sensor_id {
                    continue;
                }
                let fp_match = match (&fref.fingerprint, &h.finding.fingerprint) {
                    (Some(ref_fp), Some(finding_fp)) => ref_fp == finding_fp,
                    (Some(_), None) => false,
                    (None, _) => true,
                };
                let code_match = match &fref.code {
                    Some(ref_code) => ref_code == &h.finding.code,
                    None => true,
                };

                if fp_match && code_match {
                    matched_findings.push(MatchedFinding {
                        sensor_id: h.sensor_id.clone(),
                        code: h.finding.code.clone(),
                        fingerprint: h.finding.fingerprint.clone(),
                    });
                    any_matched = true;
                }
            }
        }

        fixes.push(FixSummary {
            fix_id: fix.id.clone(),
            sensor_id: sensor_id.to_string(),
            safety: fix.safety,
            description: fix.description.clone(),
            matched_findings,
            unmatched: !any_matched,
        });
    }

    // Sort: safety_rank → sensor_id → fix_id.
    fixes.sort_by(|a, b| {
        let a_key = (safety_rank(&a.safety), &a.sensor_id, &a.fix_id);
        let b_key = (safety_rank(&b.safety), &b.sensor_id, &b.fix_id);
        a_key.cmp(&b_key)
    });

    let total_fixes = fixes.len();
    let unmatched_count = fixes.iter().filter(|f| f.unmatched).count();
    let matched_count = total_fixes - unmatched_count;

    BuildfixSummary {
        fixes,
        total_fixes,
        matched_count,
        unmatched_count,
    }
}

fn safety_rank(s: &SafetyLevel) -> u8 {
    safety_level_rank(s)
}

/// Select fixes eligible for auto-apply under the configured safety gate.
///
/// Selection is deterministic and preserves the existing sorted order from
/// `BuildfixSummary` (`safe` -> `guarded` -> `unsafe`, then sensor/fix id).
pub fn select_auto_apply_fixes(
    summary: &BuildfixSummary,
    max_auto_apply_safety: SafetyLevel,
    require_matched_finding: bool,
) -> Vec<FixSummary> {
    let max_rank = safety_rank(&max_auto_apply_safety);
    summary
        .fixes
        .iter()
        .filter(|fix| {
            safety_rank(&fix.safety) <= max_rank && (!require_matched_finding || !fix.unmatched)
        })
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use cockpitctl_types::{
        BuildfixPlan, Finding, FindingRef, Fix, Highlight, Location, SafetyLevel, Severity,
        ToolInfo,
    };

    fn buildfix_plan() -> BuildfixPlan {
        BuildfixPlan {
            schema: "buildfix.plan.v1".to_string(),
            tool: ToolInfo {
                name: "buildfix".to_string(),
                version: "1.0.0".to_string(),
                commit: None,
            },
            fixes: vec![Fix {
                id: "fix-1".to_string(),
                safety: SafetyLevel::Safe,
                description: "Fix matching finding".to_string(),
                finding_refs: vec![FindingRef {
                    sensor_id: "sensor".to_string(),
                    fingerprint: Some("fp-1".to_string()),
                    code: Some("CODE-1".to_string()),
                    tool: None,
                    check_id: None,
                }],
                preconditions: None,
                data: None,
            }],
        }
    }

    fn highlight(code: &str, fp: Option<&str>) -> Highlight {
        Highlight {
            sensor_id: "sensor".to_string(),
            finding: Finding {
                severity: Severity::Error,
                check_id: Some("check".to_string()),
                code: code.to_string(),
                message: "message".to_string(),
                location: Some(Location {
                    path: Some("file.rs".to_string()),
                    line: Some(10),
                    col: None,
                }),
                help: None,
                url: None,
                fingerprint: fp.map(|v| v.to_string()),
                data: None,
            },
        }
    }

    #[test]
    fn matches_by_fingerprint_and_code() {
        let plan = buildfix_plan();
        let highlights = vec![highlight("CODE-1", Some("fp-1"))];
        let summary = match_buildfix_plan("sensor", &plan, &highlights);
        assert_eq!(summary.total_fixes, 1);
        assert_eq!(summary.matched_count, 1);
        assert_eq!(summary.unmatched_count, 0);
        assert!(!summary.fixes[0].unmatched);
    }

    #[test]
    fn marks_unmatched_when_no_highlights_match() {
        let plan = buildfix_plan();
        let highlights = vec![highlight("OTHER", Some("other"))];
        let summary = match_buildfix_plan("sensor", &plan, &highlights);
        assert_eq!(summary.total_fixes, 1);
        assert_eq!(summary.matched_count, 0);
        assert_eq!(summary.unmatched_count, 1);
        assert!(summary.fixes[0].unmatched);
    }

    #[test]
    fn select_auto_apply_fixes_respects_safety_and_matching() {
        let summary = BuildfixSummary {
            fixes: vec![
                FixSummary {
                    fix_id: "guarded".to_string(),
                    sensor_id: "sensor".to_string(),
                    safety: SafetyLevel::Guarded,
                    description: "guarded".to_string(),
                    matched_findings: vec![],
                    unmatched: false,
                },
                FixSummary {
                    fix_id: "unsafe".to_string(),
                    sensor_id: "sensor".to_string(),
                    safety: SafetyLevel::Unsafe,
                    description: "unsafe".to_string(),
                    matched_findings: vec![],
                    unmatched: false,
                },
                FixSummary {
                    fix_id: "safe-unmatched".to_string(),
                    sensor_id: "sensor".to_string(),
                    safety: SafetyLevel::Safe,
                    description: "safe-unmatched".to_string(),
                    matched_findings: vec![],
                    unmatched: true,
                },
            ],
            total_fixes: 3,
            matched_count: 2,
            unmatched_count: 1,
        };

        let selected_safe_only = select_auto_apply_fixes(&summary, SafetyLevel::Safe, false);
        assert_eq!(selected_safe_only.len(), 1);
        assert_eq!(selected_safe_only[0].fix_id, "safe-unmatched");

        let selected_with_match_gate = select_auto_apply_fixes(&summary, SafetyLevel::Unsafe, true);
        assert_eq!(selected_with_match_gate.len(), 2);
        assert_eq!(selected_with_match_gate[0].fix_id, "guarded");
        assert_eq!(selected_with_match_gate[1].fix_id, "unsafe");
    }
}
