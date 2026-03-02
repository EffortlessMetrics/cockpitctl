//! Property-based, edge-case, and snapshot tests for cockpitctl-domain-trend.

use cockpitctl_domain_trend::compute_trend;
use cockpitctl_types::{
    CockpitReport, Finding, Highlight, Location, MissingPolicy, PolicySnapshot, Presence, RunInfo,
    SensorSummary, Severity, ToolInfo, Verdict, VerdictCounts, VerdictStatus,
};
use proptest::prelude::*;
use std::collections::BTreeMap;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_report(
    status: VerdictStatus,
    counts: VerdictCounts,
    highlights: Vec<Highlight>,
    sensors: Vec<SensorSummary>,
) -> CockpitReport {
    CockpitReport {
        schema: "cockpit.report.v1".into(),
        tool: ToolInfo {
            name: "test".into(),
            version: "1.0.0".into(),
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
            counts,
            reasons: vec![],
        },
        sensors,
        highlights,
        policy: PolicySnapshot {
            warn_is_fail: false,
            max_highlights: 10,
            max_per_sensor_findings: 20,
            max_annotations: 10,
            section_order: vec![],
            sensors: vec![],
        },
        data: None,
    }
}

fn empty_report(status: VerdictStatus) -> CockpitReport {
    make_report(status, VerdictCounts::default(), vec![], vec![])
}

fn make_highlight(sensor: &str, code: &str, msg: &str, fp: Option<&str>) -> Highlight {
    Highlight {
        sensor_id: sensor.into(),
        finding: Finding {
            severity: Severity::Error,
            check_id: None,
            code: code.into(),
            message: msg.into(),
            location: Some(Location {
                path: Some("src/main.rs".into()),
                line: Some(1),
                col: None,
            }),
            help: None,
            url: None,
            fingerprint: fp.map(String::from),
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
            counts: VerdictCounts::default(),
            reasons: vec![],
        },
        truncated: false,
        errors: vec![],
        missing_policy_applied: None,
        policy_outcome: None,
    }
}

// ---------------------------------------------------------------------------
// Proptest strategies
// ---------------------------------------------------------------------------

fn arb_verdict_status() -> impl Strategy<Value = VerdictStatus> {
    prop_oneof![
        Just(VerdictStatus::Pass),
        Just(VerdictStatus::Warn),
        Just(VerdictStatus::Fail),
        Just(VerdictStatus::Skip),
    ]
}

fn arb_counts() -> impl Strategy<Value = VerdictCounts> {
    (0u64..100, 0u64..100, 0u64..100, 0u64..50).prop_map(|(i, w, e, s)| VerdictCounts {
        info: i,
        warn: w,
        error: e,
        suppressed: s,
    })
}

fn arb_severity() -> impl Strategy<Value = Severity> {
    prop_oneof![
        Just(Severity::Info),
        Just(Severity::Warn),
        Just(Severity::Error),
    ]
}

fn arb_highlight() -> impl Strategy<Value = Highlight> {
    (
        "[a-z]{3,8}",
        "[a-z.]{3,12}",
        "[a-z ]{5,20}",
        arb_severity(),
        proptest::option::of("[a-f0-9]{8}"),
        proptest::option::of("[a-z/]{3,15}"),
        proptest::option::of(1u32..500),
    )
        .prop_map(|(sensor, code, msg, sev, fp, path, line)| Highlight {
            sensor_id: sensor,
            finding: Finding {
                severity: sev,
                check_id: None,
                code,
                message: msg,
                location: Some(Location {
                    path,
                    line,
                    col: None,
                }),
                help: None,
                url: None,
                fingerprint: fp,
                data: None,
            },
        })
}

// ---------------------------------------------------------------------------
// Proptest: identical reports produce empty delta
// ---------------------------------------------------------------------------

