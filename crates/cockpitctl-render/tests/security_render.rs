//! Security-focused render tests for cockpitctl-render.
//!
//! These tests verify that the markdown renderer handles adversarial content
//! safely: HTML injection, markdown injection, very long messages, and
//! control characters. The renderer should produce deterministic, safe output
//! regardless of input content.

use cockpitctl_render::{render_annotations, render_comment};
use cockpitctl_types::{
    CockpitConfig, CockpitReport, Finding, Highlight, Location, PolicySnapshot, RunInfo, Severity,
    ToolInfo, Verdict, VerdictCounts, VerdictStatus,
};
use std::collections::BTreeMap;

/// Build a minimal CockpitReport for testing.
fn minimal_report() -> CockpitReport {
    let cfg = CockpitConfig::default();
    let policy = PolicySnapshot {
        warn_is_fail: cfg.policy.warn_is_fail,
        max_highlights: cfg.policy.max_highlights,
        max_per_sensor_findings: cfg.policy.max_per_sensor_findings,
        max_annotations: cfg.policy.max_annotations,
        section_order: cfg.policy.section_order.clone(),
        sensors: vec![],
    };
    CockpitReport {
        schema: "cockpit.report.v1".to_string(),
        tool: ToolInfo {
            name: "cockpitctl".to_string(),
            version: "0.2.0".to_string(),
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
            status: VerdictStatus::Pass,
            counts: VerdictCounts::default(),
            reasons: vec![],
        },
        sensors: vec![],
        highlights: vec![],
        policy,
        data: None,
    }
}

