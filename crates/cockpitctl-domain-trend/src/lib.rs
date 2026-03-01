//! Trend-domain boundary microcrate.
//!
//! Computes the delta between a baseline and current cockpit report,
//! identifying new findings, fixed findings, and sensor-level changes.

use std::collections::{BTreeMap, BTreeSet};

use cockpitctl_types::{
    CockpitReport, CountDeltas, Finding, Highlight, TrendDelta, TrendFinding, VerdictChange,
};

/// Index key for matching findings between baseline and current.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct FindingKey {
    sensor_id: String,
    code: String,
    path: String,
    line: u32,
}

fn finding_to_key(sensor_id: &str, f: &Finding) -> FindingKey {
    FindingKey {
        sensor_id: sensor_id.to_string(),
        code: f.code.clone(),
        path: f
            .location
            .as_ref()
            .and_then(|l| l.path.clone())
            .unwrap_or_default(),
        line: f.location.as_ref().and_then(|l| l.line).unwrap_or(0),
    }
}

fn highlight_to_trend_finding(h: &Highlight) -> TrendFinding {
    TrendFinding {
        sensor_id: h.sensor_id.clone(),
        code: h.finding.code.clone(),
        message: h.finding.message.clone(),
        path: h.finding.location.as_ref().and_then(|l| l.path.clone()),
        line: h.finding.location.as_ref().and_then(|l| l.line),
        fingerprint: h.finding.fingerprint.clone(),
        severity: h.finding.severity.clone(),
    }
}

