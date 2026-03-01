//! Expanded trend delta tests covering edge cases beyond snapshot_tests.rs.

use std::collections::BTreeMap;

use cockpitctl_domain_trend::compute_trend;
use cockpitctl_types::{
    CockpitReport, CountDeltas, Finding, Highlight, Location, MissingPolicy, PolicySnapshot,
    Presence, RunInfo, SensorSummary, Severity, ToolInfo, Verdict, VerdictCounts, VerdictStatus,
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

// ── Verdict change: pass → fail ────────────────────────────────────────────

#[test]
fn verdict_change_pass_to_fail() {
    let mut baseline = empty_report(VerdictStatus::Pass);
    baseline.sensors = vec![make_sensor("lint")];

    let mut current = empty_report(VerdictStatus::Fail);
    current.verdict.counts.error = 1;
    current.sensors = vec![make_sensor("lint")];
    current.highlights = vec![make_highlight(
        "lint",
        "E001",
        Severity::Error,
        Some("fp-new"),
        Some("src/main.rs"),
        Some(42),
    )];

    let trend = compute_trend(&baseline, &current);
    let vc = trend
        .verdict_change
        .as_ref()
        .expect("verdict should change");
    assert_eq!(vc.before, VerdictStatus::Pass);
    assert_eq!(vc.after, VerdictStatus::Fail);
    assert_eq!(trend.new_findings.len(), 1);
    assert!(trend.fixed_findings.is_empty());
    assert_eq!(trend.count_deltas.error_delta, 1);
}

// ── Verdict change: fail → pass ────────────────────────────────────────────

#[test]
fn verdict_change_fail_to_pass() {
    let mut baseline = empty_report(VerdictStatus::Fail);
    baseline.verdict.counts.error = 2;
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
    ];

    let current = empty_report(VerdictStatus::Pass);

    let trend = compute_trend(&baseline, &current);
    let vc = trend
        .verdict_change
        .as_ref()
        .expect("verdict should change");
    assert_eq!(vc.before, VerdictStatus::Fail);
    assert_eq!(vc.after, VerdictStatus::Pass);
    assert!(trend.new_findings.is_empty());
    assert_eq!(trend.fixed_findings.len(), 2);
    assert_eq!(trend.count_deltas.error_delta, -2);
}

// ── New sensor added → detected ────────────────────────────────────────────

#[test]
fn new_sensor_added_detected() {
    let mut baseline = empty_report(VerdictStatus::Pass);
    baseline.sensors = vec![make_sensor("lint")];

    let mut current = empty_report(VerdictStatus::Warn);
    current.verdict.counts.warn = 1;
    current.sensors = vec![make_sensor("lint"), make_sensor("security")];
    current.highlights = vec![make_highlight(
        "security",
        "SEC01",
        Severity::Warn,
        Some("fp-sec"),
        Some("src/auth.rs"),
        Some(10),
    )];

    let trend = compute_trend(&baseline, &current);
    assert_eq!(trend.sensors_added, vec!["security"]);
    assert!(trend.sensors_removed.is_empty());
    assert_eq!(trend.new_findings.len(), 1);
    assert_eq!(trend.new_findings[0].sensor_id, "security");
}

// ── Sensor removed → detected ──────────────────────────────────────────────

#[test]
fn sensor_removed_detected() {
    let mut baseline = empty_report(VerdictStatus::Warn);
    baseline.verdict.counts.warn = 1;
    baseline.sensors = vec![make_sensor("lint"), make_sensor("coverage")];
    baseline.highlights = vec![make_highlight(
        "coverage",
        "COV01",
        Severity::Warn,
        Some("fp-cov"),
        Some("src/lib.rs"),
        Some(50),
    )];

    let mut current = empty_report(VerdictStatus::Pass);
    current.sensors = vec![make_sensor("lint")];

    let trend = compute_trend(&baseline, &current);
    assert!(trend.sensors_added.is_empty());
    assert_eq!(trend.sensors_removed, vec!["coverage"]);
    assert_eq!(trend.fixed_findings.len(), 1);
    assert_eq!(trend.fixed_findings[0].sensor_id, "coverage");
}

// ── Deterministic output ───────────────────────────────────────────────────

