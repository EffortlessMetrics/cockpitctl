use cockpitctl_domain::{
    derive_fingerprint, overall_verdict, select_highlights, snapshot_policy, sort_sensor_summaries,
    summarize_sensor_report, synthesize_path_traversal_highlight,
    synthesize_schema_violation_sensor,
};
use cockpitctl_types::{
    CockpitConfig, Finding, Highlight, Location, MissingPolicy, Policy, Presence, RunInfo,
    SensorPolicy, SensorReport, SensorSummary, Severity, ToolInfo, Verdict, VerdictCounts,
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

fn finding(code: &str, severity: Severity) -> Finding {
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

#[test]
fn synthesize_schema_violation_single_error_uses_single_message() {
    let policy = SensorPolicy {
        blocking: true,
        missing: MissingPolicy::Fail,
        section: None,
        require_label: None,
        repro: None,
    };
    let (_summary, highlight) = synthesize_schema_violation_sensor(
        "sensor",
        &policy,
        "artifacts/sensor/report.json",
        None,
        vec!["missing schema".to_string()],
    );
    let message = highlight.unwrap().finding.message;
    assert!(
        message.contains("missing schema") && !message.contains("schema violations"),
        "expected single error message, got: {}",
        message
    );
}

#[test]
fn select_highlights_dedupes_and_derives_fingerprint() {
    let mut f = finding("CODE1", Severity::Error);
    f.fingerprint = None;
    let derived = derive_fingerprint("sensor", &f);

    let h1 = Highlight {
        sensor_id: "sensor".to_string(),
        finding: f,
    };
    let h2 = Highlight {
        sensor_id: "sensor".to_string(),
        finding: Finding {
            severity: Severity::Error,
            check_id: None,
            code: "CODE1".to_string(),
            message: "Message for CODE1".to_string(),
            location: Some(Location {
                path: Some("src/lib.rs".to_string()),
                line: Some(10),
                col: None,
            }),
            help: None,
            url: None,
            fingerprint: Some(derived.clone()),
            data: None,
        },
    };

    let cfg = CockpitConfig::default();
    let selected = select_highlights(vec![h1, h2], &cfg, &std::collections::BTreeMap::new());

    assert_eq!(selected.len(), 1, "expected dedupe by fingerprint");
    assert_eq!(
        selected[0].finding.fingerprint.as_deref(),
        Some(derived.as_str())
    );
}

#[test]
fn sort_sensor_summaries_respects_section_order_and_other() {
    let mut cfg = CockpitConfig {
        policy: Policy {
            warn_is_fail: false,
            max_highlights: 7,
            max_per_sensor_findings: 20,
            max_annotations: 25,
            section_order: vec!["Tests".to_string()],
            schema_validation: Default::default(),
        },
        ..Default::default()
    };
    cfg.sensors.insert(
        "alpha".to_string(),
        SensorPolicy {
            blocking: true,
            missing: MissingPolicy::Fail,
            section: Some("Tests".to_string()),
            require_label: None,
            repro: None,
        },
    );
    cfg.sensors.insert(
        "beta".to_string(),
        SensorPolicy {
            blocking: true,
            missing: MissingPolicy::Fail,
            section: Some("Misc".to_string()),
            require_label: None,
            repro: None,
        },
    );

    let mut summaries = vec![
        SensorSummary {
            id: "beta".to_string(),
            blocking: true,
            missing: MissingPolicy::Fail,
            presence: Presence::Present,
            report_path: "artifacts/beta/report.json".to_string(),
            comment_path: None,
            verdict: Verdict {
                status: VerdictStatus::Pass,
                counts: VerdictCounts::default(),
                reasons: vec![],
            },
            truncated: false,
            errors: vec![],
            missing_policy_applied: None,
            policy_outcome: None,
        },
        SensorSummary {
            id: "alpha".to_string(),
            blocking: true,
            missing: MissingPolicy::Fail,
            presence: Presence::Present,
            report_path: "artifacts/alpha/report.json".to_string(),
            comment_path: None,
            verdict: Verdict {
                status: VerdictStatus::Pass,
                counts: VerdictCounts::default(),
                reasons: vec![],
            },
            truncated: false,
            errors: vec![],
            missing_policy_applied: None,
            policy_outcome: None,
        },
    ];

    sort_sensor_summaries(&mut summaries, &cfg);
    let ids: Vec<_> = summaries.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(ids, vec!["alpha", "beta"]);
}

#[test]
fn overall_verdict_warn_is_fail_reason_only_once() {
    let summaries = vec![
        SensorSummary {
            id: "a".to_string(),
            blocking: true,
            missing: MissingPolicy::Fail,
            presence: Presence::Present,
            report_path: "a/report.json".to_string(),
            comment_path: None,
            verdict: Verdict {
                status: VerdictStatus::Warn,
                counts: VerdictCounts::default(),
                reasons: vec![],
            },
            truncated: false,
            errors: vec![],
            missing_policy_applied: None,
            policy_outcome: None,
        },
        SensorSummary {
            id: "b".to_string(),
            blocking: true,
            missing: MissingPolicy::Fail,
            presence: Presence::Present,
            report_path: "b/report.json".to_string(),
            comment_path: None,
            verdict: Verdict {
                status: VerdictStatus::Warn,
                counts: VerdictCounts::default(),
                reasons: vec![],
            },
            truncated: false,
            errors: vec![],
            missing_policy_applied: None,
            policy_outcome: None,
        },
    ];

    let mut cfg = CockpitConfig::default();
    cfg.policy.warn_is_fail = true;
    let verdict = overall_verdict(&summaries, &cfg);

    assert_eq!(verdict.status, VerdictStatus::Fail);
    assert_eq!(verdict.reasons, vec!["warn_is_fail".to_string()]);
}

#[test]
fn summarize_sensor_report_without_inconsistency_has_no_extra_highlight() {
    let findings = vec![
        finding("E1", Severity::Error),
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
                warn: 1,
                error: 1,
                suppressed: 0,
            },
            reasons: vec![],
        },
        findings: findings.clone(),
        artifacts: vec![],
        data: None,
    };

    let policy = SensorPolicy {
        blocking: true,
        missing: MissingPolicy::Fail,
        section: None,
        require_label: None,
        repro: None,
    };
    let (summary, highlights) = summarize_sensor_report(
        "sensor",
        "artifacts/sensor/report.json",
        None,
        &policy,
        report,
        10,
    );

    assert!(!summary.truncated);
    assert!(
        !summary
            .verdict
            .reasons
            .contains(&"receipt_inconsistent".to_string()),
        "should not flag receipt_inconsistent when counts match"
    );
    assert_eq!(highlights.len(), findings.len());
}

