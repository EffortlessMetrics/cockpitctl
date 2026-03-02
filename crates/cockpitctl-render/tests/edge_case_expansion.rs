//! Edge-case and stress tests for cockpitctl-render.
//!
//! Covers:
//! - Annotation rendering edge cases (very long paths, no-file, line 0, max_annotations=0)
//! - Comment budget stress tests (max_highlights=0, very large budgets, fractional budgets)
//! - Stable marker regression tests (BEGIN/END round-trips, special characters)
//! - Template rendering with special data (empty sensor names, very long names, all verdict states)

use std::collections::BTreeMap;

use cockpitctl_render::{
    append_comment_sections, render_annotations, render_comment, render_github_annotations,
};
use cockpitctl_types::{
    CockpitConfig, CockpitReport, Finding, Highlight, Location, MissingPolicy,
    PolicySensorSnapshot, PolicySnapshot, Presence, RunInfo, SensorPolicy, SensorSummary, Severity,
    ToolInfo, Verdict, VerdictCounts, VerdictStatus,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn tool_info() -> ToolInfo {
    ToolInfo {
        name: "cockpitctl".to_string(),
        version: "0.2.0".to_string(),
        commit: None,
    }
}

fn run_info() -> RunInfo {
    RunInfo {
        started_at: "2026-02-01T00:00:00Z".to_string(),
        ended_at: None,
        duration_ms: None,
        host: None,
        git: None,
        ci: None,
        capabilities: BTreeMap::new(),
    }
}

fn policy_snapshot_from_cfg(cfg: &CockpitConfig) -> PolicySnapshot {
    let mut sensors = Vec::new();
    for (id, p) in cfg.sensors.iter() {
        sensors.push(PolicySensorSnapshot {
            id: id.clone(),
            blocking: p.blocking,
            missing: p.missing,
            section: p.section.clone(),
            require_label: p.require_label.clone(),
            repro: p.repro.clone(),
        });
    }
    PolicySnapshot {
        warn_is_fail: cfg.policy.warn_is_fail,
        max_highlights: cfg.policy.max_highlights,
        max_per_sensor_findings: cfg.policy.max_per_sensor_findings,
        max_annotations: cfg.policy.max_annotations,
        section_order: cfg.policy.section_order.clone(),
        sensors,
    }
}

fn make_highlight(
    sensor_id: &str,
    code: &str,
    severity: Severity,
    message: &str,
    path: Option<&str>,
    line: Option<u32>,
    col: Option<u32>,
) -> Highlight {
    Highlight {
        sensor_id: sensor_id.to_string(),
        finding: Finding {
            severity,
            check_id: None,
            code: code.to_string(),
            message: message.to_string(),
            location: if path.is_some() || line.is_some() || col.is_some() {
                Some(Location {
                    path: path.map(String::from),
                    line,
                    col,
                })
            } else {
                None
            },
            help: None,
            url: None,
            fingerprint: None,
            data: None,
        },
    }
}

fn sensor_summary(id: &str, status: VerdictStatus, blocking: bool) -> SensorSummary {
    SensorSummary {
        id: id.to_string(),
        blocking,
        missing: MissingPolicy::Fail,
        presence: Presence::Present,
        report_path: format!("artifacts/{}/report.json", id),
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

fn make_report(
    cfg: &CockpitConfig,
    verdict_status: VerdictStatus,
    sensors: Vec<SensorSummary>,
    highlights: Vec<Highlight>,
) -> CockpitReport {
    CockpitReport {
        schema: "cockpit.report.v1".to_string(),
        tool: tool_info(),
        run: run_info(),
        verdict: Verdict {
            status: verdict_status,
            counts: VerdictCounts::default(),
            reasons: vec![],
        },
        sensors,
        highlights,
        policy: policy_snapshot_from_cfg(cfg),
        data: None,
    }
}

fn default_cfg_with_budgets(max_highlights: usize, max_annotations: usize) -> CockpitConfig {
    let mut cfg = CockpitConfig::default();
    cfg.policy.max_highlights = max_highlights;
    cfg.policy.max_annotations = max_annotations;
    cfg
}

// ===========================================================================
// 1. Annotation rendering edge cases
// ===========================================================================

#[test]
fn annotation_very_long_path() {
    let long_path = "a/".repeat(500) + "file.rs";
    let highlights = vec![make_highlight(
        "sensor",
        "E001",
        Severity::Error,
        "error msg",
        Some(&long_path),
        Some(1),
        None,
    )];
    let cfg = default_cfg_with_budgets(5, 10);
    let blocking = BTreeMap::new();
    let result = render_annotations(&highlights, &cfg, &blocking);
    assert_eq!(result.total_count, 1);
    assert_eq!(result.rendered_count, 1);
    assert!(result.content.contains(&long_path));
}

#[test]
fn annotation_no_file_location() {
    let highlights = vec![make_highlight(
        "sensor",
        "E002",
        Severity::Warn,
        "no file here",
        None,
        None,
        None,
    )];
    let cfg = default_cfg_with_budgets(5, 10);
    let blocking = BTreeMap::new();
    let result = render_annotations(&highlights, &cfg, &blocking);
    assert_eq!(result.rendered_count, 1);
    // No " at `" location string should appear
    assert!(!result.content.contains(" at `"));
    assert!(result.content.contains("E002"));
}

#[test]
fn annotation_line_zero() {
    let highlights = vec![make_highlight(
        "sensor",
        "E003",
        Severity::Info,
        "line zero",
        Some("src/lib.rs"),
        Some(0),
        None,
    )];
    let cfg = default_cfg_with_budgets(5, 10);
    let blocking = BTreeMap::new();
    let result = render_annotations(&highlights, &cfg, &blocking);
    assert!(result.content.contains("src/lib.rs:0"));
}

#[test]
fn annotation_max_annotations_zero_truncates_all() {
    let highlights = vec![
        make_highlight(
            "s",
            "C1",
            Severity::Error,
            "m1",
            Some("a.rs"),
            Some(1),
            None,
        ),
        make_highlight("s", "C2", Severity::Warn, "m2", Some("b.rs"), Some(2), None),
    ];
    let cfg = default_cfg_with_budgets(5, 0);
    let blocking = BTreeMap::new();
    let result = render_annotations(&highlights, &cfg, &blocking);
    assert!(result.truncated);
    assert_eq!(result.rendered_count, 0);
    assert_eq!(result.total_count, 2);
    assert!(result.content.contains("Showing 0 of 2 annotations"));
}

#[test]
fn github_annotation_max_zero_renders_nothing() {
    let highlights = vec![make_highlight(
        "sensor",
        "E001",
        Severity::Error,
        "msg",
        Some("a.rs"),
        Some(1),
        None,
    )];
    let cfg = default_cfg_with_budgets(5, 0);
    let blocking = BTreeMap::new();
    let result = render_github_annotations(&highlights, &cfg, &blocking);
    assert!(result.lines.is_empty());
    assert!(result.truncated);
    assert_eq!(result.rendered_count, 0);
}

#[test]
fn github_annotation_no_file_omits_file_param() {
    let highlights = vec![make_highlight(
        "sensor",
        "E001",
        Severity::Error,
        "msg",
        None,
        None,
        None,
    )];
    let cfg = default_cfg_with_budgets(5, 10);
    let blocking = BTreeMap::new();
    let result = render_github_annotations(&highlights, &cfg, &blocking);
    assert_eq!(result.lines.len(), 1);
    assert!(!result.lines[0].contains("file="));
}

#[test]
fn github_annotation_line_zero() {
    let highlights = vec![make_highlight(
        "sensor",
        "E001",
        Severity::Error,
        "msg",
        Some("src/main.rs"),
        Some(0),
        Some(0),
    )];
    let cfg = default_cfg_with_budgets(5, 10);
    let blocking = BTreeMap::new();
    let result = render_github_annotations(&highlights, &cfg, &blocking);
    assert_eq!(result.lines.len(), 1);
    assert!(result.lines[0].contains("line=0"));
    assert!(result.lines[0].contains("col=0"));
}

#[test]
fn github_annotation_escapes_percent_and_newlines() {
    let highlights = vec![make_highlight(
        "sensor",
        "E001",
        Severity::Error,
        "100% done\nnext line\rcarriage",
        Some("a.rs"),
        Some(1),
        None,
    )];
    let cfg = default_cfg_with_budgets(5, 10);
    let blocking = BTreeMap::new();
    let result = render_github_annotations(&highlights, &cfg, &blocking);
    let line = &result.lines[0];
    assert!(line.contains("100%25 done%0Anext line%0Dcarriage"));
}

// ===========================================================================
// 2. Comment budget stress tests
// ===========================================================================

#[test]
fn budget_max_highlights_zero_shows_no_highlights_message() {
    let cfg = default_cfg_with_budgets(0, 10);
    let report = make_report(
        &cfg,
        VerdictStatus::Pass,
        vec![],
        vec![make_highlight(
            "s",
            "C1",
            Severity::Error,
            "msg",
            Some("a.rs"),
            Some(1),
            None,
        )],
    );
    let md = render_comment(&report, &cfg);
    // With max_highlights=0, still renders the highlights section header
    assert!(md.contains("### Highlights"));
    assert!(md.contains("showing up to **0**"));
}

#[test]
fn budget_very_large_max_highlights() {
    let mut cfg = CockpitConfig::default();
    cfg.policy.max_highlights = 10000;
    cfg.policy.max_annotations = 10000;

    let highlights: Vec<Highlight> = (0..50)
        .map(|i| {
            make_highlight(
                "sensor",
                &format!("C{:04}", i),
                Severity::Warn,
                &format!("message {}", i),
                Some("src/lib.rs"),
                Some(i),
                None,
            )
        })
        .collect();
    let report = make_report(&cfg, VerdictStatus::Warn, vec![], highlights);
    let md = render_comment(&report, &cfg);
    assert!(md.contains("showing up to **10000**"));
    // All 50 highlights should be rendered (under the cap)
    assert!(md.contains("50."));
    assert!(!md.contains("51."));
}

#[test]
fn budget_large_annotation_cap_renders_all() {
    let cfg = default_cfg_with_budgets(5, 10000);
    let highlights: Vec<Highlight> = (0..100)
        .map(|i| {
            make_highlight(
                "sensor",
                &format!("C{:04}", i),
                Severity::Info,
                &format!("msg {}", i),
                Some("f.rs"),
                Some(i),
                None,
            )
        })
        .collect();
    let blocking = BTreeMap::new();
    let result = render_annotations(&highlights, &cfg, &blocking);
    assert!(!result.truncated);
    assert_eq!(result.rendered_count, 100);
}

// ===========================================================================
// 3. Stable marker regression tests
// ===========================================================================

#[test]
fn markers_present_in_empty_report() {
    let cfg = CockpitConfig::default();
    let report = make_report(&cfg, VerdictStatus::Pass, vec![], vec![]);
    let md = render_comment(&report, &cfg);
    assert!(md.starts_with("<!-- cockpit:begin -->\n"));
    assert!(md.ends_with("<!-- cockpit:end -->\n"));
}

#[test]
fn markers_survive_round_trip_through_append() {
    let cfg = CockpitConfig::default();
    let report = make_report(&cfg, VerdictStatus::Pass, vec![], vec![]);
    let md = render_comment(&report, &cfg);

    let sections = vec![("Extra".to_string(), "some content".to_string())];
    let appended = append_comment_sections(&md, &sections);

    assert!(appended.contains("<!-- cockpit:begin -->"));
    assert!(appended.contains("<!-- cockpit:end -->"));
    assert!(appended.contains("### Extra"));

    // End marker must come after begin marker
    let begin_pos = appended.find("<!-- cockpit:begin -->").unwrap();
    let end_pos = appended.find("<!-- cockpit:end -->").unwrap();
    assert!(end_pos > begin_pos);
}

#[test]
fn markers_survive_double_append() {
    let cfg = CockpitConfig::default();
    let report = make_report(&cfg, VerdictStatus::Pass, vec![], vec![]);
    let md = render_comment(&report, &cfg);

    let sections1 = vec![("First".to_string(), "content1".to_string())];
    let appended1 = append_comment_sections(&md, &sections1);

    let sections2 = vec![("Second".to_string(), "content2".to_string())];
    let appended2 = append_comment_sections(&appended1, &sections2);

    assert!(appended2.contains("### First"));
    assert!(appended2.contains("### Second"));
    assert!(appended2.contains("<!-- cockpit:begin -->"));
    assert!(appended2.contains("<!-- cockpit:end -->"));
}

#[test]
fn append_empty_sections_is_identity() {
    let cfg = CockpitConfig::default();
    let report = make_report(&cfg, VerdictStatus::Pass, vec![], vec![]);
    let md = render_comment(&report, &cfg);
    let appended = append_comment_sections(&md, &[]);
    assert_eq!(md, appended);
}

#[test]
fn append_special_characters_in_section_name() {
    let base = "<!-- cockpit:begin -->\n## Cockpit\n<!-- cockpit:end -->";
    let sections = vec![(
        "Notes <script>alert('xss')</script>".to_string(),
        "safe content".to_string(),
    )];
    let result = append_comment_sections(base, &sections);
    assert!(result.contains("<!-- cockpit:end -->"));
    // The section name is rendered as-is (markdown, not HTML sanitized)
    assert!(result.contains("<script>"));
}

#[test]
fn append_without_end_marker_appends_at_end() {
    let base = "<!-- cockpit:begin -->\n## Cockpit\nno end marker here";
    let sections = vec![("Tail".to_string(), "tail content".to_string())];
    let result = append_comment_sections(base, &sections);
    assert!(result.contains("### Tail"));
    assert!(result.contains("tail content"));
}

// ===========================================================================
// 4. Template rendering with special data
// ===========================================================================

#[test]
fn render_with_empty_sensor_name() {
    let mut cfg = CockpitConfig::default();
    cfg.sensors.insert(
        "".to_string(),
        SensorPolicy {
            blocking: true,
            missing: MissingPolicy::Fail,
            section: Some("Lint".to_string()),
            require_label: None,
            repro: None,
        },
    );
    cfg.policy.section_order = vec!["Lint".to_string()];

    let report = make_report(
        &cfg,
        VerdictStatus::Pass,
        vec![sensor_summary("", VerdictStatus::Pass, true)],
        vec![],
    );
    let md = render_comment(&report, &cfg);
    assert!(md.contains("``")); // empty sensor id renders as empty backtick pair
    assert!(md.contains("<!-- cockpit:begin -->"));
    assert!(md.contains("<!-- cockpit:end -->"));
}

#[test]
fn render_with_very_long_sensor_name() {
    let long_name = "a".repeat(1000);
    let cfg = CockpitConfig::default();
    let report = make_report(
        &cfg,
        VerdictStatus::Pass,
        vec![sensor_summary(&long_name, VerdictStatus::Pass, false)],
        vec![],
    );
    let md = render_comment(&report, &cfg);
    assert!(md.contains(&long_name));
}

#[test]
fn render_all_verdict_states_in_sensor_table() {
    let cfg = CockpitConfig::default();
    let sensors = vec![
        sensor_summary("pass_sensor", VerdictStatus::Pass, false),
        sensor_summary("warn_sensor", VerdictStatus::Warn, false),
        sensor_summary("fail_sensor", VerdictStatus::Fail, true),
        sensor_summary("skip_sensor", VerdictStatus::Skip, false),
    ];
    let report = make_report(&cfg, VerdictStatus::Fail, sensors, vec![]);
    let md = render_comment(&report, &cfg);
    assert!(md.contains("✅ pass"));
    assert!(md.contains("⚠️ warn"));
    assert!(md.contains("❌ fail"));
    assert!(md.contains("⏭ skip"));
}

#[test]
fn render_unicode_in_finding_message() {
    let cfg = default_cfg_with_budgets(5, 10);
    let highlights = vec![make_highlight(
        "sensor",
        "U001",
        Severity::Error,
        "变量未使用 • émoji 🎉 • αβγδ",
        Some("src/main.rs"),
        Some(1),
        None,
    )];
    let report = make_report(&cfg, VerdictStatus::Warn, vec![], highlights);
    let md = render_comment(&report, &cfg);
    assert!(md.contains("变量未使用"));
    assert!(md.contains("🎉"));
    assert!(md.contains("αβγδ"));
}

#[test]
fn render_newline_in_finding_message_is_flattened() {
    let cfg = default_cfg_with_budgets(5, 10);
    let highlights = vec![make_highlight(
        "sensor",
        "N001",
        Severity::Warn,
        "line one\nline two\nline three",
        Some("f.rs"),
        Some(1),
        None,
    )];
    let blocking = BTreeMap::new();
    let result = render_annotations(&highlights, &cfg, &blocking);
    // Newlines in messages are replaced with spaces
    assert!(result.content.contains("line one line two line three"));
    assert!(!result.content.contains("line one\nline two"));
}

#[test]
fn render_empty_report_has_no_highlights_message() {
    let cfg = CockpitConfig::default();
    let report = make_report(&cfg, VerdictStatus::Pass, vec![], vec![]);
    let md = render_comment(&report, &cfg);
    assert!(md.contains("_No highlights._"));
}

#[test]
fn render_highlights_numbered_sequentially() {
    let cfg = default_cfg_with_budgets(10, 10);
    let highlights: Vec<Highlight> = (1..=5)
        .map(|i| {
            make_highlight(
                "sensor",
                &format!("C{}", i),
                Severity::Error,
                &format!("msg {}", i),
                Some("f.rs"),
                Some(i),
                None,
            )
        })
        .collect();
    let report = make_report(&cfg, VerdictStatus::Fail, vec![], highlights);
    let md = render_comment(&report, &cfg);
    for i in 1..=5 {
        assert!(md.contains(&format!("{}. ", i)));
    }
}

// ===========================================================================
// 5. Additional stress / edge cases
// ===========================================================================

#[test]
fn annotation_with_path_but_no_line() {
    let highlights = vec![make_highlight(
        "sensor",
        "E001",
        Severity::Error,
        "msg",
        Some("src/lib.rs"),
        None,
        None,
    )];
    let cfg = default_cfg_with_budgets(5, 10);
    let blocking = BTreeMap::new();
    let result = render_annotations(&highlights, &cfg, &blocking);
    // Path without line: "at `src/lib.rs`" not "at `src/lib.rs:`"
    assert!(result.content.contains("at `src/lib.rs`"));
    assert!(!result.content.contains("src/lib.rs:"));
}

#[test]
fn annotation_with_only_line_no_path() {
    let h = Highlight {
        sensor_id: "sensor".to_string(),
        finding: Finding {
            severity: Severity::Error,
            check_id: None,
            code: "E001".to_string(),
            message: "msg".to_string(),
            location: Some(Location {
                path: None,
                line: Some(42),
                col: None,
            }),
            help: None,
            url: None,
            fingerprint: None,
            data: None,
        },
    };
    let cfg = default_cfg_with_budgets(5, 10);
    let blocking = BTreeMap::new();
    let result = render_annotations(&[h], &cfg, &blocking);
    // Line without path: "at `:42`"
    assert!(result.content.contains("at `:42`"));
}

#[test]
fn github_annotations_many_findings_respects_cap() {
    let highlights: Vec<Highlight> = (0..200)
        .map(|i| {
            make_highlight(
                "sensor",
                &format!("C{:04}", i),
                Severity::Warn,
                &format!("msg {}", i),
                Some("f.rs"),
                Some(i),
                None,
            )
        })
        .collect();
    let cfg = default_cfg_with_budgets(5, 50);
    let blocking = BTreeMap::new();
    let result = render_github_annotations(&highlights, &cfg, &blocking);
    assert_eq!(result.lines.len(), 50);
    assert!(result.truncated);
    assert_eq!(result.total_count, 200);
    assert_eq!(result.rendered_count, 50);
}

#[test]
fn empty_annotations_renders_no_annotations_message() {
    let cfg = default_cfg_with_budgets(5, 10);
    let blocking = BTreeMap::new();
    let result = render_annotations(&[], &cfg, &blocking);
    assert!(!result.truncated);
    assert_eq!(result.total_count, 0);
    assert_eq!(result.rendered_count, 0);
    assert!(result.content.contains("_No annotations._"));
}
