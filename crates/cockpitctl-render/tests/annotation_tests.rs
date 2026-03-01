//! Tests for GitHub Actions annotation rendering (`render_github_annotations`).
//!
//! Covers: single/multiple findings, missing locations, severity mapping,
//! deterministic ordering, empty input, multi-sensor sorting, and truncation.

use std::collections::BTreeMap;

use cockpitctl_render::render_github_annotations;
use cockpitctl_types::{CockpitConfig, Finding, Highlight, Location, Severity};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

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

fn default_cfg(max: usize) -> CockpitConfig {
    let mut cfg = CockpitConfig::default();
    cfg.policy.max_annotations = max;
    cfg
}

// ===========================================================================
// 1. Single finding → one annotation
// ===========================================================================

#[test]
fn single_finding_produces_one_annotation() {
    let cfg = default_cfg(10);
    let highlights = vec![make_highlight(
        "builddiag",
        "E001",
        Some("src/main.rs"),
        Some(42),
        None,
        Severity::Error,
        "build failed",
    )];

    let result = render_github_annotations(&highlights, &cfg, &BTreeMap::new());

    assert_eq!(result.lines.len(), 1);
    assert_eq!(result.total_count, 1);
    assert_eq!(result.rendered_count, 1);
    assert!(!result.truncated);
    assert!(result.lines[0].starts_with("::error "));
    assert!(result.lines[0].contains("file=src/main.rs"));
    assert!(result.lines[0].contains("line=42"));
    assert!(result.lines[0].contains("::build failed"));
}

// ===========================================================================
// 2. Multiple findings → multiple annotations, sorted
// ===========================================================================

#[test]
fn multiple_findings_sorted_by_severity_then_sensor() {
    let cfg = default_cfg(10);
    let highlights = vec![
        make_highlight(
            "z_lint",
            "I1",
            Some("z.rs"),
            Some(1),
            None,
            Severity::Info,
            "info msg",
        ),
        make_highlight(
            "a_lint",
            "E1",
            Some("a.rs"),
            Some(1),
            None,
            Severity::Error,
            "err msg",
        ),
        make_highlight(
            "m_lint",
            "W1",
            Some("m.rs"),
            Some(1),
            None,
            Severity::Warn,
            "warn msg",
        ),
    ];

    let result = render_github_annotations(&highlights, &cfg, &BTreeMap::new());

    assert_eq!(result.lines.len(), 3);
    // Severity desc: error first, then warning, then notice
    assert!(result.lines[0].starts_with("::error "));
    assert!(result.lines[1].starts_with("::warning "));
    assert!(result.lines[2].starts_with("::notice "));
}

// ===========================================================================
// 3. Finding without line → annotation without line number
// ===========================================================================

#[test]
fn finding_without_line_omits_line_param() {
    let cfg = default_cfg(10);
    let highlights = vec![make_highlight(
        "checker",
        "E1",
        Some("src/lib.rs"),
        None,
        None,
        Severity::Error,
        "file-level issue",
    )];

    let result = render_github_annotations(&highlights, &cfg, &BTreeMap::new());

    assert_eq!(result.lines.len(), 1);
    assert!(result.lines[0].contains("file=src/lib.rs"));
    assert!(!result.lines[0].contains("line="));
}

// ===========================================================================
// 4. Finding without path → annotation handled gracefully
// ===========================================================================

#[test]
fn finding_without_path_omits_file_param() {
    let cfg = default_cfg(10);
    let highlights = vec![make_highlight(
        "global",
        "G1",
        None,
        None,
        None,
        Severity::Warn,
        "global warning",
    )];

    let result = render_github_annotations(&highlights, &cfg, &BTreeMap::new());

    assert_eq!(result.lines.len(), 1);
    let line = &result.lines[0];
    assert!(!line.contains("file="));
    assert!(!line.contains("line="));
    assert!(!line.contains("col="));
    assert!(line.contains("title="));
    assert!(line.contains("::global warning"));
}

// ===========================================================================
// 5. Different severity levels → correct annotation level
// ===========================================================================

#[test]
fn severity_error_maps_to_error_level() {
    let cfg = default_cfg(10);
    let highlights = vec![make_highlight(
        "s",
        "E1",
        None,
        None,
        None,
        Severity::Error,
        "msg",
    )];
    let result = render_github_annotations(&highlights, &cfg, &BTreeMap::new());
    assert!(result.lines[0].starts_with("::error "));
}

