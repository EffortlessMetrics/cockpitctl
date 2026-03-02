//! Mutation-targeted tests for cockpitctl-render.
//!
//! Each test catches a specific mutant that survived previous cargo-mutants analysis.

use cockpitctl_render::{render_annotations, render_comment};
use cockpitctl_types::*;
use std::collections::BTreeMap;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_run() -> RunInfo {
    RunInfo {
        started_at: "2026-01-01T00:00:00Z".into(),
        ended_at: None,
        duration_ms: None,
        host: None,
        git: None,
        ci: None,
        capabilities: BTreeMap::new(),
    }
}

fn make_policy_snapshot() -> PolicySnapshot {
    PolicySnapshot {
        warn_is_fail: false,
        max_highlights: 7,
        max_per_sensor_findings: 20,
        max_annotations: 25,
        section_order: vec!["Highlights".into(), "Other".into()],
        sensors: vec![],
    }
}

fn sensor_summary(id: &str, status: VerdictStatus, blocking: bool) -> SensorSummary {
    SensorSummary {
        id: id.into(),
        blocking,
        missing: MissingPolicy::Skip,
        presence: Presence::Present,
        report_path: format!("artifacts/{id}/report.json"),
        comment_path: None,
        verdict: Verdict {
            status,
            counts: VerdictCounts::default(),
            reasons: vec![],
        },
        truncated: false,
        errors: vec![],
        missing_policy_applied: None,
        policy_outcome: None,
    }
}

fn highlight(sensor_id: &str, code: &str, sev: Severity) -> Highlight {
    Highlight {
        sensor_id: sensor_id.into(),
        finding: Finding {
            severity: sev,
            check_id: None,
            code: code.into(),
            message: format!("msg-{code}"),
            location: None,
            help: None,
            url: None,
            fingerprint: None,
            data: None,
        },
    }
}

fn highlight_with_loc(
    sensor_id: &str,
    code: &str,
    sev: Severity,
    path: &str,
    line: u32,
) -> Highlight {
    Highlight {
        sensor_id: sensor_id.into(),
        finding: Finding {
            severity: sev,
            check_id: None,
            code: code.into(),
            message: format!("msg-{code}"),
            location: Some(Location {
                path: Some(path.into()),
                line: Some(line),
                col: None,
            }),
            help: None,
            url: None,
            fingerprint: None,
            data: None,
        },
    }
}

fn make_report(
    sensors: Vec<SensorSummary>,
    highlights: Vec<Highlight>,
    overall_status: VerdictStatus,
) -> CockpitReport {
    CockpitReport {
        schema: "cockpit.report.v1".into(),
        tool: ToolInfo {
            name: "cockpitctl".into(),
            version: "0.1.0".into(),
            commit: None,
        },
        run: make_run(),
        verdict: Verdict {
            status: overall_status,
            counts: VerdictCounts::default(),
            reasons: vec![],
        },
        sensors,
        highlights,
        policy: make_policy_snapshot(),
        data: None,
    }
}

// ===========================================================================
// render_comment — status badges (✅ pass, ❌ fail, ⚠️ warn, ⏭ skip)
// ===========================================================================

#[test]
fn render_comment_pass_badge() {
    let report = make_report(
        vec![sensor_summary("s1", VerdictStatus::Pass, true)],
        vec![],
        VerdictStatus::Pass,
    );
    let cfg = CockpitConfig::default();
    let md = render_comment(&report, &cfg);
    assert!(
        md.contains("✅ pass"),
        "pass sensor should have ✅ pass badge"
    );
}

#[test]
fn render_comment_fail_badge() {
    let report = make_report(
        vec![sensor_summary("s1", VerdictStatus::Fail, true)],
        vec![],
        VerdictStatus::Fail,
    );
    let cfg = CockpitConfig::default();
    let md = render_comment(&report, &cfg);
    assert!(
        md.contains("❌ fail"),
        "fail sensor should have ❌ fail badge"
    );
}

#[test]
fn render_comment_warn_badge() {
    let report = make_report(
        vec![sensor_summary("s1", VerdictStatus::Warn, false)],
        vec![],
        VerdictStatus::Pass,
    );
    let cfg = CockpitConfig::default();
    let md = render_comment(&report, &cfg);
    assert!(
        md.contains("⚠\u{fe0f} warn"),
        "warn sensor should have ⚠️ warn badge"
    );
}

