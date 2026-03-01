//! Integration / snapshot tests for `cockpitctl-domain-buildfix`.

use cockpitctl_domain_buildfix::{match_buildfix_plan, select_auto_apply_fixes};
use cockpitctl_types::{
    BuildfixPlan, Finding, FindingRef, Fix, Highlight, Location, SafetyLevel, Severity, ToolInfo,
};

// ── Helpers ────────────────────────────────────────────────────────────────

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

fn fref(sensor: &str, fp: Option<&str>, code: Option<&str>) -> FindingRef {
    FindingRef {
        sensor_id: sensor.into(),
        fingerprint: fp.map(Into::into),
        code: code.map(Into::into),
        tool: None,
        check_id: None,
    }
}

fn highlight(sensor: &str, code: &str, fp: Option<&str>, path: &str, line: u32) -> Highlight {
    Highlight {
        sensor_id: sensor.into(),
        finding: Finding {
            severity: Severity::Error,
            check_id: None,
            code: code.into(),
            message: format!("{code} message"),
            location: Some(Location {
                path: Some(path.into()),
                line: Some(line),
                col: None,
            }),
            help: None,
            url: None,
            fingerprint: fp.map(Into::into),
            data: None,
        },
    }
}

// ── No matching fixes → empty results ──────────────────────────────────────

#[test]
fn no_matching_fixes_produces_empty_matched() {
    let plan = make_plan(vec![make_fix(
        "fix-1",
        SafetyLevel::Safe,
        vec![fref("sensor", Some("fp-x"), Some("CODE-X"))],
    )]);
    let highlights = vec![highlight(
        "sensor",
        "OTHER",
        Some("fp-other"),
        "src/a.rs",
        1,
    )];

    let summary = match_buildfix_plan("sensor", &plan, &highlights);
    insta::assert_json_snapshot!("no_matching_fixes", summary);
}

// ── Exact path+line match (code+fingerprint) → fix applied ─────────────────

#[test]
fn exact_match_applies_fix() {
    let plan = make_plan(vec![make_fix(
        "fix-exact",
        SafetyLevel::Safe,
        vec![fref("sensor", Some("fp-1"), Some("ERR001"))],
    )]);
    let highlights = vec![highlight(
        "sensor",
        "ERR001",
        Some("fp-1"),
        "src/main.rs",
        42,
    )];

    let summary = match_buildfix_plan("sensor", &plan, &highlights);
    insta::assert_json_snapshot!("exact_match_applies_fix", summary);
}

// ── Multiple fixes for same finding → deterministic selection ──────────────

#[test]
fn multiple_fixes_same_finding_deterministic_order() {
    let plan = make_plan(vec![
        make_fix(
            "fix-unsafe",
            SafetyLevel::Unsafe,
            vec![fref("sensor", Some("fp-1"), Some("W001"))],
        ),
        make_fix(
            "fix-safe-b",
            SafetyLevel::Safe,
            vec![fref("sensor", Some("fp-1"), Some("W001"))],
        ),
        make_fix(
            "fix-safe-a",
            SafetyLevel::Safe,
            vec![fref("sensor", Some("fp-1"), Some("W001"))],
        ),
        make_fix(
            "fix-guarded",
            SafetyLevel::Guarded,
            vec![fref("sensor", Some("fp-1"), Some("W001"))],
        ),
    ]);
    let highlights = vec![highlight("sensor", "W001", Some("fp-1"), "src/lib.rs", 10)];

    let summary = match_buildfix_plan("sensor", &plan, &highlights);
    insta::assert_json_snapshot!("multiple_fixes_deterministic", summary);
}

// ── Auto-apply selection with safety gating ────────────────────────────────

#[test]
fn auto_apply_safe_gate_snapshot() {
    let plan = make_plan(vec![
        make_fix(
            "safe-fix",
            SafetyLevel::Safe,
            vec![fref("sensor", Some("fp-1"), Some("E001"))],
        ),
        make_fix(
            "guarded-fix",
            SafetyLevel::Guarded,
            vec![fref("sensor", Some("fp-2"), Some("E002"))],
        ),
        make_fix(
            "unsafe-fix",
            SafetyLevel::Unsafe,
            vec![fref("sensor", Some("fp-3"), Some("E003"))],
        ),
    ]);
    let highlights = vec![
        highlight("sensor", "E001", Some("fp-1"), "src/a.rs", 1),
        highlight("sensor", "E002", Some("fp-2"), "src/b.rs", 2),
        highlight("sensor", "E003", Some("fp-3"), "src/c.rs", 3),
    ];

    let summary = match_buildfix_plan("sensor", &plan, &highlights);

    let safe_only = select_auto_apply_fixes(&summary, SafetyLevel::Safe, true);
    insta::assert_json_snapshot!("auto_apply_safe_only", safe_only);

    let up_to_guarded = select_auto_apply_fixes(&summary, SafetyLevel::Guarded, true);
    insta::assert_json_snapshot!("auto_apply_up_to_guarded", up_to_guarded);

    let all = select_auto_apply_fixes(&summary, SafetyLevel::Unsafe, true);
    insta::assert_json_snapshot!("auto_apply_all", all);
}

// ── Empty plan + empty highlights ──────────────────────────────────────────

#[test]
fn empty_plan_empty_highlights() {
    let plan = make_plan(vec![]);
    let summary = match_buildfix_plan("sensor", &plan, &[]);
    insta::assert_json_snapshot!("empty_plan_empty_highlights", summary);
}

// ── Mixed matched/unmatched with require_matched_finding ───────────────────

#[test]
fn require_matched_finding_filters_correctly() {
    let plan = make_plan(vec![
        make_fix(
            "matched-fix",
            SafetyLevel::Safe,
            vec![fref("sensor", Some("fp-1"), Some("E001"))],
        ),
        make_fix(
            "unmatched-fix",
            SafetyLevel::Safe,
            vec![fref("sensor", Some("fp-missing"), Some("E999"))],
        ),
    ]);
    let highlights = vec![highlight("sensor", "E001", Some("fp-1"), "src/a.rs", 1)];

    let summary = match_buildfix_plan("sensor", &plan, &highlights);
    let selected = select_auto_apply_fixes(&summary, SafetyLevel::Unsafe, true);
    insta::assert_json_snapshot!("require_matched_filters", selected);
}