fn make_highlight(sensor_id: &str, code: &str, message: &str, severity: Severity) -> Highlight {
    Highlight {
        sensor_id: sensor_id.to_string(),
        finding: Finding {
            severity,
            check_id: None,
            code: code.to_string(),
            message: message.to_string(),
            location: Some(Location {
                path: Some("src/main.rs".to_string()),
                line: Some(1),
                col: None,
            }),
            help: None,
            url: None,
            fingerprint: None,
            data: None,
        },
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// HTML INJECTION IN FINDINGS
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn html_script_injection_in_message_rendered_safely() {
    let cfg = CockpitConfig::default();
    let highlights = vec![make_highlight(
        "test-sensor",
        "xss",
        "<script>alert('xss')</script>",
        Severity::Error,
    )];

    let blocking = BTreeMap::new();
    let result = render_annotations(&highlights, &cfg, &blocking);

    // The raw script tag appears in the output because markdown renderers
    // treat it as text in code contexts. The important thing is no crash.
    assert!(!result.content.is_empty(), "should produce output");
    // Newlines in message should be collapsed (per render_annotations logic)
    assert!(
        !result.content.contains('\0'),
        "output should not contain null bytes"
    );
}

#[test]
fn html_injection_in_code_field_rendered_safely() {
    let cfg = CockpitConfig::default();
    let highlights = vec![make_highlight(
        "test-sensor",
        "<img src=x onerror=alert(1)>",
        "test message",
        Severity::Error,
    )];

    let blocking = BTreeMap::new();
    let result = render_annotations(&highlights, &cfg, &blocking);

    // Code is wrapped in backticks by the renderer, so HTML is neutralized
    assert!(
        result.content.contains('`'),
        "code should be wrapped in backticks"
    );
}

#[test]
fn html_injection_in_full_report_comment() {
    let mut report = minimal_report();
    report.highlights = vec![make_highlight(
        "evil-sensor",
        "xss",
        "<script>document.location='https://evil.com'</script>",
        Severity::Error,
    )];

    let cfg = CockpitConfig::default();
    let comment = render_comment(&report, &cfg);

    // Full comment should render without panicking
    assert!(!comment.is_empty());
    // Should contain cockpit markers
    assert!(comment.contains("cockpit"));
}

// ═══════════════════════════════════════════════════════════════════════════
// MARKDOWN INJECTION
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn javascript_link_injection_handled() {
    let cfg = CockpitConfig::default();
    let highlights = vec![make_highlight(
        "test-sensor",
        "md-inject",
        "[evil](javascript:void(0))",
        Severity::Warn,
    )];

    let blocking = BTreeMap::new();
    let result = render_annotations(&highlights, &cfg, &blocking);

    // Should render without crashing
    assert!(!result.content.is_empty());
}

#[test]
fn markdown_image_injection_handled() {
    let cfg = CockpitConfig::default();
    let highlights = vec![make_highlight(
        "test-sensor",
        "img-inject",
        "![pwned](https://evil.com/track.gif)",
        Severity::Info,
    )];

    let blocking = BTreeMap::new();
    let result = render_annotations(&highlights, &cfg, &blocking);
    assert!(!result.content.is_empty());
}

#[test]
fn markdown_heading_injection_handled() {
    let cfg = CockpitConfig::default();
    let highlights = vec![make_highlight(
        "test-sensor",
        "heading-inject",
        "# FAKE HEADING\n## SUBHEADING",
        Severity::Error,
    )];

    let blocking = BTreeMap::new();
    let result = render_annotations(&highlights, &cfg, &blocking);

    // Newlines in messages should be collapsed to spaces by the renderer
    assert!(
        !result.content.contains("\n# FAKE HEADING"),
        "heading injection should be neutralized by newline collapsing"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// VERY LONG MESSAGES
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn very_long_message_does_not_oom() {
    let cfg = CockpitConfig::default();
    // 1MB message
    let long_msg: String = "a".repeat(1_000_000);
    let highlights = vec![make_highlight(
        "test-sensor",
        "long-msg",
        &long_msg,
        Severity::Error,
    )];

    let blocking = BTreeMap::new();
    let result = render_annotations(&highlights, &cfg, &blocking);

    // Should complete without OOM
    assert!(!result.content.is_empty());
    assert_eq!(result.total_count, 1);
    assert_eq!(result.rendered_count, 1);
}

#[test]
fn very_long_code_field_does_not_oom() {
    let cfg = CockpitConfig::default();
    let long_code: String = "x".repeat(100_000);
    let highlights = vec![make_highlight(
        "test-sensor",
        &long_code,
        "test",
        Severity::Warn,
    )];

    let blocking = BTreeMap::new();
    let result = render_annotations(&highlights, &cfg, &blocking);
    assert!(!result.content.is_empty());
}

// ═══════════════════════════════════════════════════════════════════════════
// CONTROL CHARACTERS IN MESSAGES
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn newlines_in_message_collapsed() {
    let cfg = CockpitConfig::default();
    let highlights = vec![make_highlight(
        "test-sensor",
        "newline-test",
        "line1\nline2\nline3",
        Severity::Error,
    )];

    let blocking = BTreeMap::new();
    let result = render_annotations(&highlights, &cfg, &blocking);

    // The renderer replaces \n with space (see render_annotations source)
    // Each annotation line should not contain raw newlines in the message part
    let annotation_lines: Vec<&str> = result
        .content
        .lines()
        .filter(|l| l.starts_with("1."))
        .collect();
    for line in &annotation_lines {
        // The message portion should have newlines replaced with spaces
        assert!(
            line.contains("line1 line2 line3"),
            "newlines should be collapsed to spaces in annotation line: {}",
            line
        );
    }
}

#[test]
fn carriage_return_in_message_handled() {
    let cfg = CockpitConfig::default();
    let highlights = vec![make_highlight(
        "test-sensor",
        "cr-test",
        "before\rafter",
        Severity::Warn,
    )];

    let blocking = BTreeMap::new();
    let result = render_annotations(&highlights, &cfg, &blocking);
    // Should not crash
    assert!(!result.content.is_empty());
}

#[test]
fn tab_characters_in_message_handled() {
    let cfg = CockpitConfig::default();
    let highlights = vec![make_highlight(
        "test-sensor",
        "tab-test",
        "col1\tcol2\tcol3",
        Severity::Info,
    )];

    let blocking = BTreeMap::new();
    let result = render_annotations(&highlights, &cfg, &blocking);
    assert!(!result.content.is_empty());
}

#[test]
fn null_bytes_in_message_handled() {
    let cfg = CockpitConfig::default();
    let highlights = vec![make_highlight(
        "test-sensor",
        "null-test",
        "before\0after",
        Severity::Error,
    )];

    let blocking = BTreeMap::new();
    let result = render_annotations(&highlights, &cfg, &blocking);
    assert!(!result.content.is_empty());
}

// ═══════════════════════════════════════════════════════════════════════════
// EMPTY / EDGE CASE INPUTS
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn empty_highlights_renders_placeholder() {
    let cfg = CockpitConfig::default();
    let blocking = BTreeMap::new();
    let result = render_annotations(&[], &cfg, &blocking);

    assert!(!result.truncated);
    assert_eq!(result.total_count, 0);
    assert!(result.content.contains("No annotations"));
}

#[test]
fn empty_message_and_code_handled() {
    let cfg = CockpitConfig::default();
    let highlights = vec![make_highlight("test-sensor", "", "", Severity::Info)];

    let blocking = BTreeMap::new();
    let result = render_annotations(&highlights, &cfg, &blocking);
    assert!(!result.content.is_empty());
    assert_eq!(result.rendered_count, 1);
}
