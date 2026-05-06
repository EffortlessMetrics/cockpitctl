//! Buildfix domain boundary microcrate.
//!
//! Matches fixes from buildfix plans to surfaced findings and selects
//! fixes eligible for automatic application based on safety-level gating.

#![warn(missing_docs)]

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
///
/// # Examples
///
/// ```
/// use cockpitctl_domain_buildfix::match_buildfix_plan;
/// use cockpitctl_types::{
///     BuildfixPlan, Finding, FindingRef, Fix, Highlight, Location,
///     SafetyLevel, Severity, ToolInfo,
/// };
///
/// let plan = BuildfixPlan {
///     schema: "buildfix.plan.v1".into(),
///     tool: ToolInfo { name: "tool".into(), version: "1.0.0".into(), commit: None },
///     fixes: vec![Fix {
///         id: "fix-1".into(),
///         safety: SafetyLevel::Safe,
///         description: "Fix it".into(),
///         finding_refs: vec![FindingRef {
///             sensor_id: "clippy".into(),
///             fingerprint: None,
///             code: Some("W001".into()),
///             tool: None,
///             check_id: None,
///         }],
///         preconditions: None,
///         data: None,
///     }],
/// };
///
/// let highlights = vec![Highlight {
///     sensor_id: "clippy".into(),
///     finding: Finding {
///         severity: Severity::Warn,
///         check_id: None,
///         code: "W001".into(),
///         message: "warning".into(),
///         location: None,
///         help: None,
///         url: None,
///         fingerprint: None,
///         data: None,
///     },
/// }];
///
/// let summary = match_buildfix_plan("clippy", &plan, &highlights);
/// assert_eq!(summary.matched_count, 1);
/// ```
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
    let matched_count = total_fixes.saturating_sub(unmatched_count);

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
///
/// # Examples
///
/// ```
/// use cockpitctl_domain_buildfix::select_auto_apply_fixes;
/// use cockpitctl_types::{BuildfixSummary, FixSummary, SafetyLevel};
///
/// let summary = BuildfixSummary {
///     fixes: vec![FixSummary {
///         fix_id: "f1".into(),
///         sensor_id: "s".into(),
///         safety: SafetyLevel::Safe,
///         description: "desc".into(),
///         matched_findings: vec![],
///         unmatched: false,
///     }],
///     total_fixes: 1,
///     matched_count: 1,
///     unmatched_count: 0,
/// };
///
/// let selected = select_auto_apply_fixes(&summary, SafetyLevel::Safe, false);
/// assert_eq!(selected.len(), 1);
/// ```
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

    // ── helpers ───────────────────────────────────────────────────────

    fn tool_info() -> ToolInfo {
        ToolInfo {
            name: "buildfix".to_string(),
            version: "1.0.0".to_string(),
            commit: None,
        }
    }

    fn buildfix_plan() -> BuildfixPlan {
        BuildfixPlan {
            schema: "buildfix.plan.v1".to_string(),
            tool: tool_info(),
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

    fn make_plan(fixes: Vec<Fix>) -> BuildfixPlan {
        BuildfixPlan {
            schema: "buildfix.plan.v1".to_string(),
            tool: tool_info(),
            fixes,
        }
    }

    fn make_fix(id: &str, safety: SafetyLevel, refs: Vec<FindingRef>) -> Fix {
        Fix {
            id: id.to_string(),
            safety,
            description: format!("description for {id}"),
            finding_refs: refs,
            preconditions: None,
            data: None,
        }
    }

    fn make_finding_ref(
        sensor_id: &str,
        fingerprint: Option<&str>,
        code: Option<&str>,
    ) -> FindingRef {
        FindingRef {
            sensor_id: sensor_id.to_string(),
            fingerprint: fingerprint.map(std::string::ToString::to_string),
            code: code.map(std::string::ToString::to_string),
            tool: None,
            check_id: None,
        }
    }

    fn highlight(code: &str, fp: Option<&str>) -> Highlight {
        make_highlight("sensor", code, fp)
    }

    fn make_highlight(sensor_id: &str, code: &str, fp: Option<&str>) -> Highlight {
        Highlight {
            sensor_id: sensor_id.to_string(),
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

    fn make_fix_summary(
        fix_id: &str,
        sensor_id: &str,
        safety: SafetyLevel,
        unmatched: bool,
    ) -> FixSummary {
        FixSummary {
            fix_id: fix_id.to_string(),
            sensor_id: sensor_id.to_string(),
            safety,
            description: format!("desc-{fix_id}"),
            matched_findings: vec![],
            unmatched,
        }
    }

    // ── match_buildfix_plan ──────────────────────────────────────────

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
    fn empty_plan_produces_empty_summary() {
        let plan = make_plan(vec![]);
        let highlights = vec![highlight("CODE-1", Some("fp-1"))];
        let summary = match_buildfix_plan("sensor", &plan, &highlights);
        assert_eq!(summary.total_fixes, 0);
        assert_eq!(summary.matched_count, 0);
        assert_eq!(summary.unmatched_count, 0);
        assert!(summary.fixes.is_empty());
    }

    #[test]
    fn empty_highlights_marks_all_fixes_unmatched() {
        let plan = buildfix_plan();
        let summary = match_buildfix_plan("sensor", &plan, &[]);
        assert_eq!(summary.total_fixes, 1);
        assert_eq!(summary.matched_count, 0);
        assert_eq!(summary.unmatched_count, 1);
        assert!(summary.fixes[0].unmatched);
    }

    #[test]
    fn wrong_sensor_id_marks_fix_unmatched() {
        let plan = buildfix_plan();
        let highlights = vec![highlight("CODE-1", Some("fp-1"))];
        let summary = match_buildfix_plan("wrong-sensor", &plan, &highlights);
        assert_eq!(summary.matched_count, 0);
        assert_eq!(summary.unmatched_count, 1);
        assert!(summary.fixes[0].unmatched);
    }

    #[test]
    fn finding_ref_fingerprint_set_but_highlight_has_none_no_match() {
        let plan = buildfix_plan(); // ref has fingerprint="fp-1"
        let highlights = vec![highlight("CODE-1", None)]; // highlight has no fingerprint
        let summary = match_buildfix_plan("sensor", &plan, &highlights);
        assert_eq!(summary.matched_count, 0);
        assert!(summary.fixes[0].unmatched);
    }

    #[test]
    fn finding_ref_no_fingerprint_matches_any_highlight_fingerprint() {
        let plan = make_plan(vec![make_fix(
            "fix-no-fp",
            SafetyLevel::Safe,
            vec![make_finding_ref("sensor", None, Some("CODE-1"))],
        )]);
        // Should match: ref has no fingerprint constraint, code matches
        let highlights = vec![highlight("CODE-1", Some("any-fp"))];
        let summary = match_buildfix_plan("sensor", &plan, &highlights);
        assert_eq!(summary.matched_count, 1);
        assert!(!summary.fixes[0].unmatched);
    }

    #[test]
    fn finding_ref_no_fingerprint_matches_highlight_without_fingerprint() {
        let plan = make_plan(vec![make_fix(
            "fix-no-fp",
            SafetyLevel::Safe,
            vec![make_finding_ref("sensor", None, Some("CODE-1"))],
        )]);
        let highlights = vec![highlight("CODE-1", None)];
        let summary = match_buildfix_plan("sensor", &plan, &highlights);
        assert_eq!(summary.matched_count, 1);
    }

    #[test]
    fn finding_ref_no_code_matches_any_highlight_code() {
        let plan = make_plan(vec![make_fix(
            "fix-no-code",
            SafetyLevel::Safe,
            vec![make_finding_ref("sensor", None, None)],
        )]);
        let highlights = vec![highlight("ANY-CODE", Some("any-fp"))];
        let summary = match_buildfix_plan("sensor", &plan, &highlights);
        assert_eq!(summary.matched_count, 1);
        assert!(!summary.fixes[0].unmatched);
    }

    #[test]
    fn fingerprint_mismatch_no_match() {
        let plan = buildfix_plan(); // ref: fp-1, CODE-1
        let highlights = vec![highlight("CODE-1", Some("fp-WRONG"))];
        let summary = match_buildfix_plan("sensor", &plan, &highlights);
        assert_eq!(summary.matched_count, 0);
        assert!(summary.fixes[0].unmatched);
    }

    #[test]
    fn code_mismatch_no_match() {
        let plan = buildfix_plan(); // ref: fp-1, CODE-1
        let highlights = vec![highlight("WRONG-CODE", Some("fp-1"))];
        let summary = match_buildfix_plan("sensor", &plan, &highlights);
        assert_eq!(summary.matched_count, 0);
        assert!(summary.fixes[0].unmatched);
    }

    #[test]
    fn multiple_finding_refs_per_fix_any_can_match() {
        let plan = make_plan(vec![make_fix(
            "multi-ref",
            SafetyLevel::Safe,
            vec![
                make_finding_ref("sensor", Some("fp-a"), Some("CODE-A")),
                make_finding_ref("sensor", Some("fp-b"), Some("CODE-B")),
            ],
        )]);
        // Only the second ref matches
        let highlights = vec![highlight("CODE-B", Some("fp-b"))];
        let summary = match_buildfix_plan("sensor", &plan, &highlights);
        assert_eq!(summary.matched_count, 1);
        assert!(!summary.fixes[0].unmatched);
        assert_eq!(summary.fixes[0].matched_findings.len(), 1);
        assert_eq!(summary.fixes[0].matched_findings[0].code, "CODE-B");
    }

    #[test]
    fn multiple_highlights_can_match_same_fix() {
        let plan = make_plan(vec![make_fix(
            "multi-match",
            SafetyLevel::Safe,
            vec![make_finding_ref("sensor", None, None)], // matches everything
        )]);
        let highlights = vec![
            highlight("CODE-1", Some("fp-1")),
            highlight("CODE-2", Some("fp-2")),
        ];
        let summary = match_buildfix_plan("sensor", &plan, &highlights);
        assert_eq!(summary.matched_count, 1);
        assert_eq!(summary.fixes[0].matched_findings.len(), 2);
    }

    #[test]
    fn matched_findings_carry_sensor_code_fingerprint() {
        let plan = buildfix_plan();
        let highlights = vec![highlight("CODE-1", Some("fp-1"))];
        let summary = match_buildfix_plan("sensor", &plan, &highlights);
        let mf = &summary.fixes[0].matched_findings[0];
        assert_eq!(mf.sensor_id, "sensor");
        assert_eq!(mf.code, "CODE-1");
        assert_eq!(mf.fingerprint, Some("fp-1".to_string()));
    }

    #[test]
    fn highlight_from_different_sensor_ignored_in_matching() {
        let plan = make_plan(vec![make_fix(
            "fix-1",
            SafetyLevel::Safe,
            vec![make_finding_ref("sensor", None, Some("CODE-1"))],
        )]);
        let highlights = vec![make_highlight("other-sensor", "CODE-1", None)];
        let summary = match_buildfix_plan("sensor", &plan, &highlights);
        assert_eq!(summary.matched_count, 0);
        assert!(summary.fixes[0].unmatched);
    }

    #[test]
    fn finding_ref_for_different_sensor_is_skipped() {
        let plan = make_plan(vec![make_fix(
            "fix-cross-sensor",
            SafetyLevel::Safe,
            vec![make_finding_ref("other-sensor", None, Some("CODE-1"))],
        )]);
        // Even though highlight matches, the ref sensor differs from the plan sensor
        let highlights = vec![highlight("CODE-1", None)];
        let summary = match_buildfix_plan("sensor", &plan, &highlights);
        assert_eq!(summary.matched_count, 0);
        assert!(summary.fixes[0].unmatched);
    }

    // ── sort order ───────────────────────────────────────────────────

    #[test]
    fn fixes_sorted_by_safety_then_sensor_then_fix_id() {
        let plan = make_plan(vec![
            make_fix(
                "z-unsafe",
                SafetyLevel::Unsafe,
                vec![make_finding_ref("sensor", None, None)],
            ),
            make_fix(
                "a-safe",
                SafetyLevel::Safe,
                vec![make_finding_ref("sensor", None, None)],
            ),
            make_fix(
                "m-guarded",
                SafetyLevel::Guarded,
                vec![make_finding_ref("sensor", None, None)],
            ),
            make_fix(
                "b-safe",
                SafetyLevel::Safe,
                vec![make_finding_ref("sensor", None, None)],
            ),
        ]);
        let highlights = vec![highlight("CODE-1", None)];
        let summary = match_buildfix_plan("sensor", &plan, &highlights);
        let ids: Vec<&str> = summary.fixes.iter().map(|f| f.fix_id.as_str()).collect();
        // safe (rank 0) first, then guarded (rank 1), then unsafe (rank 2)
        // within same safety: sensor_id is identical, so sort by fix_id
        assert_eq!(ids, vec!["a-safe", "b-safe", "m-guarded", "z-unsafe"]);
    }

    #[test]
    fn fixes_sorted_by_sensor_id_within_same_safety() {
        // All fixes safe, but different sensor_ids in their summaries
        // (sensor_id in FixSummary comes from the caller, not the ref)
        // Since match_buildfix_plan sets sensor_id to the passed-in value,
        // all will have the same sensor_id; tie-break is fix_id.
        let plan = make_plan(vec![
            make_fix(
                "fix-z",
                SafetyLevel::Safe,
                vec![make_finding_ref("sensor", None, None)],
            ),
            make_fix(
                "fix-a",
                SafetyLevel::Safe,
                vec![make_finding_ref("sensor", None, None)],
            ),
        ]);
        let summary = match_buildfix_plan("sensor", &plan, &[]);
        assert_eq!(summary.fixes[0].fix_id, "fix-a");
        assert_eq!(summary.fixes[1].fix_id, "fix-z");
    }

    // ── safety_rank ──────────────────────────────────────────────────

    #[test]
    fn safety_rank_ordering() {
        assert!(safety_rank(&SafetyLevel::Safe) < safety_rank(&SafetyLevel::Guarded));
        assert!(safety_rank(&SafetyLevel::Guarded) < safety_rank(&SafetyLevel::Unsafe));
    }

    // ── select_auto_apply_fixes ──────────────────────────────────────

    #[test]
    fn select_auto_apply_fixes_respects_safety_and_matching() {
        let summary = BuildfixSummary {
            fixes: vec![
                make_fix_summary("guarded", "sensor", SafetyLevel::Guarded, false),
                make_fix_summary("unsafe", "sensor", SafetyLevel::Unsafe, false),
                make_fix_summary("safe-unmatched", "sensor", SafetyLevel::Safe, true),
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

    #[test]
    fn select_empty_summary_returns_empty() {
        let summary = BuildfixSummary {
            fixes: vec![],
            total_fixes: 0,
            matched_count: 0,
            unmatched_count: 0,
        };
        let selected = select_auto_apply_fixes(&summary, SafetyLevel::Unsafe, false);
        assert!(selected.is_empty());
    }

    #[test]
    fn safety_gate_safe_blocks_guarded_and_unsafe() {
        let summary = BuildfixSummary {
            fixes: vec![
                make_fix_summary("safe-fix", "sensor", SafetyLevel::Safe, false),
                make_fix_summary("guarded-fix", "sensor", SafetyLevel::Guarded, false),
                make_fix_summary("unsafe-fix", "sensor", SafetyLevel::Unsafe, false),
            ],
            total_fixes: 3,
            matched_count: 3,
            unmatched_count: 0,
        };
        let selected = select_auto_apply_fixes(&summary, SafetyLevel::Safe, false);
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].fix_id, "safe-fix");
    }

    #[test]
    fn safety_gate_guarded_allows_safe_and_guarded() {
        let summary = BuildfixSummary {
            fixes: vec![
                make_fix_summary("safe-fix", "sensor", SafetyLevel::Safe, false),
                make_fix_summary("guarded-fix", "sensor", SafetyLevel::Guarded, false),
                make_fix_summary("unsafe-fix", "sensor", SafetyLevel::Unsafe, false),
            ],
            total_fixes: 3,
            matched_count: 3,
            unmatched_count: 0,
        };
        let selected = select_auto_apply_fixes(&summary, SafetyLevel::Guarded, false);
        assert_eq!(selected.len(), 2);
        assert_eq!(selected[0].fix_id, "safe-fix");
        assert_eq!(selected[1].fix_id, "guarded-fix");
    }

    #[test]
    fn safety_gate_unsafe_allows_all() {
        let summary = BuildfixSummary {
            fixes: vec![
                make_fix_summary("safe-fix", "sensor", SafetyLevel::Safe, false),
                make_fix_summary("guarded-fix", "sensor", SafetyLevel::Guarded, false),
                make_fix_summary("unsafe-fix", "sensor", SafetyLevel::Unsafe, false),
            ],
            total_fixes: 3,
            matched_count: 3,
            unmatched_count: 0,
        };
        let selected = select_auto_apply_fixes(&summary, SafetyLevel::Unsafe, false);
        assert_eq!(selected.len(), 3);
    }

    #[test]
    fn require_matched_finding_filters_unmatched() {
        let summary = BuildfixSummary {
            fixes: vec![
                make_fix_summary("matched", "sensor", SafetyLevel::Safe, false),
                make_fix_summary("unmatched", "sensor", SafetyLevel::Safe, true),
            ],
            total_fixes: 2,
            matched_count: 1,
            unmatched_count: 1,
        };
        let selected = select_auto_apply_fixes(&summary, SafetyLevel::Unsafe, true);
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].fix_id, "matched");
    }

    #[test]
    fn require_matched_false_allows_unmatched() {
        let summary = BuildfixSummary {
            fixes: vec![
                make_fix_summary("matched", "sensor", SafetyLevel::Safe, false),
                make_fix_summary("unmatched", "sensor", SafetyLevel::Safe, true),
            ],
            total_fixes: 2,
            matched_count: 1,
            unmatched_count: 1,
        };
        let selected = select_auto_apply_fixes(&summary, SafetyLevel::Unsafe, false);
        assert_eq!(selected.len(), 2);
    }

    #[test]
    fn select_preserves_sorted_order() {
        let summary = BuildfixSummary {
            fixes: vec![
                make_fix_summary("a-safe", "sensor", SafetyLevel::Safe, false),
                make_fix_summary("b-safe", "sensor", SafetyLevel::Safe, false),
                make_fix_summary("c-guarded", "sensor", SafetyLevel::Guarded, false),
            ],
            total_fixes: 3,
            matched_count: 3,
            unmatched_count: 0,
        };
        let selected = select_auto_apply_fixes(&summary, SafetyLevel::Guarded, false);
        let ids: Vec<&str> = selected.iter().map(|f| f.fix_id.as_str()).collect();
        assert_eq!(ids, vec!["a-safe", "b-safe", "c-guarded"]);
    }

    #[test]
    fn no_fixes_pass_safety_gate_returns_empty() {
        let summary = BuildfixSummary {
            fixes: vec![
                make_fix_summary("unsafe-1", "sensor", SafetyLevel::Unsafe, false),
                make_fix_summary("unsafe-2", "sensor", SafetyLevel::Unsafe, false),
            ],
            total_fixes: 2,
            matched_count: 2,
            unmatched_count: 0,
        };
        let selected = select_auto_apply_fixes(&summary, SafetyLevel::Safe, false);
        assert!(selected.is_empty());
    }

    #[test]
    fn all_unmatched_with_require_match_returns_empty() {
        let summary = BuildfixSummary {
            fixes: vec![
                make_fix_summary("a", "sensor", SafetyLevel::Safe, true),
                make_fix_summary("b", "sensor", SafetyLevel::Guarded, true),
            ],
            total_fixes: 2,
            matched_count: 0,
            unmatched_count: 2,
        };
        let selected = select_auto_apply_fixes(&summary, SafetyLevel::Unsafe, true);
        assert!(selected.is_empty());
    }

    // ── integration-style: plan → select pipeline ────────────────────

    #[test]
    fn end_to_end_plan_then_select() {
        let plan = make_plan(vec![
            make_fix(
                "safe-fix",
                SafetyLevel::Safe,
                vec![make_finding_ref("sensor", Some("fp-1"), Some("CODE-1"))],
            ),
            make_fix(
                "unsafe-fix",
                SafetyLevel::Unsafe,
                vec![make_finding_ref("sensor", Some("fp-2"), Some("CODE-2"))],
            ),
        ]);
        let highlights = vec![
            highlight("CODE-1", Some("fp-1")),
            highlight("CODE-2", Some("fp-2")),
        ];
        let summary = match_buildfix_plan("sensor", &plan, &highlights);
        assert_eq!(summary.total_fixes, 2);
        assert_eq!(summary.matched_count, 2);

        // Safe gate: only the safe fix is auto-applicable
        let auto = select_auto_apply_fixes(&summary, SafetyLevel::Safe, true);
        assert_eq!(auto.len(), 1);
        assert_eq!(auto[0].fix_id, "safe-fix");
    }

    #[test]
    fn conflicting_fixes_for_same_finding_both_present_in_summary() {
        let plan = make_plan(vec![
            make_fix(
                "fix-a",
                SafetyLevel::Safe,
                vec![make_finding_ref("sensor", Some("fp-1"), Some("CODE-1"))],
            ),
            make_fix(
                "fix-b",
                SafetyLevel::Guarded,
                vec![make_finding_ref("sensor", Some("fp-1"), Some("CODE-1"))],
            ),
        ]);
        let highlights = vec![highlight("CODE-1", Some("fp-1"))];
        let summary = match_buildfix_plan("sensor", &plan, &highlights);
        // Both fixes match the same finding; both should appear
        assert_eq!(summary.total_fixes, 2);
        assert_eq!(summary.matched_count, 2);
        assert_eq!(summary.fixes[0].fix_id, "fix-a"); // safe first
        assert_eq!(summary.fixes[1].fix_id, "fix-b"); // guarded second
    }
}
