//! Stress tests for cockpitctl-domain: caps, budgets, and determinism at scale.

use std::collections::BTreeMap;

use cockpitctl_domain::{
    build_cockpit_report, cap_findings, derive_fingerprint, overall_verdict, select_highlights,
    sort_findings,
};
use cockpitctl_types::{
    CockpitConfig, Finding, Highlight, Location, MissingPolicy, PolicyOutcome, Presence, RunInfo,
    SensorSummary, Severity, ToolInfo, Verdict, VerdictCounts, VerdictStatus,
};

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

fn make_highlight(sensor_id: &str, severity: Severity, code: &str, message: &str) -> Highlight {
    Highlight {
        sensor_id: sensor_id.to_string(),
        finding: make_finding(severity, code, message, None, None),
    }
}

fn make_tool_and_run() -> (ToolInfo, RunInfo) {
    (
        ToolInfo {
            name: "cockpitctl".to_string(),
            version: "0.1.0".to_string(),
            commit: None,
        },
        RunInfo {
            started_at: "2026-01-01T00:00:00Z".to_string(),
            ended_at: None,
            duration_ms: None,
            host: None,
            git: None,
            ci: None,
            capabilities: BTreeMap::new(),
        },
    )
}

fn make_sensor_summary(id: &str, status: VerdictStatus, blocking: bool) -> SensorSummary {
    SensorSummary {
        id: id.to_string(),
        blocking,
        missing: MissingPolicy::Skip,
        presence: Presence::Present,
        report_path: format!("artifacts/{}/report.json", id),
        comment_path: None,
        verdict: Verdict {
            status,
            counts: VerdictCounts::default(),
            reasons: vec![],
        },
        truncated: false,
        errors: vec![],
        missing_policy_applied: None,
        policy_outcome: Some(PolicyOutcome::Informational),
    }
}

// ---------------------------------------------------------------------------
// 1. 1000 findings sort: deterministic order
// ---------------------------------------------------------------------------

#[test]
fn stress_1000_findings_sort_deterministic() {
    let severities = [Severity::Error, Severity::Warn, Severity::Info];
    let mut findings: Vec<Finding> = (0..1000)
        .map(|i| {
            make_finding(
                severities[i % 3].clone(),
                &format!("C{:04}", i),
                &format!("message {}", i),
                Some(&format!("src/file{}.rs", i % 50)),
                Some((i % 200) as u32),
            )
        })
        .collect();

    sort_findings("sensor-a", &mut findings);
    let sorted_a = findings.clone();

    // Shuffle via reverse and re-sort.
    findings.reverse();
    sort_findings("sensor-a", &mut findings);

    assert_eq!(sorted_a.len(), findings.len());
    for (a, b) in sorted_a.iter().zip(findings.iter()) {
        assert_eq!(a.code, b.code);
        assert_eq!(a.message, b.message);
    }

    // Errors must come first.
    assert_eq!(findings[0].severity, Severity::Error);
}

// ---------------------------------------------------------------------------
// 2. Zero budget: 1000 findings with budget=0 → empty highlights
// ---------------------------------------------------------------------------

#[test]
fn stress_zero_budget_yields_empty_highlights() {
    let candidates: Vec<Highlight> = (0..1000)
        .map(|i| {
            make_highlight(
                "sensor",
                Severity::Error,
                &format!("E{}", i),
                &format!("msg {}", i),
            )
        })
        .collect();

    let mut cfg = CockpitConfig::default();
    cfg.policy.max_highlights = 0;

    let blocking = BTreeMap::new();
    let selected = select_highlights(candidates, &cfg, &blocking);
    assert!(selected.is_empty());
}

// ---------------------------------------------------------------------------
// 3. Equal priority storm: 500 identical-severity findings → stable sort
// ---------------------------------------------------------------------------

#[test]
fn stress_equal_priority_stable_sort() {
    let mut findings: Vec<Finding> = (0..500)
        .map(|i| {
            make_finding(
                Severity::Warn,
                &format!("W{:04}", i),
                &format!("warning {}", i),
                Some(&format!("src/{}.rs", i % 20)),
                Some(i as u32),
            )
        })
        .collect();

    sort_findings("s", &mut findings);
    let first_pass = findings.clone();

    // Reverse and sort again.
    findings.reverse();
    sort_findings("s", &mut findings);

    for (a, b) in first_pass.iter().zip(findings.iter()) {
        assert_eq!(a.code, b.code, "sort must be stable across re-sorts");
    }
}

// ---------------------------------------------------------------------------
// 4. Large fingerprint set: 10000 fingerprints, no collisions
// ---------------------------------------------------------------------------

