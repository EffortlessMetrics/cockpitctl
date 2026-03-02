//! Wave-34 snapshot expansion for cockpitctl-sarif.
//!
//! Covers:
//!  - Full SARIF output for complex multi-sensor reports
//!  - SARIF with all finding types (all severities, with/without locations)
//!  - SARIF with maximum field population (fingerprints, columns, check_ids)

use cockpitctl_sarif::{cockpit_report_to_sarif, cockpit_report_to_sarif_json};
use cockpitctl_types::*;
use std::collections::BTreeMap;

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
            counts: VerdictCounts::default(),
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

fn make_highlight_full(
    sensor_id: &str,
    severity: Severity,
    check_id: Option<&str>,
    code: &str,
    message: &str,
    help: Option<&str>,
    url: Option<&str>,
    path: Option<&str>,
    line: Option<u32>,
    col: Option<u32>,
    fingerprint: Option<&str>,
) -> Highlight {
    Highlight {
        sensor_id: sensor_id.to_string(),
        finding: Finding {
            severity,
            check_id: check_id.map(|s| s.to_string()),
            code: code.to_string(),
            message: message.to_string(),
            location: path.map(|p| Location {
                path: Some(p.to_string()),
                line,
                col,
            }),
            help: help.map(|s| s.to_string()),
            url: url.map(|s| s.to_string()),
            fingerprint: fingerprint.map(|f| f.to_string()),
            data: None,
        },
    }
}

// =========================================================================
// 1. Complex multi-sensor SARIF with many rules and results
// =========================================================================

#[test]
fn snapshot_sarif_complex_multi_sensor() {
    let report = CockpitReport {
        highlights: vec![
            make_highlight(
                "builddiag",
                Severity::Error,
                "E0308",
                "mismatched types: expected &str, found String",
                Some("src/parser.rs"),
                Some(42),
                Some(10),
                Some("fp_parser_e0308"),
            ),
            make_highlight(
                "builddiag",
                Severity::Error,
                "E0412",
                "cannot find type `Config` in this scope",
                Some("src/main.rs"),
                Some(5),
                Some(15),
                None,
            ),
            make_highlight(
                "clippy",
                Severity::Warn,
                "clippy::unwrap_used",
                "used `unwrap()` on a Result value",
                Some("src/io.rs"),
                Some(88),
                None,
                Some("fp_io_unwrap"),
            ),
            make_highlight(
                "clippy",
                Severity::Warn,
                "clippy::todo",
                "TODO macro found",
                Some("src/renderer.rs"),
                Some(15),
                Some(5),
                None,
            ),
            make_highlight(
                "clippy",
                Severity::Info,
                "clippy::missing_docs",
                "missing documentation for function",
                Some("src/api.rs"),
                Some(1),
                None,
                None,
            ),
            make_highlight(
                "security",
                Severity::Error,
                "SEC-CVE-2024-001",
                "known vulnerability in dependency",
                None,
                None,
                None,
                Some("fp_sec_dep"),
            ),
            make_highlight(
                "security",
                Severity::Warn,
                "SEC-OUTDATED",
                "dependency version is outdated",
                None,
                None,
                None,
                None,
            ),
            make_highlight(
                "coverage",
                Severity::Info,
                "COV-BELOW",
                "line coverage 72% below threshold 80%",
                Some("src/complex_module.rs"),
                None,
                None,
                None,
            ),
        ],
        verdict: Verdict {
            status: VerdictStatus::Fail,
            counts: VerdictCounts {
                info: 2,
                warn: 3,
                error: 3,
                suppressed: 1,
            },
            reasons: vec!["build_errors".to_string(), "security_cve".to_string()],
        },
        ..base_report()
    };

    let sarif = cockpit_report_to_sarif(&report);
    insta::assert_json_snapshot!("sarif_complex_multi_sensor", sarif);
}

// =========================================================================
// 2. SARIF with all finding types and severity combinations
// =========================================================================

#[test]
fn snapshot_sarif_all_finding_types() {
    let report = CockpitReport {
        highlights: vec![
            // Error with full location
            make_highlight(
                "sensor-a",
                Severity::Error,
                "ERR-FULL",
                "error with full location",
                Some("src/full.rs"),
                Some(100),
                Some(25),
                Some("fp_full_err"),
            ),
            // Error without location
            make_highlight(
                "sensor-a",
                Severity::Error,
                "ERR-NOLOC",
                "error without any location",
                None,
                None,
                None,
                None,
            ),
            // Warning with path only (no line/col)
            make_highlight(
                "sensor-b",
                Severity::Warn,
                "WARN-PATH",
                "warning with path only",
                Some("src/partial.rs"),
                None,
                None,
                Some("fp_partial_warn"),
            ),
            // Warning with path and line (no col)
            make_highlight(
                "sensor-b",
                Severity::Warn,
                "WARN-LINE",
                "warning with line no col",
                Some("src/line_only.rs"),
                Some(50),
                None,
                None,
            ),
            // Info with everything
            make_highlight(
                "sensor-c",
                Severity::Info,
                "INFO-ALL",
                "informational with all fields",
                Some("src/info.rs"),
                Some(1),
                Some(1),
                Some("fp_info_all"),
            ),
            // Info without location
            make_highlight(
                "sensor-c",
                Severity::Info,
                "INFO-BARE",
                "bare informational finding",
                None,
                None,
                None,
                None,
            ),
        ],
        verdict: Verdict {
            status: VerdictStatus::Fail,
            counts: VerdictCounts {
                info: 2,
                warn: 2,
                error: 2,
                suppressed: 0,
            },
            reasons: vec![],
        },
        ..base_report()
    };

    let sarif = cockpit_report_to_sarif(&report);
    insta::assert_json_snapshot!("sarif_all_finding_types", sarif);
}

