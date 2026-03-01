//! Expanded edge case and boundary condition tests for cockpitctl-domain.
//!
//! Covers policy evaluation, highlight selection, normalization,
//! fingerprinting, and composition invariants.

use cockpitctl_domain::{
    build_cockpit_report, cap_findings, compute_counts, derive_fingerprint, finding_sort_key,
    overall_verdict, select_highlights, sort_findings, summarize_sensor_report,
};
use cockpitctl_types::{
    CockpitConfig, Finding, Highlight, Location, MissingPolicy, Presence, RunInfo, SensorPolicy,
    SensorReport, SensorSummary, Severity, ToolInfo, Verdict, VerdictCounts, VerdictStatus,
};
use std::collections::BTreeMap;

// ============================================================================
// Helpers
// ============================================================================

fn tool_info() -> ToolInfo {
    ToolInfo {
        name: "test".to_string(),
        version: "1.0.0".to_string(),
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

fn make_policy(blocking: bool) -> SensorPolicy {
    SensorPolicy {
        blocking,
        missing: MissingPolicy::Fail,
        section: None,
        require_label: None,
        repro: None,
    }
}

fn make_summary(id: &str, blocking: bool, status: VerdictStatus) -> SensorSummary {
    SensorSummary {
        id: id.to_string(),
        blocking,
        missing: MissingPolicy::Fail,
        presence: Presence::Present,
        report_path: format!("artifacts/{}/report.json", id),
        comment_path: None,
        verdict: Verdict {
            status: status.clone(),
            counts: VerdictCounts::default(),
            reasons: vec![],
        },
        truncated: false,
        errors: vec![],
        missing_policy_applied: None,
        policy_outcome: None,
    }
}

fn make_finding(code: &str, severity: Severity) -> Finding {
    Finding {
        severity,
        check_id: None,
        code: code.to_string(),
        message: format!("Message for {}", code),
        location: Some(Location {
            path: Some("src/lib.rs".to_string()),
            line: Some(10),
            col: None,
        }),
        help: None,
        url: None,
        fingerprint: None,
        data: None,
    }
}

fn make_highlight(sensor_id: &str, code: &str, severity: Severity) -> Highlight {
    Highlight {
        sensor_id: sensor_id.to_string(),
        finding: make_finding(code, severity),
    }
}

fn make_sensor_report(status: VerdictStatus, findings: Vec<Finding>) -> SensorReport {
    let counts = compute_counts(&findings);
    SensorReport {
        schema: "sensor.report.v1".to_string(),
        tool: tool_info(),
        run: run_info(),
        verdict: Verdict {
            status,
            counts,
            reasons: vec![],
        },
        findings,
        artifacts: vec![],
        data: None,
    }
}

// ============================================================================
// 1. Policy evaluation edge cases
// ============================================================================

#[test]
fn policy_all_pass_one_warn_nonblocking_is_pass() {
    let summaries = vec![
        make_summary("a", true, VerdictStatus::Pass),
        make_summary("b", true, VerdictStatus::Pass),
        make_summary("c", false, VerdictStatus::Warn),
    ];
    let cfg = CockpitConfig::default();
    let v = overall_verdict(&summaries, &cfg);
    assert_eq!(v.status, VerdictStatus::Pass);
}

#[test]
fn policy_one_fail_one_skip_is_fail() {
    let summaries = vec![
        make_summary("a", true, VerdictStatus::Fail),
        make_summary("b", true, VerdictStatus::Skip),
    ];
    let cfg = CockpitConfig::default();
    let v = overall_verdict(&summaries, &cfg);
    assert_eq!(v.status, VerdictStatus::Fail);
}

#[test]
fn policy_all_skip_is_pass() {
    // When all blocking sensors are skip, worst starts at Pass and never
    // changes because skip (rank 3) is not worse than pass (rank 2).
    let summaries = vec![
        make_summary("a", true, VerdictStatus::Skip),
        make_summary("b", true, VerdictStatus::Skip),
    ];
    let cfg = CockpitConfig::default();
    let v = overall_verdict(&summaries, &cfg);
    // skip does not lower worst below pass
    assert_eq!(v.status, VerdictStatus::Pass);
}

#[test]
fn policy_empty_sensors_is_pass() {
    let cfg = CockpitConfig::default();
    let v = overall_verdict(&[], &cfg);
    assert_eq!(v.status, VerdictStatus::Pass);
    assert!(v.reasons.is_empty());
}

#[test]
fn policy_warn_is_fail_with_warn_becomes_fail() {
    let summaries = vec![make_summary("a", true, VerdictStatus::Warn)];
    let mut cfg = CockpitConfig::default();
    cfg.policy.warn_is_fail = true;
    let v = overall_verdict(&summaries, &cfg);
    assert_eq!(v.status, VerdictStatus::Fail);
    assert!(v.reasons.contains(&"warn_is_fail".to_string()));
}

#[test]
fn policy_warn_is_fail_disabled_keeps_warn() {
    let summaries = vec![make_summary("a", true, VerdictStatus::Warn)];
    let cfg = CockpitConfig::default();
    let v = overall_verdict(&summaries, &cfg);
    assert_eq!(v.status, VerdictStatus::Warn);
    assert!(!v.reasons.contains(&"warn_is_fail".to_string()));
}

#[test]
fn policy_nonblocking_fail_does_not_affect_verdict() {
    let summaries = vec![
        make_summary("a", true, VerdictStatus::Pass),
        make_summary("b", false, VerdictStatus::Fail),
    ];
    let cfg = CockpitConfig::default();
    let v = overall_verdict(&summaries, &cfg);
    assert_eq!(v.status, VerdictStatus::Pass);
}

// ============================================================================
// 2. Highlight selection edge cases
// ============================================================================

#[test]
fn highlights_budget_zero_returns_empty() {
    let mut cfg = CockpitConfig::default();
    cfg.policy.max_highlights = 0;
    let candidates = vec![make_highlight("s", "E1", Severity::Error)];
    let selected = select_highlights(candidates, &cfg, &BTreeMap::new());
    assert!(selected.is_empty());
}

#[test]
fn highlights_budget_one_returns_highest_severity() {
    let mut cfg = CockpitConfig::default();
    cfg.policy.max_highlights = 1;
    let candidates = vec![
        make_highlight("s", "I1", Severity::Info),
        make_highlight("s", "E1", Severity::Error),
        make_highlight("s", "W1", Severity::Warn),
    ];
    let selected = select_highlights(candidates, &cfg, &BTreeMap::new());
    assert_eq!(selected.len(), 1);
    assert_eq!(selected[0].finding.severity, Severity::Error);
}

#[test]
fn highlights_budget_exceeds_findings_returns_all() {
    let mut cfg = CockpitConfig::default();
    cfg.policy.max_highlights = 100;
    let candidates = vec![
        make_highlight("s", "E1", Severity::Error),
        make_highlight("s", "W1", Severity::Warn),
    ];
    let selected = select_highlights(candidates, &cfg, &BTreeMap::new());
    assert_eq!(selected.len(), 2);
}

#[test]
fn highlights_equal_severity_deterministic_order() {
    let mut cfg = CockpitConfig::default();
    cfg.policy.max_highlights = 10;
    let candidates = vec![
        make_highlight("s", "E_B", Severity::Error),
        make_highlight("s", "E_A", Severity::Error),
    ];
    let selected_forward = select_highlights(candidates.clone(), &cfg, &BTreeMap::new());

    let reversed = vec![candidates[1].clone(), candidates[0].clone()];
    let selected_reversed = select_highlights(reversed, &cfg, &BTreeMap::new());

    // Same deterministic order regardless of input order
    let ids_forward: Vec<_> = selected_forward
        .iter()
        .map(|h| h.finding.code.as_str())
        .collect();
    let ids_reversed: Vec<_> = selected_reversed
        .iter()
        .map(|h| h.finding.code.as_str())
        .collect();
    assert_eq!(ids_forward, ids_reversed);
}

#[test]
fn highlights_blocking_takes_priority_over_nonblocking() {
    let mut cfg = CockpitConfig::default();
    cfg.policy.max_highlights = 10;

    let mut sensor_blocking = BTreeMap::new();
    sensor_blocking.insert("blocker".to_string(), true);
    sensor_blocking.insert("nonblock".to_string(), false);

    let candidates = vec![
        make_highlight("nonblock", "E1", Severity::Error),
        make_highlight("blocker", "E2", Severity::Error),
    ];
    let selected = select_highlights(candidates, &cfg, &sensor_blocking);
    assert_eq!(selected.len(), 2);
    // Blocking sensor's highlight should come first
    assert_eq!(selected[0].sensor_id, "blocker");
    assert_eq!(selected[1].sensor_id, "nonblock");
}

// ============================================================================
// 3. Normalization edge cases
// ============================================================================

#[test]
fn finding_with_none_path_handled() {
    let f = Finding {
        severity: Severity::Error,
        check_id: None,
        code: "E1".to_string(),
        message: "msg".to_string(),
        location: None,
        help: None,
        url: None,
        fingerprint: None,
        data: None,
    };
    let key = finding_sort_key("sensor", &f);
    assert_eq!(key.path, "");
    assert_eq!(key.line, u32::MAX);
}

#[test]
fn finding_with_empty_message_handled() {
    let f = Finding {
        severity: Severity::Warn,
        check_id: None,
        code: "W1".to_string(),
        message: String::new(),
        location: None,
        help: None,
        url: None,
        fingerprint: None,
        data: None,
    };
    let key = finding_sort_key("sensor", &f);
    assert_eq!(key.message, "");
    // Should still produce a valid fingerprint
    let fp = derive_fingerprint("sensor", &f);
    assert_eq!(fp.len(), 64);
}

#[test]
fn finding_with_line_zero_handled() {
    let f = Finding {
        severity: Severity::Info,
        check_id: None,
        code: "I1".to_string(),
        message: "msg".to_string(),
        location: Some(Location {
            path: Some("file.rs".to_string()),
            line: Some(0),
            col: None,
        }),
        help: None,
        url: None,
        fingerprint: None,
        data: None,
    };
    let key = finding_sort_key("sensor", &f);
    assert_eq!(key.line, 0);
}

#[test]
fn finding_with_no_code_empty_string() {
    let f = Finding {
        severity: Severity::Error,
        check_id: None,
        code: String::new(),
        message: "something".to_string(),
        location: None,
        help: None,
        url: None,
        fingerprint: None,
        data: None,
    };
    let key = finding_sort_key("sensor", &f);
    assert_eq!(key.code, "");
    let fp = derive_fingerprint("sensor", &f);
    assert_eq!(fp.len(), 64);
}

#[test]
fn very_long_message_not_truncated_in_domain() {
    let long_msg = "x".repeat(100_000);
    let f = Finding {
        severity: Severity::Error,
        check_id: None,
        code: "E1".to_string(),
        message: long_msg.clone(),
        location: None,
        help: None,
        url: None,
        fingerprint: None,
        data: None,
    };
    // Domain does not truncate; render layer is responsible for that.
    let key = finding_sort_key("sensor", &f);
    assert_eq!(key.message.len(), 100_000);

    let fp = derive_fingerprint("sensor", &f);
    assert_eq!(fp.len(), 64);
}

// ============================================================================
// 4. Fingerprint edge cases
// ============================================================================

#[test]
fn fingerprint_differs_by_message_only() {
    let f1 = Finding {
        severity: Severity::Error,
        check_id: None,
        code: "E1".to_string(),
        message: "message A".to_string(),
        location: Some(Location {
            path: Some("src/lib.rs".to_string()),
            line: Some(10),
            col: None,
        }),
        help: None,
        url: None,
        fingerprint: None,
        data: None,
    };
    let mut f2 = f1.clone();
    f2.message = "message B".to_string();

    let fp1 = derive_fingerprint("sensor", &f1);
    let fp2 = derive_fingerprint("sensor", &f2);
    assert_ne!(fp1, fp2);
}

#[test]
fn fingerprint_differs_by_line_only() {
    let f1 = Finding {
        severity: Severity::Error,
        check_id: None,
        code: "E1".to_string(),
        message: "msg".to_string(),
        location: Some(Location {
            path: Some("src/lib.rs".to_string()),
            line: Some(10),
            col: None,
        }),
        help: None,
        url: None,
        fingerprint: None,
        data: None,
    };
    let mut f2 = f1.clone();
    f2.location.as_mut().unwrap().line = Some(20);

    let fp1 = derive_fingerprint("sensor", &f1);
    let fp2 = derive_fingerprint("sensor", &f2);
    assert_ne!(fp1, fp2);
}

#[test]
fn fingerprint_all_empty_fields_is_valid() {
    let f = Finding {
        severity: Severity::Info,
        check_id: None,
        code: String::new(),
        message: String::new(),
        location: None,
        help: None,
        url: None,
        fingerprint: None,
        data: None,
    };
    let fp = derive_fingerprint("", &f);
    assert_eq!(fp.len(), 64);
    // Should be valid hex
    assert!(fp.chars().all(|c| c.is_ascii_hexdigit()));
}

#[test]
fn fingerprint_is_hex_sha256() {
    let f = make_finding("CODE", Severity::Error);
    let fp = derive_fingerprint("sensor", &f);
    assert_eq!(fp.len(), 64, "SHA-256 hex is exactly 64 characters");
    assert!(fp.chars().all(|c| c.is_ascii_hexdigit()));
}

#[test]
fn fingerprint_deterministic_across_calls() {
    let f = make_finding("CODE", Severity::Error);
    let fp1 = derive_fingerprint("sensor", &f);
    let fp2 = derive_fingerprint("sensor", &f);
    assert_eq!(fp1, fp2);
}

#[test]
fn fingerprint_differs_by_sensor_id() {
    let f = make_finding("CODE", Severity::Error);
    let fp1 = derive_fingerprint("sensor_a", &f);
    let fp2 = derive_fingerprint("sensor_b", &f);
    assert_ne!(fp1, fp2);
}

// ============================================================================
// 5. Composition edge cases
// ============================================================================

#[test]
fn sensor_order_does_not_affect_verdict() {
    let cfg = CockpitConfig::default();

    let summaries_ab = vec![
        make_summary("a", true, VerdictStatus::Fail),
        make_summary("b", true, VerdictStatus::Pass),
    ];
    let summaries_ba = vec![
        make_summary("b", true, VerdictStatus::Pass),
        make_summary("a", true, VerdictStatus::Fail),
    ];

    let v_ab = overall_verdict(&summaries_ab, &cfg);
    let v_ba = overall_verdict(&summaries_ba, &cfg);
    assert_eq!(v_ab.status, v_ba.status);
    assert_eq!(v_ab.status, VerdictStatus::Fail);
}

#[test]
fn finding_order_does_not_affect_verdict() {
    let mut findings_ab = vec![
        make_finding("E1", Severity::Error),
        make_finding("W1", Severity::Warn),
    ];
    let mut findings_ba = vec![
        make_finding("W1", Severity::Warn),
        make_finding("E1", Severity::Error),
    ];

    sort_findings("sensor", &mut findings_ab);
    sort_findings("sensor", &mut findings_ba);

    // After sorting, the order is identical
    let codes_ab: Vec<_> = findings_ab.iter().map(|f| f.code.as_str()).collect();
    let codes_ba: Vec<_> = findings_ba.iter().map(|f| f.code.as_str()).collect();
    assert_eq!(codes_ab, codes_ba);
}

#[test]
fn adding_skip_sensor_does_not_change_verdict() {
    let cfg = CockpitConfig::default();

    let summaries_no_skip = vec![
        make_summary("a", true, VerdictStatus::Fail),
        make_summary("b", true, VerdictStatus::Pass),
    ];
    let summaries_with_skip = vec![
        make_summary("a", true, VerdictStatus::Fail),
        make_summary("b", true, VerdictStatus::Pass),
        make_summary("c", true, VerdictStatus::Skip),
    ];

    let v1 = overall_verdict(&summaries_no_skip, &cfg);
    let v2 = overall_verdict(&summaries_with_skip, &cfg);
    assert_eq!(v1.status, v2.status);
}

#[test]
fn removing_skip_sensor_does_not_change_verdict() {
    let cfg = CockpitConfig::default();

    let summaries_with = vec![
        make_summary("a", true, VerdictStatus::Pass),
        make_summary("b", true, VerdictStatus::Skip),
    ];
    let summaries_without = vec![make_summary("a", true, VerdictStatus::Pass)];

    let v1 = overall_verdict(&summaries_with, &cfg);
    let v2 = overall_verdict(&summaries_without, &cfg);
    assert_eq!(v1.status, v2.status);
}

// ============================================================================
// 6. Additional boundary tests
// ============================================================================

#[test]
fn cap_findings_zero_cap_returns_empty() {
    let findings = vec![make_finding("E1", Severity::Error)];
    let (capped, truncated) = cap_findings(findings, 0);
    assert!(capped.is_empty());
    assert!(truncated);
}

#[test]
fn cap_findings_exact_cap_not_truncated() {
    let findings = vec![
        make_finding("E1", Severity::Error),
        make_finding("E2", Severity::Error),
    ];
    let (capped, truncated) = cap_findings(findings, 2);
    assert_eq!(capped.len(), 2);
    assert!(!truncated);
}

#[test]
fn compute_counts_empty_findings() {
    let counts = compute_counts(&[]);
    assert_eq!(counts.info, 0);
    assert_eq!(counts.warn, 0);
    assert_eq!(counts.error, 0);
}

#[test]
fn summarize_sensor_report_zero_max_findings() {
    let report = make_sensor_report(
        VerdictStatus::Fail,
        vec![make_finding("E1", Severity::Error)],
    );
    let policy = make_policy(true);
    let (summary, highlights) = summarize_sensor_report(
        "sensor",
        "artifacts/sensor/report.json",
        None,
        &policy,
        report,
        0,
    );
    assert!(summary.truncated);
    // Only the inconsistency highlight (counts recomputed from empty surfaced set)
    assert_eq!(highlights.len(), 1);
    assert!(
        summary
            .verdict
            .reasons
            .contains(&"receipt_inconsistent".to_string())
    );
}

#[test]
fn build_cockpit_report_empty_is_pass() {
    let cfg = CockpitConfig::default();
    let report = build_cockpit_report(&cfg, tool_info(), run_info(), vec![], vec![]);
    assert_eq!(report.schema, "cockpit.report.v1");
    assert_eq!(report.verdict.status, VerdictStatus::Pass);
    assert!(report.sensors.is_empty());
    assert!(report.highlights.is_empty());
}

#[test]
fn policy_fail_plus_warn_is_still_fail() {
    let summaries = vec![
        make_summary("a", true, VerdictStatus::Fail),
        make_summary("b", true, VerdictStatus::Warn),
    ];
    let cfg = CockpitConfig::default();
    let v = overall_verdict(&summaries, &cfg);
    assert_eq!(v.status, VerdictStatus::Fail);
}

#[test]
fn highlights_deduplication_by_fingerprint() {
    let mut cfg = CockpitConfig::default();
    cfg.policy.max_highlights = 10;

    // Two highlights with identical content → same derived fingerprint → deduplicated
    let h1 = make_highlight("s", "E1", Severity::Error);
    let h2 = make_highlight("s", "E1", Severity::Error);

    let selected = select_highlights(vec![h1, h2], &cfg, &BTreeMap::new());
    assert_eq!(selected.len(), 1);
}

#[test]
fn sort_findings_stable_for_identical_entries() {
    let mut findings = vec![
        make_finding("E1", Severity::Error),
        make_finding("E1", Severity::Error),
    ];
    sort_findings("sensor", &mut findings);
    // Both are identical so stable sort should keep them in order
    assert_eq!(findings[0].code, "E1");
    assert_eq!(findings[1].code, "E1");
}

#[test]
fn overall_verdict_counts_aggregate_across_all_sensors() {
    let mut s1 = make_summary("a", true, VerdictStatus::Pass);
    s1.verdict.counts = VerdictCounts {
        info: 1,
        warn: 2,
        error: 0,
        suppressed: 0,
    };
    let mut s2 = make_summary("b", false, VerdictStatus::Warn);
    s2.verdict.counts = VerdictCounts {
        info: 3,
        warn: 0,
        error: 1,
        suppressed: 0,
    };

    let cfg = CockpitConfig::default();
    let v = overall_verdict(&[s1, s2], &cfg);
    // Counts are aggregated from ALL sensors (including non-blocking)
    assert_eq!(v.counts.info, 4);
    assert_eq!(v.counts.warn, 2);
    assert_eq!(v.counts.error, 1);
}

#[test]
fn finding_location_with_path_but_no_line() {
    let f = Finding {
        severity: Severity::Warn,
        check_id: None,
        code: "W1".to_string(),
        message: "msg".to_string(),
        location: Some(Location {
            path: Some("Cargo.toml".to_string()),
            line: None,
            col: None,
        }),
        help: None,
        url: None,
        fingerprint: None,
        data: None,
    };
    let key = finding_sort_key("sensor", &f);
    assert_eq!(key.path, "Cargo.toml");
    assert_eq!(key.line, u32::MAX);
}
