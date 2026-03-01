//! Regression tests for render budgets, markers, and truncation behavior.
//!
//! Covers:
//! - Highlight/annotation budget enforcement with truncation messages
//! - Comment marker stability (cockpit:begin/end)
//! - Deterministic rendering across repeated runs
//! - Edge cases: zero sensors, all-skip, long messages, Unicode

use cockpitctl_render::{render_annotations, render_comment, render_github_annotations};
use cockpitctl_types::{
    CockpitConfig, CockpitReport, Finding, Highlight, Location, MissingPolicy,
    PolicySensorSnapshot, PolicySnapshot, Presence, RunInfo, SensorPolicy, SensorSummary, Severity,
    ToolInfo, Verdict, VerdictCounts, VerdictStatus,
};
use std::collections::BTreeMap;

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
    path: Option<&str>,
    line: Option<u32>,
) -> Highlight {
    Highlight {
        sensor_id: sensor_id.to_string(),
        finding: Finding {
            severity,
            check_id: None,
            code: code.to_string(),
            message: format!("Message for {}", code),
            location: Some(Location {
                path: path.map(String::from),
                line,
                col: None,
            }),
            help: None,
            url: None,
            fingerprint: None,
            data: None,
        },
    }
}

fn make_highlight_with_message(
    sensor_id: &str,
    code: &str,
    severity: Severity,
    path: Option<&str>,
    line: Option<u32>,
    message: &str,
) -> Highlight {
    Highlight {
        sensor_id: sensor_id.to_string(),
        finding: Finding {
            severity,
            check_id: None,
            code: code.to_string(),
            message: message.to_string(),
            location: Some(Location {
                path: path.map(String::from),
                line,
                col: None,
            }),
            help: None,
            url: None,
            fingerprint: None,
            data: None,
        },
    }
}

