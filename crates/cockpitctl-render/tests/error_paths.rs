//! Error path tests for the render crate.
//!
//! Verifies that the renderer handles edge-case inputs gracefully:
//! empty reports, budget overflow, unicode content, special characters,
//! and extreme input sizes.

use cockpitctl_render::{render_annotations, render_comment};
use cockpitctl_types::*;
use std::collections::BTreeMap;

fn make_tool() -> ToolInfo {
    ToolInfo {
        name: "cockpitctl".into(),
        version: "0.1.0".into(),
        commit: None,
    }
}

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

fn make_finding(severity: Severity, code: &str, message: &str) -> Finding {
    Finding {
        severity,
        check_id: None,
        code: code.to_string(),
        message: message.to_string(),
        location: None,
        help: None,
        url: None,
        fingerprint: None,
        data: None,
    }
}

fn make_finding_with_location(
    severity: Severity,
    code: &str,
    message: &str,
    path: &str,
    line: u32,
) -> Finding {
    Finding {
        severity,
        check_id: None,
        code: code.to_string(),
        message: message.to_string(),
        location: Some(Location {
            path: Some(path.to_string()),
            line: Some(line),
            col: None,
        }),
        help: None,
        url: None,
        fingerprint: None,
        data: None,
    }
}

fn make_highlight(sensor_id: &str, finding: Finding) -> Highlight {
    Highlight {
        sensor_id: sensor_id.to_string(),
        finding,
    }
}

fn empty_report() -> CockpitReport {
    CockpitReport {
        schema: "cockpit.report.v1".into(),
        tool: make_tool(),
        run: make_run(),
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
            max_annotations: 25,
            section_order: vec![],
            sensors: vec![],
        },
        data: None,
    }
}

fn report_with_highlights(highlights: Vec<Highlight>) -> CockpitReport {
    let mut report = empty_report();
    report.highlights = highlights;
    report
}

// ============================================================================
// Empty report rendering
// ============================================================================

/// Empty report → valid markdown with stable markers.
#[test]
fn render_empty_report_produces_valid_markdown() {
    let report = empty_report();
    let cfg = CockpitConfig::default();
    let md = render_comment(&report, &cfg);

    assert!(
        md.contains("<!-- cockpit:begin -->"),
        "should have begin marker"
    );
    assert!(
        md.contains("<!-- cockpit:end -->"),
        "should have end marker"
    );
    assert!(md.contains("## Cockpit"), "should have heading");
    assert!(md.contains("_No highlights._"), "should note no highlights");
}

/// Empty report → annotations section says "No annotations."
#[test]
fn render_empty_report_annotations_section() {
    let cfg = CockpitConfig::default();
    let blocking = BTreeMap::new();
    let result = render_annotations(&[], &cfg, &blocking);
    assert_eq!(result.total_count, 0);
    assert!(!result.truncated);
    assert!(result.content.contains("_No annotations._"));
}

// ============================================================================
// Budget / truncation
// ============================================================================

/// 10000 findings beyond budget → gracefully truncated, not OOM/crash.
#[test]
fn render_many_findings_truncated_gracefully() {
    let mut highlights = Vec::new();
    for i in 0..10_000 {
        let mut f = make_finding(Severity::Info, &format!("I{}", i), &format!("msg {}", i));
        // Unique fingerprints to avoid dedup.
        f.fingerprint = Some(format!("fp-{}", i));
        highlights.push(make_highlight("sensor", f));
    }

    let mut cfg = CockpitConfig::default();
    cfg.policy.max_annotations = 25;
    let blocking = BTreeMap::new();

    let result = render_annotations(&highlights, &cfg, &blocking);
    assert!(result.truncated, "should be truncated");
    assert_eq!(result.total_count, 10_000);
    assert_eq!(result.rendered_count, 25);
    assert!(result.content.contains("capped by"));
}

/// max_annotations = 0 → no annotations rendered.
#[test]
fn render_zero_annotation_budget() {
    let highlights = vec![make_highlight(
        "s1",
        make_finding(Severity::Error, "E1", "msg"),
    )];
    let mut cfg = CockpitConfig::default();
    cfg.policy.max_annotations = 0;
    let blocking = BTreeMap::new();

    let result = render_annotations(&highlights, &cfg, &blocking);
    assert!(result.truncated);
    assert_eq!(result.rendered_count, 0);
}

