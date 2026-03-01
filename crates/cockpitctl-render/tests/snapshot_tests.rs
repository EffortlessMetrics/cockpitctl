use cockpitctl_render::render_comment;
use cockpitctl_types::{
    CockpitConfig, CockpitReport, Finding, Highlight, Location, MissingPolicy,
    PolicySensorSnapshot, PolicySnapshot, Presence, RunInfo, SensorPolicy, SensorSummary, Severity,
    ToolInfo, Verdict, VerdictCounts, VerdictStatus,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

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

fn make_highlight(
    sensor_id: &str,
    code: &str,
    severity: Severity,
    path: Option<&str>,
    line: Option<u32>,
) -> Highlight {
    Highlight {
        sensor_id: sensor_id.to_string(),
        finding: Finding {
            severity,
            check_id: None,
            code: code.to_string(),
            message: format!("Message for {}", code),
            location: Some(Location {
                path: path.map(String::from),
                line,
                col: None,
            }),
            help: None,
            url: None,
            fingerprint: None,
            data: None,
        },
    }
}

fn sensor_summary(
    id: &str,
    status: VerdictStatus,
    blocking: bool,
    truncated: bool,
) -> SensorSummary {
    SensorSummary {
        id: id.to_string(),
        blocking,
        missing: MissingPolicy::Fail,
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

// ---------------------------------------------------------------------------
// Snapshot tests
// ---------------------------------------------------------------------------

#[test]
fn snapshot_pass_report() {
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

    let report = CockpitReport {
        schema: "cockpit.report.v1".to_string(),
        tool: tool_info(),
        run: run_info(),
        verdict: Verdict {
            status: VerdictStatus::Pass,
            counts: VerdictCounts::default(),
            reasons: vec![],
        },
        sensors: vec![sensor_summary(
            "builddiag",
            VerdictStatus::Pass,
            true,
            false,
        )],
        highlights: vec![],
        policy: policy_snapshot_from_cfg(&cfg),
        data: None,
    };

    let md = render_comment(&report, &cfg);
    insta::assert_snapshot!(md);
}

#[test]
fn snapshot_fail_report_with_findings() {
    let mut cfg = CockpitConfig::default();
    cfg.policy.max_highlights = 10;
    cfg.policy.max_annotations = 10;
    cfg.sensors.insert(
        "clippy".to_string(),
        SensorPolicy {
            blocking: true,
            missing: MissingPolicy::Fail,
            section: Some("Lint".to_string()),
            require_label: None,
            repro: Some("cargo clippy".to_string()),
        },
    );
    cfg.policy.section_order = vec!["Lint".to_string()];

    let report = CockpitReport {
        schema: "cockpit.report.v1".to_string(),
        tool: tool_info(),
        run: run_info(),
        verdict: Verdict {
            status: VerdictStatus::Fail,
            counts: VerdictCounts::default(),
            reasons: vec!["clippy failed".to_string()],
        },
        sensors: vec![sensor_summary("clippy", VerdictStatus::Fail, true, false)],
        highlights: vec![
            make_highlight(
                "clippy",
                "E0001",
                Severity::Error,
                Some("src/main.rs"),
                Some(10),
            ),
            make_highlight(
                "clippy",
                "W0002",
                Severity::Warn,
                Some("src/lib.rs"),
                Some(42),
            ),
            make_highlight(
                "clippy",
                "I0003",
                Severity::Info,
                Some("src/utils.rs"),
                Some(7),
            ),
        ],
        policy: policy_snapshot_from_cfg(&cfg),
        data: None,
    };

    let md = render_comment(&report, &cfg);
    insta::assert_snapshot!(md);
}

#[test]
fn snapshot_budget_truncation() {
    let mut cfg = CockpitConfig::default();
    cfg.policy.max_highlights = 2;
    cfg.policy.max_annotations = 2;
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

    let report = CockpitReport {
        schema: "cockpit.report.v1".to_string(),
        tool: tool_info(),
        run: run_info(),
        verdict: Verdict {
            status: VerdictStatus::Fail,
            counts: VerdictCounts::default(),
            reasons: vec![],
        },
        sensors: vec![sensor_summary("scanner", VerdictStatus::Fail, true, true)],
        highlights: vec![
            make_highlight(
                "scanner",
                "SEC-001",
                Severity::Error,
                Some("src/auth.rs"),
                Some(1),
            ),
            make_highlight(
                "scanner",
                "SEC-002",
                Severity::Error,
                Some("src/auth.rs"),
                Some(15),
            ),
            make_highlight(
                "scanner",
                "SEC-003",
                Severity::Warn,
                Some("src/db.rs"),
                Some(30),
            ),
            make_highlight(
                "scanner",
                "SEC-004",
                Severity::Info,
                Some("src/config.rs"),
                Some(5),
            ),
        ],
        policy: policy_snapshot_from_cfg(&cfg),
        data: None,
    };

    let md = render_comment(&report, &cfg);
    insta::assert_snapshot!(md);
}

#[test]
fn snapshot_mixed_verdicts() {
    let mut cfg = CockpitConfig::default();
    cfg.policy.max_highlights = 10;
    cfg.policy.max_annotations = 10;
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
        "covguard".to_string(),
        SensorPolicy {
            blocking: false,
            missing: MissingPolicy::Skip,
            section: Some("Coverage".to_string()),
            require_label: None,
            repro: None,
        },
    );
    cfg.sensors.insert(
        "diffguard".to_string(),
        SensorPolicy {
            blocking: true,
            missing: MissingPolicy::Fail,
            section: Some("Policy".to_string()),
            require_label: None,
            repro: None,
        },
    );
    cfg.policy.section_order = vec![
        "Build".to_string(),
        "Coverage".to_string(),
        "Policy".to_string(),
    ];

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
            sensor_summary("builddiag", VerdictStatus::Pass, true, false),
            sensor_summary("covguard", VerdictStatus::Skip, false, false),
            sensor_summary("diffguard", VerdictStatus::Warn, true, false),
        ],
        highlights: vec![make_highlight(
            "diffguard",
            "DG-001",
            Severity::Warn,
            Some("src/lib.rs"),
            Some(42),
        )],
        policy: policy_snapshot_from_cfg(&cfg),
        data: None,
    };

    let md = render_comment(&report, &cfg);
    insta::assert_snapshot!(md);
}

