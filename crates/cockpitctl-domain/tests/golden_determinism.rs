//! Golden / snapshot tests that exercise every sort key in the determinism
//! contract for findings and highlights.
//!
//! Findings sort: severity desc → sensor_id → path → line → code → message
//! Highlights sort: severity desc → blocking-first → sensor_id → path → line → code → message

use cockpitctl_domain::{select_highlights, sort_findings};
use cockpitctl_types::{CockpitConfig, Finding, Highlight, Location, Severity};
use std::collections::BTreeMap;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn finding(code: &str, severity: Severity, path: &str, line: u32, message: &str) -> Finding {
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

fn highlight(sensor_id: &str, f: Finding) -> Highlight {
    Highlight {
        sensor_id: sensor_id.to_string(),
        finding: f,
    }
}

// ===========================================================================
// FINDINGS SORT ORDER
// ===========================================================================

/// Exercise ALL sort keys in one test: severity, sensor_id (constant here),
/// path, line, code, message — with a snapshot of the final order.
#[test]
fn golden_findings_sort_all_keys() {
    let mut findings = vec![
        finding("Z100", Severity::Info, "src/z.rs", 99, "last info"),
        finding("A001", Severity::Error, "src/a.rs", 1, "first error"),
        finding("M050", Severity::Warn, "src/m.rs", 50, "mid warning"),
        finding("A001", Severity::Error, "src/a.rs", 10, "second same-code error"),
        finding("B002", Severity::Error, "src/b.rs", 5, "error in b"),
        finding("A001", Severity::Error, "src/a.rs", 1, "alpha message"),
        finding("M050", Severity::Warn, "src/a.rs", 1, "warn in a"),
        finding("A100", Severity::Info, "src/a.rs", 1, "info in a"),
    ];
    sort_findings("sensor", &mut findings);
    insta::assert_json_snapshot!("golden_findings_sort_all_keys", findings);
}

/// Same severity (Error), different paths → lexical path order.
#[test]
fn golden_findings_same_severity_different_paths() {
    let mut findings = vec![
        finding("E1", Severity::Error, "src/z.rs", 1, "z"),
        finding("E1", Severity::Error, "src/a.rs", 1, "a"),
        finding("E1", Severity::Error, "src/m.rs", 1, "m"),
    ];
    sort_findings("sensor", &mut findings);
    insta::assert_json_snapshot!("golden_findings_same_severity_different_paths", findings);
}

/// Same severity + path, different lines → ascending line order.
#[test]
fn golden_findings_same_path_different_lines() {
    let mut findings = vec![
        finding("E1", Severity::Error, "src/main.rs", 100, "line 100"),
        finding("E1", Severity::Error, "src/main.rs", 1, "line 1"),
        finding("E1", Severity::Error, "src/main.rs", 50, "line 50"),
    ];
    sort_findings("sensor", &mut findings);
    insta::assert_json_snapshot!("golden_findings_same_path_different_lines", findings);
}

/// Same severity + path + line, different codes → lexical code order.
#[test]
fn golden_findings_same_location_different_codes() {
    let mut findings = vec![
        finding("Z99", Severity::Error, "src/lib.rs", 10, "msg"),
        finding("A01", Severity::Error, "src/lib.rs", 10, "msg"),
        finding("M50", Severity::Error, "src/lib.rs", 10, "msg"),
    ];
    sort_findings("sensor", &mut findings);
    insta::assert_json_snapshot!("golden_findings_same_location_different_codes", findings);
}

/// Same everything except message → lexical message tiebreaker.
#[test]
fn golden_findings_message_tiebreaker() {
    let mut findings = vec![
        finding("E1", Severity::Error, "src/lib.rs", 10, "Zebra"),
        finding("E1", Severity::Error, "src/lib.rs", 10, "Alpha"),
        finding("E1", Severity::Error, "src/lib.rs", 10, "Middle"),
    ];
    sort_findings("sensor", &mut findings);
    insta::assert_json_snapshot!("golden_findings_message_tiebreaker", findings);
}

// ===========================================================================
// HIGHLIGHTS SORT ORDER
// ===========================================================================

/// Exercise ALL highlight sort keys: severity, blocking, sensor_id, path,
/// line, code, message.
#[test]
fn golden_highlights_sort_all_keys() {
    let mut cfg = CockpitConfig::default();
    cfg.policy.max_highlights = 20;

    let mut blocking = BTreeMap::new();
    blocking.insert("blocker".to_string(), true);
    blocking.insert("advisory".to_string(), false);

    let candidates = vec![
        highlight(
            "advisory",
            finding("I1", Severity::Info, "src/z.rs", 99, "info advisory"),
        ),
        highlight(
            "blocker",
            finding("E1", Severity::Error, "src/a.rs", 1, "error blocker"),
        ),
        highlight(
            "advisory",
            finding("W1", Severity::Warn, "src/m.rs", 50, "warn advisory"),
        ),
        highlight(
            "blocker",
            finding("W2", Severity::Warn, "src/b.rs", 10, "warn blocker"),
        ),
        highlight(
            "advisory",
            finding("E2", Severity::Error, "src/c.rs", 5, "error advisory"),
        ),
        highlight(
            "blocker",
            finding("E3", Severity::Error, "src/d.rs", 20, "error blocker 2"),
        ),
    ];

    let selected = select_highlights(candidates, &cfg, &blocking);
    insta::assert_json_snapshot!("golden_highlights_sort_all_keys", selected);
}

/// Same severity (Error), blocking vs non-blocking → blocking comes first.
#[test]
fn golden_highlights_blocking_vs_nonblocking_same_severity() {
    let mut cfg = CockpitConfig::default();
    cfg.policy.max_highlights = 10;

    let mut blocking = BTreeMap::new();
    blocking.insert("block_sensor".to_string(), true);
    blocking.insert("info_sensor".to_string(), false);

    let candidates = vec![
        highlight(
            "info_sensor",
            finding("E1", Severity::Error, "src/a.rs", 1, "non-blocking error"),
        ),
        highlight(
            "block_sensor",
            finding("E2", Severity::Error, "src/a.rs", 1, "blocking error"),
        ),
    ];

    let selected = select_highlights(candidates, &cfg, &blocking);
    // Blocking error must appear before non-blocking error
    assert_eq!(selected[0].sensor_id, "block_sensor");
    assert_eq!(selected[1].sensor_id, "info_sensor");
    insta::assert_json_snapshot!(
        "golden_highlights_blocking_vs_nonblocking_same_severity",
        selected
    );
}

/// Interleave: severity takes priority over blocking flag.
/// An Error from a non-blocking sensor should appear before a Warn from a blocking sensor.
#[test]
fn golden_highlights_severity_then_blocking_interleave() {
    let mut cfg = CockpitConfig::default();
    cfg.policy.max_highlights = 10;

    let mut blocking = BTreeMap::new();
    blocking.insert("blocker".to_string(), true);
    blocking.insert("advisory".to_string(), false);

    let candidates = vec![
        highlight(
            "blocker",
            finding("W1", Severity::Warn, "src/a.rs", 1, "blocking warn"),
        ),
        highlight(
            "advisory",
            finding("E1", Severity::Error, "src/a.rs", 1, "advisory error"),
        ),
    ];

    let selected = select_highlights(candidates, &cfg, &blocking);
    // Error (advisory) should sort before Warn (blocking) because severity > blocking
    assert_eq!(selected[0].finding.severity, Severity::Error);
    assert_eq!(selected[1].finding.severity, Severity::Warn);
    insta::assert_json_snapshot!(
        "golden_highlights_severity_then_blocking_interleave",
        selected
    );
}

/// Same severity + same blocking status → tiebreak by sensor_id, path, line, code.
#[test]
fn golden_highlights_tiebreak_sensor_path_line_code() {
    let mut cfg = CockpitConfig::default();
    cfg.policy.max_highlights = 20;

    let mut blocking = BTreeMap::new();
    blocking.insert("alpha".to_string(), true);
    blocking.insert("beta".to_string(), true);

    let candidates = vec![
        highlight(
            "beta",
            finding("E1", Severity::Error, "src/b.rs", 20, "msg b"),
        ),
        highlight(
            "alpha",
            finding("E2", Severity::Error, "src/a.rs", 10, "msg a"),
        ),
        highlight(
            "alpha",
            finding("E1", Severity::Error, "src/a.rs", 5, "msg a early"),
        ),
        highlight(
            "alpha",
            finding("E1", Severity::Error, "src/a.rs", 5, "msg a alpha"),
        ),
    ];

    let selected = select_highlights(candidates, &cfg, &blocking);
    insta::assert_json_snapshot!(
        "golden_highlights_tiebreak_sensor_path_line_code",
        selected
    );
}