#[test]
fn synthesize_path_traversal_highlight_without_context_is_plain() {
    let highlight =
        synthesize_path_traversal_highlight("sensor", "artifacts/sensor/report.json", None);
    assert!(!highlight.finding.message.contains("(unsafe"));
}

#[test]
fn snapshot_policy_includes_sensor_entries() {
    let mut cfg = CockpitConfig::default();
    cfg.sensors.insert(
        "alpha".to_string(),
        SensorPolicy {
            blocking: true,
            missing: MissingPolicy::Warn,
            section: Some("Diagnostics".to_string()),
            require_label: Some("needs-diag".to_string()),
            repro: Some("cargo test".to_string()),
        },
    );

    let snapshot = snapshot_policy(&cfg);
    assert_eq!(snapshot.sensors.len(), 1);
    let sensor = &snapshot.sensors[0];
    assert_eq!(sensor.id, "alpha");
    assert!(sensor.blocking);
    assert_eq!(sensor.missing, MissingPolicy::Warn);
    assert_eq!(sensor.section.as_deref(), Some("Diagnostics"));
    assert_eq!(sensor.require_label.as_deref(), Some("needs-diag"));
    assert_eq!(sensor.repro.as_deref(), Some("cargo test"));
}

#[test]
fn select_highlights_exercises_blocking_branch_ordering() {
    let mut cfg = CockpitConfig::default();
    cfg.policy.max_highlights = 10;

    let highlight_blocking = Highlight {
        sensor_id: "blocker".to_string(),
        finding: finding("E_BLOCK", Severity::Error),
    };
    let highlight_nonblocking = Highlight {
        sensor_id: "nonblock".to_string(),
        finding: finding("E_NON", Severity::Error),
    };

    let mut sensor_blocking = std::collections::BTreeMap::new();
    sensor_blocking.insert("blocker".to_string(), true);
    sensor_blocking.insert("nonblock".to_string(), false);

    let selected = select_highlights(
        vec![highlight_blocking.clone(), highlight_nonblocking.clone()],
        &cfg,
        &sensor_blocking,
    );
    assert_eq!(selected.len(), 2);

    let selected_reversed = select_highlights(
        vec![highlight_nonblocking, highlight_blocking],
        &cfg,
        &sensor_blocking,
    );
    assert_eq!(selected_reversed.len(), 2);
}
