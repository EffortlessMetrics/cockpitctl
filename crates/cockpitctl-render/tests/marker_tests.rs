//! Tests for stable markers (`<!-- cockpit:begin -->` / `<!-- cockpit:end -->`).
//!
//! Covers: presence, exact format, wrapping, survival through budget truncation,
//! empty report, no content outside markers, and append_comment_sections.

use std::collections::BTreeMap;

use cockpitctl_render::{append_comment_sections, render_comment};
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

fn make_sensor(id: &str, status: VerdictStatus) -> SensorSummary {
    SensorSummary {
        id: id.to_string(),
        blocking: true,
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

// ===========================================================================
// 1. Comment contains <!-- cockpit:begin --> marker
// ===========================================================================

#[test]
fn comment_contains_begin_marker() {
    let cfg = CockpitConfig::default();
    let report = make_report(&cfg, vec![], vec![]);
    let md = render_comment(&report, &cfg);

    assert!(md.contains("<!-- cockpit:begin -->"));
}

// ===========================================================================
// 2. Comment contains <!-- cockpit:end --> marker
// ===========================================================================

#[test]
fn comment_contains_end_marker() {
    let cfg = CockpitConfig::default();
    let report = make_report(&cfg, vec![], vec![]);
    let md = render_comment(&report, &cfg);

    assert!(md.contains("<!-- cockpit:end -->"));
}

// ===========================================================================
// 3. Markers survive budget truncation
// ===========================================================================

#[test]
fn markers_survive_annotation_budget_truncation() {
    let mut cfg = CockpitConfig::default();
    cfg.policy.max_annotations = 1;
    cfg.policy.max_highlights = 1;

    // Many findings to force truncation
    let highlights: Vec<Highlight> = (0..20)
        .map(|i| Highlight {
            sensor_id: "sensor".to_string(),
            finding: Finding {
                severity: Severity::Error,
                check_id: None,
                code: format!("E{:03}", i),
                message: format!("Error message number {}", i),
                location: Some(Location {
                    path: Some(format!("src/file{}.rs", i)),
                    line: Some(i + 1),
                    col: None,
                }),
                help: None,
                url: None,
                fingerprint: None,
                data: None,
            },
        })
        .collect();

    let sensors = vec![make_sensor("sensor", VerdictStatus::Fail)];
    let report = make_report(&cfg, sensors, highlights);
    let md = render_comment(&report, &cfg);

    assert!(md.contains("<!-- cockpit:begin -->"));
    assert!(md.contains("<!-- cockpit:end -->"));
}

// ===========================================================================
// 4. Markers are present even with empty report
// ===========================================================================

#[test]
fn markers_present_with_empty_report() {
    let cfg = CockpitConfig::default();
    let report = make_report(&cfg, vec![], vec![]);
    let md = render_comment(&report, &cfg);

    assert!(md.contains("<!-- cockpit:begin -->"));
    assert!(md.contains("<!-- cockpit:end -->"));
}

// ===========================================================================
// 5. Markers wrap the entire comment content
// ===========================================================================

#[test]
fn markers_wrap_entire_content() {
    let cfg = CockpitConfig::default();
    let sensors = vec![make_sensor("alpha", VerdictStatus::Pass)];
    let report = make_report(&cfg, sensors, vec![]);
    let md = render_comment(&report, &cfg);

    let begin_pos = md.find("<!-- cockpit:begin -->").unwrap();
    let end_pos = md.find("<!-- cockpit:end -->").unwrap();

    // Begin marker is at the very start
    assert_eq!(begin_pos, 0);
    // End marker is near the very end
    let after_end = &md[end_pos + "<!-- cockpit:end -->".len()..];
    assert!(
        after_end.trim().is_empty(),
        "content after end marker should be empty, got: {:?}",
        after_end
    );
}

// ===========================================================================
// 6. No content appears outside markers
// ===========================================================================

#[test]
fn no_content_outside_markers() {
    let mut cfg = CockpitConfig::default();
    cfg.sensors.insert(
        "sensor_a".to_string(),
        SensorPolicy {
            blocking: true,
            missing: MissingPolicy::Fail,
            section: Some("Tests".to_string()),
            require_label: None,
            repro: Some("cargo test".to_string()),
        },
    );

    let sensors = vec![make_sensor("sensor_a", VerdictStatus::Fail)];
    let highlights = vec![Highlight {
        sensor_id: "sensor_a".to_string(),
        finding: Finding {
            severity: Severity::Error,
            check_id: None,
            code: "E1".to_string(),
            message: "failure".to_string(),
            location: Some(Location {
                path: Some("src/lib.rs".to_string()),
                line: Some(10),
                col: None,
            }),
            help: None,
            url: None,
            fingerprint: None,
            data: None,
        },
    }];

    let report = make_report(&cfg, sensors, highlights);
    let md = render_comment(&report, &cfg);

    // Everything before begin marker should be empty
    let before_begin = &md[..md.find("<!-- cockpit:begin -->").unwrap()];
    assert!(
        before_begin.is_empty(),
        "content before begin marker: {:?}",
        before_begin
    );

    // Everything after end marker should be only whitespace
    let end_pos = md.find("<!-- cockpit:end -->").unwrap();
    let after_end = &md[end_pos + "<!-- cockpit:end -->".len()..];
    assert!(
        after_end.trim().is_empty(),
        "content after end marker: {:?}",
        after_end
    );
}

// ===========================================================================
// 7. Marker format is exact (no extra whitespace)
// ===========================================================================

#[test]
fn marker_format_is_exact() {
    let cfg = CockpitConfig::default();
    let report = make_report(&cfg, vec![], vec![]);
    let md = render_comment(&report, &cfg);

    // The comment must start with the exact begin marker followed by newline
    assert!(md.starts_with("<!-- cockpit:begin -->\n"));
    // The comment must end with the exact end marker followed by newline
    assert!(md.ends_with("<!-- cockpit:end -->\n"));
}

#[test]
fn begin_marker_is_on_its_own_line() {
    let cfg = CockpitConfig::default();
    let report = make_report(&cfg, vec![], vec![]);
    let md = render_comment(&report, &cfg);

    let lines: Vec<&str> = md.lines().collect();
    assert_eq!(lines[0], "<!-- cockpit:begin -->");
}

#[test]
fn end_marker_is_on_its_own_line() {
    let cfg = CockpitConfig::default();
    let report = make_report(&cfg, vec![], vec![]);
    let md = render_comment(&report, &cfg);

    let lines: Vec<&str> = md.lines().collect();
    assert_eq!(*lines.last().unwrap(), "<!-- cockpit:end -->");
}

// ===========================================================================
// Extra: append_comment_sections preserves markers
// ===========================================================================

#[test]
fn append_sections_preserves_both_markers() {
    let base = "<!-- cockpit:begin -->\n## Cockpit\n<!-- cockpit:end -->\n";
    let sections = vec![("Extra".to_string(), "Some extra info".to_string())];
    let result = append_comment_sections(base, &sections);

    assert!(result.contains("<!-- cockpit:begin -->"));
    assert!(result.contains("<!-- cockpit:end -->"));
    assert!(result.contains("### Extra"));
}

#[test]
fn append_sections_inserts_before_end_marker() {
    let base = "<!-- cockpit:begin -->\n## Cockpit\n<!-- cockpit:end -->\n";
    let sections = vec![("Notes".to_string(), "My notes".to_string())];
    let result = append_comment_sections(base, &sections);

    let notes_pos = result.find("### Notes").unwrap();
    let end_pos = result.find("<!-- cockpit:end -->").unwrap();
    assert!(
        notes_pos < end_pos,
        "appended section should appear before end marker"
    );
}

#[test]
fn append_empty_sections_returns_original() {
    let base = "<!-- cockpit:begin -->\n## Cockpit\n<!-- cockpit:end -->\n";
    let result = append_comment_sections(base, &[]);
    assert_eq!(result, base);
}
