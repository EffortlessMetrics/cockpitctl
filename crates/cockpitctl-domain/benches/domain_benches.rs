//! Criterion benchmarks for cockpitctl-domain hot paths.
//!
//! Run with: `cargo bench -p cockpitctl-domain`
//!
//! These benchmarks measure performance of key domain operations that matter at scale
//! (large repos with many sensors and findings).

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use rand::prelude::*;
use std::collections::BTreeMap;

use cockpitctl_domain::{derive_fingerprint, select_highlights, sort_findings};
use cockpitctl_types::{
    CockpitConfig, Finding, Highlight, Location, MissingPolicy, Policy, SchemaValidation,
    SensorPolicy, Severity,
};

// ============================================================================
// Synthetic fixture generators
// ============================================================================

/// Generate a random severity weighted towards errors/warnings (more realistic).
fn random_severity(rng: &mut impl Rng) -> Severity {
    match rng.gen_range(0..10) {
        0..=3 => Severity::Error,
        4..=6 => Severity::Warn,
        _ => Severity::Info,
    }
}

/// Generate a synthetic finding with randomized fields.
fn generate_finding(rng: &mut impl Rng, index: usize) -> Finding {
    let severity = random_severity(rng);
    let code = format!("CODE_{:04}", rng.gen_range(0..100));
    let message = format!(
        "Finding message {} with some additional context to simulate real-world length",
        index
    );

    // 80% of findings have a location
    let location = if rng.gen_bool(0.8) {
        Some(Location {
            path: Some(format!(
                "src/module_{}/file_{}.rs",
                rng.gen_range(0..20),
                rng.gen_range(0..50)
            )),
            line: Some(rng.gen_range(1..1000)),
            col: if rng.gen_bool(0.5) {
                Some(rng.gen_range(1..120))
            } else {
                None
            },
        })
    } else {
        None
    };

    Finding {
        severity,
        check_id: Some(format!("check_{}", rng.gen_range(0..50))),
        code,
        message,
        location,
        help: if rng.gen_bool(0.3) {
            Some("Consider fixing this issue by following best practices.".to_string())
        } else {
            None
        },
        url: if rng.gen_bool(0.2) {
            Some("https://docs.example.com/rules/ABC123".to_string())
        } else {
            None
        },
        fingerprint: None, // Let the system derive fingerprints
        data: None,
    }
}

/// Generate a vector of synthetic findings.
fn generate_findings(count: usize, seed: u64) -> Vec<Finding> {
    let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
    (0..count).map(|i| generate_finding(&mut rng, i)).collect()
}

/// Generate a highlight from a finding and sensor ID.
fn generate_highlight(rng: &mut impl Rng, sensor_id: &str, index: usize) -> Highlight {
    Highlight {
        sensor_id: sensor_id.to_string(),
        finding: generate_finding(rng, index),
    }
}

/// Generate highlights across multiple sensors.
fn generate_highlights(count: usize, num_sensors: usize, seed: u64) -> Vec<Highlight> {
    let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
    let sensor_ids: Vec<String> = (0..num_sensors)
        .map(|i| format!("sensor_{:03}", i))
        .collect();

    (0..count)
        .map(|i| {
            let sensor_id = &sensor_ids[i % num_sensors];
            generate_highlight(&mut rng, sensor_id, i)
        })
        .collect()
}

/// Generate a CockpitConfig with the given number of sensors.
fn generate_config(num_sensors: usize) -> CockpitConfig {
    let mut sensors = BTreeMap::new();
    for i in 0..num_sensors {
        sensors.insert(
            format!("sensor_{:03}", i),
            SensorPolicy {
                blocking: i % 3 == 0, // Every 3rd sensor is blocking
                missing: MissingPolicy::Skip,
                section: Some(format!("Section_{}", i % 5)),
                require_label: None,
                repro: None,
            },
        );
    }

    CockpitConfig {
        policy: Policy {
            warn_is_fail: false,
            max_highlights: 50,
            max_per_sensor_findings: 100,
            max_annotations: 25,
            section_order: vec![
                "Section_0".into(),
                "Section_1".into(),
                "Section_2".into(),
                "Section_3".into(),
                "Section_4".into(),
            ],
            schema_validation: SchemaValidation::Strict,
        },
        sensors,
    }
}

/// Generate sensor blocking map from config.
fn generate_sensor_blocking(cfg: &CockpitConfig) -> BTreeMap<String, bool> {
    cfg.sensors
        .iter()
        .map(|(id, p)| (id.clone(), p.blocking))
        .collect()
}

