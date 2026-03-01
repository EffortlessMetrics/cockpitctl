//! Expanded golden/snapshot tests for cockpitctl-render.
//!
//! Covers 15 scenarios:
//!  1. All-pass report
//!  2. Single failure among passing sensors
//!  3. Mixed verdicts (pass/warn/fail/skip)
//!  4. Sensors present but no findings
//!  5. Budget exactly met (annotations)
//!  6. Budget exceeded by 1
//!  7. Massive overflow (1000 findings, budget 10)
//!  8. Annotations output via render_annotations
//!  9. Multiple sensors same verdict (5 fail)
//! 10. Sensor with warnings only (non-blocking)
//! 11. Empty sensor_id handling
//! 12. Special markdown characters in messages
//! 13. Long file paths (>100 chars)
//! 14. Unicode in findings (CJK, emoji)
//! 15. Stable markers present

use cockpitctl_render::{render_annotations, render_comment, render_github_annotations};
use cockpitctl_types::{
    CockpitConfig, CockpitReport, Finding, Highlight, Location, MissingPolicy,
    PolicySensorSnapshot, PolicySnapshot, Presence, RunInfo, SensorPolicy, SensorSummary, Severity,
    ToolInfo, Verdict, VerdictCounts, VerdictStatus,
};
use std::collections::BTreeMap;

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

