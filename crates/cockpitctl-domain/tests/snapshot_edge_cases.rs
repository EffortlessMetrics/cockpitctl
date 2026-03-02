//! Snapshot tests for domain edge cases: empty fields, unicode, max-length strings.

use cockpitctl_domain::{
    cap_findings, compute_counts, derive_fingerprint, select_highlights, sort_findings,
    synthesize_receipt_inconsistent,
};
use cockpitctl_types::{CockpitConfig, Finding, Highlight, Location, Severity, VerdictCounts};

fn finding_full(
    code: &str,
    severity: Severity,
    message: &str,
    path: Option<&str>,
    line: Option<u32>,
) -> Finding {
    Finding {
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
    }
}

// ---------------------------------------------------------------------------
// Edge case: empty string fields
// ---------------------------------------------------------------------------

#[test]
fn snapshot_sorted_findings_empty_fields() {
    let mut findings = vec![
        finding_full("", Severity::Error, "", None, None),
        finding_full("", Severity::Error, "", Some(""), Some(0)),
        finding_full("A1", Severity::Error, "msg", Some("src/a.rs"), Some(1)),
    ];
    sort_findings("sensor", &mut findings);
    insta::assert_json_snapshot!("sorted_findings_empty_fields", findings);
}

// ---------------------------------------------------------------------------
// Edge case: unicode in code, message, and path
// ---------------------------------------------------------------------------

#[test]
fn snapshot_sorted_findings_unicode() {
    let mut findings = vec![
        finding_full(
            "警告",
            Severity::Warn,
            "日本語メッセージ",
            Some("ソース/ファイル.rs"),
            Some(1),
        ),
        finding_full(
            "émoji🔥",
            Severity::Error,
            "c'est un problème",
            Some("café/résumé.rs"),
            Some(42),
        ),
        finding_full(
            "Ω-check",
            Severity::Info,
            "Ελληνικά",
            Some("αρχείο.rs"),
            Some(10),
        ),
    ];
    sort_findings("sensor-α", &mut findings);
    insta::assert_json_snapshot!("sorted_findings_unicode", findings);
}

// ---------------------------------------------------------------------------
// Edge case: max-length strings (stress determinism)
// ---------------------------------------------------------------------------

#[test]
fn snapshot_sorted_findings_long_strings() {
    let long_code = "E".repeat(500);
    let long_msg = "x".repeat(2000);
    let long_path = format!("src/{}.rs", "a".repeat(200));
    let mut findings = vec![
        finding_full(
            &long_code,
            Severity::Error,
            &long_msg,
            Some(&long_path),
            Some(u32::MAX),
        ),
        finding_full("A1", Severity::Error, "short", Some("src/a.rs"), Some(1)),
    ];
    sort_findings("sensor", &mut findings);
    insta::assert_json_snapshot!("sorted_findings_long_strings", findings);
}

// ---------------------------------------------------------------------------
// Edge case: findings with identical sort keys
// ---------------------------------------------------------------------------

#[test]
fn snapshot_sorted_findings_identical_keys() {
    let mut findings = vec![
        finding_full("E1", Severity::Error, "msg", Some("src/a.rs"), Some(10)),
        finding_full("E1", Severity::Error, "msg", Some("src/a.rs"), Some(10)),
        finding_full("E1", Severity::Error, "msg", Some("src/a.rs"), Some(10)),
    ];
    sort_findings("sensor", &mut findings);
    insta::assert_json_snapshot!("sorted_findings_identical_keys", findings);
}

// ---------------------------------------------------------------------------
// Edge case: zero-cap findings
// ---------------------------------------------------------------------------

#[test]
fn snapshot_cap_findings_zero() {
    let findings = vec![
        finding_full("E1", Severity::Error, "err", Some("a.rs"), Some(1)),
        finding_full("W1", Severity::Warn, "warn", Some("b.rs"), Some(2)),
    ];
    let (capped, truncated) = cap_findings(findings, 0);
    assert!(truncated);
    insta::assert_json_snapshot!("capped_findings_zero", capped);
}

// ---------------------------------------------------------------------------
// Edge case: compute_counts with all zeroes
// ---------------------------------------------------------------------------

#[test]
fn snapshot_compute_counts_empty() {
    let counts = compute_counts(&[]);
    insta::assert_json_snapshot!("compute_counts_empty", counts);
}

// ---------------------------------------------------------------------------
// Edge case: highlight selection max_highlights=0
// ---------------------------------------------------------------------------

#[test]
fn snapshot_highlight_selection_zero_budget() {
    let mut cfg = CockpitConfig::default();
    cfg.policy.max_highlights = 0;

    let candidates = vec![Highlight {
        sensor_id: "s1".to_string(),
        finding: finding_full("E1", Severity::Error, "err", Some("a.rs"), Some(1)),
    }];
    let selected = select_highlights(candidates, &cfg, &std::collections::BTreeMap::new());
    insta::assert_json_snapshot!("highlight_selection_zero_budget", selected);
}

// ---------------------------------------------------------------------------
// Edge case: fingerprint stability with unicode
// ---------------------------------------------------------------------------

#[test]
fn snapshot_fingerprint_unicode() {
    let f = finding_full(
        "日本語",
        Severity::Error,
        "テストメッセージ",
        Some("ソース.rs"),
        Some(42),
    );
    let fp = derive_fingerprint("sensor-日本", &f);
    insta::assert_snapshot!("fingerprint_unicode", fp);
}

// ---------------------------------------------------------------------------
// Edge case: receipt inconsistent counts
// ---------------------------------------------------------------------------

#[test]
fn snapshot_receipt_inconsistent() {
    let reported = VerdictCounts {
        info: 5,
        warn: 3,
        error: 1,
        suppressed: 0,
    };
    let computed = VerdictCounts {
        info: 2,
        warn: 1,
        error: 0,
        suppressed: 0,
    };
    let highlight = synthesize_receipt_inconsistent("buggy-sensor", &reported, &computed);
    insta::assert_json_snapshot!("receipt_inconsistent", highlight);
}

// ---------------------------------------------------------------------------
// Edge case: findings with None locations mixed with Some
// ---------------------------------------------------------------------------

#[test]
fn snapshot_sorted_findings_mixed_locations() {
    let mut findings = vec![
        finding_full("W1", Severity::Warn, "no location", None, None),
        finding_full(
            "E1",
            Severity::Error,
            "has location",
            Some("src/main.rs"),
            Some(10),
        ),
        finding_full("E2", Severity::Error, "no path", None, None),
        finding_full(
            "I1",
            Severity::Info,
            "deep path",
            Some("a/b/c/d/e/f.rs"),
            Some(999),
        ),
    ];
    sort_findings("mixed", &mut findings);
    insta::assert_json_snapshot!("sorted_findings_mixed_locations", findings);
}