#[test]
fn snapshot_empty_findings() {
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
        sensors: vec![],
        highlights: vec![],
        policy: policy_snapshot_from_cfg(&cfg),
        data: None,
    };

    let md = render_comment(&report, &cfg);
    insta::assert_snapshot!(md);
}

#[test]
fn snapshot_findings_with_annotations() {
    let mut cfg = CockpitConfig::default();
    cfg.policy.max_highlights = 5;
    cfg.policy.max_annotations = 5;
    cfg.sensors.insert(
        "alpha".to_string(),
        SensorPolicy {
            blocking: true,
            missing: MissingPolicy::Fail,
            section: Some("Tests".to_string()),
            require_label: None,
            repro: Some("cargo test".to_string()),
        },
    );
    cfg.sensors.insert(
        "beta".to_string(),
        SensorPolicy {
            blocking: false,
            missing: MissingPolicy::Warn,
            section: Some("Lint".to_string()),
            require_label: None,
            repro: None,
        },
    );
    cfg.policy.section_order = vec!["Tests".to_string(), "Lint".to_string()];

    let report = CockpitReport {
        schema: "cockpit.report.v1".to_string(),
        tool: tool_info(),
        run: run_info(),
        verdict: Verdict {
            status: VerdictStatus::Fail,
            counts: VerdictCounts::default(),
            reasons: vec![],
        },
        sensors: vec![
            sensor_summary("alpha", VerdictStatus::Fail, true, false),
            sensor_summary("beta", VerdictStatus::Warn, false, false),
        ],
        highlights: vec![
            make_highlight("alpha", "E1", Severity::Error, Some("src/lib.rs"), Some(5)),
            make_highlight(
                "alpha",
                "E2",
                Severity::Error,
                Some("src/main.rs"),
                Some(20),
            ),
            make_highlight("beta", "W1", Severity::Warn, Some("src/utils.rs"), Some(10)),
            make_highlight("beta", "I1", Severity::Info, None, None),
        ],
        policy: policy_snapshot_from_cfg(&cfg),
        data: None,
    };

    let md = render_comment(&report, &cfg);
    insta::assert_snapshot!(md);
}