// ============================================================================
// Benchmarks
// ============================================================================

/// Benchmark `sort_findings` with varying sizes.
fn bench_sort_findings(c: &mut Criterion) {
    let mut group = c.benchmark_group("sort_findings");

    for size in [100, 1_000, 10_000] {
        group.throughput(Throughput::Elements(size as u64));

        group.bench_with_input(BenchmarkId::from_parameter(size), &size, |b, &size| {
            // Generate findings once per benchmark iteration batch
            let original = generate_findings(size, 42);

            b.iter(|| {
                // Clone for each iteration since sort is in-place
                let mut findings = original.clone();
                sort_findings(black_box("test_sensor"), black_box(&mut findings));
                findings
            });
        });
    }

    group.finish();
}

/// Benchmark `select_highlights` with varying sizes.
fn bench_select_highlights(c: &mut Criterion) {
    let mut group = c.benchmark_group("select_highlights");

    // Test with different numbers of highlights and sensors
    let test_cases = [(100, 5), (500, 10), (1_000, 20), (5_000, 50)];

    for (num_highlights, num_sensors) in test_cases {
        let label = format!("{}_highlights_{}_sensors", num_highlights, num_sensors);
        group.throughput(Throughput::Elements(num_highlights as u64));

        let cfg = generate_config(num_sensors);
        let sensor_blocking = generate_sensor_blocking(&cfg);
        let highlights = generate_highlights(num_highlights, num_sensors, 42);

        group.bench_function(label, |b| {
            b.iter(|| {
                select_highlights(
                    black_box(highlights.clone()),
                    black_box(&cfg),
                    black_box(&sensor_blocking),
                )
            });
        });
    }

    group.finish();
}

/// Benchmark `derive_fingerprint` throughput.
fn bench_derive_fingerprint(c: &mut Criterion) {
    let mut group = c.benchmark_group("derive_fingerprint");

    // Test with findings of varying complexity
    let simple_finding = Finding {
        severity: Severity::Error,
        check_id: None,
        code: "E001".to_string(),
        message: "Simple error".to_string(),
        location: None,
        help: None,
        url: None,
        fingerprint: None,
        data: None,
    };

    let complex_finding = Finding {
        severity: Severity::Warn,
        check_id: Some("complex_check_id".to_string()),
        code: "W1234".to_string(),
        message: "This is a much longer warning message that contains detailed information about the issue found during analysis, including context and suggestions.".to_string(),
        location: Some(Location {
            path: Some("src/very/deeply/nested/module/submodule/file.rs".to_string()),
            line: Some(12345),
            col: Some(42),
        }),
        help: Some("Consider refactoring this code to improve maintainability.".to_string()),
        url: Some("https://docs.example.com/rules/W1234".to_string()),
        fingerprint: None,
        data: None,
    };

    group.bench_function("simple_finding", |b| {
        b.iter(|| derive_fingerprint(black_box("test_sensor"), black_box(&simple_finding)));
    });

    group.bench_function("complex_finding", |b| {
        b.iter(|| derive_fingerprint(black_box("test_sensor"), black_box(&complex_finding)));
    });

    // Throughput test: many fingerprints
    let findings = generate_findings(1000, 42);
    group.throughput(Throughput::Elements(1000));
    group.bench_function("batch_1000", |b| {
        b.iter(|| {
            for f in &findings {
                black_box(derive_fingerprint("test_sensor", f));
            }
        });
    });

    group.finish();
}

/// Benchmark worst-case scenario: many findings from many sensors, all needing fingerprints.
fn bench_select_highlights_worst_case(c: &mut Criterion) {
    let mut group = c.benchmark_group("select_highlights_worst_case");

    // Simulate a large monorepo scenario
    let num_highlights = 10_000;
    let num_sensors = 100;

    group.throughput(Throughput::Elements(num_highlights as u64));

    let mut cfg = generate_config(num_sensors);
    // Increase max_highlights to force more work
    cfg.policy.max_highlights = 500;

    let sensor_blocking = generate_sensor_blocking(&cfg);
    let highlights = generate_highlights(num_highlights, num_sensors, 42);

    group.bench_function("10k_highlights_100_sensors", |b| {
        b.iter(|| {
            select_highlights(
                black_box(highlights.clone()),
                black_box(&cfg),
                black_box(&sensor_blocking),
            )
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_sort_findings,
    bench_select_highlights,
    bench_derive_fingerprint,
    bench_select_highlights_worst_case,
);
criterion_main!(benches);
