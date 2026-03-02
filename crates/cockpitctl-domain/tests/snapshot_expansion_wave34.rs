//! Wave-34 snapshot expansion for cockpitctl-domain.
//!
//! Covers:
//!  - Policy evaluation output for all verdict combinations
//!  - Highlight selection with various cap configurations
//!  - Normalization output for edge-case inputs

use cockpitctl_domain::{
    build_cockpit_report, cap_findings, compute_counts, compute_policy_outcome, derive_fingerprint,
    overall_verdict, select_highlights, sort_findings, summarize_sensor_report,
    synthesize_invalid_sensor, synthesize_missing_sensor, synthesize_receipt_inconsistent,
};
use cockpitctl_types::{
    CockpitConfig, Finding, Highlight, Location, MissingPolicy, RunInfo, SensorPolicy,
    SensorReport, SensorSummary, Severity, ToolInfo, Verdict, VerdictCounts, VerdictStatus,
};
use std::collections::BTreeMap;

// ── Helpers ─────────────────────────────────────────────────────────────

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

fn finding_no_location(code: &str, severity: Severity, message: &str) -> Finding {
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

fn summary_with_verdict(
    id: &str,
    blocking: bool,
    status: VerdictStatus,
    counts: VerdictCounts,
) -> SensorSummary {
    SensorSummary {
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

// =========================================================================
// 1. Policy evaluation: full verdict matrix
// =========================================================================

#[test]
fn snapshot_policy_outcome_full_matrix() {
    let matrix: Vec<(&str, bool, VerdictStatus)> = vec![
        ("blocking_pass", true, VerdictStatus::Pass),
        ("blocking_warn", true, VerdictStatus::Warn),
        ("blocking_fail", true, VerdictStatus::Fail),
        ("blocking_skip", true, VerdictStatus::Skip),
        ("nonblocking_pass", false, VerdictStatus::Pass),
        ("nonblocking_warn", false, VerdictStatus::Warn),
        ("nonblocking_fail", false, VerdictStatus::Fail),
        ("nonblocking_skip", false, VerdictStatus::Skip),
    ];

    let results: Vec<(&str, cockpitctl_types::PolicyOutcome)> = matrix
        .iter()
        .map(|(label, blocking, status)| (*label, compute_policy_outcome(*blocking, status)))
        .collect();

    insta::assert_debug_snapshot!("policy_outcome_full_matrix", results);
}

#[test]
fn snapshot_overall_verdict_five_sensor_mix() {
    let summaries = vec![
        summary_with_verdict("build", true, VerdictStatus::Pass, VerdictCounts::default()),
        summary_with_verdict(
            "lint",
            true,
            VerdictStatus::Warn,
            VerdictCounts {
                info: 0,
                warn: 5,
                error: 0,
                suppressed: 0,
            },
        ),
        summary_with_verdict(
            "test",
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
            "coverage",
            false,
            VerdictStatus::Fail,
            VerdictCounts {
                info: 0,
                warn: 0,
                error: 1,
                suppressed: 0,
            },
        ),
        summary_with_verdict("docs", false, VerdictStatus::Skip, VerdictCounts::default()),
    ];

    let verdict = overall_verdict(&summaries, &CockpitConfig::default());
    insta::assert_json_snapshot!("overall_verdict_five_sensor_mix", verdict);
}

#[test]
fn snapshot_overall_verdict_warn_is_fail_with_mixed() {
    let summaries = vec![
        summary_with_verdict("build", true, VerdictStatus::Pass, VerdictCounts::default()),
        summary_with_verdict(
            "lint",
            true,
            VerdictStatus::Warn,
            VerdictCounts {
                info: 1,
                warn: 2,
                error: 0,
                suppressed: 0,
            },
        ),
        summary_with_verdict(
            "coverage",
            false,
            VerdictStatus::Warn,
            VerdictCounts {
                info: 0,
                warn: 1,
                error: 0,
                suppressed: 0,
            },
        ),
    ];

    let mut cfg = CockpitConfig::default();
    cfg.policy.warn_is_fail = true;
    let verdict = overall_verdict(&summaries, &cfg);
    insta::assert_json_snapshot!("overall_verdict_warn_is_fail_mixed", verdict);
}

// =========================================================================
// 2. Highlight selection: various cap configurations
// =========================================================================

#[test]
fn snapshot_highlight_selection_cap_1() {
    let mut cfg = CockpitConfig::default();
    cfg.policy.max_highlights = 1;

    let candidates = vec![
        Highlight {
            sensor_id: "a".to_string(),
            finding: finding("E1", Severity::Error, "src/a.rs", 10),
        },
        Highlight {
            sensor_id: "b".to_string(),
            finding: finding("W1", Severity::Warn, "src/b.rs", 20),
        },
        Highlight {
            sensor_id: "c".to_string(),
            finding: finding("I1", Severity::Info, "src/c.rs", 30),
        },
    ];

    let blocking = BTreeMap::from([
        ("a".to_string(), true),
        ("b".to_string(), true),
        ("c".to_string(), false),
    ]);

    let selected = select_highlights(candidates, &cfg, &blocking);
    insta::assert_json_snapshot!("highlight_selection_cap_1", selected);
}

#[test]
fn snapshot_highlight_selection_cap_large() {
    let mut cfg = CockpitConfig::default();
    cfg.policy.max_highlights = 100;

    let candidates: Vec<Highlight> = (0..15)
        .map(|i| {
            let severity = match i % 3 {
                0 => Severity::Error,
                1 => Severity::Warn,
                _ => Severity::Info,
            };
            Highlight {
                sensor_id: format!("sensor-{}", i % 4),
                finding: finding(
                    &format!("CODE-{:03}", i),
                    severity,
                    &format!("src/mod_{}.rs", i),
                    (i * 10 + 1) as u32,
                ),
            }
        })
        .collect();

    let blocking = BTreeMap::from([
        ("sensor-0".to_string(), true),
        ("sensor-1".to_string(), false),
        ("sensor-2".to_string(), true),
        ("sensor-3".to_string(), false),
    ]);

    let selected = select_highlights(candidates, &cfg, &blocking);
    insta::assert_json_snapshot!("highlight_selection_cap_large", selected);
}

#[test]
fn snapshot_highlight_selection_all_same_severity() {
    let mut cfg = CockpitConfig::default();
    cfg.policy.max_highlights = 3;

    let candidates = vec![
        Highlight {
            sensor_id: "z-sensor".to_string(),
            finding: finding("CODE-Z", Severity::Warn, "src/z.rs", 100),
        },
        Highlight {
            sensor_id: "a-sensor".to_string(),
            finding: finding("CODE-A", Severity::Warn, "src/a.rs", 1),
        },
        Highlight {
            sensor_id: "m-sensor".to_string(),
            finding: finding("CODE-M", Severity::Warn, "src/m.rs", 50),
        },
        Highlight {
            sensor_id: "a-sensor".to_string(),
            finding: finding("CODE-B", Severity::Warn, "src/b.rs", 25),
        },
    ];

    let blocking = BTreeMap::new();
    let selected = select_highlights(candidates, &cfg, &blocking);
    insta::assert_json_snapshot!("highlight_selection_all_same_severity", selected);
}

// =========================================================================
// 3. Normalization: edge-case inputs
// =========================================================================

#[test]
fn snapshot_sort_findings_with_no_location() {
    let mut findings = vec![
        finding_no_location("E1", Severity::Error, "error without location"),
        finding("W1", Severity::Warn, "src/a.rs", 10),
        finding_no_location("E2", Severity::Error, "another error without location"),
        finding("I1", Severity::Info, "src/b.rs", 5),
    ];
    sort_findings("sensor", &mut findings);
    insta::assert_json_snapshot!("sorted_findings_with_no_location", findings);
}

#[test]
fn snapshot_sort_findings_same_file_different_lines() {
    let mut findings = vec![
        finding("W3", Severity::Warn, "src/main.rs", 100),
        finding("W1", Severity::Warn, "src/main.rs", 5),
        finding("W2", Severity::Warn, "src/main.rs", 50),
        finding("W4", Severity::Warn, "src/main.rs", 1),
    ];
    sort_findings("sensor", &mut findings);
    insta::assert_json_snapshot!("sorted_findings_same_file_diff_lines", findings);
}

#[test]
fn snapshot_cap_findings_exact_cap() {
    let findings = vec![
        finding("E1", Severity::Error, "src/a.rs", 1),
        finding("E2", Severity::Error, "src/b.rs", 2),
        finding("W1", Severity::Warn, "src/c.rs", 3),
    ];
    let (capped, truncated) = cap_findings(findings, 3);
    assert!(!truncated);
    insta::assert_json_snapshot!("capped_findings_exact_cap", capped);
}

#[test]
fn snapshot_cap_findings_zero_cap() {
    let findings = vec![
        finding("E1", Severity::Error, "src/a.rs", 1),
        finding("W1", Severity::Warn, "src/b.rs", 2),
    ];
    let (capped, truncated) = cap_findings(findings, 0);
    assert!(truncated);
    insta::assert_json_snapshot!("capped_findings_zero_cap", capped);
}

#[test]
fn snapshot_compute_counts_all_info() {
    let findings = vec![
        finding("I1", Severity::Info, "src/a.rs", 1),
        finding("I2", Severity::Info, "src/a.rs", 2),
        finding("I3", Severity::Info, "src/b.rs", 1),
    ];
    let counts = compute_counts(&findings);
    insta::assert_json_snapshot!("compute_counts_all_info", counts);
}

#[test]
fn snapshot_compute_counts_empty() {
    let findings: Vec<Finding> = vec![];
    let counts = compute_counts(&findings);
    insta::assert_json_snapshot!("compute_counts_empty", counts);
}

#[test]
fn snapshot_derive_fingerprint_no_location() {
    let f = finding_no_location("E1", Severity::Error, "error without location");
    let fp = derive_fingerprint("sensor_x", &f);
    insta::assert_snapshot!("derive_fingerprint_no_location", fp);
}

#[test]
fn snapshot_derive_fingerprint_different_sensors() {
    let f = finding("E1", Severity::Error, "src/main.rs", 42);
    let fp_a = derive_fingerprint("sensor_a", &f);
    let fp_b = derive_fingerprint("sensor_b", &f);
    // They must differ because sensor_id is part of the fingerprint
    assert_ne!(fp_a, fp_b);
    let combined = format!("sensor_a={}\nsensor_b={}", fp_a, fp_b);
    insta::assert_snapshot!("derive_fingerprint_different_sensors", combined);
}

// =========================================================================
// 4. Synthesized sensors: edge cases
// =========================================================================

#[test]
fn snapshot_synthesize_invalid_nonblocking() {
    let (summary, highlight) = synthesize_invalid_sensor(
        "optional-lint",
        &policy(false, MissingPolicy::Skip),
        "artifacts/optional-lint/report.json",
        None,
        "invalid JSON: expected ident at line 1 column 2".to_string(),
    );
    insta::assert_json_snapshot!("synthesize_invalid_nonblocking_summary", summary);
    insta::assert_json_snapshot!("synthesize_invalid_nonblocking_highlight", highlight);
}

#[test]
fn snapshot_synthesize_missing_all_policies() {
    let policies = vec![
        ("skip", MissingPolicy::Skip),
        ("warn", MissingPolicy::Warn),
        ("fail", MissingPolicy::Fail),
    ];

    let results: Vec<_> = policies
        .into_iter()
        .map(|(label, mp)| {
            let (summary, highlight) = synthesize_missing_sensor(
                &format!("sensor-{}", label),
                &policy(true, mp),
                &format!("artifacts/sensor-{}/report.json", label),
                None,
            );
            (label, summary, highlight)
        })
        .collect();

    insta::assert_json_snapshot!("synthesize_missing_all_policies", results);
}

#[test]
fn snapshot_synthesize_receipt_inconsistent() {
    let reported = VerdictCounts {
        info: 0,
        warn: 0,
        error: 0,
        suppressed: 0,
    };
    let computed = VerdictCounts {
        info: 0,
        warn: 1,
        error: 1,
        suppressed: 0,
    };

    let highlight = synthesize_receipt_inconsistent("sensor-x", &reported, &computed);
    insta::assert_json_snapshot!("synthesize_receipt_inconsistent", highlight);
}

// =========================================================================
// 5. End-to-end: build_cockpit_report with multiple sensors
// =========================================================================

#[test]
fn snapshot_build_cockpit_report_multi_sensor() {
    let mut cfg = CockpitConfig::default();
    cfg.policy.max_highlights = 5;
    cfg.policy.section_order = vec!["Build".to_string(), "Lint".to_string()];
    cfg.sensors.insert(
        "builddiag".to_string(),
        SensorPolicy {
            blocking: true,
            missing: MissingPolicy::Fail,
            section: Some("Build".to_string()),
            require_label: None,
            repro: Some("cargo build".to_string()),
        },
    );
    cfg.sensors.insert(
        "clippy".to_string(),
        SensorPolicy {
            blocking: true,
            missing: MissingPolicy::Warn,
            section: Some("Lint".to_string()),
            require_label: None,
            repro: Some("cargo clippy".to_string()),
        },
    );
    cfg.sensors.insert(
        "coverage".to_string(),
        SensorPolicy {
            blocking: false,
            missing: MissingPolicy::Skip,
            section: None,
            require_label: None,
            repro: None,
        },
    );

    let summaries = vec![
        summary_with_verdict(
            "builddiag",
            true,
            VerdictStatus::Fail,
            VerdictCounts {
                info: 0,
                warn: 0,
                error: 2,
                suppressed: 0,
            },
        ),
        summary_with_verdict(
            "clippy",
            true,
            VerdictStatus::Warn,
            VerdictCounts {
                info: 1,
                warn: 3,
                error: 0,
                suppressed: 0,
            },
        ),
        summary_with_verdict(
            "coverage",
            false,
            VerdictStatus::Pass,
            VerdictCounts::default(),
        ),
    ];

    let highlights = vec![
        Highlight {
            sensor_id: "builddiag".to_string(),
            finding: finding("E0308", Severity::Error, "src/main.rs", 10),
        },
        Highlight {
            sensor_id: "clippy".to_string(),
            finding: finding("clippy::unwrap_used", Severity::Warn, "src/lib.rs", 22),
        },
    ];

    let report = build_cockpit_report(&cfg, tool_info(), run_info(), summaries, highlights);
    insta::assert_json_snapshot!("build_cockpit_report_multi_sensor", report);
}

#[test]
fn snapshot_summarize_sensor_report_all_info_findings() {
    let findings = vec![
        finding("I1", Severity::Info, "src/a.rs", 1),
        finding("I2", Severity::Info, "src/b.rs", 2),
        finding("I3", Severity::Info, "src/c.rs", 3),
        finding("I4", Severity::Info, "src/d.rs", 4),
        finding("I5", Severity::Info, "src/e.rs", 5),
    ];

    let report = SensorReport {
        schema: "sensor.report.v1".to_string(),
        tool: tool_info(),
        run: run_info(),
        verdict: Verdict {
            status: VerdictStatus::Pass,
            counts: VerdictCounts {
                info: 5,
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

    let (summary, highlights) = summarize_sensor_report(
        "info-sensor",
        "artifacts/info-sensor/report.json",
        None,
        &policy(false, MissingPolicy::Skip),
        report,
        3,
    );
    insta::assert_json_snapshot!("summarize_all_info_summary", summary);
    insta::assert_json_snapshot!("summarize_all_info_highlights", highlights);
}
