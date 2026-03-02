//! Golden tests for rendered markdown comment stability.
//!
//! These tests verify that `render_comment` produces byte-stable markdown
//! across a variety of report shapes: no findings, single finding, many
//! findings, truncated findings, and mixed verdicts.

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
    counts: VerdictCounts,
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
            counts,
            reasons: vec![],
        },
        truncated,
        errors: vec![],
        missing_policy_applied: None,
        policy_outcome: None,
    }
}

// ===========================================================================
// GOLDEN COMMENT TESTS
// ===========================================================================

/// Zero sensors, zero findings → clean pass comment.
#[test]
fn golden_comment_no_findings() {
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
    insta::assert_snapshot!("golden_comment_no_findings", md);
}

/// Single sensor, single finding.
#[test]
fn golden_comment_single_finding() {
    let mut cfg = CockpitConfig::default();
    cfg.policy.max_highlights = 10;
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
            status: VerdictStatus::Fail,
            counts: VerdictCounts {
                info: 0,
                warn: 0,
                error: 1,
                suppressed: 0,
            },
            reasons: vec!["builddiag failed".to_string()],
        },
        sensors: vec![sensor_summary(
            "builddiag",
            VerdictStatus::Fail,
            true,
            VerdictCounts {
                info: 0,
                warn: 0,
                error: 1,
                suppressed: 0,
            },
            false,
        )],
        highlights: vec![make_highlight(
            "builddiag",
            "E0001",
            Severity::Error,
            Some("src/main.rs"),
            Some(42),
        )],
        policy: policy_snapshot_from_cfg(&cfg),
        data: None,
    };

    let md = render_comment(&report, &cfg);
    insta::assert_snapshot!("golden_comment_single_finding", md);
}

/// Multiple sensors, multiple findings across different severities.
#[test]
fn golden_comment_many_findings() {
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
    cfg.sensors.insert(
        "tests".to_string(),
        SensorPolicy {
            blocking: true,
            missing: MissingPolicy::Fail,
            section: Some("Tests".to_string()),
            require_label: None,
            repro: Some("cargo test".to_string()),
        },
    );
    cfg.sensors.insert(
        "covguard".to_string(),
        SensorPolicy {
            blocking: false,
            missing: MissingPolicy::Warn,
            section: Some("Coverage".to_string()),
            require_label: None,
            repro: None,
        },
    );
    cfg.policy.section_order = vec![
        "Lint".to_string(),
        "Tests".to_string(),
        "Coverage".to_string(),
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
                error: 1,
                suppressed: 0,
            },
            reasons: vec!["tests failed".to_string()],
        },
        sensors: vec![
            sensor_summary(
                "clippy",
                VerdictStatus::Warn,
                true,
                VerdictCounts {
                    info: 0,
                    warn: 2,
                    error: 0,
                    suppressed: 0,
                },
                false,
            ),
            sensor_summary(
                "tests",
                VerdictStatus::Fail,
                true,
                VerdictCounts {
                    info: 0,
                    warn: 0,
                    error: 1,
                    suppressed: 0,
                },
                false,
            ),
            sensor_summary(
                "covguard",
                VerdictStatus::Pass,
                false,
                VerdictCounts {
                    info: 1,
                    warn: 0,
                    error: 0,
                    suppressed: 0,
                },
                false,
            ),
        ],
        highlights: vec![
            make_highlight(
                "tests",
                "TEST-FAIL",
                Severity::Error,
                Some("tests/api.rs"),
                Some(100),
            ),
            make_highlight(
                "clippy",
                "W-UNUSED",
                Severity::Warn,
                Some("src/lib.rs"),
                Some(5),
            ),
            make_highlight(
                "clippy",
                "W-COMPLEXITY",
                Severity::Warn,
                Some("src/main.rs"),
                Some(42),
            ),
            make_highlight(
                "covguard",
                "COV-INFO",
                Severity::Info,
                Some("src/utils.rs"),
                Some(1),
            ),
        ],
        policy: policy_snapshot_from_cfg(&cfg),
        data: None,
    };

    let md = render_comment(&report, &cfg);
    insta::assert_snapshot!("golden_comment_many_findings", md);
}

/// Truncated findings: sensor has more findings than max_highlights budget.
#[test]
fn golden_comment_truncated_findings() {
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
            repro: Some("trivy scan".to_string()),
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
                info: 1,
                warn: 2,
                error: 3,
                suppressed: 0,
            },
            reasons: vec!["scanner critical findings".to_string()],
        },
        sensors: vec![sensor_summary(
            "scanner",
            VerdictStatus::Fail,
            true,
            VerdictCounts {
                info: 1,
                warn: 2,
                error: 3,
                suppressed: 0,
            },
            true, // truncated
        )],
        highlights: vec![
            make_highlight(
                "scanner",
                "CVE-2024-0001",
                Severity::Error,
                Some("Cargo.lock"),
                Some(100),
            ),
            make_highlight(
                "scanner",
                "CVE-2024-0002",
                Severity::Error,
                Some("Cargo.lock"),
                Some(200),
            ),
        ],
        policy: policy_snapshot_from_cfg(&cfg),
        data: None,
    };

    let md = render_comment(&report, &cfg);
    insta::assert_snapshot!("golden_comment_truncated_findings", md);
}

