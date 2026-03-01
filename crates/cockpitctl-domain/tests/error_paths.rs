//! Error path tests for the domain crate.
//!
//! Verifies deterministic behavior for edge cases: all-skips verdict,
//! zero budgets, empty inputs, equal-priority sort stability, and
//! contradictory policy rules.

use cockpitctl_domain::{
    build_cockpit_report, cap_findings, compute_counts, compute_policy_outcome, derive_fingerprint,
    finding_sort_key, overall_verdict, select_highlights, sort_findings, sort_sensor_summaries,
    summarize_sensor_report, synthesize_invalid_sensor, synthesize_missing_sensor,
};
use cockpitctl_types::*;
use std::collections::BTreeMap;

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

fn make_sensor_summary(id: &str, blocking: bool, status: VerdictStatus) -> SensorSummary {
    SensorSummary {
        id: id.to_string(),
        blocking,
        missing: MissingPolicy::Skip,
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

// ============================================================================
// Verdict aggregation edge cases
// ============================================================================

/// All sensors skipped → overall verdict is pass (skip doesn't worsen).
#[test]
fn verdict_all_skips_yields_pass() {
    let summaries = vec![
        make_sensor_summary("s1", true, VerdictStatus::Skip),
        make_sensor_summary("s2", true, VerdictStatus::Skip),
    ];
    let cfg = CockpitConfig::default();
    let verdict = overall_verdict(&summaries, &cfg);
    assert_eq!(
        verdict.status,
        VerdictStatus::Pass,
        "all-skip blocking sensors should not worsen verdict below pass"
    );
}

/// No sensors at all → pass verdict.
#[test]
fn verdict_no_sensors_yields_pass() {
    let cfg = CockpitConfig::default();
    let verdict = overall_verdict(&[], &cfg);
    assert_eq!(verdict.status, VerdictStatus::Pass);
}

/// Mixed blocking verdicts: fail wins over warn and pass.
#[test]
fn verdict_fail_wins_over_warn_and_pass() {
    let summaries = vec![
        make_sensor_summary("s1", true, VerdictStatus::Pass),
        make_sensor_summary("s2", true, VerdictStatus::Warn),
        make_sensor_summary("s3", true, VerdictStatus::Fail),
    ];
    let cfg = CockpitConfig::default();
    let verdict = overall_verdict(&summaries, &cfg);
    assert_eq!(verdict.status, VerdictStatus::Fail);
}

/// Non-blocking fail does NOT affect overall verdict.
#[test]
fn non_blocking_fail_does_not_affect_verdict() {
    let summaries = vec![
        make_sensor_summary("s1", false, VerdictStatus::Fail),
        make_sensor_summary("s2", true, VerdictStatus::Pass),
    ];
    let cfg = CockpitConfig::default();
    let verdict = overall_verdict(&summaries, &cfg);
    assert_eq!(
        verdict.status,
        VerdictStatus::Pass,
        "non-blocking fail should be informational only"
    );
}

/// warn_is_fail policy: blocking warn → treated as fail.
#[test]
fn warn_is_fail_promotes_warn_to_fail() {
    let summaries = vec![make_sensor_summary("s1", true, VerdictStatus::Warn)];
    let mut cfg = CockpitConfig::default();
    cfg.policy.warn_is_fail = true;
    let verdict = overall_verdict(&summaries, &cfg);
    assert_eq!(verdict.status, VerdictStatus::Fail);
    assert!(verdict.reasons.contains(&"warn_is_fail".to_string()));
}

// ============================================================================
// Findings cap and counts
// ============================================================================

/// cap_findings with zero budget → empty, truncated.
#[test]
fn cap_findings_zero_budget_returns_empty() {
    let findings = vec![make_finding(Severity::Error, "E1", "err")];
    let (capped, truncated) = cap_findings(findings, 0);
    assert!(capped.is_empty());
    assert!(truncated);
}

/// cap_findings with empty input → empty, not truncated.
#[test]
fn cap_findings_empty_input() {
    let (capped, truncated) = cap_findings(vec![], 10);
    assert!(capped.is_empty());
    assert!(!truncated);
}

/// compute_counts with empty findings → all zeros.
#[test]
fn compute_counts_empty() {
    let counts = compute_counts(&[]);
    assert_eq!(counts.info, 0);
    assert_eq!(counts.warn, 0);
    assert_eq!(counts.error, 0);
}

// ============================================================================
// Fingerprint derivation
// ============================================================================

/// Fingerprint of empty-code finding → deterministic 64-char hex.
#[test]
fn fingerprint_empty_input_deterministic() {
    let finding = make_finding(Severity::Info, "", "");
    let fp = derive_fingerprint("", &finding);
    assert_eq!(fp.len(), 64, "SHA-256 hex should be 64 chars");

    // Same inputs always produce the same fingerprint.
    let fp2 = derive_fingerprint("", &finding);
    assert_eq!(fp, fp2);
}

/// Different sensor_id → different fingerprint.
#[test]
fn fingerprint_differs_by_sensor_id() {
    let finding = make_finding(Severity::Error, "E1", "msg");
    let fp_a = derive_fingerprint("sensor-a", &finding);
    let fp_b = derive_fingerprint("sensor-b", &finding);
    assert_ne!(fp_a, fp_b);
}

/// Fingerprint with location vs without → different.
#[test]
fn fingerprint_differs_with_location() {
    let f1 = make_finding(Severity::Error, "E1", "msg");
    let f2 = make_finding_with_location(Severity::Error, "E1", "msg", "src/lib.rs", 42);
    let fp1 = derive_fingerprint("s", &f1);
    let fp2 = derive_fingerprint("s", &f2);
    assert_ne!(fp1, fp2);
}

// ============================================================================
// Sort stability and determinism
// ============================================================================

/// Sort findings with identical severity and sensor → stable by code then message.
#[test]
fn sort_stability_equal_priority_findings() {
    let mut findings = vec![
        make_finding(Severity::Error, "E2", "beta"),
        make_finding(Severity::Error, "E1", "alpha"),
        make_finding(Severity::Error, "E1", "beta"),
    ];
    sort_findings("sensor", &mut findings);

    // Should be sorted: E1/alpha < E1/beta < E2/beta
    assert_eq!(findings[0].code, "E1");
    assert_eq!(findings[0].message, "alpha");
    assert_eq!(findings[1].code, "E1");
    assert_eq!(findings[1].message, "beta");
    assert_eq!(findings[2].code, "E2");
    assert_eq!(findings[2].message, "beta");
}

/// Sort findings with no location → deterministic (no crash).
#[test]
fn sort_findings_no_location_no_crash() {
    let mut findings = vec![
        make_finding(Severity::Warn, "W1", "msg"),
        make_finding(Severity::Error, "E1", "msg"),
    ];
    sort_findings("s", &mut findings);
    assert_eq!(findings[0].severity, Severity::Error);
    assert_eq!(findings[1].severity, Severity::Warn);
}

/// finding_sort_key with None location uses empty path and u32::MAX line.
#[test]
fn finding_sort_key_no_location_defaults() {
    let finding = make_finding(Severity::Info, "I1", "msg");
    let key = finding_sort_key("s", &finding);
    assert_eq!(key.path, "");
    assert_eq!(key.line, u32::MAX);
}

// ============================================================================
// Highlight selection edge cases
// ============================================================================

/// Zero max_highlights → empty highlights.
#[test]
fn highlights_zero_budget_returns_empty() {
    let candidates = vec![make_highlight(
        "s1",
        make_finding(Severity::Error, "E1", "msg"),
    )];
    let mut cfg = CockpitConfig::default();
    cfg.policy.max_highlights = 0;
    let blocking = BTreeMap::new();

    let selected = select_highlights(candidates, &cfg, &blocking);
    assert!(selected.is_empty());
}

/// Empty candidates → empty highlights (no crash).
#[test]
fn highlights_empty_candidates() {
    let cfg = CockpitConfig::default();
    let blocking = BTreeMap::new();
    let selected = select_highlights(vec![], &cfg, &blocking);
    assert!(selected.is_empty());
}

/// Duplicate fingerprints are deduplicated.
#[test]
fn highlights_deduplicates_by_fingerprint() {
    let mut f1 = make_finding(Severity::Error, "E1", "msg");
    f1.fingerprint = Some("same-fp".to_string());
    let mut f2 = make_finding(Severity::Error, "E1", "msg");
    f2.fingerprint = Some("same-fp".to_string());

    let candidates = vec![make_highlight("s1", f1), make_highlight("s2", f2)];
    let cfg = CockpitConfig::default();
    let blocking = BTreeMap::new();

    let selected = select_highlights(candidates, &cfg, &blocking);
    assert_eq!(
        selected.len(),
        1,
        "duplicate fingerprints should be deduped"
    );
}

/// Blocking sensors sort before non-blocking in highlights.
#[test]
fn highlights_blocking_sorts_first() {
    let candidates = vec![
        make_highlight("non-blocking", make_finding(Severity::Error, "E1", "a")),
        make_highlight("blocking", make_finding(Severity::Error, "E1", "b")),
    ];
    let cfg = CockpitConfig::default();
    let blocking = BTreeMap::from([
        ("blocking".to_string(), true),
        ("non-blocking".to_string(), false),
    ]);

    let selected = select_highlights(candidates, &cfg, &blocking);
    assert_eq!(selected[0].sensor_id, "blocking");
}

// ============================================================================
// Policy outcome
// ============================================================================

/// Non-blocking sensor → always informational regardless of status.
#[test]
fn policy_outcome_non_blocking_always_informational() {
    assert_eq!(
        compute_policy_outcome(false, &VerdictStatus::Fail),
        PolicyOutcome::Informational
    );
    assert_eq!(
        compute_policy_outcome(false, &VerdictStatus::Pass),
        PolicyOutcome::Informational
    );
}

/// Blocking + fail → blocked; blocking + pass → allowed.
#[test]
fn policy_outcome_blocking_variants() {
    assert_eq!(
        compute_policy_outcome(true, &VerdictStatus::Fail),
        PolicyOutcome::Blocked
    );
    assert_eq!(
        compute_policy_outcome(true, &VerdictStatus::Pass),
        PolicyOutcome::Allowed
    );
    assert_eq!(
        compute_policy_outcome(true, &VerdictStatus::Warn),
        PolicyOutcome::Allowed
    );
    assert_eq!(
        compute_policy_outcome(true, &VerdictStatus::Skip),
        PolicyOutcome::Allowed
    );
}

// ============================================================================
// Synthesize edge cases
// ============================================================================

/// Missing sensor with skip policy → skip verdict, no highlight.
#[test]
fn synthesize_missing_skip_no_highlight() {
    let policy = SensorPolicy {
        missing: MissingPolicy::Skip,
        ..Default::default()
    };
    let (summary, highlight) =
        synthesize_missing_sensor("s1", &policy, "artifacts/s1/report.json", None);
    assert_eq!(summary.verdict.status, VerdictStatus::Skip);
    assert!(
        highlight.is_none(),
        "skip policy should not emit a highlight"
    );
}

/// Missing sensor with fail policy → fail verdict + highlight.
#[test]
fn synthesize_missing_fail_produces_highlight() {
    let policy = SensorPolicy {
        missing: MissingPolicy::Fail,
        blocking: true,
        ..Default::default()
    };
    let (summary, highlight) =
        synthesize_missing_sensor("s1", &policy, "artifacts/s1/report.json", None);
    assert_eq!(summary.verdict.status, VerdictStatus::Fail);
    assert!(highlight.is_some());
}

/// Invalid sensor produces error highlight with proper code.
#[test]
fn synthesize_invalid_sensor_highlight_code() {
    let policy = SensorPolicy::default();
    let (summary, highlight) = synthesize_invalid_sensor(
        "bad",
        &policy,
        "artifacts/bad/report.json",
        None,
        "unexpected EOF".to_string(),
    );
    assert_eq!(summary.presence, Presence::Invalid);
    let h = highlight.unwrap();
    assert_eq!(h.finding.code, "cockpit.invalid_receipt");
}

// ============================================================================
// build_cockpit_report edge cases
// ============================================================================

/// Empty summaries and highlights → valid report with pass.
#[test]
fn build_report_empty_produces_pass() {
    let cfg = CockpitConfig::default();
    let report = build_cockpit_report(&cfg, make_tool(), make_run(), vec![], vec![]);
    assert_eq!(report.schema, "cockpit.report.v1");
    assert_eq!(report.verdict.status, VerdictStatus::Pass);
    assert!(report.sensors.is_empty());
    assert!(report.highlights.is_empty());
}

/// sort_sensor_summaries with empty section_order and no sensor configs → sorted by ID.
#[test]
fn sort_sensor_summaries_by_id_fallback() {
    let mut summaries = vec![
        make_sensor_summary("zebra", false, VerdictStatus::Pass),
        make_sensor_summary("alpha", false, VerdictStatus::Pass),
    ];
    let mut cfg = CockpitConfig::default();
    cfg.policy.section_order = vec![];

    sort_sensor_summaries(&mut summaries, &cfg);
    assert_eq!(summaries[0].id, "alpha");
    assert_eq!(summaries[1].id, "zebra");
}

/// summarize_sensor_report with findings exceeding cap → truncated flag set.
#[test]
fn summarize_sensor_report_caps_findings() {
    let mut findings = Vec::new();
    for i in 0..50 {
        findings.push(make_finding(Severity::Info, &format!("I{}", i), "info"));
    }

    let sensor_report = SensorReport {
        schema: "sensor.report.v1".into(),
        tool: make_tool(),
        run: make_run(),
        verdict: Verdict {
            status: VerdictStatus::Warn,
            counts: VerdictCounts {
                info: 50,
                warn: 0,
                error: 0,
                suppressed: 0,
            },
            reasons: vec![],
        },
        findings,
        artifacts: vec![],
        data: None,
    };

    let policy = SensorPolicy::default();
    let (summary, _highlights) = summarize_sensor_report(
        "s1",
        "artifacts/s1/report.json",
        None,
        &policy,
        sensor_report,
        20, // cap at 20
    );

    assert!(summary.truncated, "findings should be truncated at cap");
}
