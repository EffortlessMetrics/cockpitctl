//! Wave-34 snapshot expansion for cockpitctl-render.
//!
//! Covers:
//!  - Multi-sensor comment with all verdict states (pass/warn/fail/skip)
//!  - Budget-truncated comment with 100+ findings
//!  - Annotation output with mixed severities
//!  - Trend section with improvements and regressions
//!  - Buildfix section rendering

use cockpitctl_render::{
    render_annotations, render_buildfix_apply_section, render_buildfix_section, render_comment,
    render_github_annotations, render_trend_section,
};
use cockpitctl_types::{
    BuildfixApplyStatus, BuildfixApplySummary, BuildfixSummary, CockpitConfig, CockpitReport,
    CountDeltas, Finding, FixSummary, Highlight, Location, MatchedFinding, MissingPolicy,
    PolicySensorSnapshot, PolicySnapshot, Presence, RunInfo, SafetyLevel, SensorPolicy,
    SensorSummary, Severity, ToolInfo, TrendDelta, TrendFinding, Verdict, VerdictChange,
    VerdictCounts, VerdictStatus,
};
use std::collections::BTreeMap;

// ── Helpers ─────────────────────────────────────────────────────────────

fn tool_info() -> ToolInfo {
    ToolInfo {
        name: "cockpitctl".to_string(),
        version: "0.3.0".to_string(),
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
    message: &str,
    path: Option<&str>,
    line: Option<u32>,
) -> Highlight {
    Highlight {
        sensor_id: sensor_id.to_string(),
        finding: Finding {
            severity,
            check_id: None,
            code: code.to_string(),
            message: message.to_string(),
            location: path.map(|p| Location {
                path: Some(p.to_string()),
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
        truncated: false,
        errors: vec![],
        missing_policy_applied: None,
        policy_outcome: None,
    }
}

fn default_sensor_cfg(section: &str) -> SensorPolicy {
    SensorPolicy {
        blocking: true,
        missing: MissingPolicy::Fail,
        section: Some(section.to_string()),
        require_label: None,
        repro: None,
    }
}

fn make_report(
    cfg: &CockpitConfig,
    verdict_status: VerdictStatus,
    reasons: Vec<String>,
    sensors: Vec<SensorSummary>,
    highlights: Vec<Highlight>,
) -> CockpitReport {
    CockpitReport {
        schema: "cockpit.report.v1".to_string(),
        tool: tool_info(),
        run: run_info(),
        verdict: Verdict {
            status: verdict_status,
            counts: VerdictCounts::default(),
            reasons,
        },
        sensors,
        highlights,
        policy: policy_snapshot_from_cfg(cfg),
        data: None,
    }
}

// =========================================================================
// 1. Multi-sensor comment with ALL verdict states
// =========================================================================

#[test]
fn snapshot_multi_sensor_all_verdict_states() {
    let mut cfg = CockpitConfig::default();
    cfg.policy.max_highlights = 10;
    cfg.policy.max_annotations = 10;
    for (id, section) in [
        ("builddiag", "Build"),
        ("clippy", "Lint"),
        ("cargo-test", "Tests"),
        ("coverage", "Coverage"),
    ] {
        cfg.sensors
            .insert(id.to_string(), default_sensor_cfg(section));
    }
    cfg.sensors.get_mut("coverage").unwrap().blocking = false;
    cfg.policy.section_order = vec![
        "Build".into(),
        "Lint".into(),
        "Tests".into(),
        "Coverage".into(),
    ];

    let report = make_report(
        &cfg,
        VerdictStatus::Fail,
        vec!["clippy_warn_promoted".to_string()],
        vec![
            sensor_summary(
                "builddiag",
                VerdictStatus::Pass,
                true,
                VerdictCounts::default(),
            ),
            sensor_summary(
                "clippy",
                VerdictStatus::Warn,
                true,
                VerdictCounts {
                    info: 0,
                    warn: 3,
                    error: 0,
                    suppressed: 1,
                },
            ),
            sensor_summary(
                "cargo-test",
                VerdictStatus::Fail,
                true,
                VerdictCounts {
                    info: 0,
                    warn: 0,
                    error: 2,
                    suppressed: 0,
                },
            ),
            sensor_summary(
                "coverage",
                VerdictStatus::Skip,
                false,
                VerdictCounts::default(),
            ),
        ],
        vec![
            make_highlight(
                "cargo-test",
                "TEST_FAIL",
                Severity::Error,
                "test `parse_receipt` panicked",
                Some("src/parser.rs"),
                Some(42),
            ),
            make_highlight(
                "cargo-test",
                "TEST_FAIL",
                Severity::Error,
                "test `validate_config` failed",
                Some("src/config.rs"),
                Some(88),
            ),
            make_highlight(
                "clippy",
                "clippy::unwrap_used",
                Severity::Warn,
                "used `unwrap()` on a Result value",
                Some("src/main.rs"),
                Some(15),
            ),
        ],
    );

    let md = render_comment(&report, &cfg);
    insta::assert_snapshot!("multi_sensor_all_verdict_states", md);
}

// =========================================================================
// 2. Budget-truncated comment with 100+ findings
// =========================================================================

#[test]
fn snapshot_budget_truncated_100_findings() {
    let mut cfg = CockpitConfig::default();
    cfg.policy.max_highlights = 5;
    cfg.policy.max_annotations = 8;
    cfg.sensors
        .insert("lint".to_string(), default_sensor_cfg("Lint"));
    cfg.policy.section_order = vec!["Lint".into()];

    let highlights: Vec<Highlight> = (0..120)
        .map(|i| {
            let severity = match i % 3 {
                0 => Severity::Error,
                1 => Severity::Warn,
                _ => Severity::Info,
            };
            make_highlight(
                "lint",
                &format!("LINT{:04}", i),
                severity,
                &format!("Finding number {} of many", i),
                Some(&format!("src/module_{}.rs", i / 10)),
                Some((i + 1) as u32),
            )
        })
        .collect();

    let report = make_report(
        &cfg,
        VerdictStatus::Fail,
        vec!["too_many_findings".to_string()],
        vec![sensor_summary(
            "lint",
            VerdictStatus::Fail,
            true,
            VerdictCounts {
                info: 40,
                warn: 40,
                error: 40,
                suppressed: 0,
            },
        )],
        highlights,
    );

    let md = render_comment(&report, &cfg);
    assert!(md.contains("capped by"));
    insta::assert_snapshot!("budget_truncated_100_findings", md);
}

// =========================================================================
// 3. Annotation output with mixed severities
// =========================================================================

#[test]
fn snapshot_annotations_mixed_severities() {
    let mut cfg = CockpitConfig::default();
    cfg.policy.max_annotations = 20;

    let highlights = vec![
        make_highlight(
            "builddiag",
            "E0308",
            Severity::Error,
            "mismatched types",
            Some("src/core.rs"),
            Some(10),
        ),
        make_highlight(
            "clippy",
            "clippy::todo",
            Severity::Warn,
            "TODO found in code",
            Some("src/lib.rs"),
            Some(55),
        ),
        make_highlight(
            "clippy",
            "clippy::missing_docs",
            Severity::Info,
            "missing documentation for public fn",
            Some("src/api.rs"),
            Some(3),
        ),
        make_highlight(
            "builddiag",
            "E0412",
            Severity::Error,
            "cannot find type `Foo`",
            Some("src/types.rs"),
            Some(22),
        ),
        make_highlight(
            "security",
            "SEC-001",
            Severity::Error,
            "potential SQL injection",
            Some("src/db.rs"),
            Some(99),
        ),
        make_highlight(
            "coverage",
            "COV-LOW",
            Severity::Info,
            "function coverage below threshold",
            None,
            None,
        ),
    ];

    let mut blocking = BTreeMap::new();
    blocking.insert("builddiag".to_string(), true);
    blocking.insert("clippy".to_string(), true);
    blocking.insert("security".to_string(), true);
    blocking.insert("coverage".to_string(), false);

    let result = render_annotations(&highlights, &cfg, &blocking);
    insta::assert_snapshot!("annotations_mixed_severities", result.content);
}

#[test]
fn snapshot_github_annotations_mixed() {
    let mut cfg = CockpitConfig::default();
    cfg.policy.max_annotations = 5;

    let highlights = vec![
        make_highlight(
            "builddiag",
            "E0308",
            Severity::Error,
            "mismatched types: expected bool",
            Some("src/main.rs"),
            Some(42),
        ),
        make_highlight(
            "clippy",
            "clippy::unwrap_used",
            Severity::Warn,
            "used `unwrap()` on a Result",
            Some("src/lib.rs"),
            Some(10),
        ),
        make_highlight(
            "lint",
            "INFO-001",
            Severity::Info,
            "consider using a constant",
            Some("src/config.rs"),
            Some(5),
        ),
    ];

    let blocking = BTreeMap::from([
        ("builddiag".to_string(), true),
        ("clippy".to_string(), true),
        ("lint".to_string(), false),
    ]);

    let result = render_github_annotations(&highlights, &cfg, &blocking);
    let combined = result.lines.join("\n");
    insta::assert_snapshot!("github_annotations_mixed", combined);
}

// =========================================================================
// 4. Trend section: improvements and regressions
// =========================================================================

#[test]
fn snapshot_trend_improvements_and_regressions() {
    let trend = TrendDelta {
        verdict_change: Some(VerdictChange {
            before: VerdictStatus::Fail,
            after: VerdictStatus::Warn,
        }),
        count_deltas: CountDeltas {
            info_delta: 2,
            warn_delta: -1,
            error_delta: -3,
        },
        new_findings: vec![
            TrendFinding {
                sensor_id: "clippy".to_string(),
                code: "clippy::needless_return".to_string(),
                message: "unneeded return statement".to_string(),
                path: Some("src/utils.rs".to_string()),
                line: Some(44),
                fingerprint: None,
                severity: Severity::Warn,
            },
            TrendFinding {
                sensor_id: "coverage".to_string(),
                code: "COV-BELOW".to_string(),
                message: "coverage dropped below 80%".to_string(),
                path: None,
                line: None,
                fingerprint: None,
                severity: Severity::Info,
            },
        ],
        fixed_findings: vec![
            TrendFinding {
                sensor_id: "builddiag".to_string(),
                code: "E0308".to_string(),
                message: "mismatched types resolved".to_string(),
                path: Some("src/main.rs".to_string()),
                line: Some(10),
                fingerprint: None,
                severity: Severity::Error,
            },
            TrendFinding {
                sensor_id: "clippy".to_string(),
                code: "clippy::unwrap_used".to_string(),
                message: "unwrap replaced with expect".to_string(),
                path: Some("src/lib.rs".to_string()),
                line: Some(22),
                fingerprint: None,
                severity: Severity::Warn,
            },
            TrendFinding {
                sensor_id: "builddiag".to_string(),
                code: "E0412".to_string(),
                message: "missing type import added".to_string(),
                path: Some("src/types.rs".to_string()),
                line: Some(5),
                fingerprint: None,
                severity: Severity::Error,
            },
        ],
        sensors_added: vec!["coverage".to_string()],
        sensors_removed: vec!["deprecated-lint".to_string()],
    };

    let section = render_trend_section(&trend);
    insta::assert_snapshot!("trend_improvements_and_regressions", section);
}

#[test]
fn snapshot_trend_no_changes() {
    let trend = TrendDelta {
        verdict_change: None,
        count_deltas: CountDeltas::default(),
        new_findings: vec![],
        fixed_findings: vec![],
        sensors_added: vec![],
        sensors_removed: vec![],
    };

    let section = render_trend_section(&trend);
    insta::assert_snapshot!("trend_no_changes", section);
}

#[test]
fn snapshot_trend_verdict_regression() {
    let trend = TrendDelta {
        verdict_change: Some(VerdictChange {
            before: VerdictStatus::Pass,
            after: VerdictStatus::Fail,
        }),
        count_deltas: CountDeltas {
            info_delta: 0,
            warn_delta: 0,
            error_delta: 5,
        },
        new_findings: vec![TrendFinding {
            sensor_id: "builddiag".to_string(),
            code: "E0308".to_string(),
            message: "new build errors introduced".to_string(),
            path: Some("src/new_module.rs".to_string()),
            line: Some(1),
            fingerprint: Some("fp_new_build".to_string()),
            severity: Severity::Error,
        }],
        fixed_findings: vec![],
        sensors_added: vec![],
        sensors_removed: vec![],
    };

    let section = render_trend_section(&trend);
    insta::assert_snapshot!("trend_verdict_regression", section);
}

// =========================================================================
// 5. Buildfix section rendering
// =========================================================================

#[test]
fn snapshot_buildfix_multiple_fixes() {
    let summary = BuildfixSummary {
        fixes: vec![
            FixSummary {
                fix_id: "fix-missing-import".to_string(),
                sensor_id: "builddiag".to_string(),
                safety: SafetyLevel::Safe,
                description: "Add missing import for std::io::Read".to_string(),
                matched_findings: vec![MatchedFinding {
                    sensor_id: "builddiag".to_string(),
                    code: "E0412".to_string(),
                    fingerprint: Some("fp_import".to_string()),
                }],
                unmatched: false,
            },
            FixSummary {
                fix_id: "fix-type-mismatch".to_string(),
                sensor_id: "builddiag".to_string(),
                safety: SafetyLevel::Guarded,
                description: "Cast i32 to bool at call site".to_string(),
                matched_findings: vec![],
                unmatched: true,
            },
            FixSummary {
                fix_id: "fix-unsafe-block".to_string(),
                sensor_id: "clippy".to_string(),
                safety: SafetyLevel::Unsafe,
                description: "Wrap raw pointer dereference in unsafe block".to_string(),
                matched_findings: vec![MatchedFinding {
                    sensor_id: "clippy".to_string(),
                    code: "clippy::not_unsafe_ptr_arg_deref".to_string(),
                    fingerprint: None,
                }],
                unmatched: false,
            },
        ],
        total_fixes: 3,
        matched_count: 2,
        unmatched_count: 1,
    };

    let section = render_buildfix_section(&summary);
    insta::assert_snapshot!("buildfix_multiple_fixes", section);
}

#[test]
fn snapshot_buildfix_empty() {
    let summary = BuildfixSummary::default();
    let section = render_buildfix_section(&summary);
    insta::assert_snapshot!("buildfix_empty", section);
}

#[test]
fn snapshot_buildfix_apply_applied() {
    let summary = BuildfixApplySummary {
        status: BuildfixApplyStatus::Applied,
        auto_apply_enabled: true,
        max_auto_apply_safety: SafetyLevel::Guarded,
        require_matched_finding: true,
        candidate_fix_ids: vec![
            "fix-import".to_string(),
            "fix-cast".to_string(),
            "fix-unsafe".to_string(),
        ],
        selected_fix_ids: vec!["fix-import".to_string(), "fix-cast".to_string()],
        applied_fix_ids: vec!["fix-import".to_string(), "fix-cast".to_string()],
        skipped_fix_ids: vec!["fix-unsafe".to_string()],
        errors: vec![],
        reason: None,
        actuator_command: None,
    };

    let section = render_buildfix_apply_section(&summary);
    insta::assert_snapshot!("buildfix_apply_applied", section);
}

#[test]
fn snapshot_buildfix_apply_failed_with_errors() {
    let summary = BuildfixApplySummary {
        status: BuildfixApplyStatus::Failed,
        auto_apply_enabled: true,
        max_auto_apply_safety: SafetyLevel::Safe,
        require_matched_finding: false,
        candidate_fix_ids: vec!["fix-import".to_string()],
        selected_fix_ids: vec!["fix-import".to_string()],
        applied_fix_ids: vec![],
        skipped_fix_ids: vec![],
        errors: vec![
            "actuator exited with code 1".to_string(),
            "file src/main.rs not found".to_string(),
        ],
        reason: Some("actuator_failed".to_string()),
        actuator_command: None,
    };

    let section = render_buildfix_apply_section(&summary);
    insta::assert_snapshot!("buildfix_apply_failed_with_errors", section);
}

#[test]
fn snapshot_buildfix_apply_skipped() {
    let summary = BuildfixApplySummary {
        status: BuildfixApplyStatus::Skipped,
        auto_apply_enabled: false,
        max_auto_apply_safety: SafetyLevel::Safe,
        require_matched_finding: true,
        candidate_fix_ids: vec![],
        selected_fix_ids: vec![],
        applied_fix_ids: vec![],
        skipped_fix_ids: vec![],
        errors: vec![],
        reason: Some("auto_apply_disabled".to_string()),
        actuator_command: None,
    };

    let section = render_buildfix_apply_section(&summary);
    insta::assert_snapshot!("buildfix_apply_skipped", section);
}