/// Compute the trend delta between a baseline and current cockpit report.
///
/// # Examples
///
/// ```
/// use cockpitctl_domain_trend::compute_trend;
/// use cockpitctl_types::{
///     CockpitReport, PolicySnapshot, RunInfo, ToolInfo,
///     Verdict, VerdictCounts, VerdictStatus,
/// };
/// use std::collections::BTreeMap;
///
/// # fn make_report(status: VerdictStatus) -> CockpitReport {
/// #     CockpitReport {
/// #         schema: "cockpit.report.v1".into(),
/// #         tool: ToolInfo { name: "t".into(), version: "1".into(), commit: None },
/// #         run: RunInfo {
/// #             started_at: "2026-01-01T00:00:00Z".into(),
/// #             ended_at: None, duration_ms: None, host: None,
/// #             git: None, ci: None, capabilities: BTreeMap::new(),
/// #         },
/// #         verdict: Verdict {
/// #             status,
/// #             counts: VerdictCounts { info: 0, warn: 0, error: 0, suppressed: 0 },
/// #             reasons: vec![],
/// #         },
/// #         sensors: vec![],
/// #         highlights: vec![],
/// #         policy: PolicySnapshot {
/// #             warn_is_fail: false, max_highlights: 5,
/// #             max_per_sensor_findings: 20, max_annotations: 10,
/// #             section_order: vec![], sensors: vec![],
/// #         },
/// #         data: None,
/// #     }
/// # }
/// let baseline = make_report(VerdictStatus::Fail);
/// let current = make_report(VerdictStatus::Pass);
/// let trend = compute_trend(&baseline, &current);
/// assert!(trend.verdict_change.is_some());
/// ```
pub fn compute_trend(baseline: &CockpitReport, current: &CockpitReport) -> TrendDelta {
    // Index baseline findings by fingerprint then by composite key.
    let mut baseline_by_fp: BTreeMap<String, &Highlight> = BTreeMap::new();
    let mut baseline_by_key: BTreeMap<FindingKey, &Highlight> = BTreeMap::new();
    for h in &baseline.highlights {
        if let Some(fp) = &h.finding.fingerprint {
            baseline_by_fp.insert(fp.clone(), h);
        }
        let key = finding_to_key(&h.sensor_id, &h.finding);
        baseline_by_key.insert(key, h);
    }

    let mut current_by_fp: BTreeMap<String, &Highlight> = BTreeMap::new();
    let mut current_by_key: BTreeMap<FindingKey, &Highlight> = BTreeMap::new();
    for h in &current.highlights {
        if let Some(fp) = &h.finding.fingerprint {
            current_by_fp.insert(fp.clone(), h);
        }
        let key = finding_to_key(&h.sensor_id, &h.finding);
        current_by_key.insert(key, h);
    }

    // New findings: in current but not in baseline.
    let mut new_findings = Vec::new();
    for h in &current.highlights {
        let matched = if let Some(fp) = &h.finding.fingerprint {
            baseline_by_fp.contains_key(fp)
        } else {
            let key = finding_to_key(&h.sensor_id, &h.finding);
            baseline_by_key.contains_key(&key)
        };
        if !matched {
            new_findings.push(highlight_to_trend_finding(h));
        }
    }

    // Fixed findings: in baseline but not in current.
    let mut fixed_findings = Vec::new();
    for h in &baseline.highlights {
        let matched = if let Some(fp) = &h.finding.fingerprint {
            current_by_fp.contains_key(fp)
        } else {
            let key = finding_to_key(&h.sensor_id, &h.finding);
            current_by_key.contains_key(&key)
        };
        if !matched {
            fixed_findings.push(highlight_to_trend_finding(h));
        }
    }

    // Verdict change.
    let verdict_change = if baseline.verdict.status != current.verdict.status {
        Some(VerdictChange {
            before: baseline.verdict.status.clone(),
            after: current.verdict.status.clone(),
        })
    } else {
        None
    };

    // Count deltas.
    let count_deltas = CountDeltas {
        info_delta: current.verdict.counts.info as i64 - baseline.verdict.counts.info as i64,
        warn_delta: current.verdict.counts.warn as i64 - baseline.verdict.counts.warn as i64,
        error_delta: current.verdict.counts.error as i64 - baseline.verdict.counts.error as i64,
    };

    // Sensors added/removed.
    let baseline_sensors: BTreeSet<String> =
        baseline.sensors.iter().map(|s| s.id.clone()).collect();
    let current_sensors: BTreeSet<String> = current.sensors.iter().map(|s| s.id.clone()).collect();
    let sensors_added: Vec<String> = current_sensors
        .difference(&baseline_sensors)
        .cloned()
        .collect();
    let sensors_removed: Vec<String> = baseline_sensors
        .difference(&current_sensors)
        .cloned()
        .collect();

    TrendDelta {
        verdict_change,
        count_deltas,
        new_findings,
        fixed_findings,
        sensors_added,
        sensors_removed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cockpitctl_types::{
        PolicySensorSnapshot, PolicySnapshot, RunInfo, Severity, ToolInfo, Verdict, VerdictCounts,
        VerdictStatus,
    };
    use std::collections::BTreeMap;

    fn baseline_report() -> CockpitReport {
        CockpitReport {
            schema: "cockpit.report.v1".to_string(),
            tool: ToolInfo {
                name: "baseline".to_string(),
                version: "1.0.0".to_string(),
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
                status: VerdictStatus::Fail,
                counts: VerdictCounts {
                    info: 1,
                    warn: 2,
                    error: 3,
                    suppressed: 0,
                },
                reasons: vec!["baseline".to_string()],
            },
            sensors: vec![cockpitctl_types::SensorSummary {
                id: "builddiag".to_string(),
                blocking: true,
                missing: cockpitctl_types::MissingPolicy::Fail,
                presence: cockpitctl_types::Presence::Present,
                report_path: "artifacts/builddiag/report.json".to_string(),
                comment_path: None,
                verdict: Verdict {
                    status: VerdictStatus::Warn,
                    counts: VerdictCounts {
                        info: 1,
                        warn: 1,
                        error: 0,
                        suppressed: 0,
                    },
                    reasons: vec![],
                },
                truncated: false,
                errors: vec![],
                missing_policy_applied: None,
                policy_outcome: None,
            }],
            highlights: vec![
                Highlight {
                    sensor_id: "builddiag".to_string(),
                    finding: Finding {
                        severity: Severity::Error,
                        check_id: Some("builddiag.error".to_string()),
                        code: "builddiag.err".to_string(),
                        message: "old".to_string(),
                        location: Some(cockpitctl_types::Location {
                            path: Some("src/main.rs".to_string()),
                            line: Some(10),
                            col: None,
                        }),
                        help: None,
                        url: None,
                        fingerprint: Some("fp_old".to_string()),
                        data: None,
                    },
                },
                Highlight {
                    sensor_id: "builddiag".to_string(),
                    finding: Finding {
                        severity: Severity::Warn,
                        check_id: Some("builddiag.warn".to_string()),
                        code: "builddiag.warn".to_string(),
                        message: "legacy".to_string(),
                        location: Some(cockpitctl_types::Location {
                            path: Some("src/legacy.rs".to_string()),
                            line: Some(20),
                            col: None,
                        }),
                        help: None,
                        url: None,
                        fingerprint: Some("fp_legacy".to_string()),
                        data: None,
                    },
                },
            ],
            policy: PolicySnapshot {
                warn_is_fail: true,
                max_highlights: 10,
                max_per_sensor_findings: 10,
                max_annotations: 20,
                section_order: vec!["qa".to_string()],
                sensors: vec![PolicySensorSnapshot {
                    id: "builddiag".to_string(),
                    blocking: true,
                    missing: cockpitctl_types::MissingPolicy::Fail,
                    section: Some("qa".to_string()),
                    require_label: None,
                    repro: None,
                }],
            },
            data: None,
        }
    }

    fn current_report() -> CockpitReport {
        CockpitReport {
            schema: "cockpit.report.v1".to_string(),
            tool: ToolInfo {
                name: "current".to_string(),
                version: "1.0.0".to_string(),
                commit: None,
            },
            run: RunInfo {
                started_at: "2026-01-01T00:00:01Z".to_string(),
                ended_at: None,
                duration_ms: None,
                host: None,
                git: None,
                ci: None,
                capabilities: BTreeMap::new(),
            },
            verdict: Verdict {
                status: VerdictStatus::Pass,
                counts: VerdictCounts {
                    info: 2,
                    warn: 1,
                    error: 3,
                    suppressed: 0,
                },
                reasons: vec!["current".to_string()],
            },
            sensors: vec![cockpitctl_types::SensorSummary {
                id: "builddiag".to_string(),
                blocking: true,
                missing: cockpitctl_types::MissingPolicy::Fail,
                presence: cockpitctl_types::Presence::Present,
                report_path: "artifacts/builddiag/report.json".to_string(),
                comment_path: None,
                verdict: Verdict {
                    status: VerdictStatus::Pass,
                    counts: VerdictCounts {
                        info: 2,
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
            }],
            highlights: vec![
                Highlight {
                    sensor_id: "builddiag".to_string(),
                    finding: Finding {
                        severity: Severity::Error,
                        check_id: Some("builddiag.error".to_string()),
                        code: "builddiag.err".to_string(),
                        message: "new".to_string(),
                        location: Some(cockpitctl_types::Location {
                            path: Some("src/main.rs".to_string()),
                            line: Some(10),
                            col: None,
                        }),
                        help: None,
                        url: None,
                        fingerprint: Some("fp_old".to_string()),
                        data: None,
                    },
                },
                Highlight {
                    sensor_id: "builddiag".to_string(),
                    finding: Finding {
                        severity: Severity::Info,
                        check_id: Some("builddiag.info".to_string()),
                        code: "builddiag.info".to_string(),
                        message: "added".to_string(),
                        location: Some(cockpitctl_types::Location {
                            path: Some("src/main.rs".to_string()),
                            line: Some(55),
                            col: None,
                        }),
                        help: None,
                        url: None,
                        fingerprint: Some("fp_added".to_string()),
                        data: None,
                    },
                },
            ],
            policy: PolicySnapshot {
                warn_is_fail: true,
                max_highlights: 10,
                max_per_sensor_findings: 10,
                max_annotations: 20,
                section_order: vec!["qa".to_string()],
                sensors: vec![PolicySensorSnapshot {
                    id: "builddiag".to_string(),
                    blocking: true,
                    missing: cockpitctl_types::MissingPolicy::Fail,
                    section: Some("qa".to_string()),
                    require_label: None,
                    repro: None,
                }],
            },
            data: None,
        }
    }

    fn trend_inputs() -> (CockpitReport, CockpitReport) {
        (baseline_report(), current_report())
    }

    #[test]
    fn compute_trend_tracks_new_fixed_and_status_change() {
        let (baseline, current) = trend_inputs();
        let trend = compute_trend(&baseline, &current);

        assert_eq!(
            trend.verdict_change.as_ref().map(|c| (&c.before, &c.after)),
            Some((&VerdictStatus::Fail, &VerdictStatus::Pass))
        );
        assert_eq!(
            trend.count_deltas,
            CountDeltas {
                info_delta: 1,
                warn_delta: -1,
                error_delta: 0,
            }
        );
        assert_eq!(trend.new_findings.len(), 1);
        assert_eq!(trend.fixed_findings.len(), 1);
        assert_eq!(trend.new_findings[0].code, "builddiag.info");
        assert_eq!(trend.fixed_findings[0].code, "builddiag.warn");
        assert!(trend.sensors_added.is_empty());
        assert!(trend.sensors_removed.is_empty());
    }

    #[test]
    fn compute_trend_matches_by_fingerprint_when_present() {
        let baseline = current_report();
        let mut current = baseline.clone();
        current.highlights.push(Highlight {
            sensor_id: "builddiag".to_string(),
            finding: Finding {
                severity: Severity::Info,
                check_id: Some("builddiag.info2".to_string()),
                code: "builddiag.info".to_string(),
                message: "changed but same fingerprint".to_string(),
                location: Some(cockpitctl_types::Location {
                    path: Some("src/main.rs".to_string()),
                    line: Some(55),
                    col: None,
                }),
                help: None,
                url: None,
                fingerprint: Some("fp_added".to_string()),
                data: None,
            },
        });

        let trend = compute_trend(&baseline, &current);
        assert_eq!(trend.new_findings.len(), 0);
        assert_eq!(trend.fixed_findings.len(), 0);
        assert_eq!(trend.verdict_change, None);
        assert_eq!(trend.count_deltas, CountDeltas::default());
    }

    /// Helper: build a minimal empty report with the given verdict status.
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

    /// Helper: build a highlight with optional fingerprint and location.
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
                    Some(cockpitctl_types::Location {
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

    fn make_sensor(id: &str) -> cockpitctl_types::SensorSummary {
        cockpitctl_types::SensorSummary {
            id: id.to_string(),
            blocking: true,
            missing: cockpitctl_types::MissingPolicy::Fail,
            presence: cockpitctl_types::Presence::Present,
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

    // ── Edge case: both reports empty ──────────────────────────────────

    #[test]
    fn both_empty_reports_yield_zero_delta() {
        let a = empty_report(VerdictStatus::Pass);
        let b = empty_report(VerdictStatus::Pass);
        let trend = compute_trend(&a, &b);

        assert!(trend.verdict_change.is_none());
        assert_eq!(trend.count_deltas, CountDeltas::default());
        assert!(trend.new_findings.is_empty());
        assert!(trend.fixed_findings.is_empty());
        assert!(trend.sensors_added.is_empty());
        assert!(trend.sensors_removed.is_empty());
    }

    // ── Edge case: empty baseline, all findings are new ────────────────

    #[test]
    fn empty_baseline_means_all_current_findings_are_new() {
        let baseline = empty_report(VerdictStatus::Pass);
        let mut current = empty_report(VerdictStatus::Warn);
        current.verdict.counts.warn = 2;
        current.highlights = vec![
            make_highlight(
                "s1",
                "c1",
                Severity::Warn,
                Some("fp1"),
                Some("a.rs"),
                Some(1),
            ),
            make_highlight("s1", "c2", Severity::Warn, None, Some("b.rs"), Some(2)),
        ];

        let trend = compute_trend(&baseline, &current);
        assert_eq!(trend.new_findings.len(), 2);
        assert!(trend.fixed_findings.is_empty());
        assert_eq!(trend.count_deltas.warn_delta, 2);
    }

    // ── Edge case: empty current, all baseline findings are fixed ──────

    #[test]
    fn empty_current_means_all_baseline_findings_are_fixed() {
        let mut baseline = empty_report(VerdictStatus::Fail);
        baseline.verdict.counts.error = 1;
        baseline.highlights = vec![make_highlight(
            "s1",
            "c1",
            Severity::Error,
            Some("fp1"),
            Some("a.rs"),
            Some(1),
        )];
        let current = empty_report(VerdictStatus::Pass);

        let trend = compute_trend(&baseline, &current);
        assert!(trend.new_findings.is_empty());
        assert_eq!(trend.fixed_findings.len(), 1);
        assert_eq!(trend.fixed_findings[0].code, "c1");
        assert_eq!(trend.count_deltas.error_delta, -1);
    }

    // ── Identical reports ──────────────────────────────────────────────

    #[test]
    fn identical_reports_produce_zero_delta() {
        let report = baseline_report();
        let trend = compute_trend(&report, &report);

        assert!(trend.verdict_change.is_none());
        assert_eq!(trend.count_deltas, CountDeltas::default());
        assert!(trend.new_findings.is_empty());
        assert!(trend.fixed_findings.is_empty());
        assert!(trend.sensors_added.is_empty());
        assert!(trend.sensors_removed.is_empty());
    }

    // ── Verdict unchanged ──────────────────────────────────────────────

    #[test]
    fn same_verdict_status_yields_no_verdict_change() {
        let mut a = empty_report(VerdictStatus::Warn);
        a.verdict.counts.warn = 3;
        let mut b = empty_report(VerdictStatus::Warn);
        b.verdict.counts.warn = 5;

        let trend = compute_trend(&a, &b);
        assert!(trend.verdict_change.is_none());
        assert_eq!(trend.count_deltas.warn_delta, 2);
    }

    // ── Verdict change from pass to fail ───────────────────────────────

    #[test]
    fn verdict_change_pass_to_fail() {
        let a = empty_report(VerdictStatus::Pass);
        let b = empty_report(VerdictStatus::Fail);

        let trend = compute_trend(&a, &b);
        let vc = trend.verdict_change.unwrap();
        assert_eq!(vc.before, VerdictStatus::Pass);
        assert_eq!(vc.after, VerdictStatus::Fail);
    }

    // ── Sensors added/removed ──────────────────────────────────────────

    #[test]
    fn detects_sensors_added() {
        let mut baseline = empty_report(VerdictStatus::Pass);
        baseline.sensors = vec![make_sensor("alpha")];

        let mut current = empty_report(VerdictStatus::Pass);
        current.sensors = vec![make_sensor("alpha"), make_sensor("beta")];

        let trend = compute_trend(&baseline, &current);
        assert_eq!(trend.sensors_added, vec!["beta"]);
        assert!(trend.sensors_removed.is_empty());
    }

    #[test]
    fn detects_sensors_removed() {
        let mut baseline = empty_report(VerdictStatus::Pass);
        baseline.sensors = vec![make_sensor("alpha"), make_sensor("beta")];

        let mut current = empty_report(VerdictStatus::Pass);
        current.sensors = vec![make_sensor("alpha")];

        let trend = compute_trend(&baseline, &current);
        assert!(trend.sensors_added.is_empty());
        assert_eq!(trend.sensors_removed, vec!["beta"]);
    }

    #[test]
    fn detects_sensors_added_and_removed_simultaneously() {
        let mut baseline = empty_report(VerdictStatus::Pass);
        baseline.sensors = vec![make_sensor("a"), make_sensor("b")];

        let mut current = empty_report(VerdictStatus::Pass);
        current.sensors = vec![make_sensor("b"), make_sensor("c")];

        let trend = compute_trend(&baseline, &current);
        assert_eq!(trend.sensors_added, vec!["c"]);
        assert_eq!(trend.sensors_removed, vec!["a"]);
    }

    // ── Finding matching by composite key (no fingerprint) ─────────────

    #[test]
    fn matches_findings_by_composite_key_when_no_fingerprint() {
        let mut baseline = empty_report(VerdictStatus::Pass);
        baseline.highlights = vec![make_highlight(
            "lint",
            "W001",
            Severity::Warn,
            None,
            Some("src/lib.rs"),
            Some(42),
        )];

        // Same sensor_id + code + path + line → matched, not new/fixed.
        let mut current = empty_report(VerdictStatus::Pass);
        current.highlights = vec![make_highlight(
            "lint",
            "W001",
            Severity::Warn,
            None,
            Some("src/lib.rs"),
            Some(42),
        )];

        let trend = compute_trend(&baseline, &current);
        assert!(trend.new_findings.is_empty());
        assert!(trend.fixed_findings.is_empty());
    }

    #[test]
    fn different_line_means_different_finding_by_key() {
        let mut baseline = empty_report(VerdictStatus::Pass);
        baseline.highlights = vec![make_highlight(
            "lint",
            "W001",
            Severity::Warn,
            None,
            Some("src/lib.rs"),
            Some(42),
        )];

        let mut current = empty_report(VerdictStatus::Pass);
        current.highlights = vec![make_highlight(
            "lint",
            "W001",
            Severity::Warn,
            None,
            Some("src/lib.rs"),
            Some(99),
        )];

        let trend = compute_trend(&baseline, &current);
        assert_eq!(trend.new_findings.len(), 1);
        assert_eq!(trend.fixed_findings.len(), 1);
    }

    #[test]
    fn different_sensor_id_means_different_finding_by_key() {
        let mut baseline = empty_report(VerdictStatus::Pass);
        baseline.highlights = vec![make_highlight(
            "lint-a",
            "W001",
            Severity::Warn,
            None,
            Some("f.rs"),
            Some(1),
        )];

        let mut current = empty_report(VerdictStatus::Pass);
        current.highlights = vec![make_highlight(
            "lint-b",
            "W001",
            Severity::Warn,
            None,
            Some("f.rs"),
            Some(1),
        )];

        let trend = compute_trend(&baseline, &current);
        assert_eq!(trend.new_findings.len(), 1);
        assert_eq!(trend.fixed_findings.len(), 1);
    }

    // ── Fingerprint takes precedence over key ──────────────────────────

    #[test]
    fn fingerprint_match_overrides_key_mismatch() {
        let mut baseline = empty_report(VerdictStatus::Pass);
        baseline.highlights = vec![make_highlight(
            "s",
            "code-old",
            Severity::Error,
            Some("fp-stable"),
            Some("old.rs"),
            Some(10),
        )];

        // Different key fields but same fingerprint → still matched.
        let mut current = empty_report(VerdictStatus::Pass);
        current.highlights = vec![make_highlight(
            "s",
            "code-new",
            Severity::Error,
            Some("fp-stable"),
            Some("new.rs"),
            Some(20),
        )];

        let trend = compute_trend(&baseline, &current);
        assert!(trend.new_findings.is_empty());
        assert!(trend.fixed_findings.is_empty());
    }

    // ── Severity change detection ──────────────────────────────────────

    #[test]
    fn severity_change_on_same_fingerprint_is_not_new_or_fixed() {
        let mut baseline = empty_report(VerdictStatus::Warn);
        baseline.verdict.counts.warn = 1;
        baseline.highlights = vec![make_highlight(
            "s",
            "C1",
            Severity::Warn,
            Some("fp1"),
            Some("a.rs"),
            Some(1),
        )];

        let mut current = empty_report(VerdictStatus::Fail);
        current.verdict.counts.error = 1;
        current.highlights = vec![make_highlight(
            "s",
            "C1",
            Severity::Error,
            Some("fp1"),
            Some("a.rs"),
            Some(1),
        )];

        let trend = compute_trend(&baseline, &current);
        // Same fingerprint → matched, so neither new nor fixed.
        assert!(trend.new_findings.is_empty());
        assert!(trend.fixed_findings.is_empty());
        // But counts changed.
        assert_eq!(trend.count_deltas.warn_delta, -1);
        assert_eq!(trend.count_deltas.error_delta, 1);
    }

    // ── Count deltas: negative deltas ──────────────────────────────────

    #[test]
    fn count_deltas_negative_when_counts_decrease() {
        let mut baseline = empty_report(VerdictStatus::Fail);
        baseline.verdict.counts = VerdictCounts {
            info: 10,
            warn: 20,
            error: 30,
            suppressed: 0,
        };
        let mut current = empty_report(VerdictStatus::Pass);
        current.verdict.counts = VerdictCounts {
            info: 3,
            warn: 5,
            error: 0,
            suppressed: 0,
        };

        let trend = compute_trend(&baseline, &current);
        assert_eq!(trend.count_deltas.info_delta, -7);
        assert_eq!(trend.count_deltas.warn_delta, -15);
        assert_eq!(trend.count_deltas.error_delta, -30);
    }

    // ── Findings without location ──────────────────────────────────────

    #[test]
    fn findings_without_location_use_default_key_values() {
        let mut baseline = empty_report(VerdictStatus::Pass);
        baseline.highlights = vec![make_highlight("s", "C1", Severity::Info, None, None, None)];

        let mut current = empty_report(VerdictStatus::Pass);
        current.highlights = vec![make_highlight("s", "C1", Severity::Info, None, None, None)];

        let trend = compute_trend(&baseline, &current);
        assert!(trend.new_findings.is_empty());
        assert!(trend.fixed_findings.is_empty());
    }

    #[test]
    fn no_location_vs_location_is_different_key() {
        let mut baseline = empty_report(VerdictStatus::Pass);
        baseline.highlights = vec![make_highlight("s", "C1", Severity::Info, None, None, None)];

        let mut current = empty_report(VerdictStatus::Pass);
        current.highlights = vec![make_highlight(
            "s",
            "C1",
            Severity::Info,
            None,
            Some("a.rs"),
            Some(1),
        )];

        let trend = compute_trend(&baseline, &current);
        assert_eq!(trend.new_findings.len(), 1);
        assert_eq!(trend.fixed_findings.len(), 1);
    }

    // ── highlight_to_trend_finding correctness ─────────────────────────

    #[test]
    fn highlight_to_trend_finding_maps_all_fields() {
        let h = make_highlight(
            "sensor-x",
            "ERR42",
            Severity::Error,
            Some("fp-abc"),
            Some("src/foo.rs"),
            Some(100),
        );
        let tf = highlight_to_trend_finding(&h);

        assert_eq!(tf.sensor_id, "sensor-x");
        assert_eq!(tf.code, "ERR42");
        assert_eq!(tf.message, "ERR42 message");
        assert_eq!(tf.path, Some("src/foo.rs".to_string()));
        assert_eq!(tf.line, Some(100));
        assert_eq!(tf.fingerprint, Some("fp-abc".to_string()));
        assert_eq!(tf.severity, Severity::Error);
    }

    #[test]
    fn highlight_to_trend_finding_handles_no_location() {
        let h = make_highlight("s", "C", Severity::Info, None, None, None);
        let tf = highlight_to_trend_finding(&h);

        assert_eq!(tf.path, None);
        assert_eq!(tf.line, None);
        assert_eq!(tf.fingerprint, None);
    }

    // ── finding_to_key correctness ─────────────────────────────────────

    #[test]
    fn finding_to_key_captures_sensor_code_path_line() {
        let finding = Finding {
            severity: Severity::Warn,
            check_id: None,
            code: "W1".to_string(),
            message: "msg".to_string(),
            location: Some(cockpitctl_types::Location {
                path: Some("p.rs".to_string()),
                line: Some(7),
                col: Some(3),
            }),
            help: None,
            url: None,
            fingerprint: None,
            data: None,
        };
        let key = finding_to_key("sensor-a", &finding);
        assert_eq!(key.sensor_id, "sensor-a");
        assert_eq!(key.code, "W1");
        assert_eq!(key.path, "p.rs");
        assert_eq!(key.line, 7);
    }

    #[test]
    fn finding_to_key_defaults_for_missing_location() {
        let finding = Finding {
            severity: Severity::Info,
            check_id: None,
            code: "C".to_string(),
            message: "m".to_string(),
            location: None,
            help: None,
            url: None,
            fingerprint: None,
            data: None,
        };
        let key = finding_to_key("s", &finding);
        assert_eq!(key.path, "");
        assert_eq!(key.line, 0);
    }

    // ── Multiple sensors with interleaved findings ─────────────────────

    #[test]
    fn multiple_sensors_new_and_fixed() {
        let mut baseline = empty_report(VerdictStatus::Fail);
        baseline.sensors = vec![make_sensor("lint"), make_sensor("sec")];
        baseline.highlights = vec![
            make_highlight(
                "lint",
                "L1",
                Severity::Warn,
                Some("fp-l1"),
                Some("a.rs"),
                Some(1),
            ),
            make_highlight(
                "sec",
                "S1",
                Severity::Error,
                Some("fp-s1"),
                Some("b.rs"),
                Some(2),
            ),
        ];
        baseline.verdict.counts = VerdictCounts {
            info: 0,
            warn: 1,
            error: 1,
            suppressed: 0,
        };

        let mut current = empty_report(VerdictStatus::Pass);
        current.sensors = vec![make_sensor("lint"), make_sensor("sec")];
        current.highlights = vec![
            // lint/L1 still there (same fingerprint).
            make_highlight(
                "lint",
                "L1",
                Severity::Warn,
                Some("fp-l1"),
                Some("a.rs"),
                Some(1),
            ),
            // sec/S1 is gone, sec/S2 is new.
            make_highlight(
                "sec",
                "S2",
                Severity::Info,
                Some("fp-s2"),
                Some("c.rs"),
                Some(3),
            ),
        ];
        current.verdict.counts = VerdictCounts {
            info: 1,
            warn: 1,
            error: 0,
            suppressed: 0,
        };

        let trend = compute_trend(&baseline, &current);
        assert_eq!(trend.new_findings.len(), 1);
        assert_eq!(trend.new_findings[0].code, "S2");
        assert_eq!(trend.new_findings[0].sensor_id, "sec");
        assert_eq!(trend.fixed_findings.len(), 1);
        assert_eq!(trend.fixed_findings[0].code, "S1");
        assert_eq!(trend.fixed_findings[0].sensor_id, "sec");
        assert_eq!(trend.count_deltas.error_delta, -1);
        assert_eq!(trend.count_deltas.info_delta, 1);
    }

    // ── Degrading trend (more errors) ──────────────────────────────────

    #[test]
    fn degrading_trend_shows_positive_error_delta_and_new_findings() {
        let mut baseline = empty_report(VerdictStatus::Pass);
        baseline.verdict.counts = VerdictCounts {
            info: 0,
            warn: 0,
            error: 0,
            suppressed: 0,
        };

        let mut current = empty_report(VerdictStatus::Fail);
        current.verdict.counts = VerdictCounts {
            info: 0,
            warn: 1,
            error: 2,
            suppressed: 0,
        };
        current.highlights = vec![
            make_highlight("s", "E1", Severity::Error, None, Some("a.rs"), Some(1)),
            make_highlight("s", "E2", Severity::Error, None, Some("b.rs"), Some(2)),
            make_highlight("s", "W1", Severity::Warn, None, Some("c.rs"), Some(3)),
        ];

        let trend = compute_trend(&baseline, &current);
        assert_eq!(trend.new_findings.len(), 3);
        assert_eq!(trend.count_deltas.error_delta, 2);
        assert_eq!(trend.count_deltas.warn_delta, 1);
        let vc = trend.verdict_change.unwrap();
        assert_eq!(vc.before, VerdictStatus::Pass);
        assert_eq!(vc.after, VerdictStatus::Fail);
    }

    // ── Improving trend (fewer errors) ─────────────────────────────────

    #[test]
    fn improving_trend_shows_negative_error_delta_and_fixed_findings() {
        let mut baseline = empty_report(VerdictStatus::Fail);
        baseline.verdict.counts = VerdictCounts {
            info: 0,
            warn: 0,
            error: 3,
            suppressed: 0,
        };
        baseline.highlights = vec![
            make_highlight("s", "E1", Severity::Error, None, Some("a.rs"), Some(1)),
            make_highlight("s", "E2", Severity::Error, None, Some("b.rs"), Some(2)),
            make_highlight("s", "E3", Severity::Error, None, Some("c.rs"), Some(3)),
        ];

        let mut current = empty_report(VerdictStatus::Pass);
        current.verdict.counts = VerdictCounts {
            info: 0,
            warn: 0,
            error: 0,
            suppressed: 0,
        };

        let trend = compute_trend(&baseline, &current);
        assert_eq!(trend.fixed_findings.len(), 3);
        assert!(trend.new_findings.is_empty());
        assert_eq!(trend.count_deltas.error_delta, -3);
    }

    // ── Stable trend (same counts, same findings) ──────────────────────

    #[test]
    fn stable_trend_no_changes() {
        let mut report = empty_report(VerdictStatus::Warn);
        report.verdict.counts = VerdictCounts {
            info: 1,
            warn: 1,
            error: 0,
            suppressed: 0,
        };
        report.sensors = vec![make_sensor("x")];
        report.highlights = vec![
            make_highlight(
                "x",
                "I1",
                Severity::Info,
                Some("fp-i"),
                Some("a.rs"),
                Some(1),
            ),
            make_highlight(
                "x",
                "W1",
                Severity::Warn,
                Some("fp-w"),
                Some("b.rs"),
                Some(2),
            ),
        ];

        let trend = compute_trend(&report, &report);
        assert!(trend.verdict_change.is_none());
        assert_eq!(trend.count_deltas, CountDeltas::default());
        assert!(trend.new_findings.is_empty());
        assert!(trend.fixed_findings.is_empty());
        assert!(trend.sensors_added.is_empty());
        assert!(trend.sensors_removed.is_empty());
    }

    // ── Duplicate fingerprints within a single report ──────────────────

    #[test]
    fn duplicate_fingerprints_in_current_only_last_indexed() {
        let baseline = empty_report(VerdictStatus::Pass);
        let mut current = empty_report(VerdictStatus::Pass);
        // Two highlights with the same fingerprint in current.
        current.highlights = vec![
            make_highlight(
                "s",
                "A",
                Severity::Warn,
                Some("dup-fp"),
                Some("a.rs"),
                Some(1),
            ),
            make_highlight(
                "s",
                "B",
                Severity::Error,
                Some("dup-fp"),
                Some("b.rs"),
                Some(2),
            ),
        ];

        let trend = compute_trend(&baseline, &current);
        // Both are new since baseline is empty.
        assert_eq!(trend.new_findings.len(), 2);
    }

    // ── All verdict status transitions ─────────────────────────────────

    #[test]
    fn verdict_skip_to_warn() {
        let a = empty_report(VerdictStatus::Skip);
        let b = empty_report(VerdictStatus::Warn);
        let trend = compute_trend(&a, &b);
        let vc = trend.verdict_change.unwrap();
        assert_eq!(vc.before, VerdictStatus::Skip);
        assert_eq!(vc.after, VerdictStatus::Warn);
    }

    #[test]
    fn verdict_warn_to_pass() {
        let a = empty_report(VerdictStatus::Warn);
        let b = empty_report(VerdictStatus::Pass);
        let trend = compute_trend(&a, &b);
        let vc = trend.verdict_change.unwrap();
        assert_eq!(vc.before, VerdictStatus::Warn);
        assert_eq!(vc.after, VerdictStatus::Pass);
    }

    // ── finding_to_key: partial location (path only, line only) ────────

    #[test]
    fn finding_to_key_with_path_but_no_line() {
        let finding = Finding {
            severity: Severity::Info,
            check_id: None,
            code: "X".to_string(),
            message: "m".to_string(),
            location: Some(cockpitctl_types::Location {
                path: Some("f.rs".to_string()),
                line: None,
                col: None,
            }),
            help: None,
            url: None,
            fingerprint: None,
            data: None,
        };
        let key = finding_to_key("s", &finding);
        assert_eq!(key.path, "f.rs");
        assert_eq!(key.line, 0);
    }

    #[test]
    fn finding_to_key_with_line_but_no_path() {
        let finding = Finding {
            severity: Severity::Info,
            check_id: None,
            code: "Y".to_string(),
            message: "m".to_string(),
            location: Some(cockpitctl_types::Location {
                path: None,
                line: Some(99),
                col: None,
            }),
            help: None,
            url: None,
            fingerprint: None,
            data: None,
        };
        let key = finding_to_key("s", &finding);
        assert_eq!(key.path, "");
        assert_eq!(key.line, 99);
    }

    // ── Mixed fingerprint and key-based matching ───────────────────────

    #[test]
    fn mixed_fingerprint_and_key_matching() {
        let mut baseline = empty_report(VerdictStatus::Pass);
        baseline.highlights = vec![
            // Has fingerprint — matched by fp.
            make_highlight(
                "s",
                "A",
                Severity::Warn,
                Some("fp-a"),
                Some("x.rs"),
                Some(1),
            ),
            // No fingerprint — matched by key.
            make_highlight("s", "B", Severity::Info, None, Some("y.rs"), Some(2)),
            // Will be fixed.
            make_highlight(
                "s",
                "C",
                Severity::Error,
                Some("fp-c"),
                Some("z.rs"),
                Some(3),
            ),
        ];

        let mut current = empty_report(VerdictStatus::Pass);
        current.highlights = vec![
            // Same fp as A — matched.
            make_highlight(
                "s",
                "A",
                Severity::Warn,
                Some("fp-a"),
                Some("x.rs"),
                Some(1),
            ),
            // Same key as B — matched.
            make_highlight("s", "B", Severity::Info, None, Some("y.rs"), Some(2)),
            // New finding.
            make_highlight(
                "s",
                "D",
                Severity::Warn,
                Some("fp-d"),
                Some("w.rs"),
                Some(4),
            ),
        ];

        let trend = compute_trend(&baseline, &current);
        assert_eq!(trend.new_findings.len(), 1);
        assert_eq!(trend.new_findings[0].code, "D");
        assert_eq!(trend.fixed_findings.len(), 1);
        assert_eq!(trend.fixed_findings[0].code, "C");
    }
}