// ---------------------------------------------------------------------------
// New expanded snapshot scenarios
// ---------------------------------------------------------------------------

#[test]
fn snapshot_many_sensors_five_sections() {
    let mut cfg = CockpitConfig::default();
    cfg.policy.max_highlights = 10;
    cfg.policy.max_annotations = 10;

    let sensors_cfg = [
        ("builddiag", true, MissingPolicy::Fail, "Build"),
        ("clippy", true, MissingPolicy::Fail, "Lint"),
        ("cargo-test", true, MissingPolicy::Fail, "Tests"),
        ("trivy", true, MissingPolicy::Fail, "Security"),
        ("covguard", false, MissingPolicy::Warn, "Coverage"),
    ];
    for (id, blocking, missing, section) in &sensors_cfg {
        cfg.sensors.insert(
            id.to_string(),
            SensorPolicy {
                blocking: *blocking,
                missing: *missing,
                section: Some(section.to_string()),
                require_label: None,
                repro: None,
            },
        );
    }
    cfg.policy.section_order = vec![
        "Build".into(),
        "Lint".into(),
        "Tests".into(),
        "Security".into(),
        "Coverage".into(),
    ];

    let report = CockpitReport {
        schema: "cockpit.report.v1".to_string(),
        tool: tool_info(),
        run: run_info(),
        verdict: Verdict {
            status: VerdictStatus::Fail,
            counts: VerdictCounts {
                info: 1,
                warn: 2,
                error: 3,
                suppressed: 0,
            },
            reasons: vec!["trivy failed".to_string()],
        },
        sensors: vec![
            sensor_summary("builddiag", VerdictStatus::Pass, true, false),
            sensor_summary("clippy", VerdictStatus::Warn, true, false),
            sensor_summary("cargo-test", VerdictStatus::Pass, true, false),
            sensor_summary("trivy", VerdictStatus::Fail, true, false),
            sensor_summary("covguard", VerdictStatus::Warn, false, false),
        ],
        highlights: vec![
            make_highlight(
                "trivy",
                "CVE-2024-0001",
                Severity::Error,
                Some("Cargo.lock"),
                Some(100),
            ),
            make_highlight(
                "trivy",
                "CVE-2024-0002",
                Severity::Error,
                Some("Cargo.lock"),
                Some(200),
            ),
            make_highlight(
                "clippy",
                "W-UNUSED",
                Severity::Warn,
                Some("src/lib.rs"),
                Some(5),
            ),
            make_highlight(
                "covguard",
                "COV-LOW",
                Severity::Info,
                Some("src/utils.rs"),
                Some(1),
            ),
        ],
        policy: policy_snapshot_from_cfg(&cfg),
        data: None,
    };

    let md = render_comment(&report, &cfg);
    insta::assert_snapshot!(md);
}

