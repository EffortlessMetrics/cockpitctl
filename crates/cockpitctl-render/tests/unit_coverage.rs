//! Additional unit tests for cockpitctl-render to close coverage gaps.
//!
//! Covers:
//! - render_comment: empty sensors, multiple findings with locations, stable markers
//! - render_github_annotations: findings with/without file locations, empty highlights
//! - render_annotations: truncation behavior, empty input
//! - render_trend_section: empty trend, verdict change, count deltas, new/fixed findings,
//!   sensors added/removed

use std::collections::BTreeMap;

use cockpitctl_render::{
    render_annotations, render_comment, render_github_annotations, render_trend_section,
};
use cockpitctl_types::{
    CockpitConfig, CockpitReport, CountDeltas, Finding, Highlight, Location, MissingPolicy,
    PolicySensorSnapshot, PolicySnapshot, Presence, RunInfo, SensorPolicy, SensorSummary, Severity,
    ToolInfo, TrendDelta, TrendFinding, Verdict, VerdictChange, VerdictCounts, VerdictStatus,
};

// ---------------------------------------------------------------------------
// Helpers (mirrors the pattern from render_comment.rs)
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
    path: Option<&str>,
    line: Option<u32>,
    col: Option<u32>,
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

fn make_sensor_summary(
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
// render_comment tests
// ===========================================================================

#[test]
fn render_comment_empty_sensors_list() {
    let cfg = CockpitConfig::default();
    let report = make_report(&cfg, vec![], vec![]);

    let md = render_comment(&report, &cfg);

    // Stable markers must always be present
    assert!(md.starts_with("<!-- cockpit:begin -->\n"));
    assert!(md.ends_with("<!-- cockpit:end -->\n"));

    // Summary table header is present even with no sensors
    assert!(md.contains("### Summary"));
    assert!(md.contains("| Sensor | Status | Blocking | Notes |"));

    // No sensor rows (only header + separator lines of the table)
    let table_start = md.find("| Sensor |").unwrap();
    let table_chunk = &md[table_start..];
    let table_lines: Vec<&str> = table_chunk.lines().take(3).collect();
    // The third line should be empty or start the next section
    assert!(
        table_lines[2].is_empty() || table_lines[2].starts_with('#'),
        "expected no sensor rows in empty table"
    );

    // Empty state markers
    assert!(md.contains("_No highlights._"));
    assert!(md.contains("_No annotations._"));
}

#[test]
fn render_comment_sensors_with_findings_and_locations() {
    let mut cfg = CockpitConfig::default();
    cfg.policy.max_highlights = 5;
    cfg.sensors.insert(
        "lint".to_string(),
        SensorPolicy {
            blocking: true,
            missing: MissingPolicy::Fail,
            section: Some("Diagnostics".to_string()),
            require_label: None,
            repro: None,
        },
    );

    let sensors = vec![make_sensor_summary(
        "lint",
        VerdictStatus::Fail,
        true,
        Some("artifacts/lint/comment.md"),
        false,
    )];

    let highlights = vec![
        make_highlight(
            "lint",
            "E001",
            Some("src/main.rs"),
            Some(42),
            None,
            Severity::Error,
            "unused variable",
        ),
        make_highlight(
            "lint",
            "W002",
            Some("src/lib.rs"),
            Some(10),
            None,
            Severity::Warn,
            "dead code",
        ),
    ];

    let report = make_report(&cfg, sensors, highlights);
    let md = render_comment(&report, &cfg);

    // Sensor row
    assert!(md.contains("| `lint` | ❌ fail | yes |"));
    assert!(md.contains("`artifacts/lint/report.json`"));
    assert!(md.contains("`artifacts/lint/comment.md`"));

    // Highlights section: both findings rendered
    assert!(md.contains("`E001` at `src/main.rs:42`"));
    assert!(md.contains("`W002` at `src/lib.rs:10`"));

    // Highlights count line
    assert!(md.contains("(showing up to **5**)"));

    // Annotations section exists
    assert!(md.contains("### Annotations"));

    // Section rendering (sensor placed in "Diagnostics")
    assert!(md.contains("### Diagnostics"));
    assert!(md.contains("- `lint`: ❌ fail"));
}

#[test]
fn render_comment_stable_markers_always_present() {
    // Verify markers for various report configurations
    for status in &[
        VerdictStatus::Pass,
        VerdictStatus::Warn,
        VerdictStatus::Fail,
        VerdictStatus::Skip,
    ] {
        let cfg = CockpitConfig::default();
        let sensors = vec![make_sensor_summary("s", status.clone(), false, None, false)];
        let report = make_report(&cfg, sensors, vec![]);
        let md = render_comment(&report, &cfg);

        assert!(
            md.contains("<!-- cockpit:begin -->"),
            "begin marker missing for {:?}",
            status
        );
        assert!(
            md.contains("<!-- cockpit:end -->"),
            "end marker missing for {:?}",
            status
        );
    }
}

#[test]
fn render_comment_highlights_with_multiline_message_collapses_newlines() {
    let cfg = CockpitConfig::default();
    let sensors = vec![make_sensor_summary(
        "s",
        VerdictStatus::Warn,
        true,
        None,
        false,
    )];
    let highlights = vec![Highlight {
        sensor_id: "s".to_string(),
        finding: Finding {
            severity: Severity::Warn,
            check_id: None,
            code: "W1".to_string(),
            message: "line one\nline two\nline three".to_string(),
            location: None,
            help: None,
            url: None,
            fingerprint: None,
            data: None,
        },
    }];

    let report = make_report(&cfg, sensors, highlights);
    let md = render_comment(&report, &cfg);

    // Newlines in messages should be replaced with spaces
    assert!(md.contains("line one line two line three"));
    assert!(!md.contains("line one\nline two"));
}

#[test]
fn render_comment_nonblocking_sensor_renders_no() {
    let cfg = CockpitConfig::default();
    let sensors = vec![make_sensor_summary(
        "advisory",
        VerdictStatus::Pass,
        false,
        None,
        false,
    )];
    let report = make_report(&cfg, sensors, vec![]);
    let md = render_comment(&report, &cfg);

    assert!(md.contains("| `advisory` | ✅ pass | no |"));
}

#[test]
fn render_comment_highlight_with_path_only_no_line() {
    let cfg = CockpitConfig::default();
    let sensors = vec![make_sensor_summary(
        "s",
        VerdictStatus::Warn,
        true,
        None,
        false,
    )];
    let highlights = vec![make_highlight(
        "s",
        "C1",
        Some("README.md"),
        None,
        None,
        Severity::Info,
        "check readme",
    )];

    let report = make_report(&cfg, sensors, highlights);
    let md = render_comment(&report, &cfg);

    // Should render path without line number (no colon)
    assert!(md.contains("at `README.md`"));
    assert!(!md.contains("at `README.md:`"));
}

// ===========================================================================
// render_github_annotations tests
// ===========================================================================

#[test]
fn github_annotations_with_file_locations() {
    let mut cfg = CockpitConfig::default();
    cfg.policy.max_annotations = 10;

    let highlights = vec![make_highlight(
        "lint",
        "E001",
        Some("src/main.rs"),
        Some(42),
        Some(5),
        Severity::Error,
        "unused import",
    )];

    let blocking = BTreeMap::new();
    let result = render_github_annotations(&highlights, &cfg, &blocking);

    assert_eq!(result.total_count, 1);
    assert_eq!(result.rendered_count, 1);
    assert!(!result.truncated);
    assert_eq!(result.lines.len(), 1);

    let line = &result.lines[0];
    assert!(line.starts_with("::error "));
    assert!(line.contains("file=src/main.rs"));
    assert!(line.contains("line=42"));
    assert!(line.contains("col=5"));
    assert!(line.contains("title=[lint] E001")); // gh_escape only escapes %, \n, \r
    assert!(line.contains("::unused import"));
}

#[test]
fn github_annotations_without_locations() {
    let mut cfg = CockpitConfig::default();
    cfg.policy.max_annotations = 10;

    // Finding with no location at all
    let highlights = vec![Highlight {
        sensor_id: "checker".to_string(),
        finding: Finding {
            severity: Severity::Warn,
            check_id: None,
            code: "W100".to_string(),
            message: "global warning".to_string(),
            location: None,
            help: None,
            url: None,
            fingerprint: None,
            data: None,
        },
    }];

    let blocking = BTreeMap::new();
    let result = render_github_annotations(&highlights, &cfg, &blocking);

    assert_eq!(result.lines.len(), 1);
    let line = &result.lines[0];
    assert!(line.starts_with("::warning "));
    // No file, line, or col params
    assert!(!line.contains("file="));
    assert!(!line.contains("line="));
    assert!(!line.contains("col="));
    assert!(line.contains("title="));
    assert!(line.contains("::global warning"));
}

#[test]
fn github_annotations_empty_highlights() {
    let cfg = CockpitConfig::default();
    let blocking = BTreeMap::new();
    let result = render_github_annotations(&[], &cfg, &blocking);

    assert_eq!(result.total_count, 0);
    assert_eq!(result.rendered_count, 0);
    assert!(!result.truncated);
    assert!(result.lines.is_empty());
}

#[test]
fn github_annotations_severity_mapping() {
    let mut cfg = CockpitConfig::default();
    cfg.policy.max_annotations = 10;
    let blocking = BTreeMap::new();

    let highlights = vec![
        make_highlight("s", "E1", None, None, None, Severity::Error, "err"),
        make_highlight("s", "W1", None, None, None, Severity::Warn, "wrn"),
        make_highlight("s", "I1", None, None, None, Severity::Info, "inf"),
    ];

    let result = render_github_annotations(&highlights, &cfg, &blocking);
    assert_eq!(result.lines.len(), 3);

    // Error -> ::error, Warn -> ::warning, Info -> ::notice
    assert!(result.lines.iter().any(|l| l.starts_with("::error ")));
    assert!(result.lines.iter().any(|l| l.starts_with("::warning ")));
    assert!(result.lines.iter().any(|l| l.starts_with("::notice ")));
}

#[test]
fn github_annotations_truncation() {
    let mut cfg = CockpitConfig::default();
    cfg.policy.max_annotations = 2;

    let highlights = vec![
        make_highlight("s", "A", Some("a.rs"), Some(1), None, Severity::Error, "m1"),
        make_highlight("s", "B", Some("b.rs"), Some(2), None, Severity::Warn, "m2"),
        make_highlight("s", "C", Some("c.rs"), Some(3), None, Severity::Info, "m3"),
    ];

    let blocking = BTreeMap::new();
    let result = render_github_annotations(&highlights, &cfg, &blocking);

    assert!(result.truncated);
    assert_eq!(result.total_count, 3);
    assert_eq!(result.rendered_count, 2);
    assert_eq!(result.lines.len(), 2);
}

#[test]
fn github_annotations_escape_special_characters() {
    let mut cfg = CockpitConfig::default();
    cfg.policy.max_annotations = 10;

    let highlights = vec![Highlight {
        sensor_id: "s".to_string(),
        finding: Finding {
            severity: Severity::Error,
            check_id: None,
            code: "E1".to_string(),
            message: "line1\nline2\r100%".to_string(),
            location: None,
            help: None,
            url: None,
            fingerprint: None,
            data: None,
        },
    }];

    let blocking = BTreeMap::new();
    let result = render_github_annotations(&highlights, &cfg, &blocking);

    let line = &result.lines[0];
    // gh_escape replaces % first -> 100%25, then \n -> %0A, \r -> %0D
    assert!(line.contains("line1%0Aline2%0D100%25"));
}

#[test]
fn github_annotations_with_partial_location() {
    let mut cfg = CockpitConfig::default();
    cfg.policy.max_annotations = 10;

    // Location with path and line but no col
    let highlights = vec![make_highlight(
        "s",
        "E1",
        Some("src/foo.rs"),
        Some(7),
        None,
        Severity::Error,
        "msg",
    )];

    let blocking = BTreeMap::new();
    let result = render_github_annotations(&highlights, &cfg, &blocking);

    let line = &result.lines[0];
    assert!(line.contains("file=src/foo.rs"));
    assert!(line.contains("line=7"));
    assert!(!line.contains("col="));
}

#[test]
fn github_annotations_deterministic_ordering() {
    let mut cfg = CockpitConfig::default();
    cfg.policy.max_annotations = 10;

    let highlights = vec![
        make_highlight("z_sensor", "Z1", None, None, None, Severity::Info, "info"),
        make_highlight(
            "a_sensor",
            "A1",
            Some("a.rs"),
            Some(1),
            None,
            Severity::Error,
            "err",
        ),
        make_highlight("m_sensor", "M1", None, None, None, Severity::Warn, "warn"),
    ];

    let blocking = BTreeMap::new();
    let result = render_github_annotations(&highlights, &cfg, &blocking);

    // Errors first, then warnings, then info
    assert!(result.lines[0].starts_with("::error "));
    assert!(result.lines[1].starts_with("::warning "));
    assert!(result.lines[2].starts_with("::notice "));
}

#[test]
fn github_annotations_blocking_sensors_sort_first() {
    let mut cfg = CockpitConfig::default();
    cfg.policy.max_annotations = 10;

    let highlights = vec![
        make_highlight(
            "non_blocking",
            "E1",
            Some("a.rs"),
            Some(1),
            None,
            Severity::Error,
            "m1",
        ),
        make_highlight(
            "blocking",
            "E2",
            Some("b.rs"),
            Some(1),
            None,
            Severity::Error,
            "m2",
        ),
    ];

    let mut blocking = BTreeMap::new();
    blocking.insert("blocking".to_string(), true);
    blocking.insert("non_blocking".to_string(), false);

    let result = render_github_annotations(&highlights, &cfg, &blocking);

    // Blocking sensor annotation should come first
    assert!(result.lines[0].contains("[blocking]"));
    assert!(result.lines[1].contains("[non_blocking]"));
}

// ===========================================================================
// render_annotations tests (markdown)
// ===========================================================================

#[test]
fn render_annotations_empty_input() {
    let cfg = CockpitConfig::default();
    let blocking = BTreeMap::new();

    let result = render_annotations(&[], &cfg, &blocking);

    assert!(!result.truncated);
    assert_eq!(result.total_count, 0);
    assert_eq!(result.rendered_count, 0);
    assert!(result.content.contains("_No annotations._"));
}

#[test]
fn render_annotations_truncation_message() {
    let mut cfg = CockpitConfig::default();
    cfg.policy.max_annotations = 1;

    let highlights = vec![
        make_highlight(
            "s",
            "A",
            Some("a.rs"),
            Some(1),
            None,
            Severity::Error,
            "msg a",
        ),
        make_highlight(
            "s",
            "B",
            Some("b.rs"),
            Some(2),
            None,
            Severity::Warn,
            "msg b",
        ),
    ];

    let blocking = BTreeMap::new();
    let result = render_annotations(&highlights, &cfg, &blocking);

    assert!(result.truncated);
    assert_eq!(result.total_count, 2);
    assert_eq!(result.rendered_count, 1);
    assert!(result.content.contains("Showing 1 of 2 annotations"));
    assert!(result.content.contains("capped by `max_annotations`"));
}

#[test]
fn render_annotations_exactly_at_limit_no_truncation() {
    let mut cfg = CockpitConfig::default();
    cfg.policy.max_annotations = 3;

    let highlights = vec![
        make_highlight("s", "A", Some("a.rs"), Some(1), None, Severity::Error, "m1"),
        make_highlight("s", "B", Some("b.rs"), Some(2), None, Severity::Warn, "m2"),
        make_highlight("s", "C", Some("c.rs"), Some(3), None, Severity::Info, "m3"),
    ];

    let blocking = BTreeMap::new();
    let result = render_annotations(&highlights, &cfg, &blocking);

    assert!(!result.truncated);
    assert_eq!(result.total_count, 3);
    assert_eq!(result.rendered_count, 3);
    assert!(!result.content.contains("Showing"));
    assert!(!result.content.contains("capped"));
}

#[test]
fn render_annotations_numbered_lines() {
    let mut cfg = CockpitConfig::default();
    cfg.policy.max_annotations = 10;

    let highlights = vec![
        make_highlight("s", "A", Some("a.rs"), Some(1), None, Severity::Error, "m1"),
        make_highlight("s", "B", Some("b.rs"), Some(2), None, Severity::Warn, "m2"),
    ];

    let blocking = BTreeMap::new();
    let result = render_annotations(&highlights, &cfg, &blocking);

    // Lines should be numbered starting at 1
    assert!(result.content.contains("1. "));
    assert!(result.content.contains("2. "));
}

#[test]
fn render_annotations_location_formatting() {
    let mut cfg = CockpitConfig::default();
    cfg.policy.max_annotations = 10;

    let highlights = vec![
        // Path + line
        make_highlight(
            "s",
            "A",
            Some("src/a.rs"),
            Some(10),
            None,
            Severity::Error,
            "m",
        ),
        // Path only (no line)
        make_highlight("s", "B", Some("README.md"), None, None, Severity::Warn, "m"),
        // No location at all
        Highlight {
            sensor_id: "s".to_string(),
            finding: Finding {
                severity: Severity::Info,
                check_id: None,
                code: "C".to_string(),
                message: "m".to_string(),
                location: None,
                help: None,
                url: None,
                fingerprint: None,
                data: None,
            },
        },
    ];

    let blocking = BTreeMap::new();
    let result = render_annotations(&highlights, &cfg, &blocking);

    assert!(result.content.contains("at `src/a.rs:10`"));
    assert!(result.content.contains("at `README.md`"));
    // The info finding with no location should not have " at `"
    let lines: Vec<&str> = result.content.lines().collect();
    let info_line = lines.iter().find(|l| l.contains("`C`")).unwrap();
    assert!(!info_line.contains(" at `"));
}

// ===========================================================================
// render_trend_section tests
// ===========================================================================

#[test]
fn render_trend_section_empty_trend_all_none() {
    let trend = TrendDelta {
        verdict_change: None,
        count_deltas: CountDeltas {
            info_delta: 0,
            warn_delta: 0,
            error_delta: 0,
        },
        new_findings: vec![],
        fixed_findings: vec![],
        sensors_added: vec![],
        sensors_removed: vec![],
    };

    let md = render_trend_section(&trend);

    assert!(md.contains("### Trend"));
    assert!(md.contains("_No changes from baseline._"));
    // Should NOT contain a delta table or verdict change
    assert!(!md.contains("Verdict:"));
    assert!(!md.contains("| Severity | Delta |"));
    assert!(!md.contains("new finding"));
    assert!(!md.contains("fixed finding"));
}

#[test]
fn render_trend_section_verdict_change() {
    let trend = TrendDelta {
        verdict_change: Some(VerdictChange {
            before: VerdictStatus::Pass,
            after: VerdictStatus::Fail,
        }),
        count_deltas: CountDeltas::default(),
        new_findings: vec![],
        fixed_findings: vec![],
        sensors_added: vec![],
        sensors_removed: vec![],
    };

    let md = render_trend_section(&trend);

    assert!(md.contains("Verdict:"));
    assert!(md.contains("✅ pass"));
    assert!(md.contains("❌ fail"));
    // No "no changes" message
    assert!(!md.contains("_No changes from baseline._"));
}

#[test]
fn render_trend_section_count_deltas_only() {
    let trend = TrendDelta {
        verdict_change: None,
        count_deltas: CountDeltas {
            info_delta: 2,
            warn_delta: -1,
            error_delta: 3,
        },
        new_findings: vec![],
        fixed_findings: vec![],
        sensors_added: vec![],
        sensors_removed: vec![],
    };

    let md = render_trend_section(&trend);

    assert!(md.contains("| Severity | Delta |"));
    assert!(md.contains("| Error | +3 |"));
    assert!(md.contains("| Warn | -1 |"));
    assert!(md.contains("| Info | +2 |"));
    assert!(!md.contains("_No changes from baseline._"));
}

#[test]
fn render_trend_section_count_deltas_zero_omitted() {
    let trend = TrendDelta {
        verdict_change: None,
        count_deltas: CountDeltas {
            info_delta: 0,
            warn_delta: 5,
            error_delta: 0,
        },
        new_findings: vec![],
        fixed_findings: vec![],
        sensors_added: vec![],
        sensors_removed: vec![],
    };

    let md = render_trend_section(&trend);

    assert!(md.contains("| Warn | +5 |"));
    // Zero deltas should not be rendered
    assert!(!md.contains("| Error |"));
    assert!(!md.contains("| Info |"));
}

#[test]
fn render_trend_section_new_findings() {
    let trend = TrendDelta {
        verdict_change: None,
        count_deltas: CountDeltas::default(),
        new_findings: vec![
            TrendFinding {
                sensor_id: "lint".to_string(),
                code: "E001".to_string(),
                message: "unused import".to_string(),
                path: Some("src/main.rs".to_string()),
                line: Some(42),
                fingerprint: None,
                severity: Severity::Error,
            },
            TrendFinding {
                sensor_id: "lint".to_string(),
                code: "W001".to_string(),
                message: "dead code".to_string(),
                path: Some("src/lib.rs".to_string()),
                line: None,
                fingerprint: None,
                severity: Severity::Warn,
            },
        ],
        fixed_findings: vec![],
        sensors_added: vec![],
        sensors_removed: vec![],
    };

    let md = render_trend_section(&trend);

    assert!(md.contains("**2 new finding(s)**:"));
    assert!(md.contains("**lint**: `E001` at `src/main.rs:42`"));
    assert!(md.contains("**lint**: `W001` at `src/lib.rs`"));
    assert!(!md.contains("_No changes from baseline._"));
}

#[test]
fn render_trend_section_new_finding_without_path() {
    let trend = TrendDelta {
        verdict_change: None,
        count_deltas: CountDeltas::default(),
        new_findings: vec![TrendFinding {
            sensor_id: "check".to_string(),
            code: "C1".to_string(),
            message: "global issue".to_string(),
            path: None,
            line: None,
            fingerprint: None,
            severity: Severity::Info,
        }],
        fixed_findings: vec![],
        sensors_added: vec![],
        sensors_removed: vec![],
    };

    let md = render_trend_section(&trend);

    assert!(md.contains("**1 new finding(s)**:"));
    assert!(md.contains("**check**: `C1`"));
    // No " at `" since path is None
    assert!(!md.contains(" at `"));
}

#[test]
fn render_trend_section_fixed_findings() {
    let trend = TrendDelta {
        verdict_change: None,
        count_deltas: CountDeltas::default(),
        new_findings: vec![],
        fixed_findings: vec![TrendFinding {
            sensor_id: "lint".to_string(),
            code: "E001".to_string(),
            message: "was unused import".to_string(),
            path: None,
            line: None,
            fingerprint: None,
            severity: Severity::Error,
        }],
        sensors_added: vec![],
        sensors_removed: vec![],
    };

    let md = render_trend_section(&trend);

    assert!(md.contains("**1 fixed finding(s)**:"));
    // Fixed findings use strikethrough
    assert!(md.contains("~**lint**: `E001`~ — was unused import"));
    assert!(!md.contains("_No changes from baseline._"));
}

#[test]
fn render_trend_section_sensors_added_and_removed() {
    let trend = TrendDelta {
        verdict_change: None,
        count_deltas: CountDeltas::default(),
        new_findings: vec![],
        fixed_findings: vec![],
        sensors_added: vec!["new_sensor".to_string(), "another_new".to_string()],
        sensors_removed: vec!["old_sensor".to_string()],
    };

    let md = render_trend_section(&trend);

    assert!(md.contains("Sensors added: `new_sensor`, `another_new`"));
    assert!(md.contains("Sensors removed: `old_sensor`"));
    assert!(!md.contains("_No changes from baseline._"));
}

#[test]
fn render_trend_section_combined_verdict_and_deltas() {
    let trend = TrendDelta {
        verdict_change: Some(VerdictChange {
            before: VerdictStatus::Warn,
            after: VerdictStatus::Pass,
        }),
        count_deltas: CountDeltas {
            info_delta: 0,
            warn_delta: -3,
            error_delta: 0,
        },
        new_findings: vec![],
        fixed_findings: vec![TrendFinding {
            sensor_id: "s".to_string(),
            code: "W1".to_string(),
            message: "fixed it".to_string(),
            path: None,
            line: None,
            fingerprint: None,
            severity: Severity::Warn,
        }],
        sensors_added: vec![],
        sensors_removed: vec![],
    };

    let md = render_trend_section(&trend);

    // All sections should be present
    assert!(md.contains("Verdict: ⚠️ warn"));
    assert!(md.contains("✅ pass"));
    assert!(md.contains("| Warn | -3 |"));
    assert!(md.contains("**1 fixed finding(s)**:"));
    assert!(!md.contains("_No changes from baseline._"));
}

#[test]
fn render_trend_section_verdict_warn_to_skip() {
    let trend = TrendDelta {
        verdict_change: Some(VerdictChange {
            before: VerdictStatus::Warn,
            after: VerdictStatus::Skip,
        }),
        count_deltas: CountDeltas::default(),
        new_findings: vec![],
        fixed_findings: vec![],
        sensors_added: vec![],
        sensors_removed: vec![],
    };

    let md = render_trend_section(&trend);
    assert!(md.contains("⚠️ warn"));
    assert!(md.contains("⏭ skip"));
}

// ===========================================================================
// render_comment: section ordering and grouping
// ===========================================================================

#[test]
fn render_comment_sensors_grouped_by_section() {
    let mut cfg = CockpitConfig::default();
    cfg.sensors.insert(
        "test_sensor".to_string(),
        SensorPolicy {
            blocking: true,
            missing: MissingPolicy::Fail,
            section: Some("Tests".to_string()),
            require_label: None,
            repro: None,
        },
    );
    cfg.sensors.insert(
        "dep_sensor".to_string(),
        SensorPolicy {
            blocking: false,
            missing: MissingPolicy::Skip,
            section: Some("Dependencies".to_string()),
            require_label: None,
            repro: None,
        },
    );

    let sensors = vec![
        make_sensor_summary("test_sensor", VerdictStatus::Pass, true, None, false),
        make_sensor_summary("dep_sensor", VerdictStatus::Pass, false, None, false),
    ];

    let report = make_report(&cfg, sensors, vec![]);
    let md = render_comment(&report, &cfg);

    // Both section headers should appear
    assert!(md.contains("### Tests"));
    assert!(md.contains("### Dependencies"));

    // Sensors should appear under their sections
    let tests_idx = md.find("### Tests").unwrap();
    let deps_idx = md.find("### Dependencies").unwrap();
    let test_sensor_idx = md.find("- `test_sensor`").unwrap();
    let dep_sensor_idx = md.find("- `dep_sensor`").unwrap();

    assert!(test_sensor_idx > tests_idx);
    assert!(dep_sensor_idx > deps_idx);

    // Dependencies comes before Tests in default section_order
    assert!(deps_idx < tests_idx);
}

#[test]
fn render_comment_sensor_without_section_grouped_in_other() {
    let mut cfg = CockpitConfig::default();
    // No section specified for this sensor
    cfg.sensors.insert(
        "misc".to_string(),
        SensorPolicy {
            blocking: false,
            missing: MissingPolicy::Skip,
            section: None,
            require_label: None,
            repro: None,
        },
    );

    let sensors = vec![make_sensor_summary(
        "misc",
        VerdictStatus::Pass,
        false,
        None,
        false,
    )];

    let report = make_report(&cfg, sensors, vec![]);
    let md = render_comment(&report, &cfg);

    assert!(md.contains("### Other"));
    let other_idx = md.find("### Other").unwrap();
    let misc_idx = md.find("- `misc`").unwrap();
    assert!(misc_idx > other_idx);
}
