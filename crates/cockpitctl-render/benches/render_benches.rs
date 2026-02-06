//! Criterion benchmarks for cockpitctl-render hot paths.
//!
//! Run with: `cargo bench -p cockpitctl-render`
//!
//! These benchmarks measure performance of comment rendering with large reports.

use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};
use rand::prelude::*;
use std::collections::BTreeMap;

use cockpitctl_render::render_comment;
use cockpitctl_types::{
    CockpitConfig, CockpitReport, Finding, Highlight, Location, MissingPolicy, Policy,
    PolicySensorSnapshot, PolicySnapshot, RunInfo, SchemaValidation, SensorPolicy, SensorSummary,
    Severity, ToolInfo, Verdict, VerdictCounts, VerdictStatus,
};

// ============================================================================
// Synthetic fixture generators
// ============================================================================

fn random_severity(rng: &mut impl Rng) -> Severity {
    match rng.gen_range(0..10) {
        0..=3 => Severity::Error,
        4..=6 => Severity::Warn,
        _ => Severity::Info,
    }
}

fn random_verdict_status(rng: &mut impl Rng) -> VerdictStatus {
    match rng.gen_range(0..10) {
        0..=5 => VerdictStatus::Pass,
        6..=7 => VerdictStatus::Warn,
        8 => VerdictStatus::Fail,
        _ => VerdictStatus::Skip,
    }
}

fn generate_finding(rng: &mut impl Rng, index: usize) -> Finding {
    let severity = random_severity(rng);
    Finding {
        severity,
        check_id: Some(format!("check_{}", rng.gen_range(0..50))),
        code: format!("CODE_{:04}", rng.gen_range(0..100)),
        message: format!(
            "Finding message {} with context about the issue found in the codebase during analysis",
            index
        ),
        location: if rng.gen_bool(0.8) {
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
        },
        help: if rng.gen_bool(0.3) {
            Some("Consider fixing this issue.".to_string())
        } else {
            None
        },
        url: None,
        fingerprint: Some(format!("fp_{:016x}", rng.gen::<u64>())),
        data: None,
    }
}

fn generate_highlight(rng: &mut impl Rng, sensor_id: &str, index: usize) -> Highlight {
    Highlight {
        sensor_id: sensor_id.to_string(),
        finding: generate_finding(rng, index),
    }
}

fn generate_sensor_summary(rng: &mut impl Rng, sensor_id: &str, _section: &str) -> SensorSummary {
    let status = random_verdict_status(rng);
    SensorSummary {
        id: sensor_id.to_string(),
        blocking: rng.gen_bool(0.3),
        missing: MissingPolicy::Skip,
        present: true,
        report_path: format!("artifacts/{}/report.json", sensor_id),
        comment_path: if rng.gen_bool(0.4) {
            Some(format!("artifacts/{}/comment.md", sensor_id))
        } else {
            None
        },
        verdict: Verdict {
            status,
            counts: VerdictCounts {
                info: rng.gen_range(0..10),
                warn: rng.gen_range(0..5),
                error: rng.gen_range(0..3),
                suppressed: 0,
            },
            reasons: vec![],
        },
        truncated: rng.gen_bool(0.1),
        errors: vec![],
    }
}

fn generate_tool_info() -> ToolInfo {
    ToolInfo {
        name: "cockpitctl".to_string(),
        version: "0.1.0".to_string(),
        commit: Some("abc1234".to_string()),
    }
}

fn generate_run_info() -> RunInfo {
    RunInfo {
        started_at: "2024-01-15T10:30:00Z".to_string(),
        ended_at: Some("2024-01-15T10:35:00Z".to_string()),
        duration_ms: Some(300000),
        host: None,
        git: None,
        ci: None,
        capabilities: BTreeMap::new(),
    }
}

fn generate_policy_snapshot(cfg: &CockpitConfig) -> PolicySnapshot {
    PolicySnapshot {
        warn_is_fail: cfg.policy.warn_is_fail,
        max_highlights: cfg.policy.max_highlights,
        max_per_sensor_findings: cfg.policy.max_per_sensor_findings,
        max_annotations: cfg.policy.max_annotations,
        section_order: cfg.policy.section_order.clone(),
        sensors: cfg
            .sensors
            .iter()
            .map(|(id, p)| PolicySensorSnapshot {
                id: id.clone(),
                blocking: p.blocking,
                missing: p.missing,
                section: p.section.clone(),
                require_label: p.require_label.clone(),
                repro: p.repro.clone(),
            })
            .collect(),
    }
}