// ============================================================================
// Unicode and emoji handling
// ============================================================================

/// Unicode/emoji in finding messages → rendered correctly in markdown.
#[test]
fn render_unicode_emoji_in_findings() {
    let highlights = vec![make_highlight(
        "sensor",
        make_finding(Severity::Warn, "W1", "🔥 Fire detected in módule «café»"),
    )];
    let report = report_with_highlights(highlights);
    let cfg = CockpitConfig::default();
    let md = render_comment(&report, &cfg);

    assert!(md.contains("🔥 Fire detected"), "emoji should be preserved");
    assert!(md.contains("café"), "accented chars should be preserved");
}

/// Unicode sensor ID.
#[test]
fn render_unicode_sensor_id_in_highlight() {
    let highlights = vec![make_highlight(
        "sensör",
        make_finding(Severity::Error, "E1", "error"),
    )];
    let report = report_with_highlights(highlights);
    let cfg = CockpitConfig::default();
    let md = render_comment(&report, &cfg);

    assert!(md.contains("sensör"), "unicode sensor_id should appear");
}

// ============================================================================
// Long file paths
// ============================================================================

/// Very long file path in finding location → rendered without crash.
#[test]
fn render_very_long_file_path() {
    let long_path = "a/".repeat(500) + "file.rs";
    let highlights = vec![make_highlight(
        "sensor",
        make_finding_with_location(Severity::Error, "E1", "error", &long_path, 1),
    )];
    let report = report_with_highlights(highlights);
    let cfg = CockpitConfig::default();
    let md = render_comment(&report, &cfg);

    assert!(md.contains("file.rs"), "long path should be present");
    assert!(md.len() > 1000, "output should contain the long path");
}

// ============================================================================
// Special markdown characters
// ============================================================================

/// Special markdown characters in messages → no rendering breakage.
#[test]
fn render_special_markdown_chars_in_message() {
    let highlights = vec![make_highlight(
        "sensor",
        make_finding(
            Severity::Error,
            "E1",
            "Failed: `foo` **bold** _italic_ [link](http://x) | pipe | <tag>",
        ),
    )];
    let report = report_with_highlights(highlights);
    let cfg = CockpitConfig::default();
    let md = render_comment(&report, &cfg);

    // The message should appear in the output (renderer doesn't escape markdown).
    assert!(md.contains("Failed:"), "message should be present");
    // Should not crash or produce empty output.
    assert!(md.contains("cockpit:end"), "should complete rendering");
}

/// Newlines in finding messages → replaced (not breaking markdown structure).
#[test]
fn render_newlines_in_message_replaced() {
    let highlights = vec![make_highlight(
        "sensor",
        make_finding(Severity::Warn, "W1", "line1\nline2\nline3"),
    )];
    let report = report_with_highlights(highlights);
    let cfg = CockpitConfig::default();
    let md = render_comment(&report, &cfg);

    // Newlines in messages should be replaced with spaces in highlight lines.
    assert!(
        md.contains("line1 line2 line3"),
        "newlines in messages should be replaced with spaces"
    );
}

/// Pipe character in sensor summary notes → table row still renders.
#[test]
fn render_pipe_in_report_path() {
    let mut report = empty_report();
    report.sensors.push(SensorSummary {
        id: "test".to_string(),
        blocking: false,
        missing: MissingPolicy::Skip,
        presence: Presence::Present,
        report_path: "artifacts/test|weird/report.json".to_string(),
        comment_path: None,
        verdict: Verdict {
            status: VerdictStatus::Pass,
            counts: VerdictCounts::default(),
            reasons: vec![],
        },
        truncated: false,
        errors: vec![],
        missing_policy_applied: None,
        policy_outcome: None,
    });
    let cfg = CockpitConfig::default();
    let md = render_comment(&report, &cfg);

    // Should not crash; the table row is present.
    assert!(md.contains("test"), "sensor should appear in output");
    assert!(md.contains("cockpit:end"));
}

// ============================================================================
// Annotations edge cases
// ============================================================================

