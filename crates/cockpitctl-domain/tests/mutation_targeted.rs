//! Mutation-targeted tests for cockpitctl-domain.
//!
//! Each test specifically catches a mutant that survived previous cargo-mutants analysis.

use cockpitctl_domain::*;
use cockpitctl_types::*;
use std::collections::BTreeMap;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn finding(code: &str, sev: Severity) -> Finding {
    Finding {
        severity: sev,
        check_id: None,
        code: code.into(),
        message: format!("msg-{code}"),
        location: None,
        help: None,
        url: None,
        fingerprint: None,
        data: None,
    }
}

fn finding_with_loc(code: &str, sev: Severity, path: &str, line: u32) -> Finding {
    Finding {
        severity: sev,
        check_id: None,
        code: code.into(),
        message: format!("msg-{code}"),
        location: Some(Location {
            path: Some(path.into()),
            line: Some(line),
            col: None,
        }),
        help: None,
        url: None,
        fingerprint: None,
        data: None,
    }
}

fn policy(blocking: bool) -> SensorPolicy {
    SensorPolicy {
        blocking,
        missing: MissingPolicy::Skip,
        section: None,
        require_label: None,
        repro: None,
    }
}

fn summary(
    id: &str,
    blocking: bool,
    status: VerdictStatus,
    counts: VerdictCounts,
) -> SensorSummary {
    SensorSummary {
        id: id.into(),
        blocking,
        missing: MissingPolicy::Skip,
        presence: Presence::Present,
        report_path: format!("artifacts/{id}/report.json"),
        comment_path: None,
        verdict: Verdict {
            status,
            counts,
            reasons: vec![],
        },
        truncated: false,
        errors: vec![],
        missing_policy_applied: None,
        policy_outcome: None,
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

fn make_report(
    findings: Vec<Finding>,
    status: VerdictStatus,
    counts: VerdictCounts,
) -> SensorReport {
    SensorReport {
        schema: "sensor.report.v1".into(),
        tool: ToolInfo {
            name: "test-sensor".into(),
            version: "1.0.0".into(),
            commit: None,
        },
        run: make_run(),
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

// ===========================================================================
// compute_policy_outcome — mutant: delete `!` on `if !blocking`
// ===========================================================================

#[test]
fn policy_outcome_non_blocking_is_always_informational() {
    assert_eq!(
        compute_policy_outcome(false, &VerdictStatus::Fail),
        PolicyOutcome::Informational
    );
    assert_eq!(
        compute_policy_outcome(false, &VerdictStatus::Pass),
        PolicyOutcome::Informational
    );
    assert_eq!(
        compute_policy_outcome(false, &VerdictStatus::Warn),
        PolicyOutcome::Informational
    );
    assert_eq!(
        compute_policy_outcome(false, &VerdictStatus::Skip),
        PolicyOutcome::Informational
    );
}

#[test]
fn policy_outcome_blocking_fail_is_blocked() {
    assert_eq!(
        compute_policy_outcome(true, &VerdictStatus::Fail),
        PolicyOutcome::Blocked
    );
}

#[test]
fn policy_outcome_blocking_pass_is_allowed() {
    assert_eq!(
        compute_policy_outcome(true, &VerdictStatus::Pass),
        PolicyOutcome::Allowed
    );
}

#[test]
fn policy_outcome_blocking_warn_is_allowed() {
    assert_eq!(
        compute_policy_outcome(true, &VerdictStatus::Warn),
        PolicyOutcome::Allowed
    );
}

#[test]
fn policy_outcome_blocking_skip_is_allowed() {
    assert_eq!(
        compute_policy_outcome(true, &VerdictStatus::Skip),
        PolicyOutcome::Allowed
    );
}

// ===========================================================================
// explain_code — mutant: returns None, or inverts == to !=
// ===========================================================================

#[test]
fn explain_code_known_code_returns_some() {
    let explanation = explain_code("cockpit.missing_receipt");
    assert!(
        explanation.is_some(),
        "explain_code must return Some for known codes"
    );
    assert_eq!(explanation.unwrap().title, "Missing Receipt");
}

#[test]
fn explain_code_unknown_returns_none() {
    assert!(explain_code("nonexistent.code.xyz").is_none());
}

#[test]
fn explain_code_all_known_codes_resolve() {
    for ce in all_codes() {
        let found = explain_code(ce.code);
        assert!(
            found.is_some(),
            "explain_code({}) should return Some",
            ce.code
        );
        assert_eq!(found.unwrap().code, ce.code);
    }
}

// ===========================================================================
// all_codes — mutant: returns vec![]
// ===========================================================================

#[test]
fn all_codes_is_non_empty() {
    let codes = all_codes();
    assert!(
        codes.len() >= 7,
        "all_codes must return at least 7 codes, got {}",
        codes.len()
    );
}

#[test]
fn all_codes_contains_expected_codes() {
    let codes = all_codes();
    let code_set: Vec<&str> = codes.iter().map(|c| c.code).collect();
    assert!(code_set.contains(&"cockpit.missing_receipt"));
    assert!(code_set.contains(&"cockpit.invalid_receipt"));
    assert!(code_set.contains(&"cockpit.schema_violation"));
    assert!(code_set.contains(&"cockpit.path_traversal"));
    assert!(code_set.contains(&"cockpit.receipt_oversized"));
}

#[test]
fn all_codes_fields_non_empty() {
    for ce in all_codes() {
        assert!(!ce.code.is_empty(), "code must not be empty");
        assert!(
            !ce.title.is_empty(),
            "title must not be empty for {}",
            ce.code
        );
        assert!(
            !ce.description.is_empty(),
            "description must not be empty for {}",
            ce.code
        );
        assert!(
            !ce.cause.is_empty(),
            "cause must not be empty for {}",
            ce.code
        );
        assert!(!ce.fix.is_empty(), "fix must not be empty for {}", ce.code);
    }
}

// ===========================================================================
// cap_findings — mutant: returns (vec![], true/false) or inverts <= to >
// ===========================================================================

#[test]
fn cap_findings_under_limit_returns_all() {
    let findings = vec![finding("A", Severity::Error), finding("B", Severity::Warn)];
    let (capped, truncated) = cap_findings(findings.clone(), 5);
    assert_eq!(capped.len(), 2);
    assert!(!truncated);
    assert_eq!(capped[0].code, "A");
    assert_eq!(capped[1].code, "B");
}

#[test]
fn cap_findings_at_exact_limit_not_truncated() {
    let findings = vec![
        finding("A", Severity::Error),
        finding("B", Severity::Warn),
        finding("C", Severity::Info),
    ];
    let (capped, truncated) = cap_findings(findings, 3);
    assert_eq!(capped.len(), 3);
    assert!(!truncated, "exactly at limit should not be truncated");
}

#[test]
fn cap_findings_over_limit_truncated() {
    let findings = vec![
        finding("A", Severity::Error),
        finding("B", Severity::Warn),
        finding("C", Severity::Info),
    ];
    let (capped, truncated) = cap_findings(findings, 2);
    assert_eq!(capped.len(), 2);
    assert!(truncated, "over limit must be truncated");
}

#[test]
fn cap_findings_zero_limit() {
    let findings = vec![finding("A", Severity::Error)];
    let (capped, truncated) = cap_findings(findings, 0);
    assert!(capped.is_empty());
    assert!(truncated);
}

#[test]
fn cap_findings_empty_input() {
    let (capped, truncated) = cap_findings(vec![], 10);
    assert!(capped.is_empty());
    assert!(!truncated);
}

// ===========================================================================
// compute_counts — mutant: returns Default, += becomes *= or -=
// ===========================================================================

#[test]
fn compute_counts_single_error() {
    let findings = vec![finding("E1", Severity::Error)];
    let c = compute_counts(&findings);
    assert_eq!(c.error, 1);
    assert_eq!(c.warn, 0);
    assert_eq!(c.info, 0);
}

#[test]
fn compute_counts_mixed_severities() {
    let findings = vec![
        finding("E1", Severity::Error),
        finding("E2", Severity::Error),
        finding("W1", Severity::Warn),
        finding("I1", Severity::Info),
        finding("I2", Severity::Info),
        finding("I3", Severity::Info),
    ];
    let c = compute_counts(&findings);
    assert_eq!(c.error, 2);
    assert_eq!(c.warn, 1);
    assert_eq!(c.info, 3);
}

#[test]
fn compute_counts_empty_findings() {
    let c = compute_counts(&[]);
    assert_eq!(c.error, 0);
    assert_eq!(c.warn, 0);
    assert_eq!(c.info, 0);
}

#[test]
fn compute_counts_all_warn() {
    let findings = vec![finding("W1", Severity::Warn), finding("W2", Severity::Warn)];
    let c = compute_counts(&findings);
    assert_eq!(c.error, 0);
    assert_eq!(c.warn, 2);
    assert_eq!(c.info, 0);
}

// ===========================================================================
// derive_fingerprint — mutant: returns "xyzzy"
// ===========================================================================

#[test]
fn derive_fingerprint_is_sha256_hex() {
    let f = finding("E001", Severity::Error);
    let fp = derive_fingerprint("builddiag", &f);
    assert_eq!(fp.len(), 64, "fingerprint must be 64 hex chars (SHA-256)");
    assert!(fp.chars().all(|c| c.is_ascii_hexdigit()), "must be hex");
}

#[test]
fn derive_fingerprint_deterministic() {
    let f = finding("E001", Severity::Error);
    let fp1 = derive_fingerprint("builddiag", &f);
    let fp2 = derive_fingerprint("builddiag", &f);
    assert_eq!(fp1, fp2);
}

#[test]
fn derive_fingerprint_different_sensors_differ() {
    let f = finding("E001", Severity::Error);
    let fp1 = derive_fingerprint("sensor-a", &f);
    let fp2 = derive_fingerprint("sensor-b", &f);
    assert_ne!(
        fp1, fp2,
        "different sensor IDs should produce different fingerprints"
    );
}

#[test]
fn derive_fingerprint_different_codes_differ() {
    let f1 = finding("E001", Severity::Error);
    let f2 = finding("E002", Severity::Error);
    let fp1 = derive_fingerprint("sensor", &f1);
    let fp2 = derive_fingerprint("sensor", &f2);
    assert_ne!(fp1, fp2);
}

#[test]
fn derive_fingerprint_with_location() {
    let f1 = finding_with_loc("E001", Severity::Error, "src/main.rs", 10);
    let f2 = finding_with_loc("E001", Severity::Error, "src/main.rs", 20);
    let fp1 = derive_fingerprint("sensor", &f1);
    let fp2 = derive_fingerprint("sensor", &f2);
    assert_ne!(
        fp1, fp2,
        "different lines should produce different fingerprints"
    );
}

// ===========================================================================
// sort_findings — mutant: becomes noop
// ===========================================================================

#[test]
fn sort_findings_by_severity() {
    let mut findings = vec![
        finding("I1", Severity::Info),
        finding("E1", Severity::Error),
        finding("W1", Severity::Warn),
    ];
    sort_findings("sensor", &mut findings);
    assert_eq!(findings[0].severity, Severity::Error);
    assert_eq!(findings[1].severity, Severity::Warn);
    assert_eq!(findings[2].severity, Severity::Info);
}

#[test]
fn sort_findings_same_severity_by_code() {
    let mut findings = vec![
        finding("C", Severity::Error),
        finding("A", Severity::Error),
        finding("B", Severity::Error),
    ];
    sort_findings("sensor", &mut findings);
    assert_eq!(findings[0].code, "A");
    assert_eq!(findings[1].code, "B");
    assert_eq!(findings[2].code, "C");
}

#[test]
fn sort_findings_with_location() {
    let mut findings = vec![
        finding_with_loc("E1", Severity::Error, "z.rs", 1),
        finding_with_loc("E1", Severity::Error, "a.rs", 1),
        finding_with_loc("E1", Severity::Error, "a.rs", 5),
    ];
    sort_findings("sensor", &mut findings);
    assert_eq!(
        findings[0].location.as_ref().unwrap().path.as_deref(),
        Some("a.rs")
    );
    assert_eq!(findings[0].location.as_ref().unwrap().line, Some(1));
    assert_eq!(findings[1].location.as_ref().unwrap().line, Some(5));
    assert_eq!(
        findings[2].location.as_ref().unwrap().path.as_deref(),
        Some("z.rs")
    );
}

// ===========================================================================
// select_highlights — mutant: returns vec![]
// ===========================================================================

#[test]
fn select_highlights_returns_candidates() {
    let candidates = vec![Highlight {
        sensor_id: "sensor-a".into(),
        finding: finding("E1", Severity::Error),
    }];
    let cfg = CockpitConfig::default();
    let blocking = BTreeMap::from([("sensor-a".to_string(), true)]);
    let result = select_highlights(candidates, &cfg, &blocking);
    assert_eq!(
        result.len(),
        1,
        "select_highlights must not return empty for non-empty input"
    );
    assert_eq!(result[0].finding.code, "E1");
}

#[test]
fn select_highlights_respects_max() {
    let mut candidates = Vec::new();
    for i in 0..20 {
        candidates.push(Highlight {
            sensor_id: "sensor".into(),
            finding: finding(&format!("E{i}"), Severity::Error),
        });
    }
    let cfg = CockpitConfig::default(); // max_highlights = 7
    let blocking = BTreeMap::new();
    let result = select_highlights(candidates, &cfg, &blocking);
    assert_eq!(result.len(), 7);
}

#[test]
fn select_highlights_deduplicates_by_fingerprint() {
    let mut f1 = finding("E1", Severity::Error);
    f1.fingerprint = Some("same-fp".into());
    let mut f2 = finding("E1", Severity::Error);
    f2.fingerprint = Some("same-fp".into());
    let candidates = vec![
        Highlight {
            sensor_id: "a".into(),
            finding: f1,
        },
        Highlight {
            sensor_id: "a".into(),
            finding: f2,
        },
    ];
    let cfg = CockpitConfig::default();
    let blocking = BTreeMap::new();
    let result = select_highlights(candidates, &cfg, &blocking);
    assert_eq!(result.len(), 1, "duplicated fingerprints should be deduped");
}

#[test]
fn select_highlights_blocking_sensors_first() {
    let candidates = vec![
        Highlight {
            sensor_id: "non-blocking".into(),
            finding: finding("E1", Severity::Error),
        },
        Highlight {
            sensor_id: "blocking".into(),
            finding: finding("E2", Severity::Error),
        },
    ];
    let cfg = CockpitConfig::default();
    let blocking = BTreeMap::from([
        ("blocking".to_string(), true),
        ("non-blocking".to_string(), false),
    ]);
    let result = select_highlights(candidates, &cfg, &blocking);
    assert_eq!(result[0].sensor_id, "blocking");
    assert_eq!(result[1].sensor_id, "non-blocking");
}

// ===========================================================================
// overall_verdict — count aggregation +=/*=/-=, !blocking negation,
//                   warn_is_fail &&/||/! logic, verdict_status_rank <
// ===========================================================================

#[test]
fn overall_verdict_all_pass() {
    let summaries = vec![
        summary(
            "a",
            true,
            VerdictStatus::Pass,
            VerdictCounts {
                info: 1,
                warn: 0,
                error: 0,
                suppressed: 0,
            },
        ),
        summary(
            "b",
            true,
            VerdictStatus::Pass,
            VerdictCounts {
                info: 0,
                warn: 2,
                error: 0,
                suppressed: 0,
            },
        ),
    ];
    let cfg = CockpitConfig::default();
    let v = overall_verdict(&summaries, &cfg);
    assert_eq!(v.status, VerdictStatus::Pass);
    assert_eq!(v.counts.info, 1);
    assert_eq!(v.counts.warn, 2);
    assert_eq!(v.counts.error, 0);
}

#[test]
fn overall_verdict_blocking_fail_wins() {
    let summaries = vec![
        summary("a", true, VerdictStatus::Pass, VerdictCounts::default()),
        summary(
            "b",
            true,
            VerdictStatus::Fail,
            VerdictCounts {
                info: 0,
                warn: 0,
                error: 1,
                suppressed: 0,
            },
        ),
    ];
    let cfg = CockpitConfig::default();
    let v = overall_verdict(&summaries, &cfg);
    assert_eq!(v.status, VerdictStatus::Fail);
}

#[test]
fn overall_verdict_non_blocking_fail_ignored() {
    let summaries = vec![
        summary(
            "a",
            false,
            VerdictStatus::Fail,
            VerdictCounts {
                info: 0,
                warn: 0,
                error: 5,
                suppressed: 0,
            },
        ),
        summary("b", true, VerdictStatus::Pass, VerdictCounts::default()),
    ];
    let cfg = CockpitConfig::default();
    let v = overall_verdict(&summaries, &cfg);
    assert_eq!(
        v.status,
        VerdictStatus::Pass,
        "non-blocking fail should not affect overall verdict"
    );
    assert_eq!(v.counts.error, 5);
}

#[test]
fn overall_verdict_warn_is_fail_escalates() {
    let summaries = vec![summary(
        "a",
        true,
        VerdictStatus::Warn,
        VerdictCounts {
            info: 0,
            warn: 1,
            error: 0,
            suppressed: 0,
        },
    )];
    let mut cfg = CockpitConfig::default();
    cfg.policy.warn_is_fail = true;
    let v = overall_verdict(&summaries, &cfg);
    assert_eq!(
        v.status,
        VerdictStatus::Fail,
        "warn_is_fail should escalate warn to fail"
    );
    assert!(v.reasons.contains(&"warn_is_fail".to_string()));
}

#[test]
fn overall_verdict_warn_not_escalated_without_flag() {
    let summaries = vec![summary(
        "a",
        true,
        VerdictStatus::Warn,
        VerdictCounts {
            info: 0,
            warn: 1,
            error: 0,
            suppressed: 0,
        },
    )];
    let cfg = CockpitConfig::default();
    let v = overall_verdict(&summaries, &cfg);
    assert_eq!(v.status, VerdictStatus::Warn);
    assert!(!v.reasons.contains(&"warn_is_fail".to_string()));
}

#[test]
fn overall_verdict_counts_aggregate_across_sensors() {
    let summaries = vec![
        summary(
            "a",
            true,
            VerdictStatus::Pass,
            VerdictCounts {
                info: 1,
                warn: 2,
                error: 3,
                suppressed: 0,
            },
        ),
        summary(
            "b",
            false,
            VerdictStatus::Pass,
            VerdictCounts {
                info: 10,
                warn: 20,
                error: 30,
                suppressed: 0,
            },
        ),
    ];
    let cfg = CockpitConfig::default();
    let v = overall_verdict(&summaries, &cfg);
    assert_eq!(v.counts.info, 11);
    assert_eq!(v.counts.warn, 22);
    assert_eq!(v.counts.error, 33);
}

#[test]
fn overall_verdict_fail_beats_warn() {
    let summaries = vec![
        summary("a", true, VerdictStatus::Warn, VerdictCounts::default()),
        summary("b", true, VerdictStatus::Fail, VerdictCounts::default()),
    ];
    let cfg = CockpitConfig::default();
    let v = overall_verdict(&summaries, &cfg);
    assert_eq!(v.status, VerdictStatus::Fail);
}

// ===========================================================================
// synthesize_schema_violation_sensor — mutant: returns defaults, == to !=
// ===========================================================================

#[test]
fn schema_violation_sensor_has_fail_verdict() {
    let (summary, highlight) = synthesize_schema_violation_sensor(
        "bad-sensor",
        &policy(true),
        "artifacts/bad-sensor/report.json",
        None,
        vec!["missing field: tool".into()],
    );
    assert_eq!(summary.verdict.status, VerdictStatus::Fail);
    assert_eq!(summary.id, "bad-sensor");
    assert_eq!(summary.presence, Presence::Invalid);
    assert!(highlight.is_some());
    let h = highlight.unwrap();
    assert_eq!(h.finding.code, "cockpit.schema_violation");
    assert_eq!(h.finding.severity, Severity::Error);
}

#[test]
fn schema_violation_sensor_multiple_errors() {
    let (summary, highlight) = synthesize_schema_violation_sensor(
        "sensor-x",
        &policy(false),
        "artifacts/sensor-x/report.json",
        None,
        vec!["err1".into(), "err2".into(), "err3".into()],
    );
    assert_eq!(summary.verdict.status, VerdictStatus::Fail);
    assert_eq!(summary.errors.len(), 3);
    let h = highlight.unwrap();
    assert!(h.finding.message.contains("3 schema violations"));
    assert_eq!(summary.policy_outcome, Some(PolicyOutcome::Informational));
}

#[test]
fn schema_violation_blocking_produces_blocked_outcome() {
    let (summary, _) = synthesize_schema_violation_sensor(
        "s",
        &policy(true),
        "artifacts/s/report.json",
        None,
        vec!["err".into()],
    );
    assert_eq!(summary.policy_outcome, Some(PolicyOutcome::Blocked));
}

// ===========================================================================
// summarize_sensor_report — mutant: != to == on line 1062 (count mismatch)
// ===========================================================================

#[test]
fn summarize_detects_count_mismatch() {
    let findings = vec![finding("E1", Severity::Error)];
    let report = make_report(
        findings,
        VerdictStatus::Fail,
        VerdictCounts {
            info: 0,
            warn: 0,
            error: 0,
            suppressed: 0,
        },
    );
    let (summary, highlights) = summarize_sensor_report(
        "test-sensor",
        "artifacts/test-sensor/report.json",
        None,
        &policy(true),
        report,
        20,
    );
    assert!(
        summary
            .verdict
            .reasons
            .contains(&"receipt_inconsistent".to_string()),
        "mismatch should produce receipt_inconsistent reason"
    );
    assert_eq!(
        summary.verdict.counts.error, 1,
        "counts should be recomputed"
    );
    assert!(
        highlights
            .iter()
            .any(|h| h.finding.code == "cockpit.receipt_inconsistent"),
        "should have receipt_inconsistent highlight"
    );
}

#[test]
fn summarize_no_mismatch_when_counts_match() {
    let findings = vec![
        finding("E1", Severity::Error),
        finding("W1", Severity::Warn),
    ];
    let report = make_report(
        findings,
        VerdictStatus::Fail,
        VerdictCounts {
            info: 0,
            warn: 1,
            error: 1,
            suppressed: 0,
        },
    );
    let (summary, highlights) = summarize_sensor_report(
        "test-sensor",
        "artifacts/test-sensor/report.json",
        None,
        &policy(true),
        report,
        20,
    );
    assert!(
        !summary
            .verdict
            .reasons
            .contains(&"receipt_inconsistent".to_string()),
        "matching counts should NOT produce receipt_inconsistent"
    );
    assert!(
        !highlights
            .iter()
            .any(|h| h.finding.code == "cockpit.receipt_inconsistent"),
    );
}

#[test]
fn summarize_caps_findings() {
    let findings: Vec<Finding> = (0..10)
        .map(|i| finding(&format!("E{i}"), Severity::Error))
        .collect();
    let report = make_report(
        findings,
        VerdictStatus::Fail,
        VerdictCounts {
            info: 0,
            warn: 0,
            error: 10,
            suppressed: 0,
        },
    );
    let (summary, _highlights) = summarize_sensor_report(
        "sensor",
        "artifacts/sensor/report.json",
        None,
        &policy(true),
        report,
        3,
    );
    assert!(
        summary.truncated,
        "should be truncated when findings exceed max"
    );
}

#[test]
fn summarize_sorts_findings_deterministically() {
    let findings = vec![
        finding("Z", Severity::Info),
        finding("A", Severity::Error),
        finding("M", Severity::Warn),
    ];
    let report = make_report(
        findings,
        VerdictStatus::Fail,
        VerdictCounts {
            info: 1,
            warn: 1,
            error: 1,
            suppressed: 0,
        },
    );
    let (_summary, highlights) = summarize_sensor_report(
        "sensor",
        "artifacts/sensor/report.json",
        None,
        &policy(true),
        report,
        20,
    );
    let finding_highlights: Vec<&Highlight> = highlights
        .iter()
        .filter(|h| !h.finding.code.starts_with("cockpit."))
        .collect();
    assert_eq!(finding_highlights[0].finding.severity, Severity::Error);
    assert_eq!(finding_highlights[1].finding.severity, Severity::Warn);
    assert_eq!(finding_highlights[2].finding.severity, Severity::Info);
}

#[test]
fn summarize_sets_policy_outcome() {
    let report = make_report(
        vec![finding("E1", Severity::Error)],
        VerdictStatus::Fail,
        VerdictCounts {
            info: 0,
            warn: 0,
            error: 1,
            suppressed: 0,
        },
    );
    let (summary, _) = summarize_sensor_report(
        "sensor",
        "artifacts/sensor/report.json",
        None,
        &policy(true),
        report,
        20,
    );
    assert_eq!(summary.policy_outcome, Some(PolicyOutcome::Blocked));
}
