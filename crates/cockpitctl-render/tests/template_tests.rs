//! Tests for template/comment rendering via `render_comment`.
//!
//! Covers: full report rendering, summary totals, findings budget,
//! sensor table, version info, special characters, and section ordering.

use std::collections::BTreeMap;

use cockpitctl_render::render_comment;
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
        version: "0.3.0".to_string(),
        commit: None,
    }
}

fn run_info() -> RunInfo {
    RunInfo {
        started_at: "2026-03-01T00:00:00Z".to_string(),
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

fn make_sensor(
    id: &str,
    status: VerdictStatus,
    blocking: bool,
    comment_path: Option<&str>,
    truncated: bool,
) -> SensorSummary {
    SensorSummary {
        id: id.to_string(),
        blocking,
        missing: MissingPolicy::Fail,
        presence: Presence::Present,
        report_path: format!("artifacts/{}/report.json", id),
        comment_path: comment_path.map(String::from),
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

fn make_highlight(
    sensor_id: &str,
    code: &str,
    path: Option<&str>,
    line: Option<u32>,
    severity: Severity,
    message: &str,
) -> Highlight {
    Highlight {
        sensor_id: sensor_id.to_string(),
        finding: Finding {
            severity,
            check_id: None,
            code: code.to_string(),
            message: message.to_string(),
            location: path.map(|p| Location {
                path: Some(p.to_string()),
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

fn make_report(
    cfg: &CockpitConfig,
    sensors: Vec<SensorSummary>,
    highlights: Vec<Highlight>,
) -> CockpitReport {
    CockpitReport {
        schema: "cockpit.report.v1".to_string(),
        tool: tool_info(),
        run: run_info(),
        verdict: Verdict {
            status: VerdictStatus::Pass,
            counts: VerdictCounts::default(),
            reasons: vec![],
        },
        sensors,
        highlights,
        policy: policy_snapshot_from_cfg(cfg),
        data: None,
    }
}

// ===========================================================================
// 1. All template sections are rendered for a full report
// ===========================================================================

#[test]
fn full_report_renders_all_sections() {
    let mut cfg = CockpitConfig::default();
    cfg.policy.max_highlights = 5;
    cfg.policy.max_annotations = 5;
    cfg.sensors.insert(
        "lint".to_string(),
        SensorPolicy {
            blocking: true,
            missing: MissingPolicy::Fail,
            section: Some("Diagnostics".to_string()),
            require_label: None,
            repro: Some("cargo clippy".to_string()),
        },
    );

    let sensors = vec![make_sensor(
        "lint",
        VerdictStatus::Fail,
        true,
        Some("artifacts/lint/comment.md"),
        false,
    )];
    let highlights = vec![make_highlight(
        "lint",
        "E001",
        Some("src/main.rs"),
        Some(42),
        Severity::Error,
        "unused import",
    )];

    let report = make_report(&cfg, sensors, highlights);
    let md = render_comment(&report, &cfg);

    // All major sections present
    assert!(md.contains("## Cockpit"));
    assert!(md.contains("### Summary"));
    assert!(md.contains("### Highlights"));
    assert!(md.contains("### Annotations"));
    assert!(md.contains("### Diagnostics"));
}

// ===========================================================================
// 2. Summary section shows correct sensor rows
// ===========================================================================

#[test]
fn summary_shows_all_sensor_rows() {
    let cfg = CockpitConfig::default();
    let sensors = vec![
        make_sensor("alpha", VerdictStatus::Pass, true, None, false),
        make_sensor("beta", VerdictStatus::Fail, false, None, true),
        make_sensor("gamma", VerdictStatus::Warn, true, None, false),
    ];

    let report = make_report(&cfg, sensors, vec![]);
    let md = render_comment(&report, &cfg);

    assert!(md.contains("| `alpha` | ✅ pass | yes |"));
    assert!(md.contains("| `beta` | ❌ fail | no |"));
    assert!(md.contains("| `gamma` | ⚠️ warn | yes |"));
    // Truncated marker for beta
    assert!(md.contains("_truncated_"));
}

#[test]
fn summary_table_has_header() {
    let cfg = CockpitConfig::default();
    let report = make_report(&cfg, vec![], vec![]);
    let md = render_comment(&report, &cfg);

    assert!(md.contains("| Sensor | Status | Blocking | Notes |"));
    assert!(md.contains("|---|---:|---:|---|"));
}

// ===========================================================================
// 3. Findings section respects budget
// ===========================================================================

#[test]
fn highlights_section_respects_max_highlights() {
    let mut cfg = CockpitConfig::default();
    cfg.policy.max_highlights = 2;

    let sensors = vec![make_sensor("s", VerdictStatus::Warn, true, None, false)];
    let highlights = vec![
        make_highlight("s", "E1", Some("a.rs"), Some(1), Severity::Error, "m1"),
        make_highlight("s", "E2", Some("b.rs"), Some(2), Severity::Error, "m2"),
        make_highlight("s", "W1", Some("c.rs"), Some(3), Severity::Warn, "m3"),
    ];

    let report = make_report(&cfg, sensors, highlights);
    let md = render_comment(&report, &cfg);

    assert!(md.contains("(showing up to **2**)"));
}

#[test]
fn annotations_section_respects_max_annotations() {
    let mut cfg = CockpitConfig::default();
    cfg.policy.max_annotations = 1;

    let sensors = vec![make_sensor("s", VerdictStatus::Warn, true, None, false)];
    let highlights = vec![
        make_highlight("s", "E1", Some("a.rs"), Some(1), Severity::Error, "m1"),
        make_highlight("s", "W1", Some("b.rs"), Some(2), Severity::Warn, "m2"),
    ];

    let report = make_report(&cfg, sensors, highlights);
    let md = render_comment(&report, &cfg);

    assert!(md.contains("Showing 1 of 2 annotations"));
}

// ===========================================================================
// 4. Sensor table shows all sensors
// ===========================================================================

#[test]
fn sensor_table_shows_report_and_comment_paths() {
    let cfg = CockpitConfig::default();
    let sensors = vec![make_sensor(
        "test_sensor",
        VerdictStatus::Pass,
        true,
        Some("artifacts/test_sensor/comment.md"),
        false,
    )];

    let report = make_report(&cfg, sensors, vec![]);
    let md = render_comment(&report, &cfg);

    assert!(md.contains("`artifacts/test_sensor/report.json`"));
    assert!(md.contains("`artifacts/test_sensor/comment.md`"));
}

#[test]
fn sensor_without_comment_path_omits_comment_link() {
    let cfg = CockpitConfig::default();
    let sensors = vec![make_sensor(
        "minimal",
        VerdictStatus::Pass,
        true,
        None,
        false,
    )];

    let report = make_report(&cfg, sensors, vec![]);
    let md = render_comment(&report, &cfg);

    // Has report path, no comment path reference for this sensor
    assert!(md.contains("`artifacts/minimal/report.json`"));
    // The line for "minimal" should not contain a comment path separator
    let line = md.lines().find(|l| l.contains("| `minimal` |")).unwrap();
    // No " · `" separator means no comment_path was appended
    assert_eq!(line.matches(" · `").count(), 0);
}

// ===========================================================================
// 5. Version/schema information is included
// ===========================================================================

#[test]
fn comment_includes_cockpit_header() {
    let cfg = CockpitConfig::default();
    let report = make_report(&cfg, vec![], vec![]);
    let md = render_comment(&report, &cfg);

    assert!(md.contains("## Cockpit"));
    assert!(md.contains("generated by `cockpitctl`"));
}

// ===========================================================================
// 6. Template handles special characters in sensor names
// ===========================================================================

#[test]
fn special_characters_in_sensor_names_rendered_safely() {
    let cfg = CockpitConfig::default();
    let sensors = vec![make_sensor(
        "my-sensor_v2.1",
        VerdictStatus::Pass,
        true,
        None,
        false,
    )];

    let report = make_report(&cfg, sensors, vec![]);
    let md = render_comment(&report, &cfg);

    assert!(md.contains("| `my-sensor_v2.1` |"));
}

#[test]
fn sensor_name_with_dots_and_dashes() {
    let cfg = CockpitConfig::default();
    let sensors = vec![
        make_sensor("build.diag", VerdictStatus::Pass, true, None, false),
        make_sensor("code-coverage", VerdictStatus::Warn, false, None, false),
    ];

    let report = make_report(&cfg, sensors, vec![]);
    let md = render_comment(&report, &cfg);

    assert!(md.contains("| `build.diag` |"));
    assert!(md.contains("| `code-coverage` |"));
}

// ===========================================================================
// Extra: section ordering follows config
// ===========================================================================

#[test]
fn sections_rendered_in_config_order() {
    let mut cfg = CockpitConfig::default();
    cfg.policy.section_order = vec!["Quality".to_string(), "Security".to_string()];
    cfg.sensors.insert(
        "sec_sensor".to_string(),
        SensorPolicy {
            blocking: true,
            missing: MissingPolicy::Fail,
            section: Some("Security".to_string()),
            require_label: None,
            repro: None,
        },
    );
    cfg.sensors.insert(
        "qual_sensor".to_string(),
        SensorPolicy {
            blocking: true,
            missing: MissingPolicy::Fail,
            section: Some("Quality".to_string()),
            require_label: None,
            repro: None,
        },
    );

    let sensors = vec![
        make_sensor("sec_sensor", VerdictStatus::Pass, true, None, false),
        make_sensor("qual_sensor", VerdictStatus::Pass, true, None, false),
    ];

    let report = make_report(&cfg, sensors, vec![]);
    let md = render_comment(&report, &cfg);

    let quality_pos = md.find("### Quality").unwrap();
    let security_pos = md.find("### Security").unwrap();
    assert!(
        quality_pos < security_pos,
        "Quality section should appear before Security"
    );
}

#[test]
fn repro_command_rendered_in_section() {
    let mut cfg = CockpitConfig::default();
    cfg.policy.section_order = vec!["Tests".to_string()];
    cfg.sensors.insert(
        "unit_tests".to_string(),
        SensorPolicy {
            blocking: true,
            missing: MissingPolicy::Fail,
            section: Some("Tests".to_string()),
            require_label: None,
            repro: Some("cargo test --workspace".to_string()),
        },
    );

    let sensors = vec![make_sensor(
        "unit_tests",
        VerdictStatus::Fail,
        true,
        None,
        false,
    )];
    let report = make_report(&cfg, sensors, vec![]);
    let md = render_comment(&report, &cfg);

    assert!(md.contains("repro: `cargo test --workspace`"));
}