/// Mixed pass/warn/fail across blocking and non-blocking sensors.
#[test]
fn golden_comment_mixed_pass_warn_fail() {
    let mut cfg = CockpitConfig::default();
    cfg.policy.max_highlights = 10;
    cfg.sensors.insert(
        "build".to_string(),
        SensorPolicy {
            blocking: true,
            missing: MissingPolicy::Fail,
            section: Some("Build".to_string()),
            require_label: None,
            repro: Some("cargo build".to_string()),
        },
    );
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
    cfg.sensors.insert(
        "coverage".to_string(),
        SensorPolicy {
            blocking: false,
            missing: MissingPolicy::Skip,
            section: Some("Coverage".to_string()),
            require_label: None,
            repro: None,
        },
    );
    cfg.policy.section_order = vec![
        "Build".to_string(),
        "Lint".to_string(),
        "Coverage".to_string(),
    ];

    let report = CockpitReport {
        schema: "cockpit.report.v1".to_string(),
        tool: tool_info(),
        run: run_info(),
        verdict: Verdict {
            status: VerdictStatus::Warn,
            counts: VerdictCounts {
                info: 0,
                warn: 1,
                error: 0,
                suppressed: 0,
            },
            reasons: vec![],
        },
        sensors: vec![
            sensor_summary(
                "build",
                VerdictStatus::Pass,
                true,
                VerdictCounts::default(),
                false,
            ),
            sensor_summary(
                "lint",
                VerdictStatus::Warn,
                true,
                VerdictCounts {
                    info: 0,
                    warn: 1,
                    error: 0,
                    suppressed: 0,
                },
                false,
            ),
            sensor_summary(
                "coverage",
                VerdictStatus::Pass,
                false,
                VerdictCounts::default(),
                false,
            ),
        ],
        highlights: vec![make_highlight(
            "lint",
            "W-UNUSED",
            Severity::Warn,
            Some("src/lib.rs"),
            Some(10),
        )],
        policy: policy_snapshot_from_cfg(&cfg),
        data: None,
    };

    let md = render_comment(&report, &cfg);
    insta::assert_snapshot!("golden_comment_mixed_pass_warn_fail", md);
}

/// All four verdict states (pass/warn/fail/skip) represented across sensors.
#[test]
fn golden_comment_all_four_verdicts() {
    let mut cfg = CockpitConfig::default();
    cfg.policy.max_highlights = 10;
    cfg.sensors.insert(
        "alpha".to_string(),
        SensorPolicy {
            blocking: true,
            missing: MissingPolicy::Fail,
            section: Some("Build".to_string()),
            require_label: None,
            repro: None,
        },
    );
    cfg.sensors.insert(
        "beta".to_string(),
        SensorPolicy {
            blocking: true,
            missing: MissingPolicy::Fail,
            section: Some("Lint".to_string()),
            require_label: None,
            repro: None,
        },
    );
    cfg.sensors.insert(
        "gamma".to_string(),
        SensorPolicy {
            blocking: true,
            missing: MissingPolicy::Fail,
            section: Some("Tests".to_string()),
            require_label: None,
            repro: None,
        },
    );
    cfg.sensors.insert(
        "delta".to_string(),
        SensorPolicy {
            blocking: false,
            missing: MissingPolicy::Skip,
            section: Some("Optional".to_string()),
            require_label: None,
            repro: None,
        },
    );
    cfg.policy.section_order = vec![
        "Build".to_string(),
        "Lint".to_string(),
        "Tests".to_string(),
        "Optional".to_string(),
    ];

    let report = CockpitReport {
        schema: "cockpit.report.v1".to_string(),
        tool: tool_info(),
        run: run_info(),
        verdict: Verdict {
            status: VerdictStatus::Fail,
            counts: VerdictCounts {
                info: 0,
                warn: 1,
                error: 1,
                suppressed: 0,
            },
            reasons: vec!["gamma failed".to_string()],
        },
        sensors: vec![
            sensor_summary(
                "alpha",
                VerdictStatus::Pass,
                true,
                VerdictCounts::default(),
                false,
            ),
            sensor_summary(
                "beta",
                VerdictStatus::Warn,
                true,
                VerdictCounts {
                    info: 0,
                    warn: 1,
                    error: 0,
                    suppressed: 0,
                },
                false,
            ),
            sensor_summary(
                "gamma",
                VerdictStatus::Fail,
                true,
                VerdictCounts {
                    info: 0,
                    warn: 0,
                    error: 1,
                    suppressed: 0,
                },
                false,
            ),
            sensor_summary(
                "delta",
                VerdictStatus::Skip,
                false,
                VerdictCounts::default(),
                false,
            ),
        ],
        highlights: vec![
            make_highlight(
                "gamma",
                "T-FAIL",
                Severity::Error,
                Some("tests/main.rs"),
                Some(50),
            ),
            make_highlight(
                "beta",
                "L-WARN",
                Severity::Warn,
                Some("src/lib.rs"),
                Some(20),
            ),
        ],
        policy: policy_snapshot_from_cfg(&cfg),
        data: None,
    };

    let md = render_comment(&report, &cfg);
    insta::assert_snapshot!("golden_comment_all_four_verdicts", md);
}