// =========================================================================
// 3. SARIF with maximum field population
// =========================================================================

#[test]
fn snapshot_sarif_maximum_fields() {
    let report = CockpitReport {
        highlights: vec![
            make_highlight_full(
                "builddiag",
                Severity::Error,
                Some("check_types"),
                "E0308",
                "mismatched types: expected `bool`, found `i32`",
                Some("See https://doc.rust-lang.org/error-index.html#E0308"),
                Some("https://doc.rust-lang.org/error-index.html#E0308"),
                Some("src/main.rs"),
                Some(42),
                Some(10),
                Some("fp_e0308_main_42"),
            ),
            make_highlight_full(
                "clippy",
                Severity::Warn,
                Some("lint_unwrap"),
                "clippy::unwrap_used",
                "used `unwrap()` on `Option` value",
                Some("Use `expect()` or handle the None case"),
                Some("https://rust-lang.github.io/rust-clippy/master/#unwrap_used"),
                Some("src/config.rs"),
                Some(88),
                Some(22),
                Some("fp_clippy_unwrap_88"),
            ),
            make_highlight_full(
                "security",
                Severity::Error,
                Some("vuln_scan"),
                "CVE-2024-99999",
                "critical vulnerability in serde_json < 1.0.100",
                Some("Upgrade serde_json to >= 1.0.100"),
                Some("https://nvd.nist.gov/vuln/detail/CVE-2024-99999"),
                None,
                None,
                None,
                Some("fp_cve_serde"),
            ),
        ],
        verdict: Verdict {
            status: VerdictStatus::Fail,
            counts: VerdictCounts {
                info: 0,
                warn: 1,
                error: 2,
                suppressed: 0,
            },
            reasons: vec!["build_error".to_string(), "cve_found".to_string()],
        },
        ..base_report()
    };

    let sarif = cockpit_report_to_sarif(&report);
    insta::assert_json_snapshot!("sarif_maximum_fields", sarif);
}

// =========================================================================
// 4. SARIF JSON string output for large report
// =========================================================================

#[test]
fn snapshot_sarif_json_complex_roundtrip() {
    let report = CockpitReport {
        highlights: vec![
            make_highlight(
                "builddiag",
                Severity::Error,
                "E0308",
                "type mismatch",
                Some("src/a.rs"),
                Some(1),
                Some(1),
                Some("fp_a"),
            ),
            make_highlight(
                "builddiag",
                Severity::Error,
                "E0308",
                "another type mismatch",
                Some("src/b.rs"),
                Some(2),
                None,
                Some("fp_b"),
            ),
            make_highlight(
                "clippy",
                Severity::Warn,
                "clippy::todo",
                "TODO found",
                Some("src/c.rs"),
                Some(3),
                None,
                None,
            ),
            make_highlight(
                "scanner",
                Severity::Info,
                "SCAN-001",
                "scan note",
                None,
                None,
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

    let json_str = cockpit_report_to_sarif_json(&report).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
    insta::assert_json_snapshot!("sarif_json_complex_roundtrip", parsed);
}

// =========================================================================
// 5. SARIF with duplicated rules across sensors
// =========================================================================

#[test]
fn snapshot_sarif_cross_sensor_rule_dedup() {
    let report = CockpitReport {
        highlights: vec![
            make_highlight(
                "sensor-a",
                Severity::Error,
                "SHARED-001",
                "shared code from sensor A",
                Some("src/a.rs"),
                Some(10),
                None,
                None,
            ),
            make_highlight(
                "sensor-b",
                Severity::Warn,
                "SHARED-001",
                "shared code from sensor B",
                Some("src/b.rs"),
                Some(20),
                None,
                None,
            ),
            make_highlight(
                "sensor-a",
                Severity::Info,
                "UNIQUE-A",
                "unique to sensor A",
                Some("src/a.rs"),
                Some(30),
                None,
                None,
            ),
            make_highlight(
                "sensor-b",
                Severity::Error,
                "UNIQUE-B",
                "unique to sensor B",
                Some("src/b.rs"),
                Some(40),
                Some(5),
                Some("fp_unique_b"),
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
    insta::assert_json_snapshot!("sarif_cross_sensor_rule_dedup", sarif);
}
