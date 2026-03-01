//! Expanded trend delta tests covering edge cases beyond snapshot_tests.rs.

use std::collections::BTreeMap;

use cockpitctl_domain_trend::compute_trend;
use cockpitctl_types::{
    CockpitReport, Finding, Highlight, Location, MissingPolicy, PolicySnapshot, Presence, RunInfo,
    SensorSummary, Severity, ToolInfo, Verdict, VerdictCounts, VerdictStatus,
};

// ── Helpers ────────────────────────────────────────────────────────────────

fn empty_report(status: VerdictStatus) -> CockpitReport {
    CockpitReport {
        schema: "cockpit.report.v1".into(),
        tool: ToolInfo {
            name: "test".into(),
            version: "0.0.0".into(),
            commit: None,
        },
        run: RunInfo {
            started_at: "2026-01-01T00:00:00Z".into(),
            ended_at: None,
            duration_ms: None,
            host: None,
            git: None,
            ci: None,
            capabilities: BTreeMap::new(),
        },
        verdict: Verdict {
            status,
            counts: VerdictCounts {
                info: 0,
                warn: 0,
                error: 0,
                suppressed: 0,
            },
            reasons: vec![],
        },
        sensors: vec![],
        highlights: vec![],
        policy: PolicySnapshot {
            warn_is_fail: false,
            max_highlights: 10,
            max_per_sensor_findings: 10,
            max_annotations: 20,
            section_order: vec![],
            sensors: vec![],
        },
        data: None,
    }
}

fn make_highlight(
    sensor_id: &str,
    code: &str,
    severity: Severity,
    fingerprint: Option<&str>,
    path: Option<&str>,
    line: Option<u32>,
) -> Highlight {
    Highlight {
        sensor_id: sensor_id.into(),
        finding: Finding {
            severity,
            check_id: None,
            code: code.into(),
            message: format!("{code} message"),
            location: if path.is_some() || line.is_some() {
                Some(Location {
                    path: path.map(Into::into),
                    line,
                    col: None,
                })
            } else {
                None
            },
            help: None,
            url: None,
            fingerprint: fingerprint.map(Into::into),
            data: None,
        },
    }
}

fn make_sensor(id: &str) -> SensorSummary {
    SensorSummary {
        id: id.into(),
        blocking: true,
        missing: MissingPolicy::Fail,
        presence: Presence::Present,
        report_path: format!("artifacts/{id}/report.json"),
        comment_path: None,
        verdict: Verdict {
            status: VerdictStatus::Pass,
            counts: VerdictCounts {
                info: 0,
                warn: 0,
                error: 0,
                suppressed: 0,
            },
            reasons: vec![],
        },
        truncated: false,
        errors: vec![],
        missing_policy_applied: None,
        policy_outcome: None,
    }
}

// ── Identical baseline → no change ─────────────────────────────────────────

#[test]
fn identical_baseline_no_change() {
    let mut report = empty_report(VerdictStatus::Warn);
    report.verdict.counts.warn = 2;
    report.sensors = vec![make_sensor("lint")];
    report.highlights = vec![
        make_highlight(
            "lint",
            "W001",
            Severity::Warn,
            Some("fp-1"),
            Some("src/a.rs"),
            Some(10),
        ),
        make_highlight(
            "lint",
            "W002",
            Severity::Warn,
            Some("fp-2"),
            Some("src/b.rs"),
            Some(20),
        ),
    ];

    let trend = compute_trend(&report, &report);
    insta::assert_json_snapshot!("identical_baseline_no_change", trend);
}

// ── Improved baseline → improvements detected ──────────────────────────────

#[test]
fn improved_baseline_detects_fixes() {
    let mut baseline = empty_report(VerdictStatus::Fail);
    baseline.verdict.counts = VerdictCounts {
        info: 0,
        warn: 1,
        error: 2,
        suppressed: 0,
    };
    baseline.sensors = vec![make_sensor("lint")];
    baseline.highlights = vec![
        make_highlight(
            "lint",
            "E001",
            Severity::Error,
            Some("fp-e1"),
            Some("src/a.rs"),
            Some(10),
        ),
        make_highlight(
            "lint",
            "E002",
            Severity::Error,
            Some("fp-e2"),
            Some("src/b.rs"),
            Some(20),
        ),
        make_highlight(
            "lint",
            "W001",
            Severity::Warn,
            Some("fp-w1"),
            Some("src/c.rs"),
            Some(30),
        ),
    ];

    // Current: only one warning remains, errors fixed.
    let mut current = empty_report(VerdictStatus::Warn);
    current.verdict.counts = VerdictCounts {
        info: 0,
        warn: 1,
        error: 0,
        suppressed: 0,
    };
    current.sensors = vec![make_sensor("lint")];
    current.highlights = vec![make_highlight(
        "lint",
        "W001",
        Severity::Warn,
        Some("fp-w1"),
        Some("src/c.rs"),
        Some(30),
    )];

    let trend = compute_trend(&baseline, &current);
    insta::assert_json_snapshot!("improved_baseline_fixes", trend);
}