proptest! {
    #[test]
    fn self_diff_yields_zero_delta(
        status in arb_verdict_status(),
        counts in arb_counts(),
    ) {
        let r = make_report(status, counts, vec![], vec![]);
        let delta = compute_trend(&r, &r);
        prop_assert!(delta.verdict_change.is_none());
        prop_assert!(delta.new_findings.is_empty());
        prop_assert!(delta.fixed_findings.is_empty());
        prop_assert!(delta.sensors_added.is_empty());
        prop_assert!(delta.sensors_removed.is_empty());
        prop_assert_eq!(delta.count_deltas.info_delta, 0);
        prop_assert_eq!(delta.count_deltas.warn_delta, 0);
        prop_assert_eq!(delta.count_deltas.error_delta, 0);
    }

    #[test]
    fn count_deltas_are_exact_difference(
        base_counts in arb_counts(),
        curr_counts in arb_counts(),
    ) {
        let base = make_report(VerdictStatus::Pass, base_counts.clone(), vec![], vec![]);
        let curr = make_report(VerdictStatus::Pass, curr_counts.clone(), vec![], vec![]);
        let delta = compute_trend(&base, &curr);
        prop_assert_eq!(
            delta.count_deltas.info_delta,
            curr_counts.info as i64 - base_counts.info as i64
        );
        prop_assert_eq!(
            delta.count_deltas.warn_delta,
            curr_counts.warn as i64 - base_counts.warn as i64
        );
        prop_assert_eq!(
            delta.count_deltas.error_delta,
            curr_counts.error as i64 - base_counts.error as i64
        );
    }

    #[test]
    fn verdict_change_iff_statuses_differ(
        base_status in arb_verdict_status(),
        curr_status in arb_verdict_status(),
    ) {
        let base = empty_report(base_status.clone());
        let curr = empty_report(curr_status.clone());
        let delta = compute_trend(&base, &curr);
        if base_status == curr_status {
            prop_assert!(delta.verdict_change.is_none());
        } else {
            let vc = delta.verdict_change.as_ref().unwrap();
            prop_assert_eq!(&vc.before, &base_status);
            prop_assert_eq!(&vc.after, &curr_status);
        }
    }

    #[test]
    fn empty_current_all_fixed(
        highlights in proptest::collection::vec(arb_highlight(), 0..6),
    ) {
        let base = make_report(VerdictStatus::Fail, VerdictCounts::default(), highlights.clone(), vec![]);
        let curr = make_report(VerdictStatus::Pass, VerdictCounts::default(), vec![], vec![]);
        let delta = compute_trend(&base, &curr);
        prop_assert!(delta.new_findings.is_empty());
        prop_assert_eq!(delta.fixed_findings.len(), highlights.len());
    }

    #[test]
    fn empty_baseline_all_new(
        highlights in proptest::collection::vec(arb_highlight(), 0..6),
    ) {
        let base = make_report(VerdictStatus::Pass, VerdictCounts::default(), vec![], vec![]);
        let curr = make_report(VerdictStatus::Fail, VerdictCounts::default(), highlights.clone(), vec![]);
        let delta = compute_trend(&base, &curr);
        prop_assert_eq!(delta.new_findings.len(), highlights.len());
        prop_assert!(delta.fixed_findings.is_empty());
    }

    #[test]
    fn sensors_added_removed_are_disjoint(
        added in proptest::collection::vec("[a-z]{3,8}", 0..4),
        removed in proptest::collection::vec("[a-z]{3,8}", 0..4),
    ) {
        let base_sensors: Vec<SensorSummary> = removed.iter().map(|id| make_sensor(id)).collect();
        let curr_sensors: Vec<SensorSummary> = added.iter().map(|id| make_sensor(id)).collect();
        let base = make_report(VerdictStatus::Pass, VerdictCounts::default(), vec![], base_sensors);
        let curr = make_report(VerdictStatus::Pass, VerdictCounts::default(), vec![], curr_sensors);
        let delta = compute_trend(&base, &curr);
        for s in &delta.sensors_added {
            prop_assert!(!delta.sensors_removed.contains(s),
                "sensor {} appears in both added and removed", s);
        }
    }
}

// ---------------------------------------------------------------------------
// Edge-case: fingerprint matching takes priority over composite key
// ---------------------------------------------------------------------------

#[test]
fn same_key_different_fingerprints_are_matched_by_fingerprint() {
    let h_base = Highlight {
        sensor_id: "s".into(),
        finding: Finding {
            severity: Severity::Error,
            check_id: None,
            code: "c".into(),
            message: "m".into(),
            location: Some(Location {
                path: Some("a.rs".into()),
                line: Some(1),
                col: None,
            }),
            help: None,
            url: None,
            fingerprint: Some("fp1".into()),
            data: None,
        },
    };

    // Same fingerprint, different code/path/line - still matched
    let h_curr = Highlight {
        sensor_id: "s".into(),
        finding: Finding {
            severity: Severity::Warn,
            check_id: None,
            code: "different".into(),
            message: "changed".into(),
            location: Some(Location {
                path: Some("b.rs".into()),
                line: Some(99),
                col: None,
            }),
            help: None,
            url: None,
            fingerprint: Some("fp1".into()),
            data: None,
        },
    };

    let base = make_report(
        VerdictStatus::Fail,
        VerdictCounts::default(),
        vec![h_base],
        vec![],
    );
    let curr = make_report(
        VerdictStatus::Warn,
        VerdictCounts::default(),
        vec![h_curr],
        vec![],
    );
    let delta = compute_trend(&base, &curr);
    assert!(delta.new_findings.is_empty());
    assert!(delta.fixed_findings.is_empty());
}

