//! Tests for arithmetic overflow protection in domain logic.

use cockpitctl_domain::{cap_findings, compute_counts, overall_verdict, select_highlights};
use cockpitctl_types::{
    CockpitConfig, Finding, Highlight, Location, MissingPolicy, Presence, SensorSummary, Severity,
    Verdict, VerdictCounts, VerdictStatus,
};

fn make_finding(severity: Severity) -> Finding {
    Finding {
        severity,
        check_id: None,
        code: "T001".into(),
        message: "test".into(),
        location: None,
        help: None,
        url: None,
        fingerprint: None,
        data: None,
    }
}

fn make_summary(counts: VerdictCounts, blocking: bool) -> SensorSummary {
    SensorSummary {
        id: "overflow-sensor".into(),
        blocking,
        missing: MissingPolicy::Fail,
        presence: Presence::Present,
        report_path: "artifacts/overflow-sensor/report.json".into(),
        comment_path: None,
        verdict: Verdict {
            status: VerdictStatus::Pass,
            counts,
            reasons: vec![],
        },
        truncated: false,
        errors: vec![],
        missing_policy_applied: None,
        policy_outcome: None,
    }
}

// ── overall_verdict: saturating aggregation ───────────────────────────

#[test]
fn overall_verdict_saturates_on_u64_max_counts() {
    let s1 = make_summary(
        VerdictCounts {
            info: u64::MAX,
            warn: u64::MAX,
            error: u64::MAX,
            suppressed: 0,
        },
        false,
    );
    let s2 = make_summary(
        VerdictCounts {
            info: 1,
            warn: 1,
            error: 1,
            suppressed: 0,
        },
        false,
    );

    let cfg = CockpitConfig::default();
    let verdict = overall_verdict(&[s1, s2], &cfg);

    assert_eq!(verdict.counts.info, u64::MAX);
    assert_eq!(verdict.counts.warn, u64::MAX);
    assert_eq!(verdict.counts.error, u64::MAX);
}

#[test]
fn overall_verdict_saturates_many_large_sensors() {
    let summaries: Vec<SensorSummary> = (0..10)
        .map(|_| {
            make_summary(
                VerdictCounts {
                    info: u64::MAX / 2,
                    warn: u64::MAX / 3,
                    error: u64::MAX / 4,
                    suppressed: 0,
                },
                false,
            )
        })
        .collect();

    let cfg = CockpitConfig::default();
    let verdict = overall_verdict(&summaries, &cfg);

    assert_eq!(verdict.counts.info, u64::MAX);
    assert_eq!(verdict.counts.warn, u64::MAX);
    assert_eq!(verdict.counts.error, u64::MAX);
}

#[test]
fn overall_verdict_normal_counts_unaffected_by_saturation() {
    let s1 = make_summary(
        VerdictCounts {
            info: 5,
            warn: 3,
            error: 1,
            suppressed: 0,
        },
        false,
    );
    let s2 = make_summary(
        VerdictCounts {
            info: 10,
            warn: 7,
            error: 2,
            suppressed: 0,
        },
        false,
    );

    let cfg = CockpitConfig::default();
    let verdict = overall_verdict(&[s1, s2], &cfg);

    assert_eq!(verdict.counts.info, 15);
    assert_eq!(verdict.counts.warn, 10);
    assert_eq!(verdict.counts.error, 3);
}

// ── compute_counts: saturating increment ──────────────────────────────

#[test]
fn compute_counts_basic_correctness() {
    let findings = vec![
        make_finding(Severity::Error),
        make_finding(Severity::Error),
        make_finding(Severity::Warn),
        make_finding(Severity::Info),
    ];
    let counts = compute_counts(&findings);
    assert_eq!(counts.error, 2);
    assert_eq!(counts.warn, 1);
    assert_eq!(counts.info, 1);
}

#[test]
fn compute_counts_empty_findings() {
    let counts = compute_counts(&[]);
    assert_eq!(counts.info, 0);
    assert_eq!(counts.warn, 0);
    assert_eq!(counts.error, 0);
}

// ── VerdictCounts with extreme values ─────────────────────────────────

