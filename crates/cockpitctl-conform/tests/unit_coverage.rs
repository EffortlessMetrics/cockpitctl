//! Comprehensive unit-level coverage tests for cockpitctl-conform.
//!
//! These tests exercise the public API surface of the conformance library,
//! targeting edge cases and branch coverage gaps not fully covered by the
//! inline `#[cfg(test)]` modules or the conformctl binary integration tests.

use std::collections::BTreeMap;

use cockpitctl_conform::{
    ConformChecks, ConformResult, Violation, check_artifact_pointers, check_cockpit_reason_tokens,
    check_determinism, check_ordering, check_path_hygiene, check_presence_semantics,
    check_reason_tokens, check_sensor_id_format, check_tool_error_identity, is_valid_reason_token,
};
use cockpitctl_conform::{check_cockpit_extended, conform_single, validate_cockpit_schema};
use cockpitctl_types::*;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn minimal_sensor_report() -> SensorReport {
    SensorReport {
        schema: "sensor.report.v1".to_string(),
        tool: ToolInfo {
            name: "test".to_string(),
            version: "1.0.0".to_string(),
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
        findings: vec![],
        artifacts: vec![],
        data: None,
    }
}

fn minimal_cockpit_report() -> CockpitReport {
    let cfg = CockpitConfig::default();
    let policy = PolicySnapshot {
        warn_is_fail: cfg.policy.warn_is_fail,
        max_highlights: cfg.policy.max_highlights,
        max_per_sensor_findings: cfg.policy.max_per_sensor_findings,
        max_annotations: cfg.policy.max_annotations,
        section_order: cfg.policy.section_order.clone(),
        sensors: vec![],
    };
    CockpitReport {
        schema: "cockpit.report.v1".to_string(),
        tool: ToolInfo {
            name: "cockpitctl".to_string(),
            version: "0.2.0".to_string(),
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
        policy,
        data: None,
    }
}

fn make_finding(severity: Severity, code: &str, msg: &str) -> Finding {
    Finding {
        severity,
        check_id: None,
        code: code.to_string(),
        message: msg.to_string(),
        location: None,
        help: None,
        url: None,
        fingerprint: None,
        data: None,
    }
}

fn make_finding_with_path(severity: Severity, code: &str, path: &str, line: u32) -> Finding {
    Finding {
        severity,
        check_id: None,
        code: code.to_string(),
        message: "msg".to_string(),
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

fn all_checks() -> ConformChecks {
    ConformChecks {
        path_hygiene: true,
        ordering: true,
        reason_lint: true,
        survivability: true,
        tool_error_identity: true,
        sensor_id_format: true,
        artifact_pointers: true,
    }
}

fn no_checks() -> ConformChecks {
    ConformChecks {
        path_hygiene: false,
        ordering: false,
        reason_lint: false,
        survivability: false,
        tool_error_identity: false,
        sensor_id_format: false,
        artifact_pointers: false,
    }
}

fn to_json<T: serde::Serialize>(val: &T) -> String {
    serde_json::to_string(val).expect("serialize")
}

// ===========================================================================
// is_valid_reason_token
// ===========================================================================

#[test]
fn reason_token_valid_lowercase_alpha() {
    assert!(is_valid_reason_token("abc"));
}

#[test]
fn reason_token_valid_digits_only() {
    assert!(is_valid_reason_token("123"));
}

#[test]
fn reason_token_valid_underscores_only() {
    assert!(is_valid_reason_token("___"));
}

#[test]
fn reason_token_valid_mixed() {
    assert!(is_valid_reason_token("tool_error_42"));
}

#[test]
fn reason_token_invalid_empty() {
    assert!(!is_valid_reason_token(""));
}

#[test]
fn reason_token_invalid_uppercase() {
    assert!(!is_valid_reason_token("ToolError"));
}

#[test]
fn reason_token_invalid_hyphen() {
    assert!(!is_valid_reason_token("tool-error"));
}

#[test]
fn reason_token_invalid_spaces() {
    assert!(!is_valid_reason_token("tool error"));
}

#[test]
fn reason_token_invalid_dot() {
    assert!(!is_valid_reason_token("tool.error"));
}

#[test]
fn reason_token_invalid_leading_space() {
    assert!(!is_valid_reason_token(" abc"));
}

#[test]
fn reason_token_invalid_unicode() {
    assert!(!is_valid_reason_token("caf\u{00e9}"));
}

// ===========================================================================
// check_path_hygiene
// ===========================================================================

#[test]
fn path_hygiene_clean_relative_path() {
    let mut report = minimal_sensor_report();
    report.findings.push(make_finding_with_path(
        Severity::Info,
        "I1",
        "src/main.rs",
        1,
    ));
    assert!(check_path_hygiene(&report).is_empty());
}

#[test]
fn path_hygiene_no_findings() {
    let report = minimal_sensor_report();
    assert!(check_path_hygiene(&report).is_empty());
}

#[test]
fn path_hygiene_finding_without_location() {
    let mut report = minimal_sensor_report();
    report
        .findings
        .push(make_finding(Severity::Info, "I1", "msg"));
    assert!(check_path_hygiene(&report).is_empty());
}

#[test]
fn path_hygiene_finding_with_location_no_path() {
    let mut report = minimal_sensor_report();
    report.findings.push(Finding {
        severity: Severity::Info,
        check_id: None,
        code: "I1".to_string(),
        message: "msg".to_string(),
        location: Some(Location {
            path: None,
            line: Some(1),
            col: None,
        }),
        help: None,
        url: None,
        fingerprint: None,
        data: None,
    });
    assert!(check_path_hygiene(&report).is_empty());
}

#[test]
fn path_hygiene_absolute_unix() {
    let mut report = minimal_sensor_report();
    report.findings.push(make_finding_with_path(
        Severity::Info,
        "I1",
        "/etc/passwd",
        1,
    ));
    let v = check_path_hygiene(&report);
    assert_eq!(v.len(), 1);
    assert!(v[0].contains("absolute path"));
}

#[test]
fn path_hygiene_absolute_backslash_start() {
    let mut report = minimal_sensor_report();
    report.findings.push(make_finding_with_path(
        Severity::Info,
        "I1",
        "\\server\\share",
        1,
    ));
    let v = check_path_hygiene(&report);
    // Should flag both absolute path and backslash.
    assert!(v.iter().any(|msg| msg.contains("absolute path")));
    assert!(v.iter().any(|msg| msg.contains("backslash")));
}

#[test]
fn path_hygiene_drive_letter() {
    let mut report = minimal_sensor_report();
    report.findings.push(make_finding_with_path(
        Severity::Info,
        "I1",
        "D:\\code\\file.rs",
        1,
    ));
    let v = check_path_hygiene(&report);
    assert!(v.iter().any(|msg| msg.contains("drive letter")));
    assert!(v.iter().any(|msg| msg.contains("backslash")));
}

#[test]
fn path_hygiene_traversal() {
    let mut report = minimal_sensor_report();
    report.findings.push(make_finding_with_path(
        Severity::Info,
        "I1",
        "foo/../bar/secret",
        1,
    ));
    let v = check_path_hygiene(&report);
    assert!(v.iter().any(|msg| msg.contains("path traversal")));
}

#[test]
fn path_hygiene_backslash_relative() {
    let mut report = minimal_sensor_report();
    report.findings.push(make_finding_with_path(
        Severity::Info,
        "I1",
        "src\\main.rs",
        1,
    ));
    let v = check_path_hygiene(&report);
    assert_eq!(v.len(), 1);
    assert!(v[0].contains("backslash"));
}

#[test]
fn path_hygiene_multiple_findings_multiple_violations() {
    let mut report = minimal_sensor_report();
    report
        .findings
        .push(make_finding_with_path(Severity::Info, "I1", "/abs/path", 1));
    report.findings.push(make_finding_with_path(
        Severity::Warn,
        "W1",
        "ok/path.rs",
        2,
    ));
    report.findings.push(make_finding_with_path(
        Severity::Error,
        "E1",
        "../traversal",
        3,
    ));
    let v = check_path_hygiene(&report);
    // Two bad paths, one clean.
    assert_eq!(v.len(), 2);
}

// ===========================================================================
// check_ordering
// ===========================================================================

#[test]
fn ordering_empty_findings() {
    let report = minimal_sensor_report();
    assert!(check_ordering(&report, "sensor").is_empty());
}

#[test]
fn ordering_single_finding() {
    let mut report = minimal_sensor_report();
    report
        .findings
        .push(make_finding(Severity::Error, "E1", "err"));
    assert!(check_ordering(&report, "sensor").is_empty());
}

#[test]
fn ordering_correctly_sorted_by_severity() {
    let mut report = minimal_sensor_report();
    // Canonical order: Error (rank 0) before Warn (rank 1) before Info (rank 2).
    report
        .findings
        .push(make_finding(Severity::Error, "E1", "err"));
    report
        .findings
        .push(make_finding(Severity::Warn, "W1", "warn"));
    report
        .findings
        .push(make_finding(Severity::Info, "I1", "info"));
    assert!(check_ordering(&report, "sensor").is_empty());
}

#[test]
fn ordering_misordered_info_before_error() {
    let mut report = minimal_sensor_report();
    report
        .findings
        .push(make_finding(Severity::Info, "I1", "info"));
    report
        .findings
        .push(make_finding(Severity::Error, "E1", "err"));
    let v = check_ordering(&report, "sensor");
    assert_eq!(v.len(), 1);
    assert!(v[0].contains("out of order"));
}

#[test]
fn ordering_same_severity_sorted_by_code() {
    let mut report = minimal_sensor_report();
    report
        .findings
        .push(make_finding(Severity::Error, "A1", "alpha"));
    report
        .findings
        .push(make_finding(Severity::Error, "B1", "beta"));
    assert!(check_ordering(&report, "sensor").is_empty());
}

#[test]
fn ordering_same_severity_wrong_code_order() {
    let mut report = minimal_sensor_report();
    report
        .findings
        .push(make_finding(Severity::Error, "Z1", "zeta"));
    report
        .findings
        .push(make_finding(Severity::Error, "A1", "alpha"));
    let v = check_ordering(&report, "sensor");
    assert_eq!(v.len(), 1);
}

#[test]
fn ordering_path_tiebreaker() {
    let mut report = minimal_sensor_report();
    report
        .findings
        .push(make_finding_with_path(Severity::Error, "E1", "src/a.rs", 1));
    report
        .findings
        .push(make_finding_with_path(Severity::Error, "E1", "src/b.rs", 1));
    assert!(check_ordering(&report, "sensor").is_empty());
}

#[test]
fn ordering_path_tiebreaker_wrong_order() {
    let mut report = minimal_sensor_report();
    report
        .findings
        .push(make_finding_with_path(Severity::Error, "E1", "src/z.rs", 1));
    report
        .findings
        .push(make_finding_with_path(Severity::Error, "E1", "src/a.rs", 1));
    let v = check_ordering(&report, "sensor");
    assert_eq!(v.len(), 1);
}

#[test]
fn ordering_line_tiebreaker() {
    let mut report = minimal_sensor_report();
    report.findings.push(make_finding_with_path(
        Severity::Error,
        "E1",
        "src/a.rs",
        10,
    ));
    report.findings.push(make_finding_with_path(
        Severity::Error,
        "E1",
        "src/a.rs",
        20,
    ));
    assert!(check_ordering(&report, "sensor").is_empty());
}

#[test]
fn ordering_line_tiebreaker_wrong_order() {
    let mut report = minimal_sensor_report();
    report.findings.push(make_finding_with_path(
        Severity::Error,
        "E1",
        "src/a.rs",
        20,
    ));
    report.findings.push(make_finding_with_path(
        Severity::Error,
        "E1",
        "src/a.rs",
        10,
    ));
    let v = check_ordering(&report, "sensor");
    assert_eq!(v.len(), 1);
}

#[test]
fn ordering_message_tiebreaker() {
    let mut report = minimal_sensor_report();
    report.findings.push(Finding {
        severity: Severity::Error,
        check_id: None,
        code: "E1".to_string(),
        message: "alpha".to_string(),
        location: Some(Location {
            path: Some("src/a.rs".to_string()),
            line: Some(1),
            col: None,
        }),
        help: None,
        url: None,
        fingerprint: None,
        data: None,
    });
    report.findings.push(Finding {
        severity: Severity::Error,
        check_id: None,
        code: "E1".to_string(),
        message: "beta".to_string(),
        location: Some(Location {
            path: Some("src/a.rs".to_string()),
            line: Some(1),
            col: None,
        }),
        help: None,
        url: None,
        fingerprint: None,
        data: None,
    });
    assert!(check_ordering(&report, "sensor").is_empty());
}

#[test]
fn ordering_multiple_misordered_pairs() {
    let mut report = minimal_sensor_report();
    // Info, Error, Warn -> two violations (Info>Error and Error>Warn is fine, but Info>Error).
    // Actually: Info(rank=2), Error(rank=0), Warn(rank=1)
    // Pair (0,1): Info(2) > Error(0) -> violation
    // Pair (1,2): Error(0) < Warn(1) -> ok
    report
        .findings
        .push(make_finding(Severity::Info, "I1", "info"));
    report
        .findings
        .push(make_finding(Severity::Error, "E1", "err"));
    report
        .findings
        .push(make_finding(Severity::Warn, "W1", "warn"));
    let v = check_ordering(&report, "sensor");
    assert_eq!(v.len(), 1);
}

// ===========================================================================
// check_reason_tokens (sensor report)
// ===========================================================================

#[test]
fn reason_tokens_empty_reasons_list() {
    let report = minimal_sensor_report();
    assert!(check_reason_tokens(&report).is_empty());
}

#[test]
fn reason_tokens_all_valid() {
    let mut report = minimal_sensor_report();
    report.verdict.reasons = vec!["no_findings".to_string(), "tool_error".to_string()];
    assert!(check_reason_tokens(&report).is_empty());
}

#[test]
fn reason_tokens_invalid_verdict_reason() {
    let mut report = minimal_sensor_report();
    report.verdict.reasons = vec!["UPPER".to_string()];
    let v = check_reason_tokens(&report);
    assert_eq!(v.len(), 1);
    assert!(v[0].contains("verdict.reasons[0]"));
}

#[test]
fn reason_tokens_multiple_invalid() {
    let mut report = minimal_sensor_report();
    report.verdict.reasons = vec![
        "valid_one".to_string(),
        "Bad-Two".to_string(),
        "Bad Three".to_string(),
    ];
    let v = check_reason_tokens(&report);
    assert_eq!(v.len(), 2);
}

#[test]
fn reason_tokens_capability_valid() {
    let mut report = minimal_sensor_report();
    report.run.capabilities.insert(
        "git".to_string(),
        Capability {
            status: CapabilityStatus::Available,
            reason: Some("valid_reason".to_string()),
        },
    );
    assert!(check_reason_tokens(&report).is_empty());
}

#[test]
fn reason_tokens_capability_none_reason() {
    let mut report = minimal_sensor_report();
    report.run.capabilities.insert(
        "git".to_string(),
        Capability {
            status: CapabilityStatus::Available,
            reason: None,
        },
    );
    assert!(check_reason_tokens(&report).is_empty());
}

#[test]
fn reason_tokens_capability_invalid() {
    let mut report = minimal_sensor_report();
    report.run.capabilities.insert(
        "git".to_string(),
        Capability {
            status: CapabilityStatus::Skipped,
            reason: Some("Not Available".to_string()),
        },
    );
    let v = check_reason_tokens(&report);
    assert_eq!(v.len(), 1);
    assert!(v[0].contains("capabilities.git.reason"));
}

#[test]
fn reason_tokens_empty_string_reason() {
    let mut report = minimal_sensor_report();
    report.verdict.reasons = vec!["".to_string()];
    let v = check_reason_tokens(&report);
    assert_eq!(v.len(), 1);
}

// ===========================================================================
// check_sensor_id_format
// ===========================================================================

#[test]
fn sensor_id_format_valid_alpha() {
    assert!(check_sensor_id_format("builddiag").is_empty());
}

#[test]
fn sensor_id_format_valid_alphanumeric() {
    assert!(check_sensor_id_format("sensor123").is_empty());
}

#[test]
fn sensor_id_format_valid_with_underscore() {
    assert!(check_sensor_id_format("my_sensor").is_empty());
}

#[test]
fn sensor_id_format_valid_with_hyphen() {
    assert!(check_sensor_id_format("my-sensor").is_empty());
}

#[test]
fn sensor_id_format_valid_mixed() {
    assert!(check_sensor_id_format("Build-Diag_v2").is_empty());
}

#[test]
fn sensor_id_format_invalid_empty() {
    let v = check_sensor_id_format("");
    assert_eq!(v.len(), 1);
    assert!(v[0].contains("does not match"));
}

#[test]
fn sensor_id_format_invalid_dot() {
    let v = check_sensor_id_format("bad.id");
    assert_eq!(v.len(), 1);
}

#[test]
fn sensor_id_format_invalid_slash() {
    let v = check_sensor_id_format("bad/id");
    assert_eq!(v.len(), 1);
}

#[test]
fn sensor_id_format_invalid_space() {
    let v = check_sensor_id_format("bad id");
    assert_eq!(v.len(), 1);
}

#[test]
fn sensor_id_format_invalid_traversal() {
    let v = check_sensor_id_format("../evil");
    assert_eq!(v.len(), 1);
}

#[test]
fn sensor_id_format_invalid_unicode() {
    let v = check_sensor_id_format("s\u{00e9}nsor");
    assert_eq!(v.len(), 1);
}

#[test]
fn sensor_id_format_single_char() {
    assert!(check_sensor_id_format("a").is_empty());
}

#[test]
fn sensor_id_format_single_digit() {
    assert!(check_sensor_id_format("1").is_empty());
}

// ===========================================================================
// check_tool_error_identity
// ===========================================================================

#[test]
fn tool_error_identity_no_tool_error_reason() {
    let report = minimal_sensor_report();
    assert!(check_tool_error_identity(&report).is_empty());
}

#[test]
fn tool_error_identity_tool_error_without_canonical_finding() {
    let mut report = minimal_sensor_report();
    report.verdict.reasons = vec!["tool_error".to_string()];
    let v = check_tool_error_identity(&report);
    assert_eq!(v.len(), 1);
    assert!(v[0].contains("tool.runtime"));
}

#[test]
fn tool_error_identity_tool_error_with_canonical_finding() {
    let mut report = minimal_sensor_report();
    report.verdict.reasons = vec!["tool_error".to_string()];
    report.findings.push(Finding {
        severity: Severity::Error,
        check_id: Some("tool.runtime".to_string()),
        code: "runtime_error".to_string(),
        message: "something crashed".to_string(),
        location: None,
        help: None,
        url: None,
        fingerprint: None,
        data: None,
    });
    assert!(check_tool_error_identity(&report).is_empty());
}

#[test]
fn tool_error_identity_wrong_check_id() {
    let mut report = minimal_sensor_report();
    report.verdict.reasons = vec!["tool_error".to_string()];
    report.findings.push(Finding {
        severity: Severity::Error,
        check_id: Some("tool.other".to_string()),
        code: "runtime_error".to_string(),
        message: "crash".to_string(),
        location: None,
        help: None,
        url: None,
        fingerprint: None,
        data: None,
    });
    let v = check_tool_error_identity(&report);
    assert_eq!(v.len(), 1);
}

#[test]
fn tool_error_identity_wrong_code() {
    let mut report = minimal_sensor_report();
    report.verdict.reasons = vec!["tool_error".to_string()];
    report.findings.push(Finding {
        severity: Severity::Error,
        check_id: Some("tool.runtime".to_string()),
        code: "wrong_code".to_string(),
        message: "crash".to_string(),
        location: None,
        help: None,
        url: None,
        fingerprint: None,
        data: None,
    });
    let v = check_tool_error_identity(&report);
    assert_eq!(v.len(), 1);
}

#[test]
fn tool_error_identity_tool_error_among_other_reasons() {
    let mut report = minimal_sensor_report();
    report.verdict.reasons = vec![
        "no_findings".to_string(),
        "tool_error".to_string(),
        "other".to_string(),
    ];
    // No canonical finding present.
    let v = check_tool_error_identity(&report);
    assert_eq!(v.len(), 1);
}

#[test]
fn tool_error_identity_canonical_finding_among_many() {
    let mut report = minimal_sensor_report();
    report.verdict.reasons = vec!["tool_error".to_string()];
    report
        .findings
        .push(make_finding(Severity::Warn, "W1", "warn msg"));
    report.findings.push(Finding {
        severity: Severity::Error,
        check_id: Some("tool.runtime".to_string()),
        code: "runtime_error".to_string(),
        message: "crash".to_string(),
        location: None,
        help: None,
        url: None,
        fingerprint: None,
        data: None,
    });
    report
        .findings
        .push(make_finding(Severity::Info, "I1", "info msg"));
    assert!(check_tool_error_identity(&report).is_empty());
}

// ===========================================================================
// check_artifact_pointers
// ===========================================================================

#[test]
fn artifact_pointers_no_artifacts() {
    let report = minimal_sensor_report();
    assert!(check_artifact_pointers(&report).is_empty());
}

#[test]
fn artifact_pointers_valid() {
    let mut report = minimal_sensor_report();
    report.artifacts = vec![ArtifactPointer {
        id: "log".to_string(),
        path: "artifacts/build.log".to_string(),
        mime: "text/plain".to_string(),
        schema: None,
    }];
    assert!(check_artifact_pointers(&report).is_empty());
}

#[test]
fn artifact_pointers_valid_with_schema() {
    let mut report = minimal_sensor_report();
    report.artifacts = vec![ArtifactPointer {
        id: "plan".to_string(),
        path: "artifacts/plan.json".to_string(),
        mime: "application/json".to_string(),
        schema: Some("buildfix.plan.v1".to_string()),
    }];
    assert!(check_artifact_pointers(&report).is_empty());
}

#[test]
fn artifact_pointers_empty_id() {
    let mut report = minimal_sensor_report();
    report.artifacts = vec![ArtifactPointer {
        id: "".to_string(),
        path: "ok/path.txt".to_string(),
        mime: "text/plain".to_string(),
        schema: None,
    }];
    let v = check_artifact_pointers(&report);
    assert_eq!(v.len(), 1);
    assert!(v[0].contains("id is empty"));
}

#[test]
fn artifact_pointers_empty_path() {
    let mut report = minimal_sensor_report();
    report.artifacts = vec![ArtifactPointer {
        id: "log".to_string(),
        path: "".to_string(),
        mime: "text/plain".to_string(),
        schema: None,
    }];
    let v = check_artifact_pointers(&report);
    assert_eq!(v.len(), 1);
    assert!(v[0].contains("path is empty"));
}

#[test]
fn artifact_pointers_empty_mime() {
    let mut report = minimal_sensor_report();
    report.artifacts = vec![ArtifactPointer {
        id: "log".to_string(),
        path: "ok/path.txt".to_string(),
        mime: "".to_string(),
        schema: None,
    }];
    let v = check_artifact_pointers(&report);
    assert_eq!(v.len(), 1);
    assert!(v[0].contains("mime is empty"));
}

#[test]
fn artifact_pointers_all_empty() {
    let mut report = minimal_sensor_report();
    report.artifacts = vec![ArtifactPointer {
        id: "".to_string(),
        path: "".to_string(),
        mime: "".to_string(),
        schema: None,
    }];
    let v = check_artifact_pointers(&report);
    // id empty + path empty + mime empty = 3 violations.
    assert_eq!(v.len(), 3);
}

#[test]
fn artifact_pointers_path_traversal() {
    let mut report = minimal_sensor_report();
    report.artifacts = vec![ArtifactPointer {
        id: "evil".to_string(),
        path: "artifacts/../../../etc/passwd".to_string(),
        mime: "text/plain".to_string(),
        schema: None,
    }];
    let v = check_artifact_pointers(&report);
    assert_eq!(v.len(), 1);
    assert!(v[0].contains(".."));
}

#[test]
fn artifact_pointers_absolute_unix() {
    let mut report = minimal_sensor_report();
    report.artifacts = vec![ArtifactPointer {
        id: "abs".to_string(),
        path: "/etc/passwd".to_string(),
        mime: "text/plain".to_string(),
        schema: None,
    }];
    let v = check_artifact_pointers(&report);
    assert_eq!(v.len(), 1);
    assert!(v[0].contains("absolute"));
}

#[test]
fn artifact_pointers_absolute_backslash() {
    let mut report = minimal_sensor_report();
    report.artifacts = vec![ArtifactPointer {
        id: "abs".to_string(),
        path: "\\server\\share\\file.txt".to_string(),
        mime: "text/plain".to_string(),
        schema: None,
    }];
    let v = check_artifact_pointers(&report);
    assert_eq!(v.len(), 1);
    assert!(v[0].contains("absolute"));
}

#[test]
fn artifact_pointers_drive_letter() {
    let mut report = minimal_sensor_report();
    report.artifacts = vec![ArtifactPointer {
        id: "win".to_string(),
        path: "C:\\Users\\file.txt".to_string(),
        mime: "text/plain".to_string(),
        schema: None,
    }];
    let v = check_artifact_pointers(&report);
    assert!(v.iter().any(|msg| msg.contains("drive letter")));
}

#[test]
fn artifact_pointers_multiple_artifacts_mixed() {
    let mut report = minimal_sensor_report();
    report.artifacts = vec![
        ArtifactPointer {
            id: "good".to_string(),
            path: "artifacts/ok.txt".to_string(),
            mime: "text/plain".to_string(),
            schema: None,
        },
        ArtifactPointer {
            id: "bad".to_string(),
            path: "../escape".to_string(),
            mime: "text/plain".to_string(),
            schema: None,
        },
    ];
    let v = check_artifact_pointers(&report);
    assert_eq!(v.len(), 1);
    assert!(v[0].contains("artifacts[1]"));
}

// ===========================================================================
// check_presence_semantics (cockpit report)
// ===========================================================================

#[test]
fn presence_semantics_no_sensors() {
    let report = minimal_cockpit_report();
    assert!(check_presence_semantics(&report).is_empty());
}

#[test]
fn presence_semantics_present_no_missing_policy() {
    let mut report = minimal_cockpit_report();
    report.sensors.push(SensorSummary {
        id: "sensor".to_string(),
        blocking: false,
        missing: MissingPolicy::Skip,
        presence: Presence::Present,
        report_path: "artifacts/sensor/report.json".to_string(),
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
    });
    assert!(check_presence_semantics(&report).is_empty());
}

#[test]
fn presence_semantics_missing_with_missing_policy_applied() {
    let mut report = minimal_cockpit_report();
    report.sensors.push(SensorSummary {
        id: "sensor".to_string(),
        blocking: false,
        missing: MissingPolicy::Warn,
        presence: Presence::Missing,
        report_path: "artifacts/sensor/report.json".to_string(),
        comment_path: None,
        verdict: Verdict {
            status: VerdictStatus::Warn,
            counts: VerdictCounts::default(),
            reasons: vec!["missing".to_string()],
        },
        truncated: false,
        errors: vec![],
        missing_policy_applied: Some(MissingPolicy::Warn),
        policy_outcome: None,
    });
    // This is the valid case: presence=Missing and missing_policy_applied is set.
    assert!(check_presence_semantics(&report).is_empty());
}

#[test]
fn presence_semantics_present_with_missing_policy_applied_violation() {
    let mut report = minimal_cockpit_report();
    report.sensors.push(SensorSummary {
        id: "sensor".to_string(),
        blocking: false,
        missing: MissingPolicy::Skip,
        presence: Presence::Present,
        report_path: "artifacts/sensor/report.json".to_string(),
        comment_path: None,
        verdict: Verdict {
            status: VerdictStatus::Pass,
            counts: VerdictCounts::default(),
            reasons: vec![],
        },
        truncated: false,
        errors: vec![],
        missing_policy_applied: Some(MissingPolicy::Skip),
        policy_outcome: None,
    });
    let v = check_presence_semantics(&report);
    assert_eq!(v.len(), 1);
    assert!(v[0].contains("missing_policy_applied is set"));
}

#[test]
fn presence_semantics_invalid_with_missing_policy_applied_violation() {
    let mut report = minimal_cockpit_report();
    report.sensors.push(SensorSummary {
        id: "sensor".to_string(),
        blocking: false,
        missing: MissingPolicy::Fail,
        presence: Presence::Invalid,
        report_path: "artifacts/sensor/report.json".to_string(),
        comment_path: None,
        verdict: Verdict {
            status: VerdictStatus::Fail,
            counts: VerdictCounts::default(),
            reasons: vec!["invalid_receipt".to_string()],
        },
        truncated: false,
        errors: vec!["parse error".to_string()],
        missing_policy_applied: Some(MissingPolicy::Fail),
        policy_outcome: None,
    });
    let v = check_presence_semantics(&report);
    assert_eq!(v.len(), 1);
    assert!(v[0].contains("Invalid"));
}

#[test]
fn presence_semantics_multiple_sensors_mixed() {
    let mut report = minimal_cockpit_report();
    // Good sensor.
    report.sensors.push(SensorSummary {
        id: "good".to_string(),
        blocking: false,
        missing: MissingPolicy::Skip,
        presence: Presence::Missing,
        report_path: "artifacts/good/report.json".to_string(),
        comment_path: None,
        verdict: Verdict {
            status: VerdictStatus::Skip,
            counts: VerdictCounts::default(),
            reasons: vec!["missing".to_string()],
        },
        truncated: false,
        errors: vec![],
        missing_policy_applied: Some(MissingPolicy::Skip),
        policy_outcome: None,
    });
    // Bad sensor: present but has missing_policy_applied.
    report.sensors.push(SensorSummary {
        id: "bad".to_string(),
        blocking: false,
        missing: MissingPolicy::Skip,
        presence: Presence::Present,
        report_path: "artifacts/bad/report.json".to_string(),
        comment_path: None,
        verdict: Verdict {
            status: VerdictStatus::Pass,
            counts: VerdictCounts::default(),
            reasons: vec![],
        },
        truncated: false,
        errors: vec![],
        missing_policy_applied: Some(MissingPolicy::Skip),
        policy_outcome: None,
    });
    let v = check_presence_semantics(&report);
    assert_eq!(v.len(), 1);
    assert!(v[0].contains("bad"));
}

// ===========================================================================
// check_cockpit_reason_tokens
// ===========================================================================

#[test]
fn cockpit_reason_tokens_empty_report() {
    let report = minimal_cockpit_report();
    assert!(check_cockpit_reason_tokens(&report).is_empty());
}

#[test]
fn cockpit_reason_tokens_all_valid() {
    let mut report = minimal_cockpit_report();
    report.verdict.reasons = vec!["all_pass".to_string(), "no_blockers".to_string()];
    report.sensors.push(SensorSummary {
        id: "sensor".to_string(),
        blocking: false,
        missing: MissingPolicy::Skip,
        presence: Presence::Present,
        report_path: "artifacts/sensor/report.json".to_string(),
        comment_path: None,
        verdict: Verdict {
            status: VerdictStatus::Pass,
            counts: VerdictCounts::default(),
            reasons: vec!["no_findings".to_string()],
        },
        truncated: false,
        errors: vec![],
        missing_policy_applied: None,
        policy_outcome: None,
    });
    assert!(check_cockpit_reason_tokens(&report).is_empty());
}

#[test]
fn cockpit_reason_tokens_invalid_top_level() {
    let mut report = minimal_cockpit_report();
    report.verdict.reasons = vec!["Bad-Token".to_string()];
    let v = check_cockpit_reason_tokens(&report);
    assert_eq!(v.len(), 1);
    assert!(v[0].contains("verdict.reasons[0]"));
}

#[test]
fn cockpit_reason_tokens_invalid_sensor_reason() {
    let mut report = minimal_cockpit_report();
    report.sensors.push(SensorSummary {
        id: "sensor".to_string(),
        blocking: false,
        missing: MissingPolicy::Skip,
        presence: Presence::Present,
        report_path: "artifacts/sensor/report.json".to_string(),
        comment_path: None,
        verdict: Verdict {
            status: VerdictStatus::Pass,
            counts: VerdictCounts::default(),
            reasons: vec!["valid_one".to_string(), "Bad-Two".to_string()],
        },
        truncated: false,
        errors: vec![],
        missing_policy_applied: None,
        policy_outcome: None,
    });
    let v = check_cockpit_reason_tokens(&report);
    assert_eq!(v.len(), 1);
    assert!(v[0].contains("sensors[0].verdict.reasons[1]"));
}

#[test]
fn cockpit_reason_tokens_invalid_capability_reason() {
    let mut report = minimal_cockpit_report();
    report.run.capabilities.insert(
        "git".to_string(),
        Capability {
            status: CapabilityStatus::Unavailable,
            reason: Some("Not-Available".to_string()),
        },
    );
    let v = check_cockpit_reason_tokens(&report);
    assert_eq!(v.len(), 1);
    assert!(v[0].contains("run.capabilities.git.reason"));
}

#[test]
fn cockpit_reason_tokens_capability_valid_reason() {
    let mut report = minimal_cockpit_report();
    report.run.capabilities.insert(
        "git".to_string(),
        Capability {
            status: CapabilityStatus::Available,
            reason: Some("repo_detected".to_string()),
        },
    );
    assert!(check_cockpit_reason_tokens(&report).is_empty());
}

#[test]
fn cockpit_reason_tokens_capability_no_reason() {
    let mut report = minimal_cockpit_report();
    report.run.capabilities.insert(
        "git".to_string(),
        Capability {
            status: CapabilityStatus::Available,
            reason: None,
        },
    );
    assert!(check_cockpit_reason_tokens(&report).is_empty());
}

#[test]
fn cockpit_reason_tokens_multiple_violations_across_fields() {
    let mut report = minimal_cockpit_report();
    report.verdict.reasons = vec!["Bad1".to_string()];
    report.sensors.push(SensorSummary {
        id: "s1".to_string(),
        blocking: false,
        missing: MissingPolicy::Skip,
        presence: Presence::Present,
        report_path: "artifacts/s1/report.json".to_string(),
        comment_path: None,
        verdict: Verdict {
            status: VerdictStatus::Pass,
            counts: VerdictCounts::default(),
            reasons: vec!["Bad2".to_string()],
        },
        truncated: false,
        errors: vec![],
        missing_policy_applied: None,
        policy_outcome: None,
    });
    report.run.capabilities.insert(
        "baseline".to_string(),
        Capability {
            status: CapabilityStatus::Skipped,
            reason: Some("Bad3".to_string()),
        },
    );
    let v = check_cockpit_reason_tokens(&report);
    assert_eq!(v.len(), 3);
}

// ===========================================================================
// check_determinism
// ===========================================================================

#[test]
fn determinism_identical_strings() {
    assert!(check_determinism("hello", "hello").is_none());
}

#[test]
fn determinism_different_strings() {
    let v = check_determinism("hello", "world");
    assert!(v.is_some());
    assert!(v.unwrap().contains("golden file"));
}

#[test]
fn determinism_empty_strings() {
    assert!(check_determinism("", "").is_none());
}

#[test]
fn determinism_whitespace_difference() {
    assert!(check_determinism("a b", "a  b").is_some());
}

#[test]
fn determinism_trailing_newline_difference() {
    assert!(check_determinism("line\n", "line").is_some());
}

// ===========================================================================
// conform_single (integration-level)
// ===========================================================================

#[test]
fn conform_single_clean_report_all_checks_pass() {
    let report = minimal_sensor_report();
    let json = to_json(&report);
    let result = conform_single(&json, "good_sensor", &all_checks()).expect("should not error");
    assert!(result.is_pass(), "violations: {:?}", result.violations);
}

#[test]
fn conform_single_no_checks_always_passes_valid_report() {
    let report = minimal_sensor_report();
    let json = to_json(&report);
    let result = conform_single(&json, "good_sensor", &no_checks()).expect("should not error");
    assert!(result.is_pass());
}

#[test]
fn conform_single_invalid_json() {
    let err = conform_single("not json at all", "sensor", &no_checks());
    assert!(err.is_err());
}

#[test]
fn conform_single_schema_violation_stops_early() {
    // Empty JSON object fails schema validation. Extended checks should NOT run.
    let result = conform_single("{}", "sensor", &all_checks()).expect("should not error");
    assert!(!result.is_pass());
    assert!(result.violations.iter().all(|v| v.check == "schema"));
}

#[test]
fn conform_single_survivability_fail_no_explanation() {
    let mut report = minimal_sensor_report();
    report.verdict.status = VerdictStatus::Fail;
    let json = to_json(&report);
    let checks = ConformChecks {
        survivability: true,
        ..no_checks()
    };
    let result = conform_single(&json, "sensor", &checks).expect("should not error");
    assert!(!result.is_pass());
    assert!(result.violations.iter().any(|v| v.check == "survivability"));
}

#[test]
fn conform_single_survivability_fail_with_findings() {
    let mut report = minimal_sensor_report();
    report.verdict.status = VerdictStatus::Fail;
    report
        .findings
        .push(make_finding(Severity::Error, "E1", "error"));
    let json = to_json(&report);
    let checks = ConformChecks {
        survivability: true,
        ..no_checks()
    };
    let result = conform_single(&json, "sensor", &checks).expect("should not error");
    assert!(result.is_pass());
}

#[test]
fn conform_single_survivability_fail_with_reasons() {
    let mut report = minimal_sensor_report();
    report.verdict.status = VerdictStatus::Fail;
    report.verdict.reasons = vec!["tool_error".to_string()];
    let json = to_json(&report);
    let checks = ConformChecks {
        survivability: true,
        ..no_checks()
    };
    let result = conform_single(&json, "sensor", &checks).expect("should not error");
    assert!(result.is_pass());
}

#[test]
fn conform_single_survivability_pass_status_skipped() {
    // Survivability only triggers on Fail status.
    let mut report = minimal_sensor_report();
    report.verdict.status = VerdictStatus::Pass;
    let json = to_json(&report);
    let checks = ConformChecks {
        survivability: true,
        ..no_checks()
    };
    let result = conform_single(&json, "sensor", &checks).expect("should not error");
    assert!(result.is_pass());
}

#[test]
fn conform_single_survivability_warn_status_skipped() {
    let mut report = minimal_sensor_report();
    report.verdict.status = VerdictStatus::Warn;
    let json = to_json(&report);
    let checks = ConformChecks {
        survivability: true,
        ..no_checks()
    };
    let result = conform_single(&json, "sensor", &checks).expect("should not error");
    assert!(result.is_pass());
}

#[test]
fn conform_single_survivability_skip_status_skipped() {
    let mut report = minimal_sensor_report();
    report.verdict.status = VerdictStatus::Skip;
    let json = to_json(&report);
    let checks = ConformChecks {
        survivability: true,
        ..no_checks()
    };
    let result = conform_single(&json, "sensor", &checks).expect("should not error");
    assert!(result.is_pass());
}

#[test]
fn conform_single_path_hygiene_violation() {
    let mut report = minimal_sensor_report();
    report
        .findings
        .push(make_finding_with_path(Severity::Info, "I1", "/abs", 1));
    let json = to_json(&report);
    let checks = ConformChecks {
        path_hygiene: true,
        ..no_checks()
    };
    let result = conform_single(&json, "sensor", &checks).expect("should not error");
    assert!(!result.is_pass());
    assert!(result.violations.iter().any(|v| v.check == "path_hygiene"));
}

#[test]
fn conform_single_ordering_violation() {
    let mut report = minimal_sensor_report();
    report
        .findings
        .push(make_finding(Severity::Info, "I1", "info"));
    report
        .findings
        .push(make_finding(Severity::Error, "E1", "error"));
    let json = to_json(&report);
    let checks = ConformChecks {
        ordering: true,
        ..no_checks()
    };
    let result = conform_single(&json, "sensor", &checks).expect("should not error");
    assert!(!result.is_pass());
    assert!(result.violations.iter().any(|v| v.check == "ordering"));
}

#[test]
fn conform_single_reason_lint_violation() {
    let mut report = minimal_sensor_report();
    report.verdict.reasons = vec!["Bad-Token".to_string()];
    let json = to_json(&report);
    let checks = ConformChecks {
        reason_lint: true,
        ..no_checks()
    };
    let result = conform_single(&json, "sensor", &checks).expect("should not error");
    assert!(!result.is_pass());
    assert!(result.violations.iter().any(|v| v.check == "reason_lint"));
}

#[test]
fn conform_single_tool_error_identity_violation() {
    let mut report = minimal_sensor_report();
    report.verdict.reasons = vec!["tool_error".to_string()];
    let json = to_json(&report);
    let checks = ConformChecks {
        tool_error_identity: true,
        ..no_checks()
    };
    let result = conform_single(&json, "sensor", &checks).expect("should not error");
    assert!(!result.is_pass());
    assert!(
        result
            .violations
            .iter()
            .any(|v| v.check == "tool_error_identity")
    );
}

#[test]
fn conform_single_sensor_id_format_violation() {
    let report = minimal_sensor_report();
    let json = to_json(&report);
    let checks = ConformChecks {
        sensor_id_format: true,
        ..no_checks()
    };
    let result = conform_single(&json, "bad.id", &checks).expect("should not error");
    assert!(!result.is_pass());
    assert!(
        result
            .violations
            .iter()
            .any(|v| v.check == "sensor_id_format")
    );
}

#[test]
fn conform_single_artifact_pointers_violation() {
    let mut report = minimal_sensor_report();
    // Use a path traversal violation that still passes schema (id/path/mime are non-empty).
    report.artifacts = vec![ArtifactPointer {
        id: "escape".to_string(),
        path: "../../../etc/passwd".to_string(),
        mime: "text/plain".to_string(),
        schema: None,
    }];
    let json = to_json(&report);
    let checks = ConformChecks {
        artifact_pointers: true,
        ..no_checks()
    };
    let result = conform_single(&json, "sensor", &checks).expect("should not error");
    assert!(!result.is_pass());
    assert!(
        result
            .violations
            .iter()
            .any(|v| v.check == "artifact_pointers")
    );
}

#[test]
fn conform_single_multiple_check_violations() {
    let mut report = minimal_sensor_report();
    report
        .findings
        .push(make_finding_with_path(Severity::Info, "I1", "/abs", 1));
    report.verdict.reasons = vec!["Bad-Token".to_string()];
    let json = to_json(&report);
    let checks = ConformChecks {
        path_hygiene: true,
        ordering: false,
        reason_lint: true,
        survivability: false,
        tool_error_identity: false,
        sensor_id_format: true,
        artifact_pointers: false,
    };
    let result = conform_single(&json, "bad.id", &checks).expect("should not error");
    assert!(!result.is_pass());
    let check_names: Vec<&str> = result.violations.iter().map(|v| v.check.as_str()).collect();
    assert!(check_names.contains(&"path_hygiene"));
    assert!(check_names.contains(&"reason_lint"));
    assert!(check_names.contains(&"sensor_id_format"));
}

// ===========================================================================
// validate_cockpit_schema
// ===========================================================================

#[test]
fn validate_cockpit_schema_valid() {
    let report = minimal_cockpit_report();
    let json = to_json(&report);
    let violations = validate_cockpit_schema(&json).expect("should not error");
    assert!(violations.is_empty());
}

#[test]
fn validate_cockpit_schema_empty_object() {
    let violations = validate_cockpit_schema("{}").expect("should not error");
    assert!(!violations.is_empty());
    assert!(violations.iter().all(|v| v.check == "schema"));
}

#[test]
fn validate_cockpit_schema_invalid_json() {
    let err = validate_cockpit_schema("not json");
    assert!(err.is_err());
}

#[test]
fn validate_cockpit_schema_wrong_schema_id() {
    let mut report = minimal_cockpit_report();
    report.schema = "wrong.schema.v99".to_string();
    let json = to_json(&report);
    let violations = validate_cockpit_schema(&json).expect("should not error");
    assert!(!violations.is_empty());
}

// ===========================================================================
// check_cockpit_extended
// ===========================================================================

#[test]
fn cockpit_extended_clean_report_both_checks() {
    let report = minimal_cockpit_report();
    let json = to_json(&report);
    let violations = check_cockpit_extended(&json, true, true).expect("should not error");
    assert!(violations.is_empty());
}

#[test]
fn cockpit_extended_both_disabled() {
    let mut report = minimal_cockpit_report();
    report.verdict.reasons = vec!["Bad-Token".to_string()];
    let json = to_json(&report);
    let violations = check_cockpit_extended(&json, false, false).expect("should not error");
    assert!(violations.is_empty());
}

#[test]
fn cockpit_extended_reason_lint_only() {
    let mut report = minimal_cockpit_report();
    report.verdict.reasons = vec!["Bad-Token".to_string()];
    let json = to_json(&report);
    let violations = check_cockpit_extended(&json, true, false).expect("should not error");
    assert_eq!(violations.len(), 1);
    assert!(violations[0].check == "reason_lint");
}

#[test]
fn cockpit_extended_presence_semantics_only() {
    let mut report = minimal_cockpit_report();
    report.sensors.push(SensorSummary {
        id: "s1".to_string(),
        blocking: false,
        missing: MissingPolicy::Skip,
        presence: Presence::Present,
        report_path: "artifacts/s1/report.json".to_string(),
        comment_path: None,
        verdict: Verdict {
            status: VerdictStatus::Pass,
            counts: VerdictCounts::default(),
            reasons: vec![],
        },
        truncated: false,
        errors: vec![],
        missing_policy_applied: Some(MissingPolicy::Skip),
        policy_outcome: None,
    });
    let json = to_json(&report);
    let violations = check_cockpit_extended(&json, false, true).expect("should not error");
    assert_eq!(violations.len(), 1);
    assert!(violations[0].check == "presence_semantics");
}

#[test]
fn cockpit_extended_invalid_json() {
    let err = check_cockpit_extended("not json", true, true);
    assert!(err.is_err());
}

#[test]
fn cockpit_extended_both_violations() {
    let mut report = minimal_cockpit_report();
    report.verdict.reasons = vec!["Bad-Token".to_string()];
    report.sensors.push(SensorSummary {
        id: "s1".to_string(),
        blocking: false,
        missing: MissingPolicy::Skip,
        presence: Presence::Present,
        report_path: "artifacts/s1/report.json".to_string(),
        comment_path: None,
        verdict: Verdict {
            status: VerdictStatus::Pass,
            counts: VerdictCounts::default(),
            reasons: vec![],
        },
        truncated: false,
        errors: vec![],
        missing_policy_applied: Some(MissingPolicy::Skip),
        policy_outcome: None,
    });
    let json = to_json(&report);
    let violations = check_cockpit_extended(&json, true, true).expect("should not error");
    let checks: Vec<&str> = violations.iter().map(|v| v.check.as_str()).collect();
    assert!(checks.contains(&"reason_lint"));
    assert!(checks.contains(&"presence_semantics"));
}

// ===========================================================================
// ConformResult::is_pass
// ===========================================================================

#[test]
fn conform_result_is_pass_empty() {
    let result = ConformResult { violations: vec![] };
    assert!(result.is_pass());
}

#[test]
fn conform_result_is_pass_with_violations() {
    let result = ConformResult {
        violations: vec![Violation {
            check: "test".to_string(),
            message: "test violation".to_string(),
        }],
    };
    assert!(!result.is_pass());
}

// ===========================================================================
// Edge case: report with data field populated
// ===========================================================================

#[test]
fn conform_single_report_with_data_field() {
    let mut report = minimal_sensor_report();
    report.data = Some(serde_json::json!({
        "_cockpit": {
            "cards": [],
            "suggested_highlights": []
        }
    }));
    let json = to_json(&report);
    let result = conform_single(&json, "sensor", &all_checks()).expect("should not error");
    assert!(result.is_pass());
}

// ===========================================================================
// Edge case: report with commit in tool info
// ===========================================================================

#[test]
fn conform_single_report_with_tool_commit() {
    let mut report = minimal_sensor_report();
    report.tool.commit = Some("abc1234".to_string());
    let json = to_json(&report);
    let result = conform_single(&json, "sensor", &all_checks()).expect("should not error");
    assert!(result.is_pass());
}

// ===========================================================================
// Edge case: report with all RunInfo optional fields filled
// ===========================================================================

#[test]
fn conform_single_report_with_full_run_info() {
    let mut report = minimal_sensor_report();
    report.run.ended_at = Some("2026-02-01T00:01:00Z".to_string());
    report.run.duration_ms = Some(60000);
    report.run.host = Some(HostInfo {
        os: Some("linux".to_string()),
        arch: Some("x86_64".to_string()),
        hostname: Some("ci-runner".to_string()),
    });
    report.run.git = Some(GitInfo {
        repo: Some("org/repo".to_string()),
        base_ref: Some("main".to_string()),
        head_ref: Some("feature".to_string()),
        base_sha: Some("aaa".to_string()),
        head_sha: Some("bbb".to_string()),
        merge_base: Some("ccc".to_string()),
    });
    report.run.ci = Some(CiInfo {
        provider: Some("github".to_string()),
        run_id: Some("123".to_string()),
        run_url: Some("https://example.com/run/123".to_string()),
        job: Some("build".to_string()),
    });
    let json = to_json(&report);
    let result = conform_single(&json, "sensor", &all_checks()).expect("should not error");
    assert!(result.is_pass());
}

// ===========================================================================
// Edge case: correctly ordered findings with mixed locations
// ===========================================================================

#[test]
fn ordering_findings_with_and_without_locations() {
    let mut report = minimal_sensor_report();
    // Error with no location (path="", line=0) should sort before Error with path.
    report.findings.push(Finding {
        severity: Severity::Error,
        check_id: None,
        code: "E1".to_string(),
        message: "no location".to_string(),
        location: None,
        help: None,
        url: None,
        fingerprint: None,
        data: None,
    });
    report.findings.push(Finding {
        severity: Severity::Error,
        check_id: None,
        code: "E1".to_string(),
        message: "with location".to_string(),
        location: Some(Location {
            path: Some("src/main.rs".to_string()),
            line: Some(10),
            col: None,
        }),
        help: None,
        url: None,
        fingerprint: None,
        data: None,
    });
    // path="" < path="src/main.rs", so "no location" sorts first -> valid order.
    assert!(check_ordering(&report, "sensor").is_empty());
}

// ===========================================================================
// Edge case: large number of findings correctly ordered
// ===========================================================================

#[test]
fn ordering_many_findings_correctly_ordered() {
    let mut report = minimal_sensor_report();
    // 50 Error findings with ascending codes.
    for i in 0..50 {
        report.findings.push(make_finding(
            Severity::Error,
            &format!("E{:04}", i),
            &format!("error {}", i),
        ));
    }
    // Then 50 Warn findings.
    for i in 0..50 {
        report.findings.push(make_finding(
            Severity::Warn,
            &format!("W{:04}", i),
            &format!("warn {}", i),
        ));
    }
    assert!(check_ordering(&report, "sensor").is_empty());
}

// ===========================================================================
// Edge case: sensor ID boundary values
// ===========================================================================

#[test]
fn sensor_id_format_long_valid() {
    let long_id: String = "a".repeat(256);
    assert!(check_sensor_id_format(&long_id).is_empty());
}

#[test]
fn sensor_id_format_all_hyphens() {
    assert!(check_sensor_id_format("---").is_empty());
}

#[test]
fn sensor_id_format_all_underscores() {
    assert!(check_sensor_id_format("___").is_empty());
}

#[test]
fn sensor_id_format_leading_digit() {
    assert!(check_sensor_id_format("123sensor").is_empty());
}

// ===========================================================================
// Edge case: artifact with just-traversal and nothing else bad
// ===========================================================================

#[test]
fn artifact_pointers_path_double_dot_only() {
    let mut report = minimal_sensor_report();
    report.artifacts = vec![ArtifactPointer {
        id: "a".to_string(),
        path: "..".to_string(),
        mime: "text/plain".to_string(),
        schema: None,
    }];
    let v = check_artifact_pointers(&report);
    assert_eq!(v.len(), 1);
    assert!(v[0].contains(".."));
}

#[test]
fn artifact_pointers_path_single_dot_ok() {
    let mut report = minimal_sensor_report();
    report.artifacts = vec![ArtifactPointer {
        id: "a".to_string(),
        path: "./ok.txt".to_string(),
        mime: "text/plain".to_string(),
        schema: None,
    }];
    let v = check_artifact_pointers(&report);
    assert!(v.is_empty());
}