#[test]
fn deterministic_output_across_invocations() {
    let mut baseline = empty_report(VerdictStatus::Fail);
    baseline.verdict.counts = VerdictCounts {
        info: 1,
        warn: 2,
        error: 3,
        suppressed: 0,
    };
    baseline.sensors = vec![make_sensor("alpha"), make_sensor("beta")];
    baseline.highlights = vec![
        make_highlight(
            "alpha",
            "A01",
            Severity::Error,
            Some("fp-a1"),
            Some("src/a.rs"),
            Some(10),
        ),
        make_highlight(
            "beta",
            "B01",
            Severity::Warn,
            Some("fp-b1"),
            Some("src/b.rs"),
            Some(20),
        ),
    ];

    let mut current = empty_report(VerdictStatus::Warn);
    current.verdict.counts = VerdictCounts {
        info: 2,
        warn: 1,
        error: 0,
        suppressed: 0,
    };
    current.sensors = vec![make_sensor("alpha"), make_sensor("gamma")];
    current.highlights = vec![
        make_highlight(
            "gamma",
            "G01",
            Severity::Info,
            Some("fp-g1"),
            Some("src/g.rs"),
            Some(5),
        ),
        make_highlight(
            "alpha",
            "A02",
            Severity::Warn,
            Some("fp-a2"),
            Some("src/a2.rs"),
            Some(15),
        ),
    ];

    let trend1 = compute_trend(&baseline, &current);
    let trend2 = compute_trend(&baseline, &current);
    assert_eq!(trend1, trend2, "compute_trend must be deterministic");
}

// ── Large report with many changes ─────────────────────────────────────────

#[test]
fn large_report_many_changes() {
    let mut baseline = empty_report(VerdictStatus::Fail);
    let mut current = empty_report(VerdictStatus::Warn);

    let num_sensors = 5;
    let findings_per_sensor = 20;

    for s in 0..num_sensors {
        let sensor_id = format!("sensor-{s}");
        baseline.sensors.push(make_sensor(&sensor_id));
        current.sensors.push(make_sensor(&sensor_id));

        for f in 0..findings_per_sensor {
            let code = format!("F{s:03}-{f:03}");
            let fp = format!("fp-{s}-{f}");
            let path = format!("src/mod{s}/file{f}.rs");
            let line = (f + 1) as u32;

            // In baseline: all findings present.
            baseline.highlights.push(make_highlight(
                &sensor_id,
                &code,
                Severity::Warn,
                Some(&fp),
                Some(&path),
                Some(line),
            ));

            // In current: only even-indexed findings survive.
            if f % 2 == 0 {
                current.highlights.push(make_highlight(
                    &sensor_id,
                    &code,
                    Severity::Warn,
                    Some(&fp),
                    Some(&path),
                    Some(line),
                ));
            }
        }
    }

    // Add some brand-new findings to current.
    for i in 0..10 {
        let code = format!("NEW-{i:03}");
        let fp = format!("fp-new-{i}");
        current.highlights.push(make_highlight(
            "sensor-0",
            &code,
            Severity::Error,
            Some(&fp),
            Some("src/new.rs"),
            Some(1000 + i),
        ));
    }

    baseline.verdict.counts.warn = (num_sensors * findings_per_sensor) as u64;
    current.verdict.counts.warn = (num_sensors * (findings_per_sensor / 2)) as u64;
    current.verdict.counts.error = 10;

    let trend = compute_trend(&baseline, &current);

    // Half of each sensor's findings removed = 50 fixed.
    assert_eq!(trend.fixed_findings.len(), 50);
    // 10 new error findings added.
    assert_eq!(trend.new_findings.len(), 10);
    assert_eq!(
        trend.count_deltas.warn_delta,
        -((num_sensors * findings_per_sensor / 2) as i64)
    );
    assert_eq!(trend.count_deltas.error_delta, 10);
    assert!(trend.sensors_added.is_empty());
    assert!(trend.sensors_removed.is_empty());
}

// ── Findings without fingerprint matched by composite key ──────────────────

#[test]
fn no_fingerprint_matched_by_composite_key() {
    let mut baseline = empty_report(VerdictStatus::Warn);
    baseline.verdict.counts.warn = 2;
    baseline.sensors = vec![make_sensor("lint")];
    baseline.highlights = vec![
        make_highlight(
            "lint",
            "W001",
            Severity::Warn,
            None,
            Some("src/a.rs"),
            Some(10),
        ),
        make_highlight(
            "lint",
            "W002",
            Severity::Warn,
            None,
            Some("src/b.rs"),
            Some(20),
        ),
    ];

    // Same composite keys, no fingerprints → should match.
    let mut current = empty_report(VerdictStatus::Warn);
    current.verdict.counts.warn = 2;
    current.sensors = vec![make_sensor("lint")];
    current.highlights = vec![
        make_highlight(
            "lint",
            "W001",
            Severity::Warn,
            None,
            Some("src/a.rs"),
            Some(10),
        ),
        make_highlight(
            "lint",
            "W002",
            Severity::Warn,
            None,
            Some("src/b.rs"),
            Some(20),
        ),
    ];

    let trend = compute_trend(&baseline, &current);
    assert!(trend.new_findings.is_empty());
    assert!(trend.fixed_findings.is_empty());
    assert!(trend.verdict_change.is_none());
    assert_eq!(
        trend.count_deltas,
        CountDeltas {
            info_delta: 0,
            warn_delta: 0,
            error_delta: 0
        }
    );
}