#[test]
fn render_comment_skip_badge() {
    let report = make_report(
        vec![sensor_summary("s1", VerdictStatus::Skip, false)],
        vec![],
        VerdictStatus::Pass,
    );
    let cfg = CockpitConfig::default();
    let md = render_comment(&report, &cfg);
    assert!(
        md.contains("⏭ skip"),
        "skip sensor should have ⏭ skip badge"
    );
}

// ===========================================================================
// render_comment — severity badges in highlights (❌, ⚠️, ℹ️)
// ===========================================================================

#[test]
fn render_comment_error_severity_badge() {
    let report = make_report(
        vec![sensor_summary("s1", VerdictStatus::Fail, true)],
        vec![highlight("s1", "E1", Severity::Error)],
        VerdictStatus::Fail,
    );
    let cfg = CockpitConfig::default();
    let md = render_comment(&report, &cfg);
    assert!(md.contains("❌"), "error highlights should have ❌ badge");
}

#[test]
fn render_comment_warn_severity_badge() {
    let report = make_report(
        vec![sensor_summary("s1", VerdictStatus::Warn, false)],
        vec![highlight("s1", "W1", Severity::Warn)],
        VerdictStatus::Warn,
    );
    let cfg = CockpitConfig::default();
    let md = render_comment(&report, &cfg);
    assert!(
        md.contains("⚠\u{fe0f}"),
        "warn highlights should have ⚠️ badge"
    );
}

#[test]
fn render_comment_info_severity_badge() {
    let report = make_report(
        vec![sensor_summary("s1", VerdictStatus::Pass, false)],
        vec![highlight("s1", "I1", Severity::Info)],
        VerdictStatus::Pass,
    );
    let cfg = CockpitConfig::default();
    let md = render_comment(&report, &cfg);
    assert!(
        md.contains("ℹ\u{fe0f}"),
        "info highlights should have ℹ️ badge"
    );
}

// ===========================================================================
// render_comment — begin/end markers and structure
// ===========================================================================

#[test]
fn render_comment_has_begin_marker() {
    let report = make_report(vec![], vec![], VerdictStatus::Pass);
    let cfg = CockpitConfig::default();
    let md = render_comment(&report, &cfg);
    assert!(md.contains("<!-- cockpit:begin -->"));
}

#[test]
fn render_comment_has_end_marker() {
    let report = make_report(vec![], vec![], VerdictStatus::Pass);
    let cfg = CockpitConfig::default();
    let md = render_comment(&report, &cfg);
    assert!(md.contains("<!-- cockpit:end -->"));
}

#[test]
fn render_comment_begin_before_end() {
    let report = make_report(vec![], vec![], VerdictStatus::Pass);
    let cfg = CockpitConfig::default();
    let md = render_comment(&report, &cfg);
    let begin = md.find("<!-- cockpit:begin -->").unwrap();
    let end = md.find("<!-- cockpit:end -->").unwrap();
    assert!(begin < end, "begin marker must come before end marker");
}

#[test]
fn render_comment_has_cockpit_heading() {
    let report = make_report(vec![], vec![], VerdictStatus::Pass);
    let cfg = CockpitConfig::default();
    let md = render_comment(&report, &cfg);
    assert!(md.contains("## Cockpit"));
}

#[test]
fn render_comment_has_summary_section() {
    let report = make_report(vec![], vec![], VerdictStatus::Pass);
    let cfg = CockpitConfig::default();
    let md = render_comment(&report, &cfg);
    assert!(md.contains("### Summary"));
}

#[test]
fn render_comment_has_highlights_section() {
    let report = make_report(vec![], vec![], VerdictStatus::Pass);
    let cfg = CockpitConfig::default();
    let md = render_comment(&report, &cfg);
    assert!(md.contains("### Highlights"));
}

#[test]
fn render_comment_no_highlights_message() {
    let report = make_report(vec![], vec![], VerdictStatus::Pass);
    let cfg = CockpitConfig::default();
    let md = render_comment(&report, &cfg);
    assert!(md.contains("_No highlights._"));
}