fn sensor_summary(
    id: &str,
    status: VerdictStatus,
    blocking: bool,
    truncated: bool,
) -> SensorSummary {
    SensorSummary {
        id: id.to_string(),
        blocking,
        missing: MissingPolicy::Fail,
        presence: Presence::Present,
        report_path: format!("artifacts/{}/report.json", id),
        comment_path: Some(format!("artifacts/{}/comment.md", id)),
        verdict: Verdict {
            status,
            counts: VerdictCounts::default(),
            reasons: vec![],
        },
        truncated,
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

fn default_sensor_cfg() -> SensorPolicy {
    SensorPolicy {
        blocking: true,
        missing: MissingPolicy::Fail,
        section: Some("Lint".to_string()),
        require_label: None,
        repro: None,
    }
}

// ===========================================================================
// Budget enforcement: highlights
// ===========================================================================

#[test]
fn budget_zero_highlights_with_max_five() {
    let mut cfg = CockpitConfig::default();
    cfg.policy.max_highlights = 5;
    cfg.sensors.insert("lint".to_string(), default_sensor_cfg());
    cfg.policy.section_order = vec!["Lint".to_string()];

    let report = make_report(
        &cfg,
        VerdictStatus::Pass,
        vec![sensor_summary("lint", VerdictStatus::Pass, true, false)],
        vec![],
    );

    let md = render_comment(&report, &cfg);
    insta::assert_snapshot!(md);
}

#[test]
fn budget_exact_highlights_no_truncation() {
    let mut cfg = CockpitConfig::default();
    cfg.policy.max_highlights = 3;
    cfg.policy.max_annotations = 3;
    cfg.sensors.insert("lint".to_string(), default_sensor_cfg());
    cfg.policy.section_order = vec!["Lint".to_string()];

    let report = make_report(
        &cfg,
        VerdictStatus::Fail,
        vec![sensor_summary("lint", VerdictStatus::Fail, true, false)],
        vec![
            make_highlight("lint", "E001", Severity::Error, Some("src/a.rs"), Some(1)),
            make_highlight("lint", "E002", Severity::Error, Some("src/b.rs"), Some(2)),
            make_highlight("lint", "W001", Severity::Warn, Some("src/c.rs"), Some(3)),
        ],
    );

    let md = render_comment(&report, &cfg);
    // Exactly 3 highlights with max_highlights=3 → no truncation message
    assert!(
        !md.contains("capped by"),
        "should not contain truncation message when count == max"
    );
    insta::assert_snapshot!(md);
}

#[test]
fn budget_highlights_over_max_shows_truncation() {
    let mut cfg = CockpitConfig::default();
    cfg.policy.max_highlights = 3;
    cfg.policy.max_annotations = 3;
    cfg.sensors.insert("lint".to_string(), default_sensor_cfg());
    cfg.policy.section_order = vec!["Lint".to_string()];

    let report = make_report(
        &cfg,
        VerdictStatus::Fail,
        vec![sensor_summary("lint", VerdictStatus::Fail, true, false)],
        vec![
            make_highlight("lint", "E001", Severity::Error, Some("src/a.rs"), Some(1)),
            make_highlight("lint", "E002", Severity::Error, Some("src/b.rs"), Some(2)),
            make_highlight("lint", "W001", Severity::Warn, Some("src/c.rs"), Some(3)),
            make_highlight("lint", "W002", Severity::Warn, Some("src/d.rs"), Some(4)),
            make_highlight("lint", "I001", Severity::Info, Some("src/e.rs"), Some(5)),
        ],
    );

    let md = render_comment(&report, &cfg);
    insta::assert_snapshot!(md);
}

// ===========================================================================
// Budget enforcement: annotations
// ===========================================================================

#[test]
fn budget_annotations_over_max_shows_truncation() {
    let mut cfg = CockpitConfig::default();
    cfg.policy.max_annotations = 10;
    cfg.sensors
        .insert("scanner".to_string(), default_sensor_cfg());

    let highlights: Vec<Highlight> = (0..20)
        .map(|i| {
            make_highlight(
                "scanner",
                &format!("FIND-{:03}", i),
                Severity::Warn,
                Some("src/lib.rs"),
                Some(i + 1),
            )
        })
        .collect();

    let blocking: BTreeMap<String, bool> = [("scanner".to_string(), true)].into_iter().collect();

    let result = render_annotations(&highlights, &cfg, &blocking);

    assert!(result.truncated, "should be truncated");
    assert_eq!(result.total_count, 20);
    assert_eq!(result.rendered_count, 10);
    assert!(
        result.content.contains("Showing 10 of 20 annotations"),
        "should contain truncation message"
    );
    insta::assert_snapshot!(result.content);
}

#[test]
fn budget_annotations_exact_no_truncation() {
    let mut cfg = CockpitConfig::default();
    cfg.policy.max_annotations = 5;
    cfg.sensors
        .insert("scanner".to_string(), default_sensor_cfg());

    let highlights: Vec<Highlight> = (0..5)
        .map(|i| {
            make_highlight(
                "scanner",
                &format!("FIND-{:03}", i),
                Severity::Error,
                Some("src/main.rs"),
                Some(i + 1),
            )
        })
        .collect();

    let blocking: BTreeMap<String, bool> = [("scanner".to_string(), true)].into_iter().collect();

    let result = render_annotations(&highlights, &cfg, &blocking);

    assert!(!result.truncated, "should not be truncated at exact cap");
    assert_eq!(result.rendered_count, 5);
    assert!(
        !result.content.contains("capped by"),
        "no truncation message when count == max"
    );
}

#[test]
fn budget_github_annotations_over_max() {
    let mut cfg = CockpitConfig::default();
    cfg.policy.max_annotations = 5;

    let highlights: Vec<Highlight> = (0..12)
        .map(|i| {
            make_highlight(
                "lint",
                &format!("GH-{:03}", i),
                Severity::Warn,
                Some("src/lib.rs"),
                Some(i + 1),
            )
        })
        .collect();

    let blocking: BTreeMap<String, bool> = [("lint".to_string(), true)].into_iter().collect();

    let result = render_github_annotations(&highlights, &cfg, &blocking);

    assert!(result.truncated);
    assert_eq!(result.total_count, 12);
    assert_eq!(result.rendered_count, 5);
    assert_eq!(result.lines.len(), 5);
}

// ===========================================================================
// Marker stability
// ===========================================================================

#[test]
fn markers_present_in_output() {
    let cfg = CockpitConfig::default();
    let report = make_report(&cfg, VerdictStatus::Pass, vec![], vec![]);

    let md = render_comment(&report, &cfg);
    assert!(
        md.contains("<!-- cockpit:begin -->"),
        "must contain begin marker"
    );
    assert!(
        md.contains("<!-- cockpit:end -->"),
        "must contain end marker"
    );
}

#[test]
fn markers_present_with_empty_content() {
    let cfg = CockpitConfig::default();
    let report = make_report(&cfg, VerdictStatus::Pass, vec![], vec![]);

    let md = render_comment(&report, &cfg);
    assert!(md.starts_with("<!-- cockpit:begin -->"));
    assert!(md.trim_end().ends_with("<!-- cockpit:end -->"));
    insta::assert_snapshot!(md);
}

#[test]
fn markers_stable_format_across_different_inputs() {
    let cfg = CockpitConfig::default();

    let report_pass = make_report(&cfg, VerdictStatus::Pass, vec![], vec![]);
    let md_pass = render_comment(&report_pass, &cfg);

    let mut cfg_fail = CockpitConfig::default();
    cfg_fail
        .sensors
        .insert("lint".to_string(), default_sensor_cfg());
    cfg_fail.policy.section_order = vec!["Lint".to_string()];
    let report_fail = make_report(
        &cfg_fail,
        VerdictStatus::Fail,
        vec![sensor_summary("lint", VerdictStatus::Fail, true, false)],
        vec![make_highlight(
            "lint",
            "E001",
            Severity::Error,
            Some("src/a.rs"),
            Some(1),
        )],
    );
    let md_fail = render_comment(&report_fail, &cfg_fail);

    // Both outputs use identical marker format
    let begin_pass = md_pass.lines().next().unwrap();
    let begin_fail = md_fail.lines().next().unwrap();
    assert_eq!(begin_pass, begin_fail, "begin markers must be identical");

    let end_pass = md_pass
        .lines()
        .rev()
        .find(|l| l.contains("cockpit:end"))
        .unwrap();
    let end_fail = md_fail
        .lines()
        .rev()
        .find(|l| l.contains("cockpit:end"))
        .unwrap();
    assert_eq!(end_pass, end_fail, "end markers must be identical");
}

// ===========================================================================
// Deterministic rendering
// ===========================================================================

#[test]
fn deterministic_rendering_100_iterations() {
    let mut cfg = CockpitConfig::default();
    cfg.policy.max_highlights = 5;
    cfg.policy.max_annotations = 5;
    cfg.sensors.insert("lint".to_string(), default_sensor_cfg());
    cfg.policy.section_order = vec!["Lint".to_string()];

    let report = make_report(
        &cfg,
        VerdictStatus::Fail,
        vec![sensor_summary("lint", VerdictStatus::Fail, true, false)],
        vec![
            make_highlight("lint", "E001", Severity::Error, Some("src/a.rs"), Some(10)),
            make_highlight("lint", "W001", Severity::Warn, Some("src/b.rs"), Some(20)),
            make_highlight("lint", "I001", Severity::Info, Some("src/c.rs"), Some(30)),
        ],
    );

    let reference = render_comment(&report, &cfg);
    for i in 0..100 {
        let rendered = render_comment(&report, &cfg);
        assert_eq!(
            reference, rendered,
            "render_comment must be deterministic (iteration {})",
            i
        );
    }
}

#[test]
fn highlights_sorted_severity_desc_blocking_first() {
    let mut cfg = CockpitConfig::default();
    cfg.policy.max_highlights = 10;
    cfg.policy.max_annotations = 10;
    cfg.sensors.insert(
        "blocking-sensor".to_string(),
        SensorPolicy {
            blocking: true,
            section: Some("A".to_string()),
            ..default_sensor_cfg()
        },
    );
    cfg.sensors.insert(
        "non-blocking".to_string(),
        SensorPolicy {
            blocking: false,
            section: Some("B".to_string()),
            ..default_sensor_cfg()
        },
    );
    cfg.policy.section_order = vec!["A".to_string(), "B".to_string()];

    // Supply highlights out of order to verify sorting
    let report = make_report(
        &cfg,
        VerdictStatus::Fail,
        vec![
            sensor_summary("blocking-sensor", VerdictStatus::Fail, true, false),
            sensor_summary("non-blocking", VerdictStatus::Warn, false, false),
        ],
        vec![
            // Info from blocking (should be last in its severity tier)
            make_highlight(
                "blocking-sensor",
                "I001",
                Severity::Info,
                Some("src/z.rs"),
                Some(99),
            ),
            // Error from non-blocking (severity beats blocking)
            make_highlight(
                "non-blocking",
                "E001",
                Severity::Error,
                Some("src/a.rs"),
                Some(1),
            ),
            // Error from blocking (same severity, blocking first)
            make_highlight(
                "blocking-sensor",
                "E002",
                Severity::Error,
                Some("src/b.rs"),
                Some(2),
            ),
            // Warn from non-blocking
            make_highlight(
                "non-blocking",
                "W001",
                Severity::Warn,
                Some("src/c.rs"),
                Some(3),
            ),
        ],
    );

    let md = render_comment(&report, &cfg);
    insta::assert_snapshot!(md);
}

#[test]
fn summary_table_rows_deterministic_order() {
    let mut cfg = CockpitConfig::default();
    cfg.sensors.insert(
        "alpha".to_string(),
        SensorPolicy {
            section: Some("A".to_string()),
            ..default_sensor_cfg()
        },
    );
    cfg.sensors.insert(
        "bravo".to_string(),
        SensorPolicy {
            section: Some("B".to_string()),
            ..default_sensor_cfg()
        },
    );
    cfg.sensors.insert(
        "charlie".to_string(),
        SensorPolicy {
            section: Some("C".to_string()),
            ..default_sensor_cfg()
        },
    );
    cfg.policy.section_order = vec!["A".to_string(), "B".to_string(), "C".to_string()];

    let report = make_report(
        &cfg,
        VerdictStatus::Pass,
        vec![
            sensor_summary("alpha", VerdictStatus::Pass, true, false),
            sensor_summary("bravo", VerdictStatus::Pass, true, false),
            sensor_summary("charlie", VerdictStatus::Pass, true, false),
        ],
        vec![],
    );

    let md = render_comment(&report, &cfg);
    // Verify ordering: alpha before bravo before charlie in summary table
    let alpha_pos = md.find("`alpha`").expect("alpha present");
    let bravo_pos = md.find("`bravo`").expect("bravo present");
    let charlie_pos = md.find("`charlie`").expect("charlie present");
    assert!(
        alpha_pos < bravo_pos && bravo_pos < charlie_pos,
        "summary table rows must follow sensor order"
    );
    insta::assert_snapshot!(md);
}

// ===========================================================================
// Edge cases
// ===========================================================================

#[test]
fn edge_zero_sensors_minimal_comment() {
    let cfg = CockpitConfig::default();
    let report = make_report(&cfg, VerdictStatus::Pass, vec![], vec![]);

    let md = render_comment(&report, &cfg);
    assert!(md.contains("cockpit:begin"), "must have begin marker");
    assert!(md.contains("cockpit:end"), "must have end marker");
    assert!(md.contains("## Cockpit"), "must have heading");
    insta::assert_snapshot!(md);
}

#[test]
fn edge_all_sensors_skip() {
    let mut cfg = CockpitConfig::default();
    cfg.sensors.insert(
        "optional-a".to_string(),
        SensorPolicy {
            blocking: false,
            missing: MissingPolicy::Skip,
            section: Some("Optional".to_string()),
            require_label: None,
            repro: None,
        },
    );
    cfg.sensors.insert(
        "optional-b".to_string(),
        SensorPolicy {
            blocking: false,
            missing: MissingPolicy::Skip,
            section: Some("Optional".to_string()),
            require_label: None,
            repro: None,
        },
    );
    cfg.policy.section_order = vec!["Optional".to_string()];

    let report = make_report(
        &cfg,
        VerdictStatus::Pass,
        vec![
            sensor_summary("optional-a", VerdictStatus::Skip, false, false),
            sensor_summary("optional-b", VerdictStatus::Skip, false, false),
        ],
        vec![],
    );

    let md = render_comment(&report, &cfg);
    assert!(md.contains("skip"), "should show skip status");
    insta::assert_snapshot!(md);
}

#[test]
fn edge_very_long_finding_message() {
    let mut cfg = CockpitConfig::default();
    cfg.policy.max_highlights = 5;
    cfg.policy.max_annotations = 5;
    cfg.sensors.insert("lint".to_string(), default_sensor_cfg());
    cfg.policy.section_order = vec!["Lint".to_string()];

    let long_message = "A".repeat(5000);
    let report = make_report(
        &cfg,
        VerdictStatus::Fail,
        vec![sensor_summary("lint", VerdictStatus::Fail, true, false)],
        vec![make_highlight_with_message(
            "lint",
            "LONG-001",
            Severity::Error,
            Some("src/big.rs"),
            Some(1),
            &long_message,
        )],
    );

    let md = render_comment(&report, &cfg);
    // Must produce finite output, contain markers, and include the code
    assert!(md.contains("cockpit:begin"));
    assert!(md.contains("cockpit:end"));
    assert!(md.contains("LONG-001"));
    assert!(md.len() < 100_000, "output must be bounded");
}

#[test]
fn edge_empty_findings_clean_output() {
    let mut cfg = CockpitConfig::default();
    cfg.sensors.insert("lint".to_string(), default_sensor_cfg());
    cfg.policy.section_order = vec!["Lint".to_string()];

    let report = make_report(
        &cfg,
        VerdictStatus::Pass,
        vec![sensor_summary("lint", VerdictStatus::Pass, true, false)],
        vec![],
    );

    let md = render_comment(&report, &cfg);
    assert!(
        md.contains("No highlights"),
        "should indicate no highlights"
    );
    insta::assert_snapshot!(md);
}

#[test]
fn edge_unicode_in_messages_preserved() {
    let mut cfg = CockpitConfig::default();
    cfg.policy.max_highlights = 5;
    cfg.policy.max_annotations = 5;
    cfg.sensors.insert("lint".to_string(), default_sensor_cfg());
    cfg.policy.section_order = vec!["Lint".to_string()];

    let report = make_report(
        &cfg,
        VerdictStatus::Warn,
        vec![sensor_summary("lint", VerdictStatus::Warn, true, false)],
        vec![
            make_highlight_with_message(
                "lint",
                "UNI-001",
                Severity::Warn,
                Some("src/i18n.rs"),
                Some(42),
                "Ошибка: неверный формат — 日本語テスト 🚀",
            ),
            make_highlight_with_message(
                "lint",
                "UNI-002",
                Severity::Info,
                Some("src/emoji.rs"),
                Some(7),
                "Check passed ✅ with résumé and naïve café",
            ),
        ],
    );

    let md = render_comment(&report, &cfg);
    assert!(md.contains("Ошибка"), "Cyrillic must be preserved");
    assert!(md.contains("日本語"), "CJK must be preserved");
    assert!(md.contains("🚀"), "emoji must be preserved");
    assert!(md.contains("résumé"), "accented chars must be preserved");
    insta::assert_snapshot!(md);
}

#[test]
fn edge_newlines_in_finding_message_flattened() {
    let mut cfg = CockpitConfig::default();
    cfg.policy.max_highlights = 5;
    cfg.policy.max_annotations = 5;
    cfg.sensors.insert("lint".to_string(), default_sensor_cfg());
    cfg.policy.section_order = vec!["Lint".to_string()];

    let report = make_report(
        &cfg,
        VerdictStatus::Fail,
        vec![sensor_summary("lint", VerdictStatus::Fail, true, false)],
        vec![make_highlight_with_message(
            "lint",
            "NL-001",
            Severity::Error,
            Some("src/main.rs"),
            Some(10),
            "first line\nsecond line\nthird line",
        )],
    );

    let md = render_comment(&report, &cfg);
    // The renderer replaces newlines with spaces in messages
    assert!(
        md.contains("first line second line third line"),
        "newlines should be flattened to spaces"
    );
    insta::assert_snapshot!(md);
}
