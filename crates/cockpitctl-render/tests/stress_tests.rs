//! Stress tests for cockpitctl-render: truncation, budgets, and extreme inputs.

use std::collections::BTreeMap;

use cockpitctl_render::{render_annotations, render_comment};
use cockpitctl_types::{
    CockpitConfig, CockpitReport, Finding, Highlight, Location, MissingPolicy, PolicyOutcome,
    PolicySnapshot, Presence, RunInfo, SensorSummary, Severity, ToolInfo, Verdict, VerdictCounts,
    VerdictStatus,
};

fn make_finding(
    severity: Severity,
    code: &str,
    message: &str,
    path: Option<&str>,
    line: Option<u32>,
) -> Finding {
    Finding {
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
    }
}

fn make_highlight(sensor_id: &str, severity: Severity, code: &str, message: &str) -> Highlight {
    Highlight {
        sensor_id: sensor_id.to_string(),
        finding: make_finding(severity, code, message, None, None),
    }
}

fn make_sensor_summary(id: &str, status: VerdictStatus) -> SensorSummary {
    SensorSummary {
        id: id.to_string(),
        blocking: false,
        missing: MissingPolicy::Skip,
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
        policy_outcome: Some(PolicyOutcome::Informational),
    }
}

fn make_report(sensors: Vec<SensorSummary>, highlights: Vec<Highlight>) -> CockpitReport {
    CockpitReport {
        schema: "cockpit.report.v1".to_string(),
        tool: ToolInfo {
            name: "cockpitctl".to_string(),
            version: "0.1.0".to_string(),
            commit: None,
        },
        run: RunInfo {
            started_at: "2026-01-01T00:00:00Z".to_string(),
            ended_at: None,
            duration_ms: None,
            host: None,
            git: None,
            ci: None,
            capabilities: BTreeMap::new(),
        },
        verdict: Verdict {
            status: VerdictStatus::Pass,
            counts: VerdictCounts::default(),
            reasons: vec![],
        },
        sensors,
        highlights,
        policy: PolicySnapshot {
            warn_is_fail: false,
            max_highlights: 7,
            max_per_sensor_findings: 20,
            max_annotations: 25,
            section_order: vec![],
            sensors: vec![],
        },
        data: None,
    }
}

// ---------------------------------------------------------------------------
// 1. 10000 findings render: verify truncation, no OOM
// ---------------------------------------------------------------------------

#[test]
fn stress_render_10000_findings_no_oom() {
    let highlights: Vec<Highlight> = (0..10_000)
        .map(|i| {
            make_highlight(
                &format!("s{}", i % 50),
                Severity::Warn,
                &format!("W{}", i),
                &format!("msg {}", i),
            )
        })
        .collect();

    let mut cfg = CockpitConfig::default();
    cfg.policy.max_annotations = 25;

    let blocking = BTreeMap::new();
    let result = render_annotations(&highlights, &cfg, &blocking);

    assert!(result.truncated);
    assert_eq!(result.rendered_count, 25);
    assert_eq!(result.total_count, 10_000);
    assert!(!result.content.is_empty());
    assert!(result.content.contains("capped by"));
}

// ---------------------------------------------------------------------------
// 2. Long sensor names: 1000-char names handled gracefully
// ---------------------------------------------------------------------------

#[test]
fn stress_long_sensor_names_render() {
    let long_name = "a".repeat(1000);
    let sensors = vec![make_sensor_summary(&long_name, VerdictStatus::Pass)];
    let highlights = vec![make_highlight(
        &long_name,
        Severity::Info,
        "I1",
        "test message",
    )];

    let report = make_report(sensors, highlights);
    let cfg = CockpitConfig::default();

    let md = render_comment(&report, &cfg);
    assert!(md.contains(&long_name));
    assert!(md.contains("cockpit:begin"));
    assert!(md.contains("cockpit:end"));
}

// ---------------------------------------------------------------------------
// 3. Unicode stress: emoji, CJK, RTL text → markdown valid
// ---------------------------------------------------------------------------

