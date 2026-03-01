//! Integration tests for cockpitctl-domain-buildfix.
//!
//! Exercises the public buildfix matching and auto-apply selection API
//! through realistic plans, verifying ordering, safety gating, and edge cases.

use cockpitctl_domain_buildfix::{match_buildfix_plan, select_auto_apply_fixes};
use cockpitctl_types::{
    BuildfixPlan, BuildfixSummary, Finding, FindingRef, Fix, FixSummary, Highlight, Location,
    SafetyLevel, Severity, ToolInfo,
};

// ── helpers ──────────────────────────────────────────────────────────

fn tool_info() -> ToolInfo {
    ToolInfo {
        name: "buildfix".into(),
        version: "1.0.0".into(),
        commit: None,
    }
}

fn make_plan(fixes: Vec<Fix>) -> BuildfixPlan {
    BuildfixPlan {
        schema: "buildfix.plan.v1".into(),
        tool: tool_info(),
        fixes,
    }
}

fn make_fix(id: &str, safety: SafetyLevel, refs: Vec<FindingRef>) -> Fix {
    Fix {
        id: id.into(),
        safety,
        description: format!("description for {id}"),
        finding_refs: refs,
        preconditions: None,
        data: None,
    }
}

fn make_ref(sensor: &str, fp: Option<&str>, code: Option<&str>) -> FindingRef {
    FindingRef {
        sensor_id: sensor.into(),
        fingerprint: fp.map(Into::into),
        code: code.map(Into::into),
        tool: None,
        check_id: None,
    }
}

fn make_highlight(sensor: &str, code: &str, fp: Option<&str>) -> Highlight {
    Highlight {
        sensor_id: sensor.into(),
        finding: Finding {
            severity: Severity::Error,
            check_id: None,
            code: code.into(),
            message: "msg".into(),
            location: Some(Location {
                path: Some("file.rs".into()),
                line: Some(10),
                col: None,
            }),
            help: None,
            url: None,
            fingerprint: fp.map(Into::into),
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
        fix_id: fix_id.into(),
        sensor_id: sensor_id.into(),
        safety,
        description: format!("desc-{fix_id}"),
        matched_findings: vec![],
        unmatched,
    }
}

// ── plan parsing from structured data ────────────────────────────────

#[test]
fn plan_from_json_roundtrips() {
    let plan = make_plan(vec![make_fix(
        "f1",
        SafetyLevel::Safe,
        vec![make_ref("clippy", None, Some("W001"))],
    )]);
    let json = serde_json::to_string(&plan).unwrap();
    let parsed: BuildfixPlan = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, plan);
}

#[test]
fn plan_with_all_safety_levels_roundtrips() {
    let plan = make_plan(vec![
        make_fix("safe-fix", SafetyLevel::Safe, vec![]),
        make_fix("guarded-fix", SafetyLevel::Guarded, vec![]),
        make_fix("unsafe-fix", SafetyLevel::Unsafe, vec![]),
    ]);
    let json = serde_json::to_string(&plan).unwrap();
    let parsed: BuildfixPlan = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.fixes.len(), 3);
}

// ── plan with multiple fix suggestions ───────────────────────────────

#[test]
fn multiple_fixes_matched_to_highlights() {
    let plan = make_plan(vec![
        make_fix(
            "f1",
            SafetyLevel::Safe,
            vec![make_ref("sensor", Some("fp-1"), Some("C001"))],
        ),
        make_fix(
            "f2",
            SafetyLevel::Guarded,
            vec![make_ref("sensor", Some("fp-2"), Some("C002"))],
        ),
    ]);
    let highlights = vec![
        make_highlight("sensor", "C001", Some("fp-1")),
        make_highlight("sensor", "C002", Some("fp-2")),
    ];
    let summary = match_buildfix_plan("sensor", &plan, &highlights);
    assert_eq!(summary.total_fixes, 2);
    assert_eq!(summary.matched_count, 2);
    assert_eq!(summary.unmatched_count, 0);
}

// ── empty plan → valid empty output ──────────────────────────────────

#[test]
fn empty_plan_produces_empty_summary() {
    let plan = make_plan(vec![]);
    let summary = match_buildfix_plan("sensor", &plan, &[]);
    assert_eq!(summary.total_fixes, 0);
    assert_eq!(summary.matched_count, 0);
    assert_eq!(summary.unmatched_count, 0);
    assert!(summary.fixes.is_empty());
}

#[test]
fn empty_plan_with_highlights_still_empty() {
    let plan = make_plan(vec![]);
    let highlights = vec![make_highlight("sensor", "C001", Some("fp-1"))];
    let summary = match_buildfix_plan("sensor", &plan, &highlights);
    assert!(summary.fixes.is_empty());
}

// ── plan execution tracking (matched findings) ───────────────────────

#[test]
fn matched_fix_records_finding_details() {
    let plan = make_plan(vec![make_fix(
        "f1",
        SafetyLevel::Safe,
        vec![make_ref("sensor", Some("fp-1"), Some("C001"))],
    )]);
    let highlights = vec![make_highlight("sensor", "C001", Some("fp-1"))];
    let summary = match_buildfix_plan("sensor", &plan, &highlights);
    assert_eq!(summary.fixes.len(), 1);
    assert!(!summary.fixes[0].unmatched);
    assert!(!summary.fixes[0].matched_findings.is_empty());
    let mf = &summary.fixes[0].matched_findings[0];
    assert_eq!(mf.sensor_id, "sensor");
    assert_eq!(mf.code, "C001");
    assert_eq!(mf.fingerprint.as_deref(), Some("fp-1"));
}