// ── Regressed baseline → regressions detected ─────────────────────────────

#[test]
fn regressed_baseline_detects_new_findings() {
    let mut baseline = empty_report(VerdictStatus::Pass);
    baseline.sensors = vec![make_sensor("lint")];

    // Current introduces new errors.
    let mut current = empty_report(VerdictStatus::Fail);
    current.verdict.counts = VerdictCounts {
        info: 0,
        warn: 0,
        error: 3,
        suppressed: 0,
    };
    current.sensors = vec![make_sensor("lint")];
    current.highlights = vec![
        make_highlight(
            "lint",
            "E001",
            Severity::Error,
            Some("fp-new-1"),
            Some("src/x.rs"),
            Some(5),
        ),
        make_highlight(
            "lint",
            "E002",
            Severity::Error,
            Some("fp-new-2"),
            Some("src/y.rs"),
            Some(15),
        ),
        make_highlight(
            "lint",
            "E003",
            Severity::Error,
            Some("fp-new-3"),
            Some("src/z.rs"),
            Some(25),
        ),
    ];

    let trend = compute_trend(&baseline, &current);
    insta::assert_json_snapshot!("regressed_baseline_new_findings", trend);
}

// ── Empty baseline → all findings are new ──────────────────────────────────

#[test]
fn empty_baseline_all_new() {
    let baseline = empty_report(VerdictStatus::Pass);

    let mut current = empty_report(VerdictStatus::Warn);
    current.verdict.counts.warn = 2;
    current.sensors = vec![make_sensor("security")];
    current.highlights = vec![
        make_highlight(
            "security",
            "SEC001",
            Severity::Warn,
            Some("fp-sec1"),
            Some("src/auth.rs"),
            Some(42),
        ),
        make_highlight(
            "security",
            "SEC002",
            Severity::Warn,
            None,
            Some("src/crypto.rs"),
            Some(88),
        ),
    ];

    let trend = compute_trend(&baseline, &current);
    insta::assert_json_snapshot!("empty_baseline_all_new", trend);
}

// ── Empty current → all findings removed ───────────────────────────────────

#[test]
fn empty_current_all_removed() {
    let mut baseline = empty_report(VerdictStatus::Fail);
    baseline.verdict.counts = VerdictCounts {
        info: 1,
        warn: 1,
        error: 1,
        suppressed: 0,
    };
    baseline.sensors = vec![make_sensor("lint"), make_sensor("security")];
    baseline.highlights = vec![
        make_highlight(
            "lint",
            "E001",
            Severity::Error,
            Some("fp-e1"),
            Some("src/a.rs"),
            Some(1),
        ),
        make_highlight(
            "lint",
            "W001",
            Severity::Warn,
            Some("fp-w1"),
            Some("src/b.rs"),
            Some(2),
        ),
        make_highlight(
            "security",
            "I001",
            Severity::Info,
            Some("fp-i1"),
            Some("src/c.rs"),
            Some(3),
        ),
    ];

    let current = empty_report(VerdictStatus::Pass);

    let trend = compute_trend(&baseline, &current);
    insta::assert_json_snapshot!("empty_current_all_removed", trend);
}

// ── Fingerprint-based matching across code changes ─────────────────────────

#[test]
fn fingerprint_matches_across_line_changes() {
    let mut baseline = empty_report(VerdictStatus::Warn);
    baseline.verdict.counts.warn = 1;
    baseline.sensors = vec![make_sensor("lint")];
    baseline.highlights = vec![make_highlight(
        "lint",
        "W001",
        Severity::Warn,
        Some("stable-fp"),
        Some("src/a.rs"),
        Some(10),
    )];

    // Same fingerprint, different line → still matched (not new/fixed).
    let mut current = empty_report(VerdictStatus::Warn);
    current.verdict.counts.warn = 1;
    current.sensors = vec![make_sensor("lint")];
    current.highlights = vec![make_highlight(
        "lint",
        "W001",
        Severity::Warn,
        Some("stable-fp"),
        Some("src/a.rs"),
        Some(25), // line shifted
    )];

    let trend = compute_trend(&baseline, &current);
    insta::assert_json_snapshot!("fingerprint_matches_across_lines", trend);
}

// ── Sensors added and removed simultaneously ───────────────────────────────

#[test]
fn sensors_added_and_removed() {
    let mut baseline = empty_report(VerdictStatus::Pass);
    baseline.sensors = vec![make_sensor("old-sensor")];

    let mut current = empty_report(VerdictStatus::Pass);
    current.sensors = vec![make_sensor("new-sensor")];

    let trend = compute_trend(&baseline, &current);
    insta::assert_json_snapshot!("sensors_added_and_removed", trend);
}