#[test]
fn verdict_counts_u64_max_values_do_not_panic() {
    let counts = VerdictCounts {
        info: u64::MAX,
        warn: u64::MAX,
        error: u64::MAX,
        suppressed: u64::MAX,
    };
    // Ensure display/debug formatting doesn't panic
    let _ = format!("{:?}", counts);
    assert_eq!(counts.info, u64::MAX);
}

// ── select_highlights with extreme cap ────────────────────────────────

#[test]
fn select_highlights_zero_cap() {
    let h = Highlight {
        sensor_id: "s1".into(),
        finding: make_finding(Severity::Error),
    };
    let mut cfg = CockpitConfig::default();
    cfg.policy.max_highlights = 0;

    let selected = select_highlights(vec![h], &cfg, &std::collections::BTreeMap::new());
    assert!(selected.is_empty());
}

#[test]
fn select_highlights_usize_max_cap() {
    let h = Highlight {
        sensor_id: "s1".into(),
        finding: make_finding(Severity::Error),
    };
    let mut cfg = CockpitConfig::default();
    cfg.policy.max_highlights = usize::MAX;

    let selected = select_highlights(vec![h], &cfg, &std::collections::BTreeMap::new());
    assert_eq!(selected.len(), 1);
}

// ── Large line numbers in sorting ─────────────────────────────────────

#[test]
fn select_highlights_sorts_with_u32_max_line_numbers() {
    let mut f1 = make_finding(Severity::Error);
    f1.location = Some(Location {
        path: Some("a.rs".into()),
        line: Some(u32::MAX),
        col: None,
    });
    f1.code = "A".into();

    let mut f2 = make_finding(Severity::Error);
    f2.location = Some(Location {
        path: Some("a.rs".into()),
        line: Some(1),
        col: None,
    });
    f2.code = "B".into();

    let h1 = Highlight {
        sensor_id: "s".into(),
        finding: f1,
    };
    let h2 = Highlight {
        sensor_id: "s".into(),
        finding: f2,
    };

    let cfg = CockpitConfig::default();
    let selected = select_highlights(vec![h1, h2], &cfg, &std::collections::BTreeMap::new());

    assert_eq!(selected.len(), 2);
    // line=1 sorts before line=u32::MAX
    assert_eq!(selected[0].finding.location.as_ref().unwrap().line, Some(1));
    assert_eq!(
        selected[1].finding.location.as_ref().unwrap().line,
        Some(u32::MAX)
    );
}

// ── cap_findings with extreme values ──────────────────────────────────

#[test]
fn cap_findings_zero_max() {
    let findings = vec![make_finding(Severity::Error)];
    let (capped, truncated) = cap_findings(findings, 0);
    assert!(capped.is_empty());
    assert!(truncated);
}

#[test]
fn cap_findings_usize_max() {
    let findings = vec![make_finding(Severity::Error)];
    let (capped, truncated) = cap_findings(findings, usize::MAX);
    assert_eq!(capped.len(), 1);
    assert!(!truncated);
}

// ── Mixed blocking/non-blocking with large counts ─────────────────────

#[test]
fn overall_verdict_large_counts_mixed_blocking() {
    let blocking_summary = SensorSummary {
        id: "blocker".into(),
        blocking: true,
        missing: MissingPolicy::Fail,
        presence: Presence::Present,
        report_path: "artifacts/blocker/report.json".into(),
        comment_path: None,
        verdict: Verdict {
            status: VerdictStatus::Fail,
            counts: VerdictCounts {
                info: u64::MAX,
                warn: 0,
                error: u64::MAX,
                suppressed: 0,
            },
            reasons: vec![],
        },
        truncated: false,
        errors: vec![],
        missing_policy_applied: None,
        policy_outcome: None,
    };
    let non_blocking = make_summary(
        VerdictCounts {
            info: 1,
            warn: u64::MAX,
            error: 1,
            suppressed: 0,
        },
        false,
    );

    let cfg = CockpitConfig::default();
    let verdict = overall_verdict(&[blocking_summary, non_blocking], &cfg);

    assert_eq!(verdict.status, VerdictStatus::Fail);
    assert_eq!(verdict.counts.info, u64::MAX);
    assert_eq!(verdict.counts.warn, u64::MAX);
    assert_eq!(verdict.counts.error, u64::MAX);
}
