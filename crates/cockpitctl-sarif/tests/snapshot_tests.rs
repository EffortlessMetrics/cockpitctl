use std::collections::BTreeMap;

use cockpitctl_sarif::cockpit_report_to_sarif;
use cockpitctl_types::*;

// ── Helpers ─────────────────────────────────────────────────────────────

fn base_report() -> CockpitReport {
    CockpitReport {
        schema: "cockpit.report.v1".to_string(),
        tool: ToolInfo {
            name: "cockpitctl".to_string(),
            version: "0.3.0".to_string(),
            commit: None,
        },
        run: RunInfo {
            started_at: "2026-02-01T00:00:00Z".to_string(),
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
            max_highlights: 7,
            max_per_sensor_findings: 20,
            max_annotations: 25,
            section_order: vec![],
            sensors: vec![],
        },
        data: None,
    }
}

#[allow(clippy::too_many_arguments)]
fn make_highlight(
    sensor_id: &str,
    severity: Severity,
    code: &str,
    message: &str,
    path: Option<&str>,
    line: Option<u32>,
    col: Option<u32>,
    fingerprint: Option<&str>,
) -> Highlight {
    Highlight {
        sensor_id: sensor_id.to_string(),
        finding: Finding {
            severity,
            check_id: None,
            code: code.to_string(),
            message: message.to_string(),
            location: path.map(|p| Location {
                path: Some(p.to_string()),
                line,
                col,
            }),
            help: None,
            url: None,
            fingerprint: fingerprint.map(|f| f.to_string()),
            data: None,
        },
    }
}

// ── Snapshot: typical cockpit report ────────────────────────────────────

#[test]
fn snapshot_typical_cockpit_report() {
    let report = CockpitReport {
        highlights: vec![
            make_highlight(
                "clippy",
                Severity::Error,
                "clippy::unwrap_used",
                "used `unwrap()` on a Result value",
                Some("src/main.rs"),
                Some(42),
                Some(10),
                Some("fp_abc123"),
            ),
            make_highlight(
                "clippy",
                Severity::Warn,
                "clippy::todo",
                "TODO found",
                Some("src/lib.rs"),
                Some(10),
                None,
                None,
            ),
        ],
        verdict: Verdict {
            status: VerdictStatus::Warn,
            counts: VerdictCounts {
                info: 0,
                warn: 1,
                error: 1,
                suppressed: 0,
            },
            reasons: vec![],
        },
        ..base_report()
    };

    let sarif = cockpit_report_to_sarif(&report);
    insta::assert_json_snapshot!("typical_cockpit_report", sarif);
}

// ── Snapshot: multiple sensors and findings ─────────────────────────────

#[test]
fn snapshot_multi_sensor_findings() {
    let report = CockpitReport {
        highlights: vec![
            make_highlight(
                "clippy",
                Severity::Error,
                "clippy::unwrap_used",
                "unwrap in main",
                Some("src/main.rs"),
                Some(10),
                None,
                None,
            ),
            make_highlight(
                "clippy",
                Severity::Warn,
                "clippy::todo",
                "todo in lib",
                Some("src/lib.rs"),
                Some(20),
                None,
                None,
            ),
            make_highlight(
                "builddiag",
                Severity::Error,
                "E0308",
                "mismatched types",
                Some("src/utils.rs"),
                Some(5),
                Some(12),
                Some("fp_build"),
            ),
            make_highlight(
                "builddiag",
                Severity::Info,
                "E0308",
                "expected bool, found i32",
                Some("src/utils.rs"),
                Some(6),
                None,
                None,
            ),
        ],
        verdict: Verdict {
            status: VerdictStatus::Fail,
            counts: VerdictCounts {
                info: 1,
                warn: 1,
                error: 2,
                suppressed: 0,
            },
            reasons: vec![],
        },
        ..base_report()
    };

    let sarif = cockpit_report_to_sarif(&report);
    insta::assert_json_snapshot!("multi_sensor_findings", sarif);
}

// ── Snapshot: empty report (no findings) ────────────────────────────────

#[test]
fn snapshot_empty_report() {
    let report = base_report();
    let sarif = cockpit_report_to_sarif(&report);
    insta::assert_json_snapshot!("empty_report", sarif);
}

// ── Snapshot: all severity levels ───────────────────────────────────────

#[test]
fn snapshot_all_severity_levels() {
    let report = CockpitReport {
        highlights: vec![
            make_highlight(
                "sensor-a",
                Severity::Error,
                "ERR001",
                "critical failure detected",
                Some("src/critical.rs"),
                Some(100),
                Some(5),
                Some("fp_err"),
            ),
            make_highlight(
                "sensor-a",
                Severity::Warn,
                "WARN001",
                "deprecated API usage",
                Some("src/compat.rs"),
                Some(55),
                None,
                None,
            ),
            make_highlight(
                "sensor-b",
                Severity::Info,
                "INFO001",
                "consider using a constant here",
                Some("src/config.rs"),
                Some(12),
                Some(1),
                Some("fp_info"),
            ),
        ],
        verdict: Verdict {
            status: VerdictStatus::Fail,
            counts: VerdictCounts {
                info: 1,
                warn: 1,
                error: 1,
                suppressed: 0,
            },
            reasons: vec![],
        },
        ..base_report()
    };

    let sarif = cockpit_report_to_sarif(&report);
    insta::assert_json_snapshot!("all_severity_levels", sarif);
}
