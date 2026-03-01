//! Criterion benchmarks for cockpitctl-sarif conversion.
//!
//! Run with: `cargo bench -p cockpitctl-sarif`
//!
//! These benchmarks measure SARIF conversion performance with varying report sizes.

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use rand::prelude::*;
use std::collections::BTreeMap;
use std::hint::black_box;

use cockpitctl_sarif::{cockpit_report_to_sarif, cockpit_report_to_sarif_json};
use cockpitctl_types::{
    CockpitReport, Finding, Highlight, Location, MissingPolicy, PolicySensorSnapshot,
    PolicySnapshot, Presence, RunInfo, SensorSummary, Severity, ToolInfo, Verdict, VerdictCounts,
    VerdictStatus,
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
        help: None,
        url: None,
        fingerprint: Some(format!("fp_{:016x}", rng.random::<u64>())),
        data: None,
    }
}

fn generate_highlight(rng: &mut impl Rng, sensor_id: &str, index: usize) -> Highlight {
    Highlight {
        sensor_id: sensor_id.to_string(),
        finding: generate_finding(rng, index),
    }
}

/// Generate a CockpitReport with the given number of highlights.
fn generate_report(num_sensors: usize, num_highlights: usize, seed: u64) -> CockpitReport {
    let mut rng = rand::rngs::StdRng::seed_from_u64(seed);

    let sensor_ids: Vec<String> = (0..num_sensors)
        .map(|i| format!("sensor_{:03}", i))
        .collect();

    let highlights: Vec<Highlight> = (0..num_highlights)
        .map(|i| {
            let sensor_id = &sensor_ids[i % num_sensors];
            generate_highlight(&mut rng, sensor_id, i)
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

// ============================================================================
// Benchmarks
// ============================================================================

/// Benchmark `cockpit_report_to_sarif` with varying report sizes.
fn bench_sarif_conversion(c: &mut Criterion) {
    let mut group = c.benchmark_group("sarif_conversion");

    let test_cases = [(3, 5, "small"), (10, 50, "medium"), (50, 200, "large")];

    for (num_sensors, num_highlights, label) in test_cases {
        group.throughput(Throughput::Elements(num_highlights as u64));

        let report = generate_report(num_sensors, num_highlights, 42);

        group.bench_function(label, |b| {
            b.iter(|| cockpit_report_to_sarif(black_box(&report)));
        });
    }

    group.finish();
}

/// Benchmark `cockpit_report_to_sarif_json` (conversion + serialization).
fn bench_sarif_json_serialization(c: &mut Criterion) {
    let mut group = c.benchmark_group("sarif_json_serialization");

    let test_cases = [(3, 5, "small"), (10, 50, "medium"), (50, 200, "large")];

    for (num_sensors, num_highlights, label) in test_cases {
        group.throughput(Throughput::Elements(num_highlights as u64));

        let report = generate_report(num_sensors, num_highlights, 42);

        group.bench_function(label, |b| {
            b.iter(|| cockpit_report_to_sarif_json(black_box(&report)));
        });
    }

    group.finish();
}

/// Benchmark scaling behavior with increasing highlight counts.
fn bench_sarif_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("sarif_scaling");

    for num_highlights in [10, 100, 500, 1_000] {
        group.throughput(Throughput::Elements(num_highlights as u64));

        let report = generate_report(20, num_highlights as usize, 42);

        group.bench_with_input(
            BenchmarkId::from_parameter(num_highlights),
            &report,
            |b, report| {
                b.iter(|| cockpit_report_to_sarif(black_box(report)));
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_sarif_conversion,
    bench_sarif_json_serialization,
    bench_sarif_scaling,
);
criterion_main!(benches);
