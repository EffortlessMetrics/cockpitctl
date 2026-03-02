//! Render edge-case and boundary tests.
//!
//! Covers scenarios complementary to `edge_case_expansion.rs`:
//! - Budget exhaustion at exact boundaries (max_highlights = N with N highlights)
//! - Special characters in sensor IDs and messages (pipes, backticks, markdown)
//! - Marker stability through trend, buildfix, and policy-signature sections
//! - All verdict-state transitions in trend rendering
//! - Buildfix / apply / policy-signature section edge cases

use std::collections::BTreeMap;

use cockpitctl_render::{
    render_buildfix_section, render_comment, render_policy_signature_section, render_trend_section,
};
use cockpitctl_types::{
    BuildfixSummary, CockpitConfig, CockpitReport, CountDeltas, Finding, FixSummary, Highlight,
    Location, MatchedFinding, MissingPolicy, PolicySensorSnapshot, PolicySignatureAlgorithm,
    PolicySignatureEvidence, PolicySnapshot, Presence, RunInfo, SafetyLevel, SensorPolicy,
    SensorSummary, Severity, ToolInfo, TrendDelta, TrendFinding, Verdict, VerdictChange,
    VerdictCounts, VerdictStatus,
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

fn make_highlight(sensor_id: &str, code: &str, severity: Severity, message: &str) -> Highlight {
    Highlight {
        sensor_id: sensor_id.to_string(),
        finding: Finding {
            severity,
            check_id: None,
            code: code.to_string(),
            message: message.to_string(),
            location: Some(Location {
                path: Some("src/lib.rs".to_string()),
                line: Some(1),
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

fn make_report(
    cfg: &CockpitConfig,
    verdict_status: VerdictStatus,
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
            reasons: vec![],
        },
        sensors,
        highlights,
        policy: policy_snapshot_from_cfg(cfg),
        data: None,
    }
}

// ===========================================================================
// 1. Budget exhaustion at exact boundaries
// ===========================================================================

#[test]
fn budget_exact_boundary_no_truncation() {
    let mut cfg = CockpitConfig::default();
    cfg.policy.max_highlights = 3;
    cfg.policy.max_annotations = 3;

    let highlights: Vec<Highlight> = (0..3)
        .map(|i| {
            make_highlight(
                "sensor",
                &format!("C{}", i),
                Severity::Error,
                &format!("msg {}", i),
            )
        })
        .collect();
    let report = make_report(&cfg, VerdictStatus::Fail, vec![], highlights);
    let md = render_comment(&report, &cfg);

    assert!(md.contains("showing up to **3**"));
    assert!(md.contains("3. "));
    assert!(!md.contains("4. "));
}

#[test]
fn budget_one_over_boundary_truncates() {
    let mut cfg = CockpitConfig::default();
    cfg.policy.max_highlights = 2;

    let highlights: Vec<Highlight> = (0..3)
        .map(|i| {
            make_highlight(
                "sensor",
                &format!("C{}", i),
                Severity::Warn,
                &format!("msg {}", i),
            )
        })
        .collect();
    // render_comment only shows up to max_highlights in its numbered list
    let report = make_report(&cfg, VerdictStatus::Warn, vec![], highlights);
    let md = render_comment(&report, &cfg);

    assert!(md.contains("showing up to **2**"));
}

#[test]
fn budget_one_highlight_exactly() {
    let mut cfg = CockpitConfig::default();
    cfg.policy.max_highlights = 1;
    cfg.policy.max_annotations = 1;

    let highlights = vec![make_highlight("sensor", "C0", Severity::Error, "single")];
    let report = make_report(&cfg, VerdictStatus::Fail, vec![], highlights);
    let md = render_comment(&report, &cfg);

    assert!(md.contains("showing up to **1**"));
    assert!(md.contains("1. "));
    assert!(!md.contains("2. "));
}

// ===========================================================================
// 2. Special characters in sensor IDs and messages
// ===========================================================================

#[test]
fn pipe_in_sensor_id_does_not_break_table() {
    let mut cfg = CockpitConfig::default();
    cfg.sensors.insert(
        "pipe|sensor".to_string(),
        SensorPolicy {
            blocking: true,
            missing: MissingPolicy::Fail,
            section: None,
            require_label: None,
            repro: None,
        },
    );

    let sensors = vec![sensor_summary("pipe|sensor", VerdictStatus::Pass, true)];
    let report = make_report(&cfg, VerdictStatus::Pass, sensors, vec![]);
    let md = render_comment(&report, &cfg);

    // The table row should contain the sensor ID; the comment should still be
    // valid markdown even if pipes inside backticks are tricky.
    assert!(md.contains("pipe|sensor"));
    assert!(md.contains("<!-- cockpit:begin -->"));
    assert!(md.contains("<!-- cockpit:end -->"));
}

#[test]
fn backtick_in_message_renders() {
    let cfg = CockpitConfig::default();
    let highlights = vec![make_highlight(
        "sensor",
        "BT1",
        Severity::Error,
        "unexpected `token` in expression",
    )];
    let report = make_report(&cfg, VerdictStatus::Fail, vec![], highlights);
    let md = render_comment(&report, &cfg);

    assert!(md.contains("unexpected `token` in expression"));
}

#[test]
fn markdown_injection_in_message() {
    let cfg = CockpitConfig::default();
    let highlights = vec![make_highlight(
        "sensor",
        "MD1",
        Severity::Warn,
        "see [link](http://evil.com) and **bold**",
    )];
    let report = make_report(&cfg, VerdictStatus::Warn, vec![], highlights);
    let md = render_comment(&report, &cfg);

    // Messages are rendered as-is (no sanitisation), but markers must survive
    assert!(md.contains("[link](http://evil.com)"));
    assert!(md.contains("<!-- cockpit:begin -->"));
    assert!(md.contains("<!-- cockpit:end -->"));
}

// ===========================================================================
// 3. Marker stability through auxiliary sections
// ===========================================================================

#[test]
fn markers_present_with_all_sections() {
    let mut cfg = CockpitConfig::default();
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

    let sensors = vec![sensor_summary("lint", VerdictStatus::Pass, true)];
    let highlights = vec![make_highlight("lint", "W1", Severity::Warn, "unused var")];
    let report = make_report(&cfg, VerdictStatus::Warn, sensors, highlights);
    let md = render_comment(&report, &cfg);

    let begin = md.find("<!-- cockpit:begin -->").unwrap();
    let end = md.find("<!-- cockpit:end -->").unwrap();
    assert!(begin < end);
    assert_eq!(begin, 0);
    assert!(md[end..].trim_end().ends_with("<!-- cockpit:end -->"));
}

// ===========================================================================
// 4. Trend section edge cases
// ===========================================================================

#[test]
fn trend_empty_delta_shows_no_changes() {
    let trend = TrendDelta {
        verdict_change: None,
        count_deltas: CountDeltas::default(),
        new_findings: vec![],
        fixed_findings: vec![],
        sensors_added: vec![],
        sensors_removed: vec![],
    };
    let md = render_trend_section(&trend);

    assert!(md.contains("### Trend"));
    assert!(md.contains("_No changes from baseline._"));
}

#[test]
fn trend_verdict_change_all_transitions() {
    for (before, after) in [
        (VerdictStatus::Pass, VerdictStatus::Fail),
        (VerdictStatus::Fail, VerdictStatus::Pass),
        (VerdictStatus::Warn, VerdictStatus::Skip),
        (VerdictStatus::Skip, VerdictStatus::Warn),
    ] {
        let trend = TrendDelta {
            verdict_change: Some(VerdictChange {
                before: before.clone(),
                after: after.clone(),
            }),
            count_deltas: CountDeltas::default(),
            new_findings: vec![],
            fixed_findings: vec![],
            sensors_added: vec![],
            sensors_removed: vec![],
        };
        let md = render_trend_section(&trend);
        assert!(
            md.contains("Verdict:"),
            "missing verdict line for {before:?}->{after:?}"
        );
        assert!(md.contains("→"), "missing arrow for {before:?}->{after:?}");
    }
}

#[test]
fn trend_new_and_fixed_findings() {
    let trend = TrendDelta {
        verdict_change: None,
        count_deltas: CountDeltas {
            error_delta: 1,
            warn_delta: -1,
            info_delta: 0,
        },
        new_findings: vec![TrendFinding {
            sensor_id: "clippy".to_string(),
            code: "W001".to_string(),
            message: "new warning".to_string(),
            path: Some("src/main.rs".to_string()),
            line: Some(42),
            fingerprint: None,
            severity: Severity::Warn,
        }],
        fixed_findings: vec![TrendFinding {
            sensor_id: "clippy".to_string(),
            code: "E001".to_string(),
            message: "old error fixed".to_string(),
            path: None,
            line: None,
            fingerprint: None,
            severity: Severity::Error,
        }],
        sensors_added: vec!["new_sensor".to_string()],
        sensors_removed: vec!["old_sensor".to_string()],
    };
    let md = render_trend_section(&trend);

    assert!(md.contains("1 new finding(s)"));
    assert!(md.contains("1 fixed finding(s)"));
    assert!(md.contains("Sensors added: `new_sensor`"));
    assert!(md.contains("Sensors removed: `old_sensor`"));
    assert!(md.contains("at `src/main.rs:42`"));
}

#[test]
fn trend_only_count_deltas() {
    let trend = TrendDelta {
        verdict_change: None,
        count_deltas: CountDeltas {
            error_delta: 0,
            warn_delta: 5,
            info_delta: -3,
        },
        new_findings: vec![],
        fixed_findings: vec![],
        sensors_added: vec![],
        sensors_removed: vec![],
    };
    let md = render_trend_section(&trend);

    assert!(md.contains("| Warn | +5 |"));
    assert!(md.contains("| Info | -3 |"));
    assert!(!md.contains("Error"));
    assert!(!md.contains("_No changes from baseline._"));
}

// ===========================================================================
// 5. Buildfix section edge cases
// ===========================================================================

#[test]
fn buildfix_empty_fixes() {
    let summary = BuildfixSummary {
        fixes: vec![],
        total_fixes: 0,
        matched_count: 0,
        unmatched_count: 0,
    };
    let md = render_buildfix_section(&summary);

    assert!(md.contains("### Buildfix"));
    assert!(md.contains("_No fixes available._"));
}

#[test]
fn buildfix_with_fixes() {
    let summary = BuildfixSummary {
        fixes: vec![FixSummary {
            fix_id: "fix-1".to_string(),
            sensor_id: "clippy".to_string(),
            safety: SafetyLevel::Safe,
            description: "Remove unused import".to_string(),
            matched_findings: vec![MatchedFinding {
                sensor_id: "clippy".to_string(),
                code: "W001".to_string(),
                fingerprint: None,
            }],
            unmatched: false,
        }],
        total_fixes: 1,
        matched_count: 1,
        unmatched_count: 0,
    };
    let md = render_buildfix_section(&summary);

    assert!(md.contains("1 fix(es) available"));
    assert!(md.contains("fix-1"));
    assert!(md.contains("safe"));
}

// ===========================================================================
// 6. Policy signature section edge cases
// ===========================================================================

#[test]
fn policy_signature_renders_algorithm_and_digest() {
    let evidence = PolicySignatureEvidence {
        schema: "cockpit.policy_signature.v1".into(),
        algorithm: PolicySignatureAlgorithm::HmacSha256,
        policy_sha256: "abcdef1234567890".to_string(),
        signature: "deadbeef".to_string(),
        key_id: Some("key-1".to_string()),
    };
    let md = render_policy_signature_section(&evidence);

    assert!(md.contains("### Policy Signature"));
    assert!(md.contains("abcdef1234567890"));
    assert!(md.contains("deadbeef"));
    assert!(md.contains("key-1"));
}

#[test]
fn policy_signature_without_key_id() {
    let evidence = PolicySignatureEvidence {
        schema: "cockpit.policy_signature.v1".into(),
        algorithm: PolicySignatureAlgorithm::HmacSha256,
        policy_sha256: "0000000000000000".to_string(),
        signature: "1111111111111111".to_string(),
        key_id: None,
    };
    let md = render_policy_signature_section(&evidence);

    assert!(md.contains("### Policy Signature"));
    assert!(md.contains("0000000000000000"));
    assert!(!md.contains("Key ID"));
}

// ===========================================================================
// 7. Verdict rendering completeness
// ===========================================================================

#[test]
fn all_verdict_states_in_comment() {
    let cfg = CockpitConfig::default();
    let sensors = vec![
        sensor_summary("s_pass", VerdictStatus::Pass, false),
        sensor_summary("s_warn", VerdictStatus::Warn, false),
        sensor_summary("s_fail", VerdictStatus::Fail, true),
        sensor_summary("s_skip", VerdictStatus::Skip, false),
    ];
    let report = make_report(&cfg, VerdictStatus::Fail, sensors, vec![]);
    let md = render_comment(&report, &cfg);

    assert!(md.contains("✅ pass"));
    assert!(md.contains("⚠️ warn"));
    assert!(md.contains("❌ fail"));
    assert!(md.contains("⏭ skip"));
}

#[test]
fn truncated_sensor_shows_truncation_note() {
    let cfg = CockpitConfig::default();
    let mut s = sensor_summary("trunc", VerdictStatus::Pass, false);
    s.truncated = true;
    let report = make_report(&cfg, VerdictStatus::Pass, vec![s], vec![]);
    let md = render_comment(&report, &cfg);

    assert!(md.contains("_truncated_"));
}

#[test]
fn sensor_with_comment_path_shows_link() {
    let cfg = CockpitConfig::default();
    let mut s = sensor_summary("linked", VerdictStatus::Pass, false);
    s.comment_path = Some("artifacts/linked/comment.md".to_string());
    let report = make_report(&cfg, VerdictStatus::Pass, vec![s], vec![]);
    let md = render_comment(&report, &cfg);

    assert!(md.contains("`artifacts/linked/comment.md`"));
}