// ---------------------------------------------------------------------------
// Edge-case: asymmetric fp matching (baseline has fp, current doesn't)
// ---------------------------------------------------------------------------

#[test]
fn baseline_fp_current_no_fp_asymmetric_match() {
    let h_base = make_highlight("sensor", "code1", "msg", Some("fp_x"));
    let h_curr = make_highlight("sensor", "code1", "msg", None);

    let base = make_report(
        VerdictStatus::Fail,
        VerdictCounts::default(),
        vec![h_base],
        vec![],
    );
    let curr = make_report(
        VerdictStatus::Fail,
        VerdictCounts::default(),
        vec![h_curr],
        vec![],
    );
    let delta = compute_trend(&base, &curr);
    // current has no fp -> falls back to composite key -> matches baseline -> NOT new
    assert!(delta.new_findings.is_empty());
    // baseline has fp "fp_x" -> checks current_by_fp -> not found -> IS fixed
    assert_eq!(delta.fixed_findings.len(), 1);
}

// ---------------------------------------------------------------------------
// Edge-case: deterministic across calls
// ---------------------------------------------------------------------------

#[test]
fn deterministic_across_calls() {
    let base = make_report(
        VerdictStatus::Fail,
        VerdictCounts {
            info: 5,
            warn: 3,
            error: 2,
            suppressed: 0,
        },
        vec![
            make_highlight("a", "c1", "m1", Some("fp1")),
            make_highlight("b", "c2", "m2", None),
        ],
        vec![make_sensor("a"), make_sensor("b")],
    );
    let curr = make_report(
        VerdictStatus::Pass,
        VerdictCounts {
            info: 1,
            warn: 0,
            error: 0,
            suppressed: 0,
        },
        vec![make_highlight("b", "c2", "m2", None)],
        vec![make_sensor("b"), make_sensor("c")],
    );
    let d1 = compute_trend(&base, &curr);
    let d2 = compute_trend(&base, &curr);
    assert_eq!(format!("{d1:?}"), format!("{d2:?}"));
}

// ---------------------------------------------------------------------------
// Edge-case: new + fixed consistent with inputs
// ---------------------------------------------------------------------------

#[test]
fn new_plus_fixed_consistent_with_baseline_and_current() {
    let base_h = vec![
        make_highlight("s", "c1", "m1", Some("fp_a")),
        make_highlight("s", "c2", "m2", Some("fp_b")),
        make_highlight("s", "c3", "m3", None),
    ];
    let curr_h = vec![
        make_highlight("s", "c1", "m1", Some("fp_a")), // retained
        make_highlight("s", "c4", "m4", Some("fp_d")), // new
        make_highlight("s", "c5", "m5", None),         // new
    ];
    let base = make_report(
        VerdictStatus::Fail,
        VerdictCounts::default(),
        base_h,
        vec![],
    );
    let curr = make_report(
        VerdictStatus::Fail,
        VerdictCounts::default(),
        curr_h,
        vec![],
    );
    let delta = compute_trend(&base, &curr);
    assert_eq!(delta.new_findings.len(), 2, "two new findings");
    assert_eq!(delta.fixed_findings.len(), 2, "two fixed findings");
}

// ---------------------------------------------------------------------------
// Edge-case: complete sensor swap
// ---------------------------------------------------------------------------

#[test]
fn complete_sensor_swap() {
    let base = make_report(
        VerdictStatus::Pass,
        VerdictCounts::default(),
        vec![],
        vec![make_sensor("alpha"), make_sensor("beta")],
    );
    let curr = make_report(
        VerdictStatus::Pass,
        VerdictCounts::default(),
        vec![],
        vec![make_sensor("gamma"), make_sensor("delta")],
    );
    let delta = compute_trend(&base, &curr);
    assert_eq!(delta.sensors_added, vec!["delta", "gamma"]);
    assert_eq!(delta.sensors_removed, vec!["alpha", "beta"]);
}