// ── Verdict unchanged but counts differ ────────────────────────────────────

#[test]
fn verdict_unchanged_counts_differ() {
    let mut baseline = empty_report(VerdictStatus::Warn);
    baseline.verdict.counts = VerdictCounts {
        info: 5,
        warn: 3,
        error: 0,
        suppressed: 1,
    };

    let mut current = empty_report(VerdictStatus::Warn);
    current.verdict.counts = VerdictCounts {
        info: 2,
        warn: 7,
        error: 1,
        suppressed: 3,
    };

    let trend = compute_trend(&baseline, &current);
    assert!(trend.verdict_change.is_none());
    assert_eq!(trend.count_deltas.info_delta, -3);
    assert_eq!(trend.count_deltas.warn_delta, 4);
    assert_eq!(trend.count_deltas.error_delta, 1);
}

// ── Multiple sensors: some new, some removed, findings interleaved ─────────

#[test]
fn multi_sensor_mixed_add_remove_findings() {
    let mut baseline = empty_report(VerdictStatus::Fail);
    baseline.verdict.counts = VerdictCounts {
        info: 0,
        warn: 1,
        error: 1,
        suppressed: 0,
    };
    baseline.sensors = vec![make_sensor("lint"), make_sensor("coverage")];
    baseline.highlights = vec![
        make_highlight(
            "lint",
            "L01",
            Severity::Error,
            Some("fp-l01"),
            Some("src/a.rs"),
            Some(1),
        ),
        make_highlight(
            "coverage",
            "COV01",
            Severity::Warn,
            Some("fp-cov01"),
            Some("src/lib.rs"),
            Some(100),
        ),
    ];

    let mut current = empty_report(VerdictStatus::Warn);
    current.verdict.counts = VerdictCounts {
        info: 1,
        warn: 0,
        error: 0,
        suppressed: 0,
    };
    // coverage removed, security added.
    current.sensors = vec![make_sensor("lint"), make_sensor("security")];
    current.highlights = vec![
        // lint/L01 fixed (different fingerprint → new finding).
        make_highlight(
            "security",
            "SEC01",
            Severity::Info,
            Some("fp-sec01"),
            Some("src/auth.rs"),
            Some(42),
        ),
    ];

    let trend = compute_trend(&baseline, &current);
    assert_eq!(trend.sensors_added, vec!["security"]);
    assert_eq!(trend.sensors_removed, vec!["coverage"]);
    assert_eq!(trend.new_findings.len(), 1);
    assert_eq!(trend.new_findings[0].sensor_id, "security");
    assert_eq!(trend.fixed_findings.len(), 2);
    assert_eq!(trend.count_deltas.error_delta, -1);
    assert_eq!(trend.count_deltas.warn_delta, -1);
    assert_eq!(trend.count_deltas.info_delta, 1);
}

// ── Both reports fully empty → zero delta ──────────────────────────────────

#[test]
fn both_reports_empty_zero_delta() {
    let baseline = empty_report(VerdictStatus::Pass);
    let current = empty_report(VerdictStatus::Pass);

    let trend = compute_trend(&baseline, &current);
    assert!(trend.verdict_change.is_none());
    assert_eq!(
        trend.count_deltas,
        CountDeltas {
            info_delta: 0,
            warn_delta: 0,
            error_delta: 0
        }
    );
    assert!(trend.new_findings.is_empty());
    assert!(trend.fixed_findings.is_empty());
    assert!(trend.sensors_added.is_empty());
    assert!(trend.sensors_removed.is_empty());
}

// ── Verdict skip → pass ────────────────────────────────────────────────────

#[test]
fn verdict_skip_to_pass() {
    let baseline = empty_report(VerdictStatus::Skip);
    let current = empty_report(VerdictStatus::Pass);

    let trend = compute_trend(&baseline, &current);
    let vc = trend
        .verdict_change
        .as_ref()
        .expect("verdict should change");
    assert_eq!(vc.before, VerdictStatus::Skip);
    assert_eq!(vc.after, VerdictStatus::Pass);
}

// ── Verdict warn → fail ────────────────────────────────────────────────────

#[test]
fn verdict_warn_to_fail() {
    let mut baseline = empty_report(VerdictStatus::Warn);
    baseline.verdict.counts.warn = 1;

    let mut current = empty_report(VerdictStatus::Fail);
    current.verdict.counts.error = 1;

    let trend = compute_trend(&baseline, &current);
    let vc = trend
        .verdict_change
        .as_ref()
        .expect("verdict should change");
    assert_eq!(vc.before, VerdictStatus::Warn);
    assert_eq!(vc.after, VerdictStatus::Fail);
    assert_eq!(trend.count_deltas.warn_delta, -1);
    assert_eq!(trend.count_deltas.error_delta, 1);
}