#[test]
fn unmatched_fix_has_empty_matched_findings() {
    let plan = make_plan(vec![make_fix(
        "f1",
        SafetyLevel::Safe,
        vec![make_ref("sensor", Some("fp-missing"), None)],
    )]);
    let highlights = vec![make_highlight("sensor", "C001", Some("fp-other"))];
    let summary = match_buildfix_plan("sensor", &plan, &highlights);
    assert!(summary.fixes[0].unmatched);
    assert!(summary.fixes[0].matched_findings.is_empty());
}

// ── buildfix verdict derivation (auto-apply selection) ───────────────

#[test]
fn select_safe_only_excludes_guarded_and_unsafe() {
    let summary = BuildfixSummary {
        fixes: vec![
            make_fix_summary("f1", "s", SafetyLevel::Safe, false),
            make_fix_summary("f2", "s", SafetyLevel::Guarded, false),
            make_fix_summary("f3", "s", SafetyLevel::Unsafe, false),
        ],
        total_fixes: 3,
        matched_count: 3,
        unmatched_count: 0,
    };
    let selected = select_auto_apply_fixes(&summary, SafetyLevel::Safe, false);
    assert_eq!(selected.len(), 1);
    assert_eq!(selected[0].fix_id, "f1");
}

#[test]
fn select_guarded_includes_safe_and_guarded() {
    let summary = BuildfixSummary {
        fixes: vec![
            make_fix_summary("f1", "s", SafetyLevel::Safe, false),
            make_fix_summary("f2", "s", SafetyLevel::Guarded, false),
            make_fix_summary("f3", "s", SafetyLevel::Unsafe, false),
        ],
        total_fixes: 3,
        matched_count: 3,
        unmatched_count: 0,
    };
    let selected = select_auto_apply_fixes(&summary, SafetyLevel::Guarded, false);
    assert_eq!(selected.len(), 2);
}

#[test]
fn require_matched_finding_filters_unmatched() {
    let summary = BuildfixSummary {
        fixes: vec![
            make_fix_summary("f1", "s", SafetyLevel::Safe, false),
            make_fix_summary("f2", "s", SafetyLevel::Safe, true),
        ],
        total_fixes: 2,
        matched_count: 1,
        unmatched_count: 1,
    };
    let selected = select_auto_apply_fixes(&summary, SafetyLevel::Safe, true);
    assert_eq!(selected.len(), 1);
    assert_eq!(selected[0].fix_id, "f1");
}

// ── priority ordering of fixes ───────────────────────────────────────

#[test]
fn fixes_sorted_by_safety_then_sensor_then_id() {
    let plan = make_plan(vec![
        make_fix(
            "z-fix",
            SafetyLevel::Unsafe,
            vec![make_ref("sensor", None, None)],
        ),
        make_fix(
            "a-fix",
            SafetyLevel::Safe,
            vec![make_ref("sensor", None, None)],
        ),
        make_fix(
            "m-fix",
            SafetyLevel::Guarded,
            vec![make_ref("sensor", None, None)],
        ),
    ]);
    let summary = match_buildfix_plan("sensor", &plan, &[]);
    // Expected: safe → guarded → unsafe.
    assert_eq!(summary.fixes[0].safety, SafetyLevel::Safe);
    assert_eq!(summary.fixes[1].safety, SafetyLevel::Guarded);
    assert_eq!(summary.fixes[2].safety, SafetyLevel::Unsafe);
}

#[test]
fn fixes_with_same_safety_sorted_by_sensor_id_then_fix_id() {
    let plan = make_plan(vec![
        make_fix(
            "b-fix",
            SafetyLevel::Safe,
            vec![make_ref("sensor", None, None)],
        ),
        make_fix(
            "a-fix",
            SafetyLevel::Safe,
            vec![make_ref("sensor", None, None)],
        ),
    ]);
    let summary = match_buildfix_plan("sensor", &plan, &[]);
    assert_eq!(summary.fixes[0].fix_id, "a-fix");
    assert_eq!(summary.fixes[1].fix_id, "b-fix");
}

// ── cross-sensor filtering ───────────────────────────────────────────

#[test]
fn wrong_sensor_id_marks_fix_unmatched() {
    let plan = make_plan(vec![make_fix(
        "f1",
        SafetyLevel::Safe,
        vec![make_ref("clippy", Some("fp-1"), Some("C001"))],
    )]);
    let highlights = vec![make_highlight("clippy", "C001", Some("fp-1"))];
    // Call with different sensor_id → unmatched.
    let summary = match_buildfix_plan("wrong-sensor", &plan, &highlights);
    assert_eq!(summary.unmatched_count, 1);
}

// ── end-to-end pipeline ──────────────────────────────────────────────

#[test]
fn end_to_end_match_then_select() {
    let plan = make_plan(vec![
        make_fix(
            "safe-fix",
            SafetyLevel::Safe,
            vec![make_ref("sensor", Some("fp-1"), Some("C001"))],
        ),
        make_fix(
            "guarded-fix",
            SafetyLevel::Guarded,
            vec![make_ref("sensor", None, Some("C002"))],
        ),
    ]);
    let highlights = vec![
        make_highlight("sensor", "C001", Some("fp-1")),
        make_highlight("sensor", "C002", None),
    ];
    let summary = match_buildfix_plan("sensor", &plan, &highlights);
    assert_eq!(summary.matched_count, 2);

    let safe_only = select_auto_apply_fixes(&summary, SafetyLevel::Safe, false);
    assert_eq!(safe_only.len(), 1);
    assert_eq!(safe_only[0].fix_id, "safe-fix");

    let up_to_guarded = select_auto_apply_fixes(&summary, SafetyLevel::Guarded, false);
    assert_eq!(up_to_guarded.len(), 2);
}
