//! Criterion benchmarks for cockpitctl-types serialization and sort keys.
//!
//! Run with: `cargo bench -p cockpitctl-types`
//!
//! These benchmarks measure JSON serialization/deserialization performance
//! and finding sort key comparison.

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use rand::prelude::*;
use std::collections::BTreeMap;
use std::hint::black_box;

use cockpitctl_types::{
    CockpitReport, Finding, FindingSortKey, Highlight, Location, MissingPolicy,
    PolicySensorSnapshot, PolicySnapshot, Presence, RunInfo, SensorReport, SensorSummary, Severity,
    ToolInfo, Verdict, VerdictCounts, VerdictStatus, severity_rank,
};

// ============================================================================
// Synthetic fixture generators
// ============================================================================

fn random_severity(rng: &mut impl Rng) -> Severity {
    match rng.random_range(0..10) {
        0..=3 => Severity::Error,
        4..=6 => Severity::Warn,
        _ => Severity::Info,
    }
}

fn generate_finding(rng: &mut impl Rng, index: usize) -> Finding {
    Finding {
        severity: random_severity(rng),
        check_id: Some(format!("check_{}", rng.random_range(0..50))),
        code: format!("CODE_{:04}", rng.random_range(0..100)),
        message: format!(
            "Finding message {} with context about the issue found during analysis",
            index
        ),
        location: if rng.random_bool(0.8) {
            Some(Location {
                path: Some(format!(
                    "src/module_{}/file_{}.rs",
                    rng.random_range(0..20),
                    rng.random_range(0..50)
                )),
                line: Some(rng.random_range(1..1000)),
                col: if rng.random_bool(0.5) {
                    Some(rng.random_range(1..120))
                } else {
                    None
                },
            })
        } else {
            None
        },
        help: if rng.random_bool(0.3) {
            Some("Consider fixing this issue.".to_string())
        } else {
            None
        },
        url: None,
        fingerprint: Some(format!("fp_{:016x}", rng.random::<u64>())),
        data: None,
    }
}

fn generate_sensor_report(num_findings: usize, seed: u64) -> SensorReport {
    let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
    let findings: Vec<Finding> = (0..num_findings)
        .map(|i| generate_finding(&mut rng, i))
        .collect();

    SensorReport {
        schema: "sensor.report.v1".to_string(),
        tool: ToolInfo {
            name: "test-sensor".to_string(),
            version: "1.0.0".to_string(),
            commit: Some("abc1234".to_string()),
        },
        run: RunInfo {
            started_at: "2024-01-15T10:30:00Z".to_string(),
            ended_at: Some("2024-01-15T10:35:00Z".to_string()),
            duration_ms: Some(300000),
            host: None,
            git: None,
            ci: None,
            capabilities: BTreeMap::new(),
        },
        verdict: Verdict {
            status: VerdictStatus::Pass,
            counts: VerdictCounts {
                info: num_findings as u64 / 3,
                warn: num_findings as u64 / 3,
                error: num_findings as u64 / 3,
                suppressed: 0,
            },
            reasons: vec![],
        },
        findings,
        artifacts: vec![],
        data: None,
    }
}