/// Generate a complete CockpitReport for benchmarking.
fn generate_report(
    num_sensors: usize,
    num_highlights: usize,
    seed: u64,
) -> (CockpitReport, CockpitConfig) {
    let mut rng = rand::rngs::StdRng::seed_from_u64(seed);

    let sections = [
        "Repo contract",
        "Dependencies",
        "Tests",
        "Diagnostics",
        "Other",
    ];
    let section_order: Vec<String> = sections.iter().map(|s| s.to_string()).collect();

    // Generate config
    let mut sensors_map = BTreeMap::new();
    for i in 0..num_sensors {
        let section = sections[i % sections.len()];
        sensors_map.insert(
            format!("sensor_{:03}", i),
            SensorPolicy {
                blocking: i % 3 == 0,
                missing: MissingPolicy::Skip,
                section: Some(section.to_string()),
                require_label: None,
                repro: if rng.gen_bool(0.3) {
                    Some(format!("./run-sensor.sh {}", i))
                } else {
                    None
                },
            },
        );
    }

    let cfg = CockpitConfig {
        policy: Policy {
            warn_is_fail: false,
            max_highlights: num_highlights.min(100),
            max_per_sensor_findings: 50,
            max_annotations: 25,
            section_order: section_order.clone(),
            schema_validation: SchemaValidation::Strict,
        },
        sensors: sensors_map.clone(),
    };

    // Generate sensor summaries
    let sensor_summaries: Vec<SensorSummary> = sensors_map
        .iter()
        .map(|(id, p)| {
            let section = p.section.as_deref().unwrap_or("Other");
            generate_sensor_summary(&mut rng, id, section)
        })
        .collect();

    // Generate highlights
    let sensor_ids: Vec<&String> = sensors_map.keys().collect();
    let highlights: Vec<Highlight> = (0..num_highlights)
        .map(|i| {
            let sensor_id = sensor_ids[i % sensor_ids.len()];
            generate_highlight(&mut rng, sensor_id, i)
        })
        .collect();

    // Compute overall verdict
    let overall_verdict = Verdict {
        status: if sensor_summaries
            .iter()
            .any(|s| matches!(s.verdict.status, VerdictStatus::Fail))
        {
            VerdictStatus::Fail
        } else if sensor_summaries
            .iter()
            .any(|s| matches!(s.verdict.status, VerdictStatus::Warn))
        {
            VerdictStatus::Warn
        } else {
            VerdictStatus::Pass
        },
        counts: VerdictCounts {
            info: sensor_summaries.iter().map(|s| s.verdict.counts.info).sum(),
            warn: sensor_summaries.iter().map(|s| s.verdict.counts.warn).sum(),
            error: sensor_summaries
                .iter()
                .map(|s| s.verdict.counts.error)
                .sum(),
            suppressed: sensor_summaries
                .iter()
                .map(|s| s.verdict.counts.suppressed)
                .sum(),
        },
        reasons: vec![],
    };

    let report = CockpitReport {
        schema: "cockpit.report.v1".to_string(),
        tool: generate_tool_info(),
        run: generate_run_info(),
        verdict: overall_verdict,
        sensors: sensor_summaries,
        highlights,
        policy: generate_policy_snapshot(&cfg),
        data: None,
    };

    (report, cfg)
}

// ============================================================================
// Benchmarks
// ============================================================================

/// Benchmark `render_comment` with varying report sizes.
fn bench_render_comment(c: &mut Criterion) {
    let mut group = c.benchmark_group("render_comment");

    // Test different scales
    let test_cases = [
        (5, 7, "small"),      // 5 sensors, 7 highlights (typical small project)
        (20, 20, "medium"),   // 20 sensors, 20 highlights (medium project)
        (50, 50, "large"),    // 50 sensors, 50 highlights (large project)
        (100, 100, "xlarge"), // 100 sensors, 100 highlights (monorepo)
    ];

    for (num_sensors, num_highlights, label) in test_cases {
        let (report, cfg) = generate_report(num_sensors, num_highlights, 42);

        group.bench_function(label, |b| {
            b.iter(|| render_comment(black_box(&report), black_box(&cfg)));
        });
    }

    group.finish();
}

/// Benchmark `render_comment` with emphasis on highlight count.
fn bench_render_comment_highlights(c: &mut Criterion) {
    let mut group = c.benchmark_group("render_comment_highlights");

    // Fixed sensors, varying highlights
    let num_sensors = 20;

    for num_highlights in [10, 50, 100, 200] {
        group.throughput(Throughput::Elements(num_highlights as u64));

        let (report, cfg) = generate_report(num_sensors, num_highlights, 42);
        let label = format!("{}_highlights", num_highlights);

        group.bench_function(label, |b| {
            b.iter(|| render_comment(black_box(&report), black_box(&cfg)));
        });
    }

    group.finish();
}

/// Benchmark worst case: many sensors with many sections.
fn bench_render_comment_worst_case(c: &mut Criterion) {
    let mut group = c.benchmark_group("render_comment_worst_case");

    // Monorepo simulation
    let (report, cfg) = generate_report(200, 200, 42);

    group.bench_function("200_sensors_200_highlights", |b| {
        b.iter(|| render_comment(black_box(&report), black_box(&cfg)));
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_render_comment,
    bench_render_comment_highlights,
    bench_render_comment_worst_case,
);
criterion_main!(benches);
