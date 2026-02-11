use cockpitctl_render::render_comment;
use cockpitctl_types::{
    CockpitConfig, CockpitReport, Finding, Highlight, Location, PolicySensorSnapshot,
    PolicySnapshot, Presence, RunInfo, SensorPolicy, SensorSummary, Severity, ToolInfo, Verdict,
    VerdictCounts, VerdictStatus,
};

fn tool_info() -> ToolInfo {
    ToolInfo {
        name: "cockpitctl".to_string(),
        version: "0.2.0".to_string(),
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

fn policy_snapshot_from_cfg(cfg: &CockpitConfig) -> PolicySnapshot {
    let mut sensors = Vec::new();
    for (id, p) in cfg.sensors.iter() {
        sensors.push(PolicySensorSnapshot {
            id: id.clone(),
            blocking: p.blocking,
            missing: p.missing,
            section: p.section.clone(),
            require_label: p.require_label.clone(),
            repro: p.repro.clone(),
        });
    }
    PolicySnapshot {
        warn_is_fail: cfg.policy.warn_is_fail,
        max_highlights: cfg.policy.max_highlights,
        max_per_sensor_findings: cfg.policy.max_per_sensor_findings,
        max_annotations: cfg.policy.max_annotations,
        section_order: cfg.policy.section_order.clone(),
        sensors,
    }
}

fn highlight(sensor_id: &str, code: &str, severity: Severity) -> Highlight {
    Highlight {
        sensor_id: sensor_id.to_string(),
        finding: Finding {
            severity,
            check_id: None,
            code: code.to_string(),
            message: format!("Message for {}", code),
            location: Some(Location {
                path: Some("src/lib.rs".to_string()),
                line: Some(5),
                col: None,
            }),
            help: None,
            url: None,
            fingerprint: None,
            data: None,
        },
    }
}

fn sensor_summary(id: &str, status: VerdictStatus, truncated: bool) -> SensorSummary {
    SensorSummary {
        id: id.to_string(),
        blocking: true,
        missing: cockpitctl_types::MissingPolicy::Fail,
        presence: Presence::Present,
        report_path: format!("artifacts/{}/report.json", id),
        comment_path: Some(format!("artifacts/{}/comment.md", id)),
        verdict: Verdict {
            status,
            counts: VerdictCounts::default(),
            reasons: vec![],
        },
        truncated,
        errors: vec![],
        missing_policy_applied: None,
        policy_outcome: None,
    }
}

#[test]
fn render_comment_includes_summary_highlights_annotations_and_sections() {
    let mut cfg = CockpitConfig::default();
    cfg.policy.max_highlights = 3;
    cfg.policy.max_annotations = 2;
    cfg.sensors.insert(
        "alpha".to_string(),
        SensorPolicy {
            blocking: true,
            missing: cockpitctl_types::MissingPolicy::Fail,
            section: Some("Tests".to_string()),
            require_label: None,
            repro: Some("cargo test".to_string()),
        },
    );
    cfg.sensors.insert(
        "beta".to_string(),
        SensorPolicy {
            blocking: false,
            missing: cockpitctl_types::MissingPolicy::Warn,
            section: None,
            require_label: None,
            repro: None,
        },
    );

    let report = CockpitReport {
        schema: "cockpit.report.v1".to_string(),
        tool: tool_info(),
        run: run_info(),
        verdict: Verdict {
            status: VerdictStatus::Warn,
            counts: VerdictCounts::default(),
            reasons: vec![],
        },
        sensors: vec![
            sensor_summary("alpha", VerdictStatus::Fail, false),
            sensor_summary("beta", VerdictStatus::Pass, true),
        ],
        highlights: vec![highlight("alpha", "E1", Severity::Error)],
        policy: policy_snapshot_from_cfg(&cfg),
        data: None,
    };

    let md = render_comment(&report, &cfg);

    assert!(md.contains("<!-- cockpit:begin -->"));
    assert!(md.contains("<!-- cockpit:end -->"));
    assert!(md.contains("## Cockpit"));
    assert!(md.contains("| `alpha` |"));
    assert!(md.contains("_truncated_"));
    assert!(md.contains("### Highlights"));
    assert!(md.contains("(showing up to **3**)"));
    assert!(md.contains("`E1` at `src/lib.rs:5`"));
    assert!(md.contains("### Annotations"));
    assert!(md.contains("### Tests"));
    assert!(md.contains("repro: `cargo test`"));
}

#[test]
fn render_comment_no_highlights_renders_empty_states() {
    let cfg = CockpitConfig::default();
    let report = CockpitReport {
        schema: "cockpit.report.v1".to_string(),
        tool: tool_info(),
        run: run_info(),
        verdict: Verdict {
            status: VerdictStatus::Pass,
            counts: VerdictCounts::default(),
            reasons: vec![],
        },
        sensors: vec![sensor_summary("alpha", VerdictStatus::Pass, false)],
        highlights: vec![],
        policy: policy_snapshot_from_cfg(&cfg),
        data: None,
    };

    let md = render_comment(&report, &cfg);
    assert!(md.contains("_No highlights._"));
    assert!(md.contains("_No annotations._"));
}

#[test]
fn render_comment_handles_missing_locations_and_comment_paths() {
    let cfg = CockpitConfig::default();
    let report = CockpitReport {
        schema: "cockpit.report.v1".to_string(),
        tool: tool_info(),
        run: run_info(),
        verdict: Verdict {
            status: VerdictStatus::Pass,
            counts: VerdictCounts::default(),
            reasons: vec![],
        },
        sensors: vec![SensorSummary {
            id: "alpha".to_string(),
            blocking: true,
            missing: cockpitctl_types::MissingPolicy::Fail,
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
        }],
        highlights: vec![Highlight {
            sensor_id: "alpha".to_string(),
            finding: Finding {
                severity: Severity::Warn,
                check_id: None,
                code: "W1".to_string(),
                message: "message".to_string(),
                location: None,
                help: None,
                url: None,
                fingerprint: None,
                data: None,
            },
        }],
        policy: policy_snapshot_from_cfg(&cfg),
        data: None,
    };

    let md = render_comment(&report, &cfg);
    assert!(md.contains("| `alpha` |"));
    assert!(md.contains("`W1`"));
    assert!(!md.contains(" at `"));
}

#[test]
fn render_comment_covers_warn_skip_and_nonblocking() {
    let mut cfg = CockpitConfig::default();
    cfg.sensors.insert(
        "warn_sensor".to_string(),
        SensorPolicy {
            blocking: true,
            missing: cockpitctl_types::MissingPolicy::Fail,
            section: None,
            require_label: None,
            repro: None,
        },
    );
    cfg.sensors.insert(
        "skip_sensor".to_string(),
        SensorPolicy {
            blocking: false,
            missing: cockpitctl_types::MissingPolicy::Skip,
            section: None,
            require_label: None,
            repro: None,
        },
    );

    let report = CockpitReport {
        schema: "cockpit.report.v1".to_string(),
        tool: tool_info(),
        run: run_info(),
        verdict: Verdict {
            status: VerdictStatus::Warn,
            counts: VerdictCounts::default(),
            reasons: vec![],
        },
        sensors: vec![
            SensorSummary {
                id: "warn_sensor".to_string(),
                blocking: true,
                missing: cockpitctl_types::MissingPolicy::Fail,
                presence: Presence::Present,
                report_path: "artifacts/warn_sensor/report.json".to_string(),
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
                id: "skip_sensor".to_string(),
                blocking: false,
                missing: cockpitctl_types::MissingPolicy::Skip,
                presence: Presence::Present,
                report_path: "artifacts/skip_sensor/report.json".to_string(),
                comment_path: None,
                verdict: Verdict {
                    status: VerdictStatus::Skip,
                    counts: VerdictCounts::default(),
                    reasons: vec![],
                },
                truncated: false,
                errors: vec![],
                missing_policy_applied: None,
                policy_outcome: None,
            },
        ],
        highlights: vec![],
        policy: policy_snapshot_from_cfg(&cfg),
        data: None,
    };

    let md = render_comment(&report, &cfg);
    assert!(md.contains("⚠️ warn"));
    assert!(md.contains("⏭ skip"));
    assert!(md.contains("| `skip_sensor` | ⏭ skip | no |"));
}

#[test]
fn render_comment_highlight_with_empty_location_omits_loc() {
    let cfg = CockpitConfig::default();
    let report = CockpitReport {
        schema: "cockpit.report.v1".to_string(),
        tool: tool_info(),
        run: run_info(),
        verdict: Verdict {
            status: VerdictStatus::Pass,
            counts: VerdictCounts::default(),
            reasons: vec![],
        },
        sensors: vec![sensor_summary("alpha", VerdictStatus::Pass, false)],
        highlights: vec![Highlight {
            sensor_id: "alpha".to_string(),
            finding: Finding {
                severity: Severity::Info,
                check_id: None,
                code: "I1".to_string(),
                message: "message".to_string(),
                location: Some(Location {
                    path: None,
                    line: None,
                    col: None,
                }),
                help: None,
                url: None,
                fingerprint: None,
                data: None,
            },
        }],
        policy: policy_snapshot_from_cfg(&cfg),
        data: None,
    };

    let md = render_comment(&report, &cfg);
    assert!(md.contains("`I1`"));
    assert!(!md.contains(" at `"));
}
