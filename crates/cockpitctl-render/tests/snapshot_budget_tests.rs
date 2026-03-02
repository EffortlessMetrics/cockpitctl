//! Snapshot tests for render_comment with different budget sizes (0, 1, 5, 50 findings).

use cockpitctl_render::render_comment;
use cockpitctl_types::{
    CockpitConfig, CockpitReport, Finding, Highlight, Location, MissingPolicy,
    PolicySensorSnapshot, PolicySnapshot, Presence, RunInfo, SensorPolicy, SensorSummary, Severity,
    ToolInfo, Verdict, VerdictCounts, VerdictStatus,
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

fn make_highlight(sensor_id: &str, code: &str, severity: Severity, idx: u32) -> Highlight {
    Highlight {
        sensor_id: sensor_id.to_string(),
        finding: Finding {
            severity,
            check_id: None,
            code: code.to_string(),
            message: format!("Finding {} from {}", code, sensor_id),
            location: Some(Location {
                path: Some(format!("src/file_{}.rs", idx)),
                line: Some(idx),
                col: None,
            }),
            help: None,
            url: None,
            fingerprint: None,
            data: None,
        },
    }
}

fn sensor_summary(id: &str, status: VerdictStatus, blocking: bool) -> SensorSummary {
    SensorSummary {
        id: id.to_string(),
        blocking,
        missing: MissingPolicy::Fail,
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

fn base_cfg() -> CockpitConfig {
    let mut cfg = CockpitConfig::default();
    cfg.sensors.insert(
        "scanner".to_string(),
        SensorPolicy {
            blocking: true,
            missing: MissingPolicy::Fail,
            section: Some("Security".to_string()),
            require_label: None,
            repro: None,
        },
    );
    cfg.policy.section_order = vec!["Security".to_string()];
    cfg
}

// ---------------------------------------------------------------------------
// Budget = 0 findings (highlights present but zero budget)
// ---------------------------------------------------------------------------

#[test]
fn snapshot_render_zero_budget() {
    let mut cfg = base_cfg();
    cfg.policy.max_highlights = 0;
    cfg.policy.max_annotations = 0;

    let report = CockpitReport {
        schema: "cockpit.report.v1".to_string(),
        tool: tool_info(),
        run: run_info(),
        verdict: Verdict {
            status: VerdictStatus::Fail,
            counts: VerdictCounts {
                info: 0,
                warn: 0,
                error: 1,
                suppressed: 0,
            },
            reasons: vec![],
        },
        sensors: vec![sensor_summary("scanner", VerdictStatus::Fail, true)],
        highlights: vec![],
        policy: policy_snapshot_from_cfg(&cfg),
        data: None,
    };

    let md = render_comment(&report, &cfg);
    insta::assert_snapshot!("render_zero_budget", md);
}

// ---------------------------------------------------------------------------
// Budget = 1 finding
// ---------------------------------------------------------------------------

#[test]
fn snapshot_render_one_finding() {
    let mut cfg = base_cfg();
    cfg.policy.max_highlights = 1;
    cfg.policy.max_annotations = 1;

    let report = CockpitReport {
        schema: "cockpit.report.v1".to_string(),
        tool: tool_info(),
        run: run_info(),
        verdict: Verdict {
            status: VerdictStatus::Fail,
            counts: VerdictCounts {
                info: 0,
                warn: 0,
                error: 1,
                suppressed: 0,
            },
            reasons: vec![],
        },
        sensors: vec![sensor_summary("scanner", VerdictStatus::Fail, true)],
        highlights: vec![make_highlight("scanner", "SEC-001", Severity::Error, 1)],
        policy: policy_snapshot_from_cfg(&cfg),
        data: None,
    };

    let md = render_comment(&report, &cfg);
    insta::assert_snapshot!("render_one_finding", md);
}

// ---------------------------------------------------------------------------
// Budget = 5 findings
// ---------------------------------------------------------------------------

#[test]
fn snapshot_render_five_findings() {
    let mut cfg = base_cfg();
    cfg.policy.max_highlights = 5;
    cfg.policy.max_annotations = 5;

    let highlights: Vec<Highlight> = (1..=5)
        .map(|i| {
            let sev = if i <= 2 {
                Severity::Error
            } else if i <= 4 {
                Severity::Warn
            } else {
                Severity::Info
            };
            make_highlight("scanner", &format!("SEC-{:03}", i), sev, i)
        })
        .collect();

    let report = CockpitReport {
        schema: "cockpit.report.v1".to_string(),
        tool: tool_info(),
        run: run_info(),
        verdict: Verdict {
            status: VerdictStatus::Fail,
            counts: VerdictCounts {
                info: 1,
                warn: 2,
                error: 2,
                suppressed: 0,
            },
            reasons: vec![],
        },
        sensors: vec![sensor_summary("scanner", VerdictStatus::Fail, true)],
        highlights,
        policy: policy_snapshot_from_cfg(&cfg),
        data: None,
    };

    let md = render_comment(&report, &cfg);
    insta::assert_snapshot!("render_five_findings", md);
}

// ---------------------------------------------------------------------------
// Budget = 50 findings (stress test)
// ---------------------------------------------------------------------------

#[test]
fn snapshot_render_fifty_findings() {
    let mut cfg = base_cfg();
    cfg.policy.max_highlights = 50;
    cfg.policy.max_annotations = 50;

    let highlights: Vec<Highlight> = (1..=50)
        .map(|i| {
            let sev = match i % 3 {
                0 => Severity::Error,
                1 => Severity::Warn,
                _ => Severity::Info,
            };
            make_highlight("scanner", &format!("SEC-{:03}", i), sev, i)
        })
        .collect();

    let report = CockpitReport {
        schema: "cockpit.report.v1".to_string(),
        tool: tool_info(),
        run: run_info(),
        verdict: Verdict {
            status: VerdictStatus::Fail,
            counts: VerdictCounts {
                info: 16,
                warn: 17,
                error: 17,
                suppressed: 0,
            },
            reasons: vec![],
        },
        sensors: vec![sensor_summary("scanner", VerdictStatus::Fail, true)],
        highlights,
        policy: policy_snapshot_from_cfg(&cfg),
        data: None,
    };

    let md = render_comment(&report, &cfg);
    insta::assert_snapshot!("render_fifty_findings", md);
}
