//! Buildfix plan matching and deterministic auto-apply selection.

use cockpitctl_types::{
    BuildfixPlan, BuildfixSummary, FixSummary, Highlight, MatchedFinding, SafetyLevel,
    safety_level_rank,
};

/// Rank a safety level for sorting: safe=0, guarded=1, unsafe=2.
fn safety_rank(s: &SafetyLevel) -> u8 {
    safety_level_rank(s)
}

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
