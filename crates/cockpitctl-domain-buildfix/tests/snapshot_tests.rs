use cockpitctl_domain_buildfix::{match_buildfix_plan, select_auto_apply_fixes};
use cockpitctl_types::{
    BuildfixPlan, BuildfixSummary, Finding, FindingRef, Fix, FixSummary, Highlight, Location,
    MatchedFinding, SafetyLevel, Severity, ToolInfo,
};

fn tool_info() -> ToolInfo {
    ToolInfo {
        name: "buildfix".to_string(),
        version: "1.0.0".to_string(),
        commit: None,
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

fn make_finding_ref(sensor_id: &str, fingerprint: Option<&str>, code: Option<&str>) -> FindingRef {
    FindingRef {
        sensor_id: sensor_id.to_string(),
        fingerprint: fingerprint.map(|s| s.to_string()),
        code: code.map(|s| s.to_string()),
        tool: None,
        check_id: None,
    }
}

fn highlight(code: &str, fp: Option<&str>) -> Highlight {
    Highlight {
        sensor_id: "sensor".to_string(),
        finding: Finding {
            severity: Severity::Error,
            check_id: Some("check".to_string()),
            code: code.to_string(),
            message: format!("message for {code}"),
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
    matched: Vec<MatchedFinding>,
) -> FixSummary {
    FixSummary {
        fix_id: fix_id.to_string(),
        sensor_id: sensor_id.to_string(),
        safety,
        description: format!("desc-{fix_id}"),
        matched_findings: matched,
        unmatched,
    }
}

// ---------------------------------------------------------------------------
// match_buildfix_plan snapshots
// ---------------------------------------------------------------------------

#[test]
fn snapshot_buildfix_plan_single_match() {
    let plan = make_plan(vec![make_fix(
        "fix-1",
        SafetyLevel::Safe,
        vec![make_finding_ref("sensor", Some("fp-1"), Some("CODE-1"))],
    )]);
    let highlights = vec![highlight("CODE-1", Some("fp-1"))];
    let summary = match_buildfix_plan("sensor", &plan, &highlights);
    insta::assert_json_snapshot!("buildfix_plan_single_match", summary);
}

#[test]
fn snapshot_buildfix_plan_no_match() {
    let plan = make_plan(vec![make_fix(
        "fix-1",
        SafetyLevel::Safe,
        vec![make_finding_ref("sensor", Some("fp-1"), Some("CODE-1"))],
    )]);
    let highlights = vec![highlight("OTHER-CODE", Some("other-fp"))];
    let summary = match_buildfix_plan("sensor", &plan, &highlights);
    insta::assert_json_snapshot!("buildfix_plan_no_match", summary);
}

#[test]
fn snapshot_buildfix_plan_multiple_fixes_sorted() {
    let plan = make_plan(vec![
        make_fix(
            "z-unsafe-fix",
            SafetyLevel::Unsafe,
            vec![make_finding_ref("sensor", None, Some("CODE-1"))],
        ),
        make_fix(
            "a-safe-fix",
            SafetyLevel::Safe,
            vec![make_finding_ref("sensor", None, Some("CODE-1"))],
        ),
        make_fix(
            "m-guarded-fix",
            SafetyLevel::Guarded,
            vec![make_finding_ref("sensor", None, Some("CODE-2"))],
        ),
    ]);
    let highlights = vec![highlight("CODE-1", None), highlight("CODE-2", None)];
    let summary = match_buildfix_plan("sensor", &plan, &highlights);
    insta::assert_json_snapshot!("buildfix_plan_multiple_fixes_sorted", summary);
}

#[test]
fn snapshot_buildfix_plan_empty() {
    let plan = make_plan(vec![]);
    let summary = match_buildfix_plan("sensor", &plan, &[]);
    insta::assert_json_snapshot!("buildfix_plan_empty", summary);
}

// ---------------------------------------------------------------------------
// select_auto_apply_fixes snapshots
// ---------------------------------------------------------------------------

#[test]
fn snapshot_select_auto_apply_safe_gate() {
    let summary = BuildfixSummary {
        fixes: vec![
            make_fix_summary(
                "safe-fix",
                "sensor",
                SafetyLevel::Safe,
                false,
                vec![MatchedFinding {
                    sensor_id: "sensor".into(),
                    code: "CODE-1".into(),
                    fingerprint: Some("fp-1".into()),
                }],
            ),
            make_fix_summary("guarded-fix", "sensor", SafetyLevel::Guarded, false, vec![]),
            make_fix_summary("unsafe-fix", "sensor", SafetyLevel::Unsafe, false, vec![]),
        ],
        total_fixes: 3,
        matched_count: 3,
        unmatched_count: 0,
    };
    let selected = select_auto_apply_fixes(&summary, SafetyLevel::Safe, false);
    insta::assert_json_snapshot!("select_auto_apply_safe_gate", selected);
}

#[test]
fn snapshot_select_auto_apply_with_match_requirement() {
    let summary = BuildfixSummary {
        fixes: vec![
            make_fix_summary(
                "matched-safe",
                "sensor",
                SafetyLevel::Safe,
                false,
                vec![MatchedFinding {
                    sensor_id: "sensor".into(),
                    code: "CODE-1".into(),
                    fingerprint: None,
                }],
            ),
            make_fix_summary("unmatched-safe", "sensor", SafetyLevel::Safe, true, vec![]),
        ],
        total_fixes: 2,
        matched_count: 1,
        unmatched_count: 1,
    };
    let selected = select_auto_apply_fixes(&summary, SafetyLevel::Unsafe, true);
    insta::assert_json_snapshot!("select_auto_apply_with_match_requirement", selected);
}

#[test]
fn snapshot_buildfix_end_to_end_plan_then_select() {
    let plan = make_plan(vec![
        make_fix(
            "safe-fix",
            SafetyLevel::Safe,
            vec![make_finding_ref("sensor", Some("fp-1"), Some("CODE-1"))],
        ),
        make_fix(
            "guarded-fix",
            SafetyLevel::Guarded,
            vec![make_finding_ref("sensor", Some("fp-2"), Some("CODE-2"))],
        ),
        make_fix(
            "unsafe-fix",
            SafetyLevel::Unsafe,
            vec![make_finding_ref("sensor", None, None)],
        ),
    ]);
    let highlights = vec![
        highlight("CODE-1", Some("fp-1")),
        highlight("CODE-2", Some("fp-2")),
    ];
    let summary = match_buildfix_plan("sensor", &plan, &highlights);
    let auto = select_auto_apply_fixes(&summary, SafetyLevel::Guarded, true);
    insta::assert_json_snapshot!("buildfix_end_to_end_guarded_gate", auto);
}