fn generate_cockpit_report(num_sensors: usize, num_highlights: usize, seed: u64) -> CockpitReport {
    let mut rng = rand::rngs::StdRng::seed_from_u64(seed);

    let sensor_ids: Vec<String> = (0..num_sensors)
        .map(|i| format!("sensor_{:03}", i))
        .collect();

    let highlights: Vec<Highlight> = (0..num_highlights)
        .map(|i| {
            let sensor_id = &sensor_ids[i % num_sensors];
            Highlight {
                sensor_id: sensor_id.clone(),
                finding: generate_finding(&mut rng, i),
            }
        })
        .collect();

    let sensors: Vec<SensorSummary> = sensor_ids
        .iter()
        .map(|id| SensorSummary {
            id: id.clone(),
            blocking: false,
            missing: MissingPolicy::Skip,
            presence: Presence::Present,
            report_path: format!("artifacts/{}/report.json", id),
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
        })
        .collect();

    let policy = PolicySnapshot {
        warn_is_fail: false,
        max_highlights: num_highlights,
        max_per_sensor_findings: 50,
        max_annotations: 25,
        section_order: vec!["Other".into()],
        sensors: sensor_ids
            .iter()
            .map(|id| PolicySensorSnapshot {
                id: id.clone(),
                blocking: false,
                missing: MissingPolicy::Skip,
                section: Some("Other".to_string()),
                require_label: None,
                repro: None,
            })
            .collect(),
    };

    CockpitReport {
        schema: "cockpit.report.v1".to_string(),
        tool: ToolInfo {
            name: "cockpitctl".to_string(),
            version: "0.2.0".to_string(),
            commit: Some("abc1234".to_string()),
        },
        run: RunInfo {
            started_at: "2024-01-15T10:30:00Z".to_string(),
            ended_at: Some("2024-01-15T10:35:00Z".to_string()),
            duration_ms: Some(300000),
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
        policy,
        data: None,
    }
}

fn generate_sort_keys(count: usize, seed: u64) -> Vec<FindingSortKey> {
    let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
    (0..count)
        .map(|i| {
            let severity = match rng.random_range(0..3) {
                0 => Severity::Error,
                1 => Severity::Warn,
                _ => Severity::Info,
            };
            FindingSortKey {
                severity_rank: severity_rank(&severity),
                sensor_id: format!("sensor_{:03}", rng.random_range(0..20)),
                path: format!(
                    "src/module_{}/file_{}.rs",
                    rng.random_range(0..20),
                    rng.random_range(0..50)
                ),
                line: rng.random_range(1..1000),
                code: format!("CODE_{:04}", rng.random_range(0..100)),
                message: format!("Finding message {} with context", i),
            }
        })
        .collect()
}

// ============================================================================
// Benchmarks
// ============================================================================

/// Benchmark JSON serialization of SensorReport.
fn bench_sensor_report_serialize(c: &mut Criterion) {
    let mut group = c.benchmark_group("sensor_report_serialize");

    for num_findings in [10, 100, 500] {
        group.throughput(Throughput::Elements(num_findings as u64));

        let report = generate_sensor_report(num_findings, 42);

        group.bench_with_input(
            BenchmarkId::from_parameter(num_findings),
            &report,
            |b, report| {
                b.iter(|| serde_json::to_string(black_box(report)));
            },
        );
    }

    group.finish();
}

/// Benchmark JSON deserialization of SensorReport.
fn bench_sensor_report_deserialize(c: &mut Criterion) {
    let mut group = c.benchmark_group("sensor_report_deserialize");

    for num_findings in [10, 100, 500] {
        let report = generate_sensor_report(num_findings, 42);
        let json = serde_json::to_string(&report).expect("serialize");

        group.throughput(Throughput::Bytes(json.len() as u64));

        group.bench_with_input(
            BenchmarkId::from_parameter(num_findings),
            &json,
            |b, json| {
                b.iter(|| serde_json::from_str::<SensorReport>(black_box(json)));
            },
        );
    }

    group.finish();
}

/// Benchmark JSON serialization of CockpitReport.
fn bench_cockpit_report_serialize(c: &mut Criterion) {
    let mut group = c.benchmark_group("cockpit_report_serialize");

    let test_cases = [(5, 10, "small"), (20, 50, "medium"), (50, 200, "large")];

    for (num_sensors, num_highlights, label) in test_cases {
        let report = generate_cockpit_report(num_sensors, num_highlights, 42);

        group.bench_function(label, |b| {
            b.iter(|| serde_json::to_string(black_box(&report)));
        });
    }

    group.finish();
}

/// Benchmark JSON deserialization of CockpitReport.
fn bench_cockpit_report_deserialize(c: &mut Criterion) {
    let mut group = c.benchmark_group("cockpit_report_deserialize");

    let test_cases = [(5, 10, "small"), (20, 50, "medium"), (50, 200, "large")];

    for (num_sensors, num_highlights, label) in test_cases {
        let report = generate_cockpit_report(num_sensors, num_highlights, 42);
        let json = serde_json::to_string(&report).expect("serialize");

        group.throughput(Throughput::Bytes(json.len() as u64));

        group.bench_function(label, |b| {
            b.iter(|| serde_json::from_str::<CockpitReport>(black_box(&json)));
        });
    }

    group.finish();
}

/// Benchmark FindingSortKey comparison (Ord).
fn bench_sort_key_comparison(c: &mut Criterion) {
    let mut group = c.benchmark_group("finding_sort_key_cmp");

    for count in [100, 1_000, 10_000] {
        group.throughput(Throughput::Elements(count as u64));

        let mut keys = generate_sort_keys(count, 42);

        group.bench_with_input(
            BenchmarkId::from_parameter(count),
            &keys.clone(),
            |b, original| {
                b.iter(|| {
                    keys.clone_from(original);
                    keys.sort();
                    black_box(&keys);
                });
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_sensor_report_serialize,
    bench_sensor_report_deserialize,
    bench_cockpit_report_serialize,
    bench_cockpit_report_deserialize,
    bench_sort_key_comparison,
);
criterion_main!(benches);