#[test]
fn render_comment_highlight_shows_code() {
    let report = make_report(
        vec![sensor_summary("s1", VerdictStatus::Fail, true)],
        vec![highlight("s1", "MY_CODE", Severity::Error)],
        VerdictStatus::Fail,
    );
    let cfg = CockpitConfig::default();
    let md = render_comment(&report, &cfg);
    assert!(
        md.contains("MY_CODE"),
        "highlight code should appear in comment"
    );
    assert!(
        md.contains("msg-MY_CODE"),
        "highlight message should appear in comment"
    );
}

#[test]
fn render_comment_highlight_with_location() {
    let report = make_report(
        vec![sensor_summary("s1", VerdictStatus::Fail, true)],
        vec![highlight_with_loc(
            "s1",
            "E1",
            Severity::Error,
            "src/main.rs",
            42,
        )],
        VerdictStatus::Fail,
    );
    let cfg = CockpitConfig::default();
    let md = render_comment(&report, &cfg);
    assert!(
        md.contains("src/main.rs:42"),
        "location should appear in comment"
    );
}

// ===========================================================================
// render_annotations — capping, truncation flag, counts
// ===========================================================================

#[test]
fn render_annotations_empty() {
    let cfg = CockpitConfig::default();
    let blocking = BTreeMap::new();
    let result = render_annotations(&[], &cfg, &blocking);
    assert_eq!(result.total_count, 0);
    assert_eq!(result.rendered_count, 0);
    assert!(!result.truncated);
    assert!(result.content.contains("_No annotations._"));
}

#[test]
fn render_annotations_under_limit() {
    let highlights = vec![
        highlight("s1", "E1", Severity::Error),
        highlight("s1", "E2", Severity::Warn),
    ];
    let cfg = CockpitConfig::default();
    let blocking = BTreeMap::from([("s1".to_string(), true)]);
    let result = render_annotations(&highlights, &cfg, &blocking);
    assert_eq!(result.total_count, 2);
    assert_eq!(result.rendered_count, 2);
    assert!(!result.truncated);
    assert!(result.content.contains("E1"));
    assert!(result.content.contains("E2"));
}

#[test]
fn render_annotations_caps_at_max() {
    let mut highlights = Vec::new();
    for i in 0..30 {
        highlights.push(highlight("s1", &format!("E{i}"), Severity::Error));
    }
    let cfg = CockpitConfig::default();
    let blocking = BTreeMap::new();
    let result = render_annotations(&highlights, &cfg, &blocking);
    assert_eq!(result.total_count, 30);
    assert_eq!(result.rendered_count, 25);
    assert!(result.truncated);
    assert!(result.content.contains("25 of 30"));
}

#[test]
fn render_annotations_severity_badges() {
    let highlights = vec![
        highlight("s1", "E1", Severity::Error),
        highlight("s1", "W1", Severity::Warn),
        highlight("s1", "I1", Severity::Info),
    ];
    let cfg = CockpitConfig::default();
    let blocking = BTreeMap::new();
    let result = render_annotations(&highlights, &cfg, &blocking);
    assert!(result.content.contains("❌"), "should contain error badge");
    assert!(
        result.content.contains("⚠\u{fe0f}"),
        "should contain warn badge"
    );
    assert!(
        result.content.contains("ℹ\u{fe0f}"),
        "should contain info badge"
    );
}

#[test]
fn render_annotations_includes_location() {
    let highlights = vec![highlight_with_loc(
        "s1",
        "E1",
        Severity::Error,
        "src/lib.rs",
        99,
    )];
    let cfg = CockpitConfig::default();
    let blocking = BTreeMap::new();
    let result = render_annotations(&highlights, &cfg, &blocking);
    assert!(result.content.contains("src/lib.rs:99"));
}

#[test]
fn render_annotations_exactly_at_limit() {
    let mut highlights = Vec::new();
    for i in 0..25 {
        highlights.push(highlight("s1", &format!("E{i}"), Severity::Error));
    }
    let cfg = CockpitConfig::default();
    let blocking = BTreeMap::new();
    let result = render_annotations(&highlights, &cfg, &blocking);
    assert_eq!(result.total_count, 25);
    assert_eq!(result.rendered_count, 25);
    assert!(
        !result.truncated,
        "exactly at limit should not be truncated"
    );
}