#[test]
fn severity_warn_maps_to_warning_level() {
    let cfg = default_cfg(10);
    let highlights = vec![make_highlight(
        "s",
        "W1",
        None,
        None,
        None,
        Severity::Warn,
        "msg",
    )];
    let result = render_github_annotations(&highlights, &cfg, &BTreeMap::new());
    assert!(result.lines[0].starts_with("::warning "));
}

#[test]
fn severity_info_maps_to_notice_level() {
    let cfg = default_cfg(10);
    let highlights = vec![make_highlight(
        "s",
        "I1",
        None,
        None,
        None,
        Severity::Info,
        "msg",
    )];
    let result = render_github_annotations(&highlights, &cfg, &BTreeMap::new());
    assert!(result.lines[0].starts_with("::notice "));
}

// ===========================================================================
// 6. Annotations are deterministic (same input → same output)
// ===========================================================================

#[test]
fn annotations_are_deterministic_across_runs() {
    let cfg = default_cfg(10);
    let highlights = vec![
        make_highlight("z", "Z1", Some("z.rs"), Some(9), None, Severity::Info, "z"),
        make_highlight("a", "A1", Some("a.rs"), Some(1), None, Severity::Error, "a"),
        make_highlight("m", "M1", Some("m.rs"), Some(5), None, Severity::Warn, "m"),
    ];

    let blocking = BTreeMap::new();
    let first = render_github_annotations(&highlights, &cfg, &blocking);
    let second = render_github_annotations(&highlights, &cfg, &blocking);

    assert_eq!(first.lines, second.lines);
    assert_eq!(first.total_count, second.total_count);
    assert_eq!(first.rendered_count, second.rendered_count);
    assert_eq!(first.truncated, second.truncated);
}

// ===========================================================================
// 7. Empty findings list → no annotations
// ===========================================================================

#[test]
fn empty_findings_produce_no_annotations() {
    let cfg = default_cfg(10);
    let result = render_github_annotations(&[], &cfg, &BTreeMap::new());

    assert!(result.lines.is_empty());
    assert_eq!(result.total_count, 0);
    assert_eq!(result.rendered_count, 0);
    assert!(!result.truncated);
}

// ===========================================================================
// 8. Findings from multiple sensors → sorted correctly
// ===========================================================================

#[test]
fn multi_sensor_findings_sorted_by_severity_blocking_sensor() {
    let cfg = default_cfg(10);
    let highlights = vec![
        make_highlight(
            "clippy",
            "C1",
            Some("c.rs"),
            Some(1),
            None,
            Severity::Warn,
            "clippy warn",
        ),
        make_highlight(
            "builddiag",
            "B1",
            Some("b.rs"),
            Some(1),
            None,
            Severity::Error,
            "build err",
        ),
        make_highlight(
            "test",
            "T1",
            Some("t.rs"),
            Some(1),
            None,
            Severity::Error,
            "test err",
        ),
    ];

    let mut blocking = BTreeMap::new();
    blocking.insert("builddiag".to_string(), true);
    blocking.insert("test".to_string(), false);
    blocking.insert("clippy".to_string(), false);

    let result = render_github_annotations(&highlights, &cfg, &blocking);

    // builddiag (error, blocking) first, then test (error, non-blocking), then clippy (warn)
    assert!(result.lines[0].contains("[builddiag]"));
    assert!(result.lines[1].contains("[test]"));
    assert!(result.lines[2].contains("[clippy]"));
}

#[test]
fn annotations_cap_respects_max_annotations() {
    let cfg = default_cfg(2);
    let highlights = vec![
        make_highlight(
            "a",
            "A1",
            Some("a.rs"),
            Some(1),
            None,
            Severity::Error,
            "m1",
        ),
        make_highlight("b", "B1", Some("b.rs"), Some(2), None, Severity::Warn, "m2"),
        make_highlight("c", "C1", Some("c.rs"), Some(3), None, Severity::Info, "m3"),
    ];

    let result = render_github_annotations(&highlights, &cfg, &BTreeMap::new());

    assert!(result.truncated);
    assert_eq!(result.total_count, 3);
    assert_eq!(result.rendered_count, 2);
    assert_eq!(result.lines.len(), 2);
}

#[test]
fn annotation_title_includes_sensor_id_and_code() {
    let cfg = default_cfg(10);
    let highlights = vec![make_highlight(
        "my-sensor",
        "ERR_42",
        Some("src/foo.rs"),
        Some(7),
        Some(3),
        Severity::Error,
        "something broke",
    )];

    let result = render_github_annotations(&highlights, &cfg, &BTreeMap::new());
    let line = &result.lines[0];

    assert!(line.contains("title=[my-sensor] ERR_42"));
    assert!(line.contains("col=3"));
}