fn make_highlight_with_message(
    sensor_id: &str,
    code: &str,
    severity: Severity,
    path: Option<&str>,
    line: Option<u32>,
    message: &str,
) -> Highlight {
    Highlight {
        sensor_id: sensor_id.to_string(),
        finding: Finding {
            severity,
            check_id: None,
            code: code.to_string(),
            message: message.to_string(),
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

fn default_sensor_cfg(section: &str) -> SensorPolicy {
    SensorPolicy {
        blocking: true,
        missing: MissingPolicy::Fail,
        section: Some(section.to_string()),
        require_label: None,
        repro: None,
    }
}

// ===========================================================================
// 1. All-pass report: every sensor passes → clean summary
// ===========================================================================

#[test]
fn golden_all_pass_report() {
    let mut cfg = CockpitConfig::default();
    for (id, section) in [
        ("builddiag", "Build"),
        ("clippy", "Lint"),
        ("cargo-test", "Tests"),
    ] {
        cfg.sensors
            .insert(id.to_string(), default_sensor_cfg(section));
    }
    cfg.policy.section_order = vec!["Build".into(), "Lint".into(), "Tests".into()];

    let report = make_report(
        &cfg,
        VerdictStatus::Pass,
        vec![],
        vec![
            sensor_summary("builddiag", VerdictStatus::Pass, true, false),
            sensor_summary("cargo-test", VerdictStatus::Pass, true, false),
            sensor_summary("clippy", VerdictStatus::Pass, true, false),
        ],
        vec![],
    );

    let md = render_comment(&report, &cfg);
    assert!(md.contains("✅ pass"), "all sensors should show pass");
    assert!(!md.contains("❌"), "no failures expected");
    assert!(!md.contains("⚠️"), "no warnings expected");
    insta::assert_snapshot!(md);
}

// ===========================================================================
// 2. Single failure: one sensor fails → failure highlighted
// ===========================================================================

#[test]
fn golden_single_failure() {
    let mut cfg = CockpitConfig::default();
    cfg.policy.max_highlights = 10;
    cfg.policy.max_annotations = 10;
    cfg.sensors
        .insert("builddiag".to_string(), default_sensor_cfg("Build"));
    cfg.sensors
        .insert("clippy".to_string(), default_sensor_cfg("Lint"));
    cfg.sensors
        .insert("cargo-test".to_string(), default_sensor_cfg("Tests"));
    cfg.policy.section_order = vec!["Build".into(), "Lint".into(), "Tests".into()];

    let report = make_report(
        &cfg,
        VerdictStatus::Fail,
        vec!["cargo-test failed".to_string()],
        vec![
            sensor_summary("builddiag", VerdictStatus::Pass, true, false),
            sensor_summary("cargo-test", VerdictStatus::Fail, true, false),
            sensor_summary("clippy", VerdictStatus::Pass, true, false),
        ],
        vec![make_highlight(
            "cargo-test",
            "TEST-FAIL-001",
            Severity::Error,
            Some("tests/integration.rs"),
            Some(55),
        )],
    );

    let md = render_comment(&report, &cfg);
    assert!(md.contains("❌ fail"), "failure must appear");
    assert!(
        md.contains("TEST-FAIL-001"),
        "finding code must appear in highlights"
    );
    insta::assert_snapshot!(md);
}

// ===========================================================================
// 3. Mixed verdicts: pass/warn/fail/skip all present
// ===========================================================================

#[test]
fn golden_mixed_verdicts_all_four() {
    let mut cfg = CockpitConfig::default();
    cfg.policy.max_highlights = 10;
    cfg.policy.max_annotations = 10;
    cfg.sensors.insert(
        "builddiag".to_string(),
        SensorPolicy {
            blocking: true,
            section: Some("Build".to_string()),
            ..default_sensor_cfg("Build")
        },
    );
    cfg.sensors.insert(
        "clippy".to_string(),
        SensorPolicy {
            blocking: true,
            section: Some("Lint".to_string()),
            ..default_sensor_cfg("Lint")
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
        "trivy".to_string(),
        SensorPolicy {
            blocking: true,
            section: Some("Security".to_string()),
            ..default_sensor_cfg("Security")
        },
    );
    cfg.policy.section_order = vec![
        "Build".into(),
        "Lint".into(),
        "Security".into(),
        "Coverage".into(),
    ];

    let report = make_report(
        &cfg,
        VerdictStatus::Fail,
        vec!["trivy failed".to_string()],
        vec![
            sensor_summary("builddiag", VerdictStatus::Pass, true, false),
            sensor_summary("clippy", VerdictStatus::Warn, true, false),
            sensor_summary("covguard", VerdictStatus::Skip, false, false),
            sensor_summary("trivy", VerdictStatus::Fail, true, false),
        ],
        vec![
            make_highlight(
                "trivy",
                "CVE-2024-9999",
                Severity::Error,
                Some("Cargo.lock"),
                Some(42),
            ),
            make_highlight(
                "clippy",
                "W-UNUSED-VAR",
                Severity::Warn,
                Some("src/lib.rs"),
                Some(10),
            ),
        ],
    );

    let md = render_comment(&report, &cfg);
    assert!(md.contains("✅ pass"), "pass verdict present");
    assert!(md.contains("⚠️ warn"), "warn verdict present");
    assert!(md.contains("❌ fail"), "fail verdict present");
    assert!(md.contains("⏭ skip"), "skip verdict present");
    assert!(md.contains("### Build"), "Build section present");
    assert!(md.contains("### Lint"), "Lint section present");
    assert!(md.contains("### Security"), "Security section present");
    assert!(md.contains("### Coverage"), "Coverage section present");
    insta::assert_snapshot!(md);
}

// ===========================================================================
// 4. Sensors present but no findings → minimal comment
// ===========================================================================

#[test]
fn golden_no_findings() {
    let mut cfg = CockpitConfig::default();
    cfg.sensors
        .insert("builddiag".to_string(), default_sensor_cfg("Build"));
    cfg.sensors.insert(
        "clippy".to_string(),
        SensorPolicy {
            repro: Some("cargo clippy".to_string()),
            ..default_sensor_cfg("Lint")
        },
    );
    cfg.policy.section_order = vec!["Build".into(), "Lint".into()];

    let report = make_report(
        &cfg,
        VerdictStatus::Pass,
        vec![],
        vec![
            sensor_summary("builddiag", VerdictStatus::Pass, true, false),
            sensor_summary("clippy", VerdictStatus::Pass, true, false),
        ],
        vec![],
    );

    let md = render_comment(&report, &cfg);
    assert!(md.contains("No highlights"), "should show no highlights");
    assert!(md.contains("No annotations"), "should show no annotations");
    insta::assert_snapshot!(md);
}

// ===========================================================================
// 5. Budget exactly met: findings count = budget → no truncation
// ===========================================================================

#[test]
fn golden_budget_exactly_met() {
    let mut cfg = CockpitConfig::default();
    cfg.policy.max_highlights = 5;
    cfg.policy.max_annotations = 5;
    cfg.sensors
        .insert("lint".to_string(), default_sensor_cfg("Lint"));
    cfg.policy.section_order = vec!["Lint".to_string()];

    let highlights: Vec<Highlight> = (0..5)
        .map(|i| {
            make_highlight(
                "lint",
                &format!("EXACT-{:03}", i),
                Severity::Error,
                Some("src/main.rs"),
                Some(i + 1),
            )
        })
        .collect();

    let report = make_report(
        &cfg,
        VerdictStatus::Fail,
        vec![],
        vec![sensor_summary("lint", VerdictStatus::Fail, true, false)],
        highlights.clone(),
    );

    let md = render_comment(&report, &cfg);
    assert!(
        !md.contains("capped by"),
        "no truncation when count == budget"
    );
    // Also verify via render_annotations directly
    let blocking: BTreeMap<String, bool> = [("lint".to_string(), true)].into_iter().collect();
    let result = render_annotations(&highlights, &cfg, &blocking);
    assert!(!result.truncated);
    assert_eq!(result.rendered_count, 5);
    insta::assert_snapshot!(md);
}

// ===========================================================================
// 6. Budget exceeded by 1: truncation notice with "1 more"
// ===========================================================================

#[test]
fn golden_budget_exceeded_by_one() {
    let mut cfg = CockpitConfig::default();
    cfg.policy.max_highlights = 5;
    cfg.policy.max_annotations = 5;
    cfg.sensors
        .insert("lint".to_string(), default_sensor_cfg("Lint"));
    cfg.policy.section_order = vec!["Lint".to_string()];

    let highlights: Vec<Highlight> = (0..6)
        .map(|i| {
            make_highlight(
                "lint",
                &format!("OVER1-{:03}", i),
                Severity::Warn,
                Some("src/lib.rs"),
                Some(i * 10 + 1),
            )
        })
        .collect();

    let report = make_report(
        &cfg,
        VerdictStatus::Warn,
        vec![],
        vec![sensor_summary("lint", VerdictStatus::Warn, true, true)],
        highlights.clone(),
    );

    let md = render_comment(&report, &cfg);
    assert!(
        md.contains("Showing 5 of 6 annotations"),
        "must show truncation notice: got:\n{}",
        md
    );

    let blocking: BTreeMap<String, bool> = [("lint".to_string(), true)].into_iter().collect();
    let result = render_annotations(&highlights, &cfg, &blocking);
    assert!(result.truncated);
    assert_eq!(result.rendered_count, 5);
    assert_eq!(result.total_count, 6);
    insta::assert_snapshot!(md);
}

// ===========================================================================
// 7. Massive overflow: 1000 findings, budget 10
// ===========================================================================

#[test]
fn golden_massive_overflow() {
    let mut cfg = CockpitConfig::default();
    cfg.policy.max_highlights = 10;
    cfg.policy.max_annotations = 10;
    cfg.sensors
        .insert("scanner".to_string(), default_sensor_cfg("Security"));
    cfg.policy.section_order = vec!["Security".to_string()];

    let highlights: Vec<Highlight> = (0..1000)
        .map(|i| {
            make_highlight(
                "scanner",
                &format!("MASS-{:04}", i),
                if i < 100 {
                    Severity::Error
                } else if i < 500 {
                    Severity::Warn
                } else {
                    Severity::Info
                },
                Some(&format!("src/module_{}.rs", i % 50)),
                Some(i + 1),
            )
        })
        .collect();

    let blocking: BTreeMap<String, bool> = [("scanner".to_string(), true)].into_iter().collect();

    // Test render_annotations directly for precise assertions
    let result = render_annotations(&highlights, &cfg, &blocking);
    assert!(result.truncated);
    assert_eq!(result.total_count, 1000);
    assert_eq!(result.rendered_count, 10);
    assert!(
        result.content.contains("Showing 10 of 1000 annotations"),
        "truncation notice must show 990 omitted"
    );
    insta::assert_snapshot!(result.content);
}

// ===========================================================================
// 8. Annotations output: render_annotations produces valid format
// ===========================================================================

#[test]
fn golden_annotations_output() {
    let mut cfg = CockpitConfig::default();
    cfg.policy.max_annotations = 10;
    cfg.sensors
        .insert("lint".to_string(), default_sensor_cfg("Lint"));

    let highlights = vec![
        make_highlight(
            "lint",
            "E001",
            Severity::Error,
            Some("src/auth.rs"),
            Some(10),
        ),
        make_highlight("lint", "W001", Severity::Warn, Some("src/db.rs"), Some(20)),
        make_highlight(
            "lint",
            "I001",
            Severity::Info,
            Some("src/utils.rs"),
            Some(30),
        ),
    ];

    let blocking: BTreeMap<String, bool> = [("lint".to_string(), true)].into_iter().collect();

    // Test markdown annotations
    let result = render_annotations(&highlights, &cfg, &blocking);
    assert!(!result.truncated);
    assert_eq!(result.total_count, 3);
    assert_eq!(result.rendered_count, 3);
    insta::assert_snapshot!("annotations_markdown", &result.content);

    // Test GitHub annotations
    let gh_result = render_github_annotations(&highlights, &cfg, &blocking);
    assert!(!gh_result.truncated);
    assert_eq!(gh_result.lines.len(), 3);
    for line in &gh_result.lines {
        assert!(
            line.starts_with("::error")
                || line.starts_with("::warning")
                || line.starts_with("::notice"),
            "each line must be a valid GH annotation: {}",
            line
        );
    }
    let gh_output = gh_result.lines.join("\n");
    insta::assert_snapshot!("annotations_github", &gh_output);
}

// ===========================================================================
// 9. Multiple sensors same verdict: 5 sensors all fail
// ===========================================================================

#[test]
fn golden_five_sensors_all_fail() {
    let mut cfg = CockpitConfig::default();
    cfg.policy.max_highlights = 10;
    cfg.policy.max_annotations = 10;

    let sensor_names = ["alpha", "bravo", "charlie", "delta", "echo"];
    for name in &sensor_names {
        cfg.sensors
            .insert(name.to_string(), default_sensor_cfg("Checks"));
    }
    cfg.policy.section_order = vec!["Checks".to_string()];

    let sensors: Vec<SensorSummary> = sensor_names
        .iter()
        .map(|name| sensor_summary(name, VerdictStatus::Fail, true, false))
        .collect();

    let highlights: Vec<Highlight> = sensor_names
        .iter()
        .map(|name| {
            make_highlight(
                name,
                &format!("{}-ERR", name.to_uppercase()),
                Severity::Error,
                Some(&format!("src/{}.rs", name)),
                Some(1),
            )
        })
        .collect();

    let report = make_report(
        &cfg,
        VerdictStatus::Fail,
        vec!["all sensors failed".to_string()],
        sensors,
        highlights,
    );

    let md = render_comment(&report, &cfg);
    // All 5 sensors should appear
    for name in &sensor_names {
        assert!(
            md.contains(&format!("`{}`", name)),
            "sensor {} must appear",
            name
        );
    }
    // All should be fail
    assert_eq!(
        md.matches("❌ fail").count(),
        // 5 in summary + 5 in section listing
        10,
        "each sensor appears twice (summary + section) with fail badge"
    );
    insta::assert_snapshot!(md);
}

// ===========================================================================
// 10. Sensor with warnings only: non-blocking → warn verdict
// ===========================================================================

#[test]
fn golden_warnings_only_non_blocking() {
    let mut cfg = CockpitConfig::default();
    cfg.policy.max_highlights = 10;
    cfg.policy.max_annotations = 10;
    cfg.sensors.insert(
        "advisory".to_string(),
        SensorPolicy {
            blocking: false,
            missing: MissingPolicy::Warn,
            section: Some("Advisory".to_string()),
            require_label: None,
            repro: Some("npm audit".to_string()),
        },
    );
    cfg.policy.section_order = vec!["Advisory".to_string()];

    let report = make_report(
        &cfg,
        VerdictStatus::Warn,
        vec![],
        vec![sensor_summary(
            "advisory",
            VerdictStatus::Warn,
            false,
            false,
        )],
        vec![
            make_highlight(
                "advisory",
                "WARN-001",
                Severity::Warn,
                Some("package.json"),
                Some(5),
            ),
            make_highlight(
                "advisory",
                "WARN-002",
                Severity::Warn,
                Some("yarn.lock"),
                Some(100),
            ),
            make_highlight(
                "advisory",
                "WARN-003",
                Severity::Warn,
                Some("package.json"),
                Some(12),
            ),
        ],
    );

    let md = render_comment(&report, &cfg);
    assert!(md.contains("| no |"), "sensor must be non-blocking");
    assert!(md.contains("⚠️ warn"), "warn badge must appear");
    assert!(!md.contains("❌"), "no errors expected");
    insta::assert_snapshot!(md);
}

// ===========================================================================
// 11. Empty sensor_id handling
// ===========================================================================

#[test]
fn golden_empty_sensor_id() {
    let mut cfg = CockpitConfig::default();
    cfg.policy.max_highlights = 5;
    cfg.policy.max_annotations = 5;
    cfg.sensors.insert(
        "".to_string(),
        SensorPolicy {
            blocking: true,
            missing: MissingPolicy::Fail,
            section: Some("Other".to_string()),
            require_label: None,
            repro: None,
        },
    );
    cfg.policy.section_order = vec!["Other".to_string()];

    let report = make_report(
        &cfg,
        VerdictStatus::Fail,
        vec![],
        vec![sensor_summary("", VerdictStatus::Fail, true, false)],
        vec![make_highlight(
            "",
            "EMPTY-ID",
            Severity::Error,
            Some("src/main.rs"),
            Some(1),
        )],
    );

    let md = render_comment(&report, &cfg);
    // Should not panic and should produce valid markdown
    assert!(md.contains("cockpit:begin"));
    assert!(md.contains("cockpit:end"));
    assert!(md.contains("EMPTY-ID"));
    insta::assert_snapshot!(md);
}

// ===========================================================================
// 12. Special markdown characters in messages
// ===========================================================================

#[test]
fn golden_special_markdown_characters() {
    let mut cfg = CockpitConfig::default();
    cfg.policy.max_highlights = 10;
    cfg.policy.max_annotations = 10;
    cfg.sensors
        .insert("lint".to_string(), default_sensor_cfg("Lint"));
    cfg.policy.section_order = vec!["Lint".to_string()];

    let report = make_report(
        &cfg,
        VerdictStatus::Fail,
        vec![],
        vec![sensor_summary("lint", VerdictStatus::Fail, true, false)],
        vec![
            make_highlight_with_message(
                "lint",
                "MD-STAR",
                Severity::Error,
                Some("src/lib.rs"),
                Some(1),
                "Use *bold* and **double bold** carefully",
            ),
            make_highlight_with_message(
                "lint",
                "MD-UNDER",
                Severity::Warn,
                Some("src/lib.rs"),
                Some(2),
                "Variable _foo_ and __bar__ are unused",
            ),
            make_highlight_with_message(
                "lint",
                "MD-PIPE",
                Severity::Warn,
                Some("src/lib.rs"),
                Some(3),
                "Expression a | b | c is ambiguous",
            ),
            make_highlight_with_message(
                "lint",
                "MD-BACKTICK",
                Severity::Info,
                Some("src/lib.rs"),
                Some(4),
                "Consider using `Option<T>` instead of `Result<T, ()>`",
            ),
            make_highlight_with_message(
                "lint",
                "MD-MIXED",
                Severity::Error,
                Some("src/lib.rs"),
                Some(5),
                "Found `*ptr | mask` with _unsafe_ block at **line 5**",
            ),
        ],
    );

    let md = render_comment(&report, &cfg);
    // All messages should be present verbatim (renderer doesn't escape)
    assert!(md.contains("*bold*"));
    assert!(md.contains("a | b | c"));
    assert!(md.contains("`Option<T>`"));
    insta::assert_snapshot!(md);
}

// ===========================================================================
// 13. Long file paths (>100 chars)
// ===========================================================================

#[test]
fn golden_long_file_paths() {
    let mut cfg = CockpitConfig::default();
    cfg.policy.max_highlights = 5;
    cfg.policy.max_annotations = 5;
    cfg.sensors
        .insert("lint".to_string(), default_sensor_cfg("Lint"));
    cfg.policy.section_order = vec!["Lint".to_string()];

    let long_path = format!(
        "src/very/deeply/nested/directory/structure/that/goes/on/and/on/and/on/module_{}.rs",
        "a".repeat(50)
    );
    assert!(
        long_path.len() > 100,
        "path must be >100 chars for this test"
    );

    let report = make_report(
        &cfg,
        VerdictStatus::Fail,
        vec![],
        vec![sensor_summary("lint", VerdictStatus::Fail, true, false)],
        vec![
            make_highlight(
                "lint",
                "LONG-PATH-001",
                Severity::Error,
                Some(&long_path),
                Some(42),
            ),
            make_highlight(
                "lint",
                "LONG-PATH-002",
                Severity::Warn,
                Some("src/normal.rs"),
                Some(1),
            ),
        ],
    );

    let md = render_comment(&report, &cfg);
    assert!(
        md.contains(&long_path),
        "long path must appear in output verbatim"
    );
    assert!(md.contains("cockpit:end"), "must close with end marker");
    insta::assert_snapshot!(md);
}

// ===========================================================================
// 14. Unicode in findings: CJK, emoji, accented chars
// ===========================================================================

#[test]
fn golden_unicode_in_findings() {
    let mut cfg = CockpitConfig::default();
    cfg.policy.max_highlights = 10;
    cfg.policy.max_annotations = 10;
    cfg.sensors
        .insert("i18n-check".to_string(), default_sensor_cfg("I18N"));
    cfg.policy.section_order = vec!["I18N".to_string()];

    let report = make_report(
        &cfg,
        VerdictStatus::Warn,
        vec![],
        vec![sensor_summary(
            "i18n-check",
            VerdictStatus::Warn,
            true,
            false,
        )],
        vec![
            make_highlight_with_message(
                "i18n-check",
                "CJK-001",
                Severity::Warn,
                Some("src/translations.rs"),
                Some(10),
                "翻訳が見つかりません: 日本語テスト",
            ),
            make_highlight_with_message(
                "i18n-check",
                "EMOJI-001",
                Severity::Info,
                Some("src/emoji.rs"),
                Some(20),
                "Emoji test: 🚀🔥💯 passed ✅ with 🎉",
            ),
            make_highlight_with_message(
                "i18n-check",
                "ACCENT-001",
                Severity::Warn,
                Some("src/locale.rs"),
                Some(30),
                "Résumé naïve café über Straße — diacritics preserved",
            ),
            make_highlight_with_message(
                "i18n-check",
                "CYRILLIC-001",
                Severity::Error,
                Some("src/ru.rs"),
                Some(5),
                "Ошибка компиляции: неверный тип данных",
            ),
        ],
    );

    let md = render_comment(&report, &cfg);
    assert!(md.contains("翻訳が見つかりません"), "Japanese preserved");
    assert!(md.contains("🚀🔥💯"), "emoji preserved");
    assert!(md.contains("Résumé"), "accented chars preserved");
    assert!(md.contains("Ошибка"), "Cyrillic preserved");
    insta::assert_snapshot!(md);
}

// ===========================================================================
// 15. Stable markers present
// ===========================================================================

#[test]
fn golden_stable_markers_present() {
    let mut cfg = CockpitConfig::default();
    cfg.policy.max_highlights = 5;
    cfg.policy.max_annotations = 5;
    cfg.sensors
        .insert("builddiag".to_string(), default_sensor_cfg("Build"));
    cfg.sensors
        .insert("clippy".to_string(), default_sensor_cfg("Lint"));
    cfg.policy.section_order = vec!["Build".into(), "Lint".into()];

    let report = make_report(
        &cfg,
        VerdictStatus::Fail,
        vec!["clippy failed".to_string()],
        vec![
            sensor_summary("builddiag", VerdictStatus::Pass, true, false),
            sensor_summary("clippy", VerdictStatus::Fail, true, false),
        ],
        vec![make_highlight(
            "clippy",
            "E001",
            Severity::Error,
            Some("src/main.rs"),
            Some(10),
        )],
    );

    let md = render_comment(&report, &cfg);

    // Verify exact marker format
    assert!(
        md.starts_with("<!-- cockpit:begin -->\n"),
        "must start with begin marker on its own line"
    );
    assert!(
        md.ends_with("<!-- cockpit:end -->\n"),
        "must end with end marker followed by newline"
    );

    // Markers appear exactly once each
    assert_eq!(
        md.matches("<!-- cockpit:begin -->").count(),
        1,
        "begin marker must appear exactly once"
    );
    assert_eq!(
        md.matches("<!-- cockpit:end -->").count(),
        1,
        "end marker must appear exactly once"
    );

    // Begin marker is the first line, end marker is the last non-empty line
    let lines: Vec<&str> = md.lines().collect();
    assert_eq!(lines[0], "<!-- cockpit:begin -->");
    assert_eq!(lines[lines.len() - 1], "<!-- cockpit:end -->");

    insta::assert_snapshot!(md);
}