#[test]
fn stress_unicode_findings_render() {
    let unicode_messages = [
        "🚀 Build failed with rocket errors",
        "编译失败：找不到模块",
        "بناء فشل: خطأ في التحليل",
        "テストが失敗しました",
        "🎉🎊🥳 Mixed emoji: 你好世界 مرحبا",
        "Path: café/naïve/résumé.rs",
        "Null\u{0000}byte handling",
        "Line\nbreak\tinside",
    ];

    let highlights: Vec<Highlight> = unicode_messages
        .iter()
        .enumerate()
        .map(|(i, msg)| make_highlight("unicode-sensor", Severity::Warn, &format!("U{}", i), msg))
        .collect();

    let sensors = vec![make_sensor_summary("unicode-sensor", VerdictStatus::Warn)];
    let report = make_report(sensors, highlights);
    let cfg = CockpitConfig::default();

    let md = render_comment(&report, &cfg);
    assert!(md.contains("cockpit:begin"));
    assert!(md.contains("cockpit:end"));
    // Verify no panics and markdown is not empty.
    assert!(md.len() > 100);
}

// ---------------------------------------------------------------------------
// 4. Budget exhaustion: exactly at budget boundary → correct count
// ---------------------------------------------------------------------------

#[test]
fn stress_exact_budget_boundary() {
    for budget in [1, 5, 10, 25, 50] {
        let highlights: Vec<Highlight> = (0..budget)
            .map(|i| make_highlight("s", Severity::Error, &format!("E{}", i), &format!("m{}", i)))
            .collect();

        let mut cfg = CockpitConfig::default();
        cfg.policy.max_annotations = budget;

        let blocking = BTreeMap::new();
        let result = render_annotations(&highlights, &cfg, &blocking);

        assert_eq!(result.rendered_count, budget);
        assert!(
            !result.truncated,
            "should not truncate at exact boundary for budget={}",
            budget
        );
    }

    // One over budget.
    let highlights: Vec<Highlight> = (0..26)
        .map(|i| make_highlight("s", Severity::Error, &format!("E{}", i), &format!("m{}", i)))
        .collect();

    let mut cfg = CockpitConfig::default();
    cfg.policy.max_annotations = 25;

    let blocking = BTreeMap::new();
    let result = render_annotations(&highlights, &cfg, &blocking);
    assert!(result.truncated);
    assert_eq!(result.rendered_count, 25);
}

// ---------------------------------------------------------------------------
// 5. Many sections: 50 sensors each with findings → all rendered
// ---------------------------------------------------------------------------

#[test]
fn stress_50_sensors_all_sections_rendered() {
    let sensors: Vec<SensorSummary> = (0..50)
        .map(|i| make_sensor_summary(&format!("sensor-{:03}", i), VerdictStatus::Warn))
        .collect();
    let highlights: Vec<Highlight> = (0..50)
        .map(|i| {
            make_highlight(
                &format!("sensor-{:03}", i),
                Severity::Warn,
                &format!("W{}", i),
                &format!("finding from sensor {}", i),
            )
        })
        .collect();

    let report = make_report(sensors, highlights);
    let cfg = CockpitConfig::default();

    let md = render_comment(&report, &cfg);
    assert!(md.contains("cockpit:begin"));
    assert!(md.contains("cockpit:end"));

    // All 50 sensors appear in the summary table.
    for i in 0..50 {
        assert!(
            md.contains(&format!("sensor-{:03}", i)),
            "sensor-{:03} should appear in rendered output",
            i
        );
    }
}

// ---------------------------------------------------------------------------
// 6. Full render_comment with large report (100 sensors, 7 highlights)
// ---------------------------------------------------------------------------

#[test]
fn stress_full_render_100_sensors() {
    let sensors: Vec<SensorSummary> = (0..100)
        .map(|i| make_sensor_summary(&format!("s{:03}", i), VerdictStatus::Pass))
        .collect();
    let highlights: Vec<Highlight> = (0..7)
        .map(|i| {
            make_highlight(
                &format!("s{:03}", i),
                Severity::Error,
                &format!("E{}", i),
                &format!("err {}", i),
            )
        })
        .collect();

    let report = make_report(sensors, highlights);
    let cfg = CockpitConfig::default();

    let md = render_comment(&report, &cfg);
    assert!(md.contains("cockpit:begin"));
    assert!(md.contains("cockpit:end"));
    assert!(md.contains("Highlights"));
    // Should mention all 100 sensors in summary table.
    assert_eq!(md.matches("| `s").count(), 100);
}