#[test]
fn single_sensor_change_among_many_stable() {
    let base = make_report(
        VerdictStatus::Pass,
        VerdictCounts::default(),
        vec![],
        vec![make_sensor("a"), make_sensor("b"), make_sensor("c")],
    );
    let curr = make_report(
        VerdictStatus::Pass,
        VerdictCounts::default(),
        vec![],
        vec![make_sensor("a"), make_sensor("b"), make_sensor("d")],
    );
    let delta = compute_trend(&base, &curr);
    assert_eq!(delta.sensors_added, vec!["d"]);
    assert_eq!(delta.sensors_removed, vec!["c"]);
}

#[test]
fn all_sensors_new() {
    let base = empty_report(VerdictStatus::Pass);
    let curr = make_report(
        VerdictStatus::Pass,
        VerdictCounts::default(),
        vec![],
        vec![make_sensor("x"), make_sensor("y")],
    );
    let delta = compute_trend(&base, &curr);
    assert_eq!(delta.sensors_added, vec!["x", "y"]);
    assert!(delta.sensors_removed.is_empty());
}

#[test]
fn all_sensors_removed() {
    let base = make_report(
        VerdictStatus::Pass,
        VerdictCounts::default(),
        vec![],
        vec![make_sensor("x"), make_sensor("y")],
    );
    let curr = empty_report(VerdictStatus::Pass);
    let delta = compute_trend(&base, &curr);
    assert!(delta.sensors_added.is_empty());
    assert_eq!(delta.sensors_removed, vec!["x", "y"]);
}

// ---------------------------------------------------------------------------
// Snapshot: all verdict transitions
// ---------------------------------------------------------------------------

#[test]
fn snapshot_all_verdict_transitions() {
    let statuses = [
        VerdictStatus::Pass,
        VerdictStatus::Warn,
        VerdictStatus::Fail,
        VerdictStatus::Skip,
    ];
    let mut transitions = Vec::new();
    for base_status in &statuses {
        for curr_status in &statuses {
            let delta = compute_trend(
                &empty_report(base_status.clone()),
                &empty_report(curr_status.clone()),
            );
            let label = format!("{base_status:?} -> {curr_status:?}");
            let change = match &delta.verdict_change {
                Some(vc) => format!("{:?} -> {:?}", vc.before, vc.after),
                None => "unchanged".into(),
            };
            transitions.push(format!("{label}: {change}"));
        }
    }
    insta::assert_snapshot!(transitions.join("\n"));
}

// ---------------------------------------------------------------------------
// Snapshot: complex mixed scenario
// ---------------------------------------------------------------------------

#[test]
fn snapshot_complex_mixed_scenario() {
    let base = make_report(
        VerdictStatus::Fail,
        VerdictCounts {
            info: 5,
            warn: 3,
            error: 2,
            suppressed: 1,
        },
        vec![
            make_highlight("clippy", "unused_var", "unused x", Some("fp_a")),
            make_highlight("clippy", "dead_code", "dead fn", Some("fp_b")),
            make_highlight("test", "flaky", "flaky test", None),
        ],
        vec![make_sensor("clippy"), make_sensor("test")],
    );
    let curr = make_report(
        VerdictStatus::Warn,
        VerdictCounts {
            info: 3,
            warn: 4,
            error: 0,
            suppressed: 0,
        },
        vec![
            make_highlight("clippy", "unused_var", "unused x", Some("fp_a")),
            make_highlight("audit", "vuln", "CVE-2025-1234", Some("fp_c")),
        ],
        vec![make_sensor("clippy"), make_sensor("audit")],
    );
    let delta = compute_trend(&base, &curr);
    insta::assert_snapshot!(format!("{delta:#?}"));
}

// ---------------------------------------------------------------------------
// Edge-case: finding with no location matches by key
// ---------------------------------------------------------------------------

#[test]
fn finding_without_location_matches_by_key() {
    let h = Highlight {
        sensor_id: "s".into(),
        finding: Finding {
            severity: Severity::Info,
            check_id: None,
            code: "no_loc".into(),
            message: "no location".into(),
            location: None,
            help: None,
            url: None,
            fingerprint: None,
            data: None,
        },
    };
    let base = make_report(
        VerdictStatus::Pass,
        VerdictCounts::default(),
        vec![h.clone()],
        vec![],
    );
    let curr = make_report(
        VerdictStatus::Pass,
        VerdictCounts::default(),
        vec![h],
        vec![],
    );
    let delta = compute_trend(&base, &curr);
    assert!(delta.new_findings.is_empty());
    assert!(delta.fixed_findings.is_empty());
}