#[test]
fn stress_10000_fingerprints_no_collisions() {
    let mut seen = std::collections::HashSet::new();
    for i in 0..10_000u32 {
        let finding = make_finding(
            Severity::Error,
            &format!("C{}", i),
            &format!("msg {}", i),
            Some(&format!("path/{}.rs", i)),
            Some(i),
        );
        let fp = derive_fingerprint(&format!("sensor-{}", i % 100), &finding);
        assert_eq!(fp.len(), 64, "fingerprint must be 64 hex chars");
        assert!(seen.insert(fp), "fingerprint collision at i={}", i);
    }
}

// ---------------------------------------------------------------------------
// 5. Verdict aggregation at scale: 100 sensors with mixed verdicts
// ---------------------------------------------------------------------------

#[test]
fn stress_verdict_aggregation_100_sensors() {
    let statuses = [
        VerdictStatus::Pass,
        VerdictStatus::Warn,
        VerdictStatus::Fail,
        VerdictStatus::Skip,
    ];

    let summaries: Vec<SensorSummary> = (0..100)
        .map(|i| {
            let status = statuses[i % 4].clone();
            let blocking = i % 2 == 0;
            make_sensor_summary(&format!("sensor-{:03}", i), status, blocking)
        })
        .collect();

    let cfg = CockpitConfig::default();
    let verdict = overall_verdict(&summaries, &cfg);

    // At least one blocking sensor has Fail (i=2, blocking=true, Fail), so overall must be Fail.
    assert_eq!(verdict.status, VerdictStatus::Fail);
}

// ---------------------------------------------------------------------------
// 6. cap_findings at large scale
// ---------------------------------------------------------------------------

#[test]
fn stress_cap_findings_large_input() {
    let findings: Vec<Finding> = (0..5000)
        .map(|i| {
            make_finding(
                Severity::Info,
                &format!("I{}", i),
                &format!("m{}", i),
                None,
                None,
            )
        })
        .collect();

    let (capped, truncated) = cap_findings(findings, 20);
    assert_eq!(capped.len(), 20);
    assert!(truncated);
}

// ---------------------------------------------------------------------------
// 7. build_cockpit_report with many sensors and highlights
// ---------------------------------------------------------------------------

#[test]
fn stress_build_report_many_sensors() {
    let cfg = CockpitConfig::default();
    let (tool, run) = make_tool_and_run();

    let summaries: Vec<SensorSummary> = (0..100)
        .map(|i| make_sensor_summary(&format!("s{:03}", i), VerdictStatus::Pass, false))
        .collect();
    let highlights: Vec<Highlight> = (0..50)
        .map(|i| {
            make_highlight(
                &format!("s{:03}", i),
                Severity::Warn,
                &format!("W{}", i),
                &format!("m{}", i),
            )
        })
        .collect();

    let report = build_cockpit_report(&cfg, tool, run, summaries, highlights);
    assert_eq!(report.sensors.len(), 100);
    assert_eq!(report.highlights.len(), 50);
    assert_eq!(report.schema, "cockpit.report.v1");
}

// ---------------------------------------------------------------------------
// 8. Highlights dedup + cap under stress
// ---------------------------------------------------------------------------

#[test]
fn stress_highlight_selection_dedup_and_cap() {
    // 200 candidates with many duplicates (same fingerprint pattern).
    let mut candidates: Vec<Highlight> = Vec::new();
    for i in 0..200 {
        let mut h = make_highlight(
            &format!("sensor-{}", i % 10),
            Severity::Error,
            &format!("E{}", i % 20),
            &format!("msg {}", i % 20),
        );
        // Give half explicit fingerprints to test dedup.
        if i % 2 == 0 {
            h.finding.fingerprint = Some(format!("fp-{}", i % 20));
        }
        candidates.push(h);
    }

    let cfg = CockpitConfig::default(); // max_highlights = 7
    let blocking = BTreeMap::new();
    let selected = select_highlights(candidates, &cfg, &blocking);

    assert!(selected.len() <= cfg.policy.max_highlights);
    // Verify all fingerprints are unique.
    let fps: Vec<_> = selected
        .iter()
        .map(|h| h.finding.fingerprint.clone().unwrap())
        .collect();
    let unique: std::collections::HashSet<_> = fps.iter().collect();
    assert_eq!(
        fps.len(),
        unique.len(),
        "no duplicate fingerprints in output"
    );
}

// ---------------------------------------------------------------------------
// 9. warn_is_fail aggregation at scale
// ---------------------------------------------------------------------------

#[test]
fn stress_warn_is_fail_100_sensors() {
    let summaries: Vec<SensorSummary> = (0..100)
        .map(|i| make_sensor_summary(&format!("s{:03}", i), VerdictStatus::Warn, true))
        .collect();

    let mut cfg = CockpitConfig::default();
    cfg.policy.warn_is_fail = true;

    let verdict = overall_verdict(&summaries, &cfg);
    assert_eq!(verdict.status, VerdictStatus::Fail);
    assert!(verdict.reasons.contains(&"warn_is_fail".to_string()));
}