#[test]
fn snapshot_warn_is_fail_policy() {
    let mut cfg = CockpitConfig::default();
    cfg.policy.warn_is_fail = true;
    cfg.policy.max_highlights = 5;
    cfg.sensors.insert(
        "lint".to_string(),
        SensorPolicy {
            blocking: true,
            missing: MissingPolicy::Fail,
            section: Some("Lint".to_string()),
            require_label: None,
            repro: Some("cargo clippy".to_string()),
        },
    );
    cfg.policy.section_order = vec!["Lint".to_string()];

    let report = CockpitReport {
        schema: "cockpit.report.v1".to_string(),
        tool: tool_info(),
        run: run_info(),
        verdict: Verdict {
            status: VerdictStatus::Fail,
            counts: VerdictCounts {
                info: 0,
                warn: 2,
                error: 0,
                suppressed: 0,
            },
            reasons: vec!["warn_is_fail".to_string()],
        },
        sensors: vec![sensor_summary("lint", VerdictStatus::Warn, true, false)],
        highlights: vec![
            make_highlight("lint", "W001", Severity::Warn, Some("src/a.rs"), Some(10)),
            make_highlight("lint", "W002", Severity::Warn, Some("src/b.rs"), Some(20)),
        ],
        policy: policy_snapshot_from_cfg(&cfg),
        data: None,
    };

    let md = render_comment(&report, &cfg);
    insta::assert_snapshot!(md);
}

#[test]
fn snapshot_single_sensor_no_findings() {
    let mut cfg = CockpitConfig::default();
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
    cfg.policy.section_order = vec!["Build".to_string()];

    let report = CockpitReport {
        schema: "cockpit.report.v1".to_string(),
        tool: tool_info(),
        run: run_info(),
        verdict: Verdict {
            status: VerdictStatus::Pass,
            counts: VerdictCounts::default(),
            reasons: vec![],
        },
        sensors: vec![sensor_summary(
            "builddiag",
            VerdictStatus::Pass,
            true,
            false,
        )],
        highlights: vec![],
        policy: policy_snapshot_from_cfg(&cfg),
        data: None,
    };

    let md = render_comment(&report, &cfg);
    insta::assert_snapshot!(md);
}

#[test]
fn snapshot_all_sensors_skip() {
    let mut cfg = CockpitConfig::default();
    cfg.sensors.insert(
        "alpha".to_string(),
        SensorPolicy {
            blocking: false,
            missing: MissingPolicy::Skip,
            section: Some("Optional".to_string()),
            require_label: None,
            repro: None,
        },
    );
    cfg.sensors.insert(
        "beta".to_string(),
        SensorPolicy {
            blocking: false,
            missing: MissingPolicy::Skip,
            section: Some("Optional".to_string()),
            require_label: None,
            repro: None,
        },
    );
    cfg.policy.section_order = vec!["Optional".to_string()];

    let report = CockpitReport {
        schema: "cockpit.report.v1".to_string(),
        tool: tool_info(),
        run: run_info(),
        verdict: Verdict {
            status: VerdictStatus::Pass,
            counts: VerdictCounts::default(),
            reasons: vec![],
        },
        sensors: vec![
            sensor_summary("alpha", VerdictStatus::Skip, false, false),
            sensor_summary("beta", VerdictStatus::Skip, false, false),
        ],
        highlights: vec![],
        policy: policy_snapshot_from_cfg(&cfg),
        data: None,
    };

    let md = render_comment(&report, &cfg);
    insta::assert_snapshot!(md);
}

#[test]
fn snapshot_max_highlights_one() {
    let mut cfg = CockpitConfig::default();
    cfg.policy.max_highlights = 1;
    cfg.policy.max_annotations = 1;
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

    let report = CockpitReport {
        schema: "cockpit.report.v1".to_string(),
        tool: tool_info(),
        run: run_info(),
        verdict: Verdict {
            status: VerdictStatus::Fail,
            counts: VerdictCounts {
                info: 0,
                warn: 1,
                error: 2,
                suppressed: 0,
            },
            reasons: vec![],
        },
        sensors: vec![sensor_summary("scanner", VerdictStatus::Fail, true, true)],
        highlights: vec![make_highlight(
            "scanner",
            "SEC-CRIT",
            Severity::Error,
            Some("src/auth.rs"),
            Some(42),
        )],
        policy: policy_snapshot_from_cfg(&cfg),
        data: None,
    };

    let md = render_comment(&report, &cfg);
    insta::assert_snapshot!(md);
}
