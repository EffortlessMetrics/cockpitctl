//! Stress tests for cockpitctl-types: serde roundtrip at scale, ordering consistency.

use cockpitctl_types::{
    Finding, FindingSortKey, Highlight, Location, RunInfo, SensorReport, Severity, ToolInfo,
    Verdict, VerdictCounts, VerdictStatus, severity_rank,
};
use std::collections::BTreeMap;

fn make_finding(
    severity: Severity,
    code: &str,
    message: &str,
    path: Option<&str>,
    line: Option<u32>,
) -> Finding {
    Finding {
        severity,
        check_id: None,
        code: code.to_string(),
        message: message.to_string(),
        location: path.map(|p| Location {
            path: Some(p.to_string()),
            line,
            col: None,
        }),
        help: None,
        url: None,
        fingerprint: None,
        data: None,
    }
}

fn make_sort_key(
    severity: Severity,
    sensor_id: &str,
    path: &str,
    line: u32,
    code: &str,
    message: &str,
) -> FindingSortKey {
    FindingSortKey {
        severity_rank: severity_rank(&severity),
        sensor_id: sensor_id.to_string(),
        path: path.to_string(),
        line,
        code: code.to_string(),
        message: message.to_string(),
    }
}

// ---------------------------------------------------------------------------
// 1. Serde roundtrip at scale: 1000 Finding objects
// ---------------------------------------------------------------------------

#[test]
fn stress_serde_roundtrip_1000_findings() {
    let findings: Vec<Finding> = (0..1000)
        .map(|i| {
            let severity = match i % 3 {
                0 => Severity::Error,
                1 => Severity::Warn,
                _ => Severity::Info,
            };
            make_finding(
                severity,
                &format!("CODE-{:04}", i),
                &format!("Message number {} with some content", i),
                Some(&format!("src/module{}/file{}.rs", i / 10, i)),
                Some(i as u32),
            )
        })
        .collect();

    let json = serde_json::to_string(&findings).unwrap();
    let parsed: Vec<Finding> = serde_json::from_str(&json).unwrap();

    assert_eq!(findings.len(), parsed.len());
    for (orig, rt) in findings.iter().zip(parsed.iter()) {
        assert_eq!(orig, rt);
    }
}

// ---------------------------------------------------------------------------
// 2. Ordering consistency: sort 10000 items, verify against shuffle+re-sort
// ---------------------------------------------------------------------------

#[test]
fn stress_ordering_consistency_10000_sort_keys() {
    let severities = [Severity::Error, Severity::Warn, Severity::Info];

    let mut keys: Vec<FindingSortKey> = (0..10_000)
        .map(|i| {
            make_sort_key(
                severities[i % 3].clone(),
                &format!("sensor-{:03}", i % 50),
                &format!("src/file{}.rs", i % 100),
                (i % 500) as u32,
                &format!("C{:05}", i),
                &format!("msg {}", i),
            )
        })
        .collect();

    keys.sort();
    let sorted_once = keys.clone();

    // Reverse and re-sort.
    keys.reverse();
    keys.sort();

    assert_eq!(sorted_once.len(), keys.len());
    for (a, b) in sorted_once.iter().zip(keys.iter()) {
        assert_eq!(a, b, "ordering must be consistent after reverse+re-sort");
    }
}

// ---------------------------------------------------------------------------
// 3. SensorReport serde roundtrip at scale (full report objects)
// ---------------------------------------------------------------------------

#[test]
fn stress_sensor_report_roundtrip() {
    let reports: Vec<SensorReport> = (0..100)
        .map(|i| {
            let findings: Vec<Finding> = (0..20)
                .map(|j| {
                    make_finding(
                        Severity::Warn,
                        &format!("W{}", j),
                        &format!("msg {} {}", i, j),
                        None,
                        None,
                    )
                })
                .collect();
            SensorReport {
                schema: "sensor.report.v1".to_string(),
                tool: ToolInfo {
                    name: format!("sensor-{}", i),
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
                    status: VerdictStatus::Warn,
                    counts: VerdictCounts {
                        info: 0,
                        warn: 20,
                        error: 0,
                        suppressed: 0,
                    },
                    reasons: vec![],
                },
                findings,
                artifacts: vec![],
                data: None,
            }
        })
        .collect();

    for report in &reports {
        let json = serde_json::to_string(report).unwrap();
        let parsed: SensorReport = serde_json::from_str(&json).unwrap();
        assert_eq!(*report, parsed);
    }
}

// ---------------------------------------------------------------------------
// 4. Highlight serde roundtrip at scale
// ---------------------------------------------------------------------------

#[test]
fn stress_highlight_roundtrip_500() {
    let highlights: Vec<Highlight> = (0..500)
        .map(|i| Highlight {
            sensor_id: format!("sensor-{}", i % 50),
            finding: make_finding(
                Severity::Error,
                &format!("E{}", i),
                &format!("error msg {}", i),
                Some(&format!("src/{}.rs", i)),
                Some(i as u32),
            ),
        })
        .collect();

    let json = serde_json::to_string(&highlights).unwrap();
    let parsed: Vec<Highlight> = serde_json::from_str(&json).unwrap();

    assert_eq!(highlights.len(), parsed.len());
    for (orig, rt) in highlights.iter().zip(parsed.iter()) {
        assert_eq!(orig, rt);
    }
}

// ---------------------------------------------------------------------------
// 5. severity_rank correctness across all variants (exhaustive)
// ---------------------------------------------------------------------------

#[test]
fn stress_severity_rank_ordering() {
    assert!(severity_rank(&Severity::Error) < severity_rank(&Severity::Warn));
    assert!(severity_rank(&Severity::Warn) < severity_rank(&Severity::Info));

    // Verify ranks are stable across 10000 calls.
    for _ in 0..10_000 {
        assert_eq!(severity_rank(&Severity::Error), 0);
        assert_eq!(severity_rank(&Severity::Warn), 1);
        assert_eq!(severity_rank(&Severity::Info), 2);
    }
}

// ---------------------------------------------------------------------------
// 6. Large JSON payload roundtrip (mimics big receipts)
// ---------------------------------------------------------------------------

#[test]
fn stress_large_json_roundtrip() {
    let findings: Vec<Finding> = (0..500)
        .map(|i| Finding {
            severity: Severity::Warn,
            check_id: Some(format!("check.{}", i)),
            code: format!("CODE-{}", i),
            message: "x".repeat(200),
            location: Some(Location {
                path: Some(format!("very/deep/nested/path/to/file{}.rs", i)),
                line: Some(i as u32),
                col: Some(i as u32 % 80),
            }),
            help: Some("some help text repeated".to_string()),
            url: Some(format!("https://example.com/issue/{}", i)),
            fingerprint: Some(format!("fp-{:064x}", i)),
            data: None,
        })
        .collect();

    let json = serde_json::to_string_pretty(&findings).unwrap();
    assert!(json.len() > 100_000, "payload should be substantial");

    let parsed: Vec<Finding> = serde_json::from_str(&json).unwrap();
    assert_eq!(findings, parsed);
}
