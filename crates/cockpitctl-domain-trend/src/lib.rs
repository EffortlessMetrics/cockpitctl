//! Trend-domain boundary microcrate.

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
}
