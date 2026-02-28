//! Insta snapshot tests for `compute_trend` output.

use std::collections::BTreeMap;

use cockpitctl_domain_trend::compute_trend;
use cockpitctl_types::{
    CockpitReport, Finding, Highlight, Location, MissingPolicy, PolicySnapshot, Presence, RunInfo,
    SensorSummary, Severity, ToolInfo, Verdict, VerdictCounts, VerdictStatus,
};

// ── Helpers ────────────────────────────────────────────────────────────────

fn empty_report(status: VerdictStatus) -> CockpitReport {
    CockpitReport {
        schema: "cockpit.report.v1".to_string(),
        tool: ToolInfo {
            name: "test".to_string(),
            version: "0.0.0".to_string(),
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
        sensor_id: sensor_id.to_string(),
        finding: Finding {
            severity,
            check_id: None,
            code: code.to_string(),
            message: format!("{code} message"),
            location: if path.is_some() || line.is_some() {
                Some(Location {
                    path: path.map(|p| p.to_string()),
                    line,
                    col: None,
                })
            } else {
                None
            },
            help: None,
            url: None,
            fingerprint: fingerprint.map(|f| f.to_string()),
            data: None,
        },
    }
}

fn make_sensor(id: &str) -> SensorSummary {
    SensorSummary {
        id: id.to_string(),
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

// ── Snapshot tests ─────────────────────────────────────────────────────────

#[test]
fn snapshot_improving_report() {
    // Baseline: 2 errors, 1 warning. Current: 0 errors, 0 warnings.
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
            Some("fp1"),
            Some("src/a.rs"),
            Some(10),
        ),
        make_highlight(
            "lint",
            "E002",
            Severity::Error,
            Some("fp2"),
            Some("src/b.rs"),
            Some(20),
        ),
        make_highlight(
            "lint",
            "W001",
            Severity::Warn,
            Some("fp3"),
            Some("src/c.rs"),
            Some(30),
        ),
    ];

    let mut current = empty_report(VerdictStatus::Pass);
    current.sensors = vec![make_sensor("lint")];
    // All findings fixed — no highlights.

    let trend = compute_trend(&baseline, &current);
    insta::assert_json_snapshot!("improving_report", trend);
}

#[test]
fn snapshot_degrading_report() {
    // Baseline: clean. Current: 2 new errors.
    let mut baseline = empty_report(VerdictStatus::Pass);
    baseline.sensors = vec![make_sensor("lint")];

    let mut current = empty_report(VerdictStatus::Fail);
    current.verdict.counts = VerdictCounts {
        info: 0,
        warn: 0,
        error: 2,
        suppressed: 0,
    };
    current.sensors = vec![make_sensor("lint")];
    current.highlights = vec![
        make_highlight(
            "lint",
            "E001",
            Severity::Error,
            Some("fp1"),
            Some("src/a.rs"),
            Some(10),
        ),
        make_highlight(
            "lint",
            "E002",
            Severity::Error,
            Some("fp2"),
            Some("src/b.rs"),
            Some(20),
        ),
    ];

    let trend = compute_trend(&baseline, &current);
    insta::assert_json_snapshot!("degrading_report", trend);
}

#[test]
fn snapshot_stable_report() {
    // Both baseline and current have the same findings.
    let mut report = empty_report(VerdictStatus::Warn);
    report.verdict.counts = VerdictCounts {
        info: 1,
        warn: 1,
        error: 0,
        suppressed: 0,
    };
    report.sensors = vec![make_sensor("lint")];
    report.highlights = vec![
        make_highlight(
            "lint",
            "I001",
            Severity::Info,
            Some("fp1"),
            Some("src/a.rs"),
            Some(5),
        ),
        make_highlight(
            "lint",
            "W001",
            Severity::Warn,
            Some("fp2"),
            Some("src/b.rs"),
            Some(15),
        ),
    ];

    let trend = compute_trend(&report, &report);
    insta::assert_json_snapshot!("stable_report", trend);
}

#[test]
fn snapshot_new_sensor_added() {
    let mut baseline = empty_report(VerdictStatus::Pass);
    baseline.sensors = vec![make_sensor("lint")];
    baseline.highlights = vec![make_highlight(
        "lint",
        "W001",
        Severity::Warn,
        Some("fp1"),
        Some("src/a.rs"),
        Some(1),
    )];
    baseline.verdict.counts.warn = 1;

    let mut current = empty_report(VerdictStatus::Warn);
    current.sensors = vec![make_sensor("lint"), make_sensor("security")];
    current.highlights = vec![
        make_highlight(
            "lint",
            "W001",
            Severity::Warn,
            Some("fp1"),
            Some("src/a.rs"),
            Some(1),
        ),
        make_highlight(
            "security",
            "SEC01",
            Severity::Error,
            Some("fp-sec1"),
            Some("src/auth.rs"),
            Some(42),
        ),
    ];
    current.verdict.counts = VerdictCounts {
        info: 0,
        warn: 1,
        error: 1,
        suppressed: 0,
    };

    let trend = compute_trend(&baseline, &current);
    insta::assert_json_snapshot!("new_sensor_added", trend);
}

#[test]
fn snapshot_sensor_removed() {
    let mut baseline = empty_report(VerdictStatus::Warn);
    baseline.sensors = vec![make_sensor("lint"), make_sensor("coverage")];
    baseline.highlights = vec![
        make_highlight(
            "lint",
            "W001",
            Severity::Warn,
            Some("fp1"),
            Some("src/a.rs"),
            Some(1),
        ),
        make_highlight(
            "coverage",
            "COV01",
            Severity::Info,
            Some("fp-cov1"),
            Some("src/lib.rs"),
            Some(100),
        ),
    ];
    baseline.verdict.counts = VerdictCounts {
        info: 1,
        warn: 1,
        error: 0,
        suppressed: 0,
    };

    // Coverage sensor removed; only lint remains.
    let mut current = empty_report(VerdictStatus::Warn);
    current.sensors = vec![make_sensor("lint")];
    current.highlights = vec![make_highlight(
        "lint",
        "W001",
        Severity::Warn,
        Some("fp1"),
        Some("src/a.rs"),
        Some(1),
    )];
    current.verdict.counts = VerdictCounts {
        info: 0,
        warn: 1,
        error: 0,
        suppressed: 0,
    };

    let trend = compute_trend(&baseline, &current);
    insta::assert_json_snapshot!("sensor_removed", trend);
}
