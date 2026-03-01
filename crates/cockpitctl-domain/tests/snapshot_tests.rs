use cockpitctl_domain::{
    build_cockpit_report, cap_findings, compute_counts, compute_policy_outcome, derive_fingerprint,
    overall_verdict, select_highlights, snapshot_policy, sort_findings, summarize_sensor_report,
    synthesize_invalid_sensor, synthesize_missing_sensor, synthesize_path_traversal_sensor,
    synthesize_receipt_oversized_sensor, synthesize_schema_violation_sensor,
    synthesize_sensors_truncated,
};
use cockpitctl_types::{
    CockpitConfig, Finding, Highlight, Location, MissingPolicy, RunInfo, SensorPolicy,
    SensorReport, Severity, ToolInfo, Verdict, VerdictCounts, VerdictStatus,
};

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
        capabilities: std::collections::BTreeMap::new(),
    }
}

fn policy(blocking: bool, missing: MissingPolicy) -> SensorPolicy {
    SensorPolicy {
        blocking,
        missing,
        section: None,
        require_label: None,
        repro: None,
    }
}

fn finding(code: &str, severity: Severity, path: &str, line: u32) -> Finding {
    Finding {
        severity,
        check_id: None,
        code: code.to_string(),
        message: format!("Message for {}", code),
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

// ---------------------------------------------------------------------------
// Normalized findings: sorting + dedup
// ---------------------------------------------------------------------------

#[test]
fn snapshot_sorted_findings() {
    let mut findings = vec![
        finding("W1", Severity::Warn, "src/b.rs", 20),
        finding("E1", Severity::Error, "src/a.rs", 10),
        finding("I1", Severity::Info, "src/c.rs", 5),
        finding("E2", Severity::Error, "src/a.rs", 5),
        finding("W2", Severity::Warn, "src/a.rs", 1),
    ];
    sort_findings("sensor_a", &mut findings);
    insta::assert_json_snapshot!("sorted_findings", findings);
}

#[test]
fn snapshot_capped_findings() {
    let findings = vec![
        finding("E1", Severity::Error, "src/a.rs", 1),
        finding("E2", Severity::Error, "src/a.rs", 2),
        finding("E3", Severity::Error, "src/a.rs", 3),
        finding("W1", Severity::Warn, "src/b.rs", 1),
    ];
    let (capped, truncated) = cap_findings(findings, 2);
    assert!(truncated);
    insta::assert_json_snapshot!("capped_findings", capped);
}

#[test]
fn snapshot_compute_counts() {
    let findings = vec![
        finding("E1", Severity::Error, "src/a.rs", 1),
        finding("E2", Severity::Error, "src/a.rs", 2),
        finding("W1", Severity::Warn, "src/b.rs", 1),
        finding("I1", Severity::Info, "src/c.rs", 1),
        finding("I2", Severity::Info, "src/c.rs", 2),
    ];
    let counts = compute_counts(&findings);
    insta::assert_json_snapshot!("compute_counts", counts);
}

// ---------------------------------------------------------------------------
// Highlight selection (top-N, blocking-first, dedupe)
// ---------------------------------------------------------------------------

#[test]
fn snapshot_highlight_selection_blocking_first() {
    let mut cfg = CockpitConfig::default();
    cfg.policy.max_highlights = 3;

    let candidates = vec![
        Highlight {
            sensor_id: "nonblocking".to_string(),
            finding: finding("E1", Severity::Error, "src/a.rs", 10),
        },
        Highlight {
            sensor_id: "blocker".to_string(),
            finding: finding("E2", Severity::Error, "src/b.rs", 20),
        },
        Highlight {
            sensor_id: "blocker".to_string(),
            finding: finding("W1", Severity::Warn, "src/c.rs", 5),
        },
        Highlight {
            sensor_id: "nonblocking".to_string(),
            finding: finding("I1", Severity::Info, "src/d.rs", 1),
        },
    ];

    let mut sensor_blocking = std::collections::BTreeMap::new();
    sensor_blocking.insert("blocker".to_string(), true);
    sensor_blocking.insert("nonblocking".to_string(), false);

    let selected = select_highlights(candidates, &cfg, &sensor_blocking);
    insta::assert_json_snapshot!("highlight_selection_blocking_first", selected);
}

#[test]
fn snapshot_highlight_selection_dedup() {
    let cfg = CockpitConfig::default();
    let fp = derive_fingerprint("sensor", &finding("E1", Severity::Error, "src/a.rs", 10));

    let mut f1 = finding("E1", Severity::Error, "src/a.rs", 10);
    f1.fingerprint = Some(fp.clone());
    let mut f2 = finding("E1", Severity::Error, "src/a.rs", 10);
    f2.fingerprint = Some(fp);

    let candidates = vec![
        Highlight {
            sensor_id: "sensor".to_string(),
            finding: f1,
        },
        Highlight {
            sensor_id: "sensor".to_string(),
            finding: f2,
        },
    ];

    let selected = select_highlights(candidates, &cfg, &std::collections::BTreeMap::new());
    insta::assert_json_snapshot!("highlight_selection_dedup", selected);
}

// ---------------------------------------------------------------------------
// Policy evaluation
// ---------------------------------------------------------------------------

#[test]
fn snapshot_compute_policy_outcome_matrix() {
    let results = vec![
        (
            "blocking_fail",
            compute_policy_outcome(true, &VerdictStatus::Fail),
        ),
        (
            "blocking_warn",
            compute_policy_outcome(true, &VerdictStatus::Warn),
        ),
        (
            "blocking_pass",
            compute_policy_outcome(true, &VerdictStatus::Pass),
        ),
        (
            "nonblocking_fail",
            compute_policy_outcome(false, &VerdictStatus::Fail),
        ),
    ];
    insta::assert_debug_snapshot!("policy_outcome_matrix", results);
}

#[test]
fn snapshot_policy_snapshot() {
    let mut cfg = CockpitConfig::default();
    cfg.policy.warn_is_fail = true;
    cfg.policy.max_highlights = 5;
    cfg.sensors.insert(
        "clippy".to_string(),
        SensorPolicy {
            blocking: true,
            missing: MissingPolicy::Fail,
            section: Some("Diagnostics".to_string()),
            require_label: None,
            repro: Some("cargo clippy".to_string()),
        },
    );
    cfg.sensors.insert(
        "tests".to_string(),
        SensorPolicy {
            blocking: true,
            missing: MissingPolicy::Warn,
            section: Some("Tests".to_string()),
            require_label: None,
            repro: None,
        },
    );
    let snap = snapshot_policy(&cfg);
    insta::assert_json_snapshot!("policy_snapshot", snap);
}

// ---------------------------------------------------------------------------
// Composite verdict computation
// ---------------------------------------------------------------------------

fn summary_with_verdict(
    id: &str,
    blocking: bool,
    status: VerdictStatus,
    counts: VerdictCounts,
) -> cockpitctl_types::SensorSummary {
    cockpitctl_types::SensorSummary {
        id: id.to_string(),
        blocking,
        missing: MissingPolicy::Fail,
        presence: cockpitctl_types::Presence::Present,
        report_path: format!("artifacts/{}/report.json", id),
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

#[test]
fn snapshot_overall_verdict_all_pass() {
    let summaries = vec![
        summary_with_verdict("a", true, VerdictStatus::Pass, VerdictCounts::default()),
        summary_with_verdict("b", true, VerdictStatus::Pass, VerdictCounts::default()),
    ];
    let verdict = overall_verdict(&summaries, &CockpitConfig::default());
    insta::assert_json_snapshot!("overall_verdict_all_pass", verdict);
}

#[test]
fn snapshot_overall_verdict_blocking_fail() {
    let summaries = vec![
        summary_with_verdict(
            "a",
            true,
            VerdictStatus::Fail,
            VerdictCounts {
                info: 0,
                warn: 0,
                error: 3,
                suppressed: 0,
            },
        ),
        summary_with_verdict(
            "b",
            false,
            VerdictStatus::Pass,
            VerdictCounts {
                info: 2,
                warn: 0,
                error: 0,
                suppressed: 0,
            },
        ),
    ];
    let verdict = overall_verdict(&summaries, &CockpitConfig::default());
    insta::assert_json_snapshot!("overall_verdict_blocking_fail", verdict);
}

#[test]
fn snapshot_overall_verdict_warn_is_fail() {
    let summaries = vec![summary_with_verdict(
        "a",
        true,
        VerdictStatus::Warn,
        VerdictCounts {
            info: 0,
            warn: 2,
            error: 0,
            suppressed: 0,
        },
    )];
    let mut cfg = CockpitConfig::default();
    cfg.policy.warn_is_fail = true;
    let verdict = overall_verdict(&summaries, &cfg);
    insta::assert_json_snapshot!("overall_verdict_warn_is_fail", verdict);
}

#[test]
fn snapshot_overall_verdict_nonblocking_ignored() {
    let summaries = vec![summary_with_verdict(
        "info_only",
        false,
        VerdictStatus::Fail,
        VerdictCounts {
            info: 0,
            warn: 0,
            error: 5,
            suppressed: 0,
        },
    )];
    let verdict = overall_verdict(&summaries, &CockpitConfig::default());
    insta::assert_json_snapshot!("overall_verdict_nonblocking_ignored", verdict);
}

// ---------------------------------------------------------------------------
// Synthesized sensor summaries
// ---------------------------------------------------------------------------

#[test]
fn snapshot_synthesize_missing_skip() {
    let (summary, highlight) = synthesize_missing_sensor(
        "coverage",
        &policy(true, MissingPolicy::Skip),
        "artifacts/coverage/report.json",
        None,
    );
    insta::assert_json_snapshot!("synthesize_missing_skip_summary", summary);
    insta::assert_debug_snapshot!("synthesize_missing_skip_highlight", highlight);
}

#[test]
fn snapshot_synthesize_missing_fail() {
    let (summary, highlight) = synthesize_missing_sensor(
        "coverage",
        &policy(true, MissingPolicy::Fail),
        "artifacts/coverage/report.json",
        None,
    );
    insta::assert_json_snapshot!("synthesize_missing_fail_summary", summary);
    insta::assert_json_snapshot!("synthesize_missing_fail_highlight", highlight);
}

#[test]
fn snapshot_synthesize_invalid_sensor() {
    let (summary, highlight) = synthesize_invalid_sensor(
        "builddiag",
        &policy(true, MissingPolicy::Fail),
        "artifacts/builddiag/report.json",
        None,
        "unexpected EOF".to_string(),
    );
    insta::assert_json_snapshot!("synthesize_invalid_summary", summary);
    insta::assert_json_snapshot!("synthesize_invalid_highlight", highlight);
}

#[test]
fn snapshot_synthesize_schema_violation() {
    let (summary, highlight) = synthesize_schema_violation_sensor(
        "lint",
        &policy(true, MissingPolicy::Fail),
        "artifacts/lint/report.json",
        None,
        vec![
            "missing required field: verdict".to_string(),
            "invalid type for findings".to_string(),
        ],
    );
    insta::assert_json_snapshot!("synthesize_schema_violation_summary", summary);
    insta::assert_json_snapshot!("synthesize_schema_violation_highlight", highlight);
}

#[test]
fn snapshot_synthesize_path_traversal() {
    let (summary, highlight) = synthesize_path_traversal_sensor(
        "../escape",
        &policy(true, MissingPolicy::Fail),
        "artifacts/../escape/report.json",
        None,
        Some("report.json".to_string()),
    );
    insta::assert_json_snapshot!("synthesize_path_traversal_summary", summary);
    insta::assert_json_snapshot!("synthesize_path_traversal_highlight", highlight);
}

#[test]
fn snapshot_synthesize_receipt_oversized() {
    let (summary, highlight) = synthesize_receipt_oversized_sensor(
        "bigdata",
        &policy(true, MissingPolicy::Fail),
        "artifacts/bigdata/report.json",
        None,
        5_000_000,
        2_097_152,
    );
    insta::assert_json_snapshot!("synthesize_oversized_summary", summary);
    insta::assert_json_snapshot!("synthesize_oversized_highlight", highlight);
}

#[test]
fn snapshot_synthesize_sensors_truncated() {
    let highlight = synthesize_sensors_truncated(100, 250);
    insta::assert_json_snapshot!("synthesize_sensors_truncated", highlight);
}

// ---------------------------------------------------------------------------
// End-to-end: summarize_sensor_report
// ---------------------------------------------------------------------------

#[test]
fn snapshot_summarize_sensor_report_with_truncation() {
    let findings = vec![
        finding("E1", Severity::Error, "src/main.rs", 10),
        finding("W1", Severity::Warn, "src/lib.rs", 20),
        finding("I1", Severity::Info, "src/util.rs", 5),
        finding("E2", Severity::Error, "src/main.rs", 30),
    ];

    let report = SensorReport {
        schema: "sensor.report.v1".to_string(),
        tool: tool_info(),
        run: run_info(),
        verdict: Verdict {
            status: VerdictStatus::Fail,
            counts: VerdictCounts {
                info: 1,
                warn: 1,
                error: 2,
                suppressed: 0,
            },
            reasons: vec![],
        },
        findings,
        artifacts: vec![],
        data: None,
    };

    let (summary, highlights) = summarize_sensor_report(
        "builddiag",
        "artifacts/builddiag/report.json",
        None,
        &policy(true, MissingPolicy::Fail),
        report,
        3,
    );
    insta::assert_json_snapshot!("summarize_report_truncated_summary", summary);
    insta::assert_json_snapshot!("summarize_report_truncated_highlights", highlights);
}

#[test]
fn snapshot_derive_fingerprint_stability() {
    let f = finding("E1", Severity::Error, "src/main.rs", 42);
    let fp = derive_fingerprint("sensor_a", &f);
    insta::assert_snapshot!("derive_fingerprint_stable", fp);
}

// ---------------------------------------------------------------------------
// New expanded snapshot scenarios
// ---------------------------------------------------------------------------

#[test]
fn snapshot_overall_verdict_all_skip() {
    let summaries = vec![
        summary_with_verdict("a", false, VerdictStatus::Skip, VerdictCounts::default()),
        summary_with_verdict("b", false, VerdictStatus::Skip, VerdictCounts::default()),
    ];
    let verdict = overall_verdict(&summaries, &CockpitConfig::default());
    insta::assert_json_snapshot!("overall_verdict_all_skip", verdict);
}

#[test]
fn snapshot_overall_verdict_mixed_blocking_and_nonblocking() {
    let summaries = vec![
        summary_with_verdict(
            "blocking_pass",
            true,
            VerdictStatus::Pass,
            VerdictCounts::default(),
        ),
        summary_with_verdict(
            "nonblocking_fail",
            false,
            VerdictStatus::Fail,
            VerdictCounts {
                info: 0,
                warn: 0,
                error: 10,
                suppressed: 0,
            },
        ),
        summary_with_verdict(
            "blocking_warn",
            true,
            VerdictStatus::Warn,
            VerdictCounts {
                info: 0,
                warn: 3,
                error: 0,
                suppressed: 0,
            },
        ),
    ];
    let verdict = overall_verdict(&summaries, &CockpitConfig::default());
    insta::assert_json_snapshot!("overall_verdict_mixed_blocking_nonblocking", verdict);
}

#[test]
fn snapshot_synthesize_missing_warn() {
    let (summary, highlight) = synthesize_missing_sensor(
        "lint",
        &policy(true, MissingPolicy::Warn),
        "artifacts/lint/report.json",
        Some("artifacts/lint/comment.md".to_string()),
    );
    insta::assert_json_snapshot!("synthesize_missing_warn_summary", summary);
    insta::assert_json_snapshot!("synthesize_missing_warn_highlight", highlight);
}

#[test]
fn snapshot_capped_findings_no_truncation() {
    let findings = vec![
        finding("E1", Severity::Error, "src/a.rs", 1),
        finding("W1", Severity::Warn, "src/b.rs", 1),
    ];
    let (capped, truncated) = cap_findings(findings, 10);
    assert!(!truncated);
    insta::assert_json_snapshot!("capped_findings_no_truncation", capped);
}

#[test]
fn snapshot_highlight_selection_empty() {
    let cfg = CockpitConfig::default();
    let selected = select_highlights(vec![], &cfg, &std::collections::BTreeMap::new());
    insta::assert_json_snapshot!("highlight_selection_empty", selected);
}

#[test]
fn snapshot_compute_policy_outcome_nonblocking_matrix() {
    let results = vec![
        (
            "nonblocking_pass",
            compute_policy_outcome(false, &VerdictStatus::Pass),
        ),
        (
            "nonblocking_warn",
            compute_policy_outcome(false, &VerdictStatus::Warn),
        ),
        (
            "nonblocking_skip",
            compute_policy_outcome(false, &VerdictStatus::Skip),
        ),
        (
            "blocking_skip",
            compute_policy_outcome(true, &VerdictStatus::Skip),
        ),
    ];
    insta::assert_debug_snapshot!("policy_outcome_nonblocking_matrix", results);
}

#[test]
fn snapshot_summarize_sensor_report_no_truncation() {
    let findings = vec![
        finding("W1", Severity::Warn, "src/lib.rs", 20),
        finding("I1", Severity::Info, "src/util.rs", 5),
    ];

    let report = SensorReport {
        schema: "sensor.report.v1".to_string(),
        tool: tool_info(),
        run: run_info(),
        verdict: Verdict {
            status: VerdictStatus::Warn,
            counts: VerdictCounts {
                info: 1,
                warn: 1,
                error: 0,
                suppressed: 0,
            },
            reasons: vec![],
        },
        findings,
        artifacts: vec![],
        data: None,
    };

    let (summary, highlights) = summarize_sensor_report(
        "lint",
        "artifacts/lint/report.json",
        None,
        &policy(false, MissingPolicy::Skip),
        report,
        20,
    );
    insta::assert_json_snapshot!("summarize_report_no_truncation_summary", summary);
    insta::assert_json_snapshot!("summarize_report_no_truncation_highlights", highlights);
}

#[test]
fn snapshot_build_cockpit_report() {
    let mut cfg = CockpitConfig::default();
    cfg.sensors.insert(
        "builddiag".to_string(),
        SensorPolicy {
            blocking: true,
            missing: MissingPolicy::Fail,
            section: Some("Build".to_string()),
            require_label: None,
            repro: None,
        },
    );
    cfg.policy.section_order = vec!["Build".to_string()];

    let summaries = vec![summary_with_verdict(
        "builddiag",
        true,
        VerdictStatus::Pass,
        VerdictCounts::default(),
    )];

    let report = build_cockpit_report(&cfg, tool_info(), run_info(), summaries, vec![]);
    insta::assert_json_snapshot!("build_cockpit_report", report);
}

#[test]
fn snapshot_sorted_findings_same_severity() {
    let mut findings = vec![
        finding("Z1", Severity::Error, "src/z.rs", 100),
        finding("A1", Severity::Error, "src/a.rs", 1),
        finding("M1", Severity::Error, "src/m.rs", 50),
    ];
    sort_findings("sensor", &mut findings);
    insta::assert_json_snapshot!("sorted_findings_same_severity", findings);
}
