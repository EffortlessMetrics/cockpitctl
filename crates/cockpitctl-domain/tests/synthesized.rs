use cockpitctl_domain::{
    COCKPIT_SCHEMA_ID, build_cockpit_report, compute_policy_outcome, select_auto_apply_fixes,
    summarize_sensor_report, synthesize_invalid_sensor, synthesize_missing_sensor,
    synthesize_path_traversal_highlight, synthesize_path_traversal_sensor,
    synthesize_receipt_inconsistent, synthesize_receipt_oversized_sensor,
    synthesize_schema_violation_sensor, synthesize_sensors_truncated,
};
use cockpitctl_types::{
    BuildfixSummary, CockpitConfig, Finding, FixSummary, Location, MissingPolicy, PolicyOutcome,
    Presence, RunInfo, SensorPolicy, SensorReport, Severity, ToolInfo, Verdict, VerdictCounts,
    VerdictStatus,
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

fn finding(code: &str, severity: Severity) -> Finding {
    Finding {
        severity,
        check_id: None,
        code: code.to_string(),
        message: format!("Message for {}", code),
        location: Some(Location {
            path: Some("src/main.rs".to_string()),
            line: Some(10),
            col: None,
        }),
        help: None,
        url: None,
        fingerprint: None,
        data: None,
    }
}

#[test]
fn compute_policy_outcome_respects_blocking_and_status() {
    assert_eq!(
        compute_policy_outcome(false, &VerdictStatus::Fail),
        PolicyOutcome::Informational
    );
    assert_eq!(
        compute_policy_outcome(true, &VerdictStatus::Fail),
        PolicyOutcome::Blocked
    );
    assert_eq!(
        compute_policy_outcome(true, &VerdictStatus::Warn),
        PolicyOutcome::Allowed
    );
}

#[test]
fn synthesize_missing_sensor_respects_policy() {
    let (summary_skip, highlight_skip) = synthesize_missing_sensor(
        "sensor",
        &policy(true, MissingPolicy::Skip),
        "artifacts/sensor/report.json",
        None,
    );
    assert_eq!(summary_skip.verdict.status, VerdictStatus::Skip);
    assert!(highlight_skip.is_none());

    let (summary_warn, highlight_warn) = synthesize_missing_sensor(
        "sensor",
        &policy(true, MissingPolicy::Warn),
        "artifacts/sensor/report.json",
        None,
    );
    assert_eq!(summary_warn.verdict.status, VerdictStatus::Warn);
    assert_eq!(highlight_warn.unwrap().finding.severity, Severity::Warn);

    let (summary_fail, highlight_fail) = synthesize_missing_sensor(
        "sensor",
        &policy(true, MissingPolicy::Fail),
        "artifacts/sensor/report.json",
        None,
    );
    assert_eq!(summary_fail.verdict.status, VerdictStatus::Fail);
    assert_eq!(highlight_fail.unwrap().finding.severity, Severity::Error);
}

#[test]
fn synthesize_invalid_sensor_populates_error_and_highlight() {
    let (summary, highlight) = synthesize_invalid_sensor(
        "sensor",
        &policy(true, MissingPolicy::Fail),
        "artifacts/sensor/report.json",
        None,
        "bad json".to_string(),
    );
    assert_eq!(summary.presence, Presence::Invalid);
    assert_eq!(summary.verdict.status, VerdictStatus::Fail);
    assert_eq!(summary.errors, vec!["bad json".to_string()]);
    assert_eq!(highlight.unwrap().finding.code, "cockpit.invalid_receipt");
}

#[test]
fn synthesize_schema_violation_sensor_includes_error_details() {
    let (summary, highlight) = synthesize_schema_violation_sensor(
        "sensor",
        &policy(true, MissingPolicy::Fail),
        "artifacts/sensor/report.json",
        None,
        vec!["missing schema".to_string(), "bad status".to_string()],
    );
    assert_eq!(summary.presence, Presence::Invalid);
    assert_eq!(summary.verdict.status, VerdictStatus::Fail);
    assert_eq!(summary.errors.len(), 2);
    let message = highlight.unwrap().finding.message;
    assert!(
        message.contains("2 schema violations"),
        "message should summarize multiple errors: {}",
        message
    );
}

#[test]
fn synthesize_receipt_oversized_sensor_marks_invalid() {
    let (summary, highlight) = synthesize_receipt_oversized_sensor(
        "sensor",
        &policy(true, MissingPolicy::Fail),
        "artifacts/sensor/report.json",
        None,
        99,
        10,
    );
    assert_eq!(summary.presence, Presence::Invalid);
    assert_eq!(summary.verdict.status, VerdictStatus::Fail);
    assert_eq!(
        highlight.finding.code,
        "cockpit.receipt_oversized".to_string()
    );
}

#[test]
fn synthesize_path_traversal_sensor_blocks_and_highlights() {
    let (summary, highlight) = synthesize_path_traversal_sensor(
        "sensor",
        &policy(true, MissingPolicy::Fail),
        "artifacts/sensor/report.json",
        None,
        Some("report.json".to_string()),
    );
    assert_eq!(summary.presence, Presence::Missing);
    assert_eq!(summary.verdict.status, VerdictStatus::Fail);
    assert_eq!(summary.policy_outcome, Some(PolicyOutcome::Blocked));
    assert_eq!(highlight.finding.code, "cockpit.path_traversal");
    assert_eq!(highlight.sensor_id, "_cockpit");
}

#[test]
fn synthesize_path_traversal_highlight_context_is_in_message() {
    let highlight = synthesize_path_traversal_highlight(
        "sensor",
        "artifacts/sensor/report.json",
        Some("comment.md".to_string()),
    );
    assert!(highlight.finding.message.contains("comment.md"));
}

#[test]
fn synthesize_receipt_inconsistent_sets_code() {
    let reported = VerdictCounts {
        info: 0,
        warn: 0,
        error: 1,
        suppressed: 0,
    };
    let computed = VerdictCounts {
        info: 1,
        warn: 0,
        error: 0,
        suppressed: 0,
    };
    let highlight = synthesize_receipt_inconsistent("sensor", &reported, &computed);
    assert_eq!(highlight.finding.code, "cockpit.receipt_inconsistent");
}

#[test]
fn synthesize_sensors_truncated_is_warning() {
    let highlight = synthesize_sensors_truncated(5, 12);
    assert_eq!(highlight.finding.severity, Severity::Warn);
    assert!(highlight.finding.message.contains("processed 5 of 12"));
}

#[test]
fn summarize_sensor_report_recomputes_counts_and_truncates() {
    let findings = vec![
        finding("E1", Severity::Error),
        finding("E2", Severity::Error),
        finding("W1", Severity::Warn),
    ];

    let report = SensorReport {
        schema: "sensor.report.v1".to_string(),
        tool: tool_info(),
        run: run_info(),
        verdict: Verdict {
            status: VerdictStatus::Warn,
            counts: VerdictCounts {
                info: 0,
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
        "sensor",
        "artifacts/sensor/report.json",
        None,
        &policy(true, MissingPolicy::Fail),
        report,
        2,
    );

    assert!(summary.truncated);
    assert!(
        summary
            .verdict
            .reasons
            .contains(&"receipt_inconsistent".to_string())
    );
    assert_eq!(summary.verdict.counts.error, 2);
    assert_eq!(highlights.len(), 3, "1 inconsistency + 2 findings");
}

#[test]
fn build_cockpit_report_sets_schema_id() {
    let report = build_cockpit_report(
        &CockpitConfig::default(),
        tool_info(),
        run_info(),
        vec![],
        vec![],
    );
    assert_eq!(report.schema, COCKPIT_SCHEMA_ID);
}

#[test]
fn select_auto_apply_fixes_respects_safety_and_match_gate() {
    let summary = BuildfixSummary {
        fixes: vec![
            FixSummary {
                fix_id: "safe_matched".to_string(),
                sensor_id: "builddiag".to_string(),
                safety: cockpitctl_types::SafetyLevel::Safe,
                description: "safe".to_string(),
                matched_findings: vec![],
                unmatched: false,
            },
            FixSummary {
                fix_id: "guarded_matched".to_string(),
                sensor_id: "builddiag".to_string(),
                safety: cockpitctl_types::SafetyLevel::Guarded,
                description: "guarded".to_string(),
                matched_findings: vec![],
                unmatched: false,
            },
            FixSummary {
                fix_id: "safe_unmatched".to_string(),
                sensor_id: "builddiag".to_string(),
                safety: cockpitctl_types::SafetyLevel::Safe,
                description: "unmatched".to_string(),
                matched_findings: vec![],
                unmatched: true,
            },
        ],
        total_fixes: 3,
        matched_count: 2,
        unmatched_count: 1,
    };

    let safe_only = select_auto_apply_fixes(&summary, cockpitctl_types::SafetyLevel::Safe, true);
    let ids: Vec<&str> = safe_only.iter().map(|f| f.fix_id.as_str()).collect();
    assert_eq!(ids, vec!["safe_matched"]);

    let guarded_and_safe =
        select_auto_apply_fixes(&summary, cockpitctl_types::SafetyLevel::Guarded, true);
    let ids: Vec<&str> = guarded_and_safe.iter().map(|f| f.fix_id.as_str()).collect();
    assert_eq!(ids, vec!["safe_matched", "guarded_matched"]);

    let include_unmatched =
        select_auto_apply_fixes(&summary, cockpitctl_types::SafetyLevel::Safe, false);
    let ids: Vec<&str> = include_unmatched
        .iter()
        .map(|f| f.fix_id.as_str())
        .collect();
    assert_eq!(ids, vec!["safe_matched", "safe_unmatched"]);
}
