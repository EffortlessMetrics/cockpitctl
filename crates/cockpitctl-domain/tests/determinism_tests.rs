//! Determinism golden tests: prove byte-stable output for ordering-critical paths.
//!
//! Each test runs sorting/selection 100 times with shuffled input to prove
//! the output is identical on every iteration.

use std::collections::BTreeMap;

use cockpitctl_domain::{
    compute_counts, derive_fingerprint, overall_verdict, select_highlights, sort_findings,
    sort_sensor_summaries,
};
use cockpitctl_types::{
    CockpitConfig, Finding, Highlight, Location, MissingPolicy, Presence, SensorPolicy,
    SensorSummary, Severity, Verdict, VerdictCounts, VerdictStatus,
};
use rand::rng;
use rand::seq::SliceRandom;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn finding(code: &str, severity: Severity, path: &str, line: u32, message: &str) -> Finding {
    Finding {
        severity,
        check_id: None,
        code: code.to_string(),
        message: message.to_string(),
        location: Some(Location {
            path: Some(path.to_string()),
            line: Some(line),
            col: None,
        }),
        help: None,
        url: None,
        fingerprint: None,
        data: None,
    }
}

fn summary_with_verdict(
    id: &str,
    blocking: bool,
    status: VerdictStatus,
    counts: VerdictCounts,
) -> SensorSummary {
    SensorSummary {
        id: id.to_string(),
        blocking,
        missing: MissingPolicy::Fail,
        presence: Presence::Present,
        report_path: format!("artifacts/{}/report.json", id),
        comment_path: None,
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

/// Build a rich set of findings that exercise every tie-breaker in the sort key:
/// severity desc → sensor_id (implicit via sort_findings arg) → path → line → code → message
fn diverse_findings() -> Vec<Finding> {
    vec![
        finding("W002", Severity::Warn, "src/b.rs", 10, "Warn in b"),
        finding("E001", Severity::Error, "src/a.rs", 5, "Error in a:5"),
        finding("I001", Severity::Info, "src/c.rs", 1, "Info in c"),
        finding(
            "E002",
            Severity::Error,
            "src/a.rs",
            5,
            "Error in a:5 dup code",
        ),
        finding("E001", Severity::Error, "src/a.rs", 20, "Error in a:20"),
        finding("W001", Severity::Warn, "src/a.rs", 1, "Warn in a"),
        finding(
            "E001",
            Severity::Error,
            "src/a.rs",
            5,
            "Error in a:5 alt msg",
        ),
        finding("I002", Severity::Info, "src/a.rs", 100, "Info in a"),
    ]
}

// ---------------------------------------------------------------------------
// sort_findings: shuffled inputs → same output every time
// ---------------------------------------------------------------------------

#[test]
fn determinism_sort_findings_shuffled_100x() {
    let mut rng = rng();
    let mut reference = diverse_findings();
    sort_findings("sensor_x", &mut reference);
    let reference_json = serde_json::to_string_pretty(&reference).unwrap();

    // Snapshot the canonical ordering once.
    insta::assert_json_snapshot!("determinism_sorted_findings", reference);

    // Run 100 shuffled iterations and assert identical output.
    for _ in 0..100 {
        let mut shuffled = diverse_findings();
        shuffled.shuffle(&mut rng);
        sort_findings("sensor_x", &mut shuffled);
        let json = serde_json::to_string_pretty(&shuffled).unwrap();
        assert_eq!(json, reference_json, "sort_findings must be deterministic");
    }
}

// ---------------------------------------------------------------------------
// Findings sort order: severity desc → path → line → code → message
// ---------------------------------------------------------------------------

#[test]
fn determinism_findings_sort_order_canonical() {
    // Construct findings that differ only in the final tie-breaker (message).
    let mut findings = vec![
        finding("SAME", Severity::Error, "src/x.rs", 10, "Zzz last"),
        finding("SAME", Severity::Error, "src/x.rs", 10, "Aaa first"),
        finding("SAME", Severity::Error, "src/x.rs", 10, "Mmm middle"),
    ];
    sort_findings("sensor", &mut findings);
    insta::assert_json_snapshot!("determinism_findings_message_tiebreak", findings);

    // Verify messages are in ascending order (stable tie-break).
    let messages: Vec<&str> = findings.iter().map(|f| f.message.as_str()).collect();
    assert_eq!(messages, vec!["Aaa first", "Mmm middle", "Zzz last"]);
}

// ---------------------------------------------------------------------------
// select_highlights: dedup by fingerprint + cap + deterministic order
// ---------------------------------------------------------------------------

#[test]
fn determinism_select_highlights_dedup_and_cap() {
    let mut cfg = CockpitConfig::default();
    cfg.policy.max_highlights = 3;

    let base = finding("E1", Severity::Error, "src/a.rs", 10, "Error in a");
    let fp = derive_fingerprint("sensor_a", &base);

    // Create 5 highlights: 2 share the same fingerprint (should dedup to 1).
    let mut h1_finding = base.clone();
    h1_finding.fingerprint = Some(fp.clone());
    let mut h2_finding = base.clone();
    h2_finding.fingerprint = Some(fp.clone());

    let candidates = vec![
        Highlight {
            sensor_id: "sensor_c".to_string(),
            finding: finding("W1", Severity::Warn, "src/c.rs", 5, "Warn in c"),
        },
        Highlight {
            sensor_id: "sensor_a".to_string(),
            finding: h1_finding,
        },
        Highlight {
            sensor_id: "sensor_a".to_string(),
            finding: h2_finding,
        },
        Highlight {
            sensor_id: "sensor_b".to_string(),
            finding: finding("E2", Severity::Error, "src/b.rs", 1, "Error in b"),
        },
        Highlight {
            sensor_id: "sensor_d".to_string(),
            finding: finding("I1", Severity::Info, "src/d.rs", 1, "Info in d"),
        },
    ];

    let mut sensor_blocking = BTreeMap::new();
    sensor_blocking.insert("sensor_a".to_string(), true);
    sensor_blocking.insert("sensor_b".to_string(), false);
    sensor_blocking.insert("sensor_c".to_string(), true);
    sensor_blocking.insert("sensor_d".to_string(), false);

    let selected = select_highlights(candidates, &cfg, &sensor_blocking);
    insta::assert_json_snapshot!("determinism_highlights_dedup_cap", selected);

    // After dedup we have 4 unique highlights, capped to 3.
    assert_eq!(selected.len(), 3, "should cap at max_highlights=3");
}

// ---------------------------------------------------------------------------
// Highlights sort order: severity desc → blocking-first → sensor_id → path → line → code
// ---------------------------------------------------------------------------

#[test]
fn determinism_highlights_sort_order_canonical() {
    let cfg = CockpitConfig::default(); // max_highlights = 10

    // All error-severity: tie-break is blocking → sensor_id → path → line → code
    let candidates = vec![
        Highlight {
            sensor_id: "z_sensor".to_string(),
            finding: finding("E1", Severity::Error, "src/a.rs", 1, "Err z"),
        },
        Highlight {
            sensor_id: "a_sensor".to_string(),
            finding: finding("E1", Severity::Error, "src/b.rs", 1, "Err a non-blocking"),
        },
        Highlight {
            sensor_id: "a_sensor".to_string(),
            finding: finding("E1", Severity::Error, "src/a.rs", 1, "Err a blocking"),
        },
    ];

    let mut sensor_blocking = BTreeMap::new();
    sensor_blocking.insert("a_sensor".to_string(), true);
    sensor_blocking.insert("z_sensor".to_string(), false);

    let selected = select_highlights(candidates, &cfg, &sensor_blocking);
    insta::assert_json_snapshot!("determinism_highlights_sort_order", selected);

    // First highlight should be from the blocking sensor.
    assert_eq!(selected[0].sensor_id, "a_sensor");
}

// ---------------------------------------------------------------------------
// overall_verdict: stable across sensor combinations
// ---------------------------------------------------------------------------

#[test]
fn determinism_overall_verdict_combinations() {
    let cfg = CockpitConfig::default();

    let scenarios: Vec<(&str, Vec<SensorSummary>)> = vec![
        (
            "all_pass",
            vec![
                summary_with_verdict("alpha", true, VerdictStatus::Pass, VerdictCounts::default()),
                summary_with_verdict("beta", true, VerdictStatus::Pass, VerdictCounts::default()),
                summary_with_verdict(
                    "gamma",
                    false,
                    VerdictStatus::Pass,
                    VerdictCounts::default(),
                ),
            ],
        ),
        (
            "blocking_fail_overrides",
            vec![
                summary_with_verdict("alpha", true, VerdictStatus::Pass, VerdictCounts::default()),
                summary_with_verdict(
                    "beta",
                    true,
                    VerdictStatus::Fail,
                    VerdictCounts {
                        info: 0,
                        warn: 0,
                        error: 5,
                        suppressed: 0,
                    },
                ),
                summary_with_verdict(
                    "gamma",
                    false,
                    VerdictStatus::Pass,
                    VerdictCounts::default(),
                ),
            ],
        ),
        (
            "nonblocking_fail_ignored",
            vec![
                summary_with_verdict("alpha", true, VerdictStatus::Pass, VerdictCounts::default()),
                summary_with_verdict(
                    "beta",
                    false,
                    VerdictStatus::Fail,
                    VerdictCounts {
                        info: 0,
                        warn: 0,
                        error: 10,
                        suppressed: 0,
                    },
                ),
            ],
        ),
        (
            "mixed_warn_and_fail",
            vec![
                summary_with_verdict(
                    "alpha",
                    true,
                    VerdictStatus::Warn,
                    VerdictCounts {
                        info: 0,
                        warn: 3,
                        error: 0,
                        suppressed: 0,
                    },
                ),
                summary_with_verdict(
                    "beta",
                    true,
                    VerdictStatus::Fail,
                    VerdictCounts {
                        info: 0,
                        warn: 0,
                        error: 2,
                        suppressed: 0,
                    },
                ),
            ],
        ),
        (
            "all_skip",
            vec![
                summary_with_verdict(
                    "alpha",
                    false,
                    VerdictStatus::Skip,
                    VerdictCounts::default(),
                ),
                summary_with_verdict("beta", false, VerdictStatus::Skip, VerdictCounts::default()),
            ],
        ),
    ];

    let mut results: Vec<(&str, Verdict)> = Vec::new();
    for (name, summaries) in &scenarios {
        let verdict = overall_verdict(summaries, &cfg);
        results.push((name, verdict));
    }
    insta::assert_json_snapshot!("determinism_overall_verdict_combinations", results);
}

// ---------------------------------------------------------------------------
// compute_counts: stable for same findings
// ---------------------------------------------------------------------------

#[test]
fn determinism_compute_counts_stable() {
    let findings = vec![
        finding("E1", Severity::Error, "src/a.rs", 1, "error 1"),
        finding("E2", Severity::Error, "src/a.rs", 2, "error 2"),
        finding("E3", Severity::Error, "src/b.rs", 1, "error 3"),
        finding("W1", Severity::Warn, "src/c.rs", 1, "warn 1"),
        finding("W2", Severity::Warn, "src/c.rs", 2, "warn 2"),
        finding("I1", Severity::Info, "src/d.rs", 1, "info 1"),
    ];

    let reference = compute_counts(&findings);
    insta::assert_json_snapshot!("determinism_compute_counts", reference);

    // Verify exact values.
    assert_eq!(reference.error, 3);
    assert_eq!(reference.warn, 2);
    assert_eq!(reference.info, 1);

    // Run 100 times to prove stability.
    for _ in 0..100 {
        let counts = compute_counts(&findings);
        assert_eq!(counts, reference, "compute_counts must be deterministic");
    }
}

// ---------------------------------------------------------------------------
// sort_sensor_summaries: deterministic section + id ordering
// ---------------------------------------------------------------------------

#[test]
fn determinism_sort_sensor_summaries_shuffled_100x() {
    let mut rng = rng();

    let mut cfg = CockpitConfig::default();
    cfg.policy.section_order = vec!["Build".to_string(), "Quality".to_string()];
    cfg.sensors.insert(
        "build".to_string(),
        SensorPolicy {
            blocking: true,
            missing: MissingPolicy::Fail,
            section: Some("Build".to_string()),
            require_label: None,
            repro: None,
        },
    );
    cfg.sensors.insert(
        "lint".to_string(),
        SensorPolicy {
            blocking: true,
            missing: MissingPolicy::Fail,
            section: Some("Quality".to_string()),
            require_label: None,
            repro: None,
        },
    );
    cfg.sensors.insert(
        "coverage".to_string(),
        SensorPolicy {
            blocking: false,
            missing: MissingPolicy::Warn,
            section: Some("Quality".to_string()),
            require_label: None,
            repro: None,
        },
    );
    cfg.sensors.insert(
        "security".to_string(),
        SensorPolicy {
            blocking: true,
            missing: MissingPolicy::Fail,
            section: None, // no section → sorted to end
            require_label: None,
            repro: None,
        },
    );

    let make_summaries = || {
        vec![
            summary_with_verdict(
                "security",
                true,
                VerdictStatus::Pass,
                VerdictCounts::default(),
            ),
            summary_with_verdict("lint", true, VerdictStatus::Warn, VerdictCounts::default()),
            summary_with_verdict("build", true, VerdictStatus::Pass, VerdictCounts::default()),
            summary_with_verdict(
                "coverage",
                false,
                VerdictStatus::Pass,
                VerdictCounts::default(),
            ),
        ]
    };

    let mut reference = make_summaries();
    sort_sensor_summaries(&mut reference, &cfg);
    let reference_json = serde_json::to_string_pretty(&reference).unwrap();
    let reference_ids: Vec<&str> = reference.iter().map(|s| s.id.as_str()).collect();

    insta::assert_json_snapshot!("determinism_sorted_sensor_summaries", reference);

    for _ in 0..100 {
        let mut shuffled = make_summaries();
        shuffled.shuffle(&mut rng);
        sort_sensor_summaries(&mut shuffled, &cfg);
        let json = serde_json::to_string_pretty(&shuffled).unwrap();
        assert_eq!(
            json, reference_json,
            "sort_sensor_summaries must be deterministic"
        );
        let ids: Vec<&str> = shuffled.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, reference_ids);
    }
}