/// Annotations with no findings → "No annotations." message.
#[test]
fn annotations_no_findings_minimal() {
    let cfg = CockpitConfig::default();
    let blocking = BTreeMap::new();
    let result = render_annotations(&[], &cfg, &blocking);
    assert!(result.content.contains("_No annotations._"));
    assert_eq!(result.rendered_count, 0);
    assert!(!result.truncated);
}

/// Annotations preserve deterministic order: severity desc, blocking first.
#[test]
fn annotations_deterministic_order() {
    let highlights = vec![
        make_highlight("info-sensor", make_finding(Severity::Info, "I1", "info")),
        make_highlight("error-sensor", make_finding(Severity::Error, "E1", "error")),
        make_highlight("warn-sensor", make_finding(Severity::Warn, "W1", "warn")),
    ];
    let cfg = CockpitConfig::default();
    let blocking = BTreeMap::new();

    let result = render_annotations(&highlights, &cfg, &blocking);
    let lines: Vec<&str> = result.content.lines().collect();

    // Error should come first (severity_rank 0), then warn (1), then info (2).
    assert!(
        lines[0].contains("E1"),
        "first annotation should be error: {}",
        lines[0]
    );
    assert!(
        lines[1].contains("W1"),
        "second annotation should be warn: {}",
        lines[1]
    );
    assert!(
        lines[2].contains("I1"),
        "third annotation should be info: {}",
        lines[2]
    );
}

// ============================================================================
// append_comment_sections edge cases
// ============================================================================

/// Appending to comment with no end marker → appended at end.
#[test]
fn append_sections_no_end_marker() {
    use cockpitctl_render::append_comment_sections;

    let base = "## Cockpit\nSome content";
    let sections = vec![("Extra".to_string(), "More info".to_string())];
    let result = append_comment_sections(base, &sections);
    assert!(result.contains("### Extra"));
    assert!(result.contains("More info"));
}

/// Appending empty sections → unchanged.
#[test]
fn append_sections_empty_is_noop() {
    use cockpitctl_render::append_comment_sections;

    let base = "<!-- cockpit:begin -->\n## Cockpit\n<!-- cockpit:end -->";
    let result = append_comment_sections(base, &[]);
    assert_eq!(result, base);
}

// ============================================================================
// Render with sensors in sections
// ============================================================================

/// Report with sensor in section_order → section header rendered.
#[test]
fn render_sensor_in_configured_section() {
    let mut report = empty_report();
    report.sensors.push(SensorSummary {
        id: "builddiag".to_string(),
        blocking: true,
        missing: MissingPolicy::Skip,
        presence: Presence::Present,
        report_path: "artifacts/builddiag/report.json".to_string(),
        comment_path: None,
        verdict: Verdict {
            status: VerdictStatus::Pass,
            counts: VerdictCounts::default(),
            reasons: vec![],
        },
        truncated: false,
        errors: vec![],
        missing_policy_applied: None,
        policy_outcome: Some(PolicyOutcome::Allowed),
    });

    let mut cfg = CockpitConfig::default();
    cfg.sensors.insert(
        "builddiag".to_string(),
        SensorPolicy {
            section: Some("Diagnostics".to_string()),
            blocking: true,
            ..Default::default()
        },
    );

    let md = render_comment(&report, &cfg);
    assert!(
        md.contains("### Diagnostics"),
        "section header should appear"
    );
    assert!(
        md.contains("`builddiag`"),
        "sensor should appear in section"
    );
}

/// Report with truncated sensor → _truncated_ note in summary table.
#[test]
fn render_truncated_sensor_note() {
    let mut report = empty_report();
    report.sensors.push(SensorSummary {
        id: "big-sensor".to_string(),
        blocking: false,
        missing: MissingPolicy::Skip,
        presence: Presence::Present,
        report_path: "artifacts/big-sensor/report.json".to_string(),
        comment_path: None,
        verdict: Verdict {
            status: VerdictStatus::Warn,
            counts: VerdictCounts::default(),
            reasons: vec![],
        },
        truncated: true,
        errors: vec![],
        missing_policy_applied: None,
        policy_outcome: None,
    });

    let cfg = CockpitConfig::default();
    let md = render_comment(&report, &cfg);
    assert!(md.contains("_truncated_"), "truncated note should appear");
}
