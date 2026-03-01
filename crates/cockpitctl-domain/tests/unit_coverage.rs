//! Targeted unit tests to close coverage gaps in cockpitctl-domain.
//!
//! Each public function listed in the coverage gap analysis gets at least
//! two tests: a happy-path case and an edge case.

use cockpitctl_domain::{
    canonical_policy_snapshot_bytes, cap_findings, compute_counts, finding_sort_key,
    match_buildfix_plan, policy_snapshot_sha256_hex, select_auto_apply_fixes, snapshot_policy,
    sort_findings, synthesize_invalid_sensor, synthesize_missing_sensor,
    synthesize_path_traversal_sensor, synthesize_receipt_inconsistent,
    synthesize_receipt_oversized_sensor,
};
use cockpitctl_types::{
    BuildfixPlan, BuildfixSummary, CockpitConfig, Finding, FindingRef, FindingSortKey, Fix,
    FixSummary, Highlight, Location, MissingPolicy, PolicyOutcome, Presence, SafetyLevel,
    SensorPolicy, Severity, ToolInfo, VerdictCounts, VerdictStatus,
};
use pretty_assertions::assert_eq;

// ============================================================================
// Test helpers
// ============================================================================

fn make_policy(blocking: bool, missing: MissingPolicy) -> SensorPolicy {
    SensorPolicy {
        blocking,
        missing,
        section: None,
        require_label: None,
        repro: None,
    }
}

fn make_finding(code: &str, severity: Severity) -> Finding {
    Finding {
        severity,
        check_id: None,
        code: code.to_string(),
        message: format!("Message for {}", code),
        location: Some(Location {
            path: Some("src/main.rs".to_string()),
            line: Some(10),
            col: None,
        }),
        help: None,
        url: None,
        fingerprint: None,
        data: None,
    }
}

fn make_finding_at(code: &str, severity: Severity, path: &str, line: u32) -> Finding {
    Finding {
        severity,
        check_id: None,
        code: code.to_string(),
        message: format!("Message for {}", code),
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

fn make_finding_no_location(code: &str, severity: Severity) -> Finding {
    Finding {
        severity,
        check_id: None,
        code: code.to_string(),
        message: format!("Message for {}", code),
        location: None,
        help: None,
        url: None,
        fingerprint: None,
        data: None,
    }
}

fn make_highlight(sensor_id: &str, code: &str, severity: Severity) -> Highlight {
    Highlight {
        sensor_id: sensor_id.to_string(),
        finding: make_finding(code, severity),
    }
}

fn make_highlight_with_fingerprint(
    sensor_id: &str,
    code: &str,
    severity: Severity,
    fingerprint: &str,
) -> Highlight {
    Highlight {
        sensor_id: sensor_id.to_string(),
        finding: Finding {
            fingerprint: Some(fingerprint.to_string()),
            ..make_finding(code, severity)
        },
    }
}

fn tool_info() -> ToolInfo {
    ToolInfo {
        name: "test-tool".to_string(),
        version: "0.1.0".to_string(),
        commit: None,
    }
}

// ============================================================================
// cap_findings
// ============================================================================

#[test]
fn cap_findings_under_limit_returns_all_without_truncation() {
    let findings = vec![
        make_finding("A", Severity::Error),
        make_finding("B", Severity::Warn),
    ];
    let (result, truncated) = cap_findings(findings.clone(), 5);
    assert_eq!(result, findings);
    assert!(!truncated);
}

#[test]
fn cap_findings_at_exact_limit_returns_all_without_truncation() {
    let findings = vec![
        make_finding("A", Severity::Error),
        make_finding("B", Severity::Warn),
        make_finding("C", Severity::Info),
    ];
    let (result, truncated) = cap_findings(findings.clone(), 3);
    assert_eq!(result, findings);
    assert!(!truncated);
}

#[test]
fn cap_findings_over_limit_truncates_and_sets_flag() {
    let findings = vec![
        make_finding("A", Severity::Error),
        make_finding("B", Severity::Warn),
        make_finding("C", Severity::Info),
    ];
    let (result, truncated) = cap_findings(findings.clone(), 2);
    assert_eq!(result.len(), 2);
    assert!(truncated);
    assert_eq!(result[0], findings[0]);
    assert_eq!(result[1], findings[1]);
}

#[test]
fn cap_findings_empty_input_returns_empty() {
    let (result, truncated) = cap_findings(vec![], 10);
    assert!(result.is_empty());
    assert!(!truncated);
}

#[test]
fn cap_findings_zero_max_truncates_everything() {
    let findings = vec![make_finding("A", Severity::Error)];
    let (result, truncated) = cap_findings(findings, 0);
    assert!(result.is_empty());
    assert!(truncated);
}

// ============================================================================
// compute_counts
// ============================================================================

#[test]
fn compute_counts_mixed_severities() {
    let findings = vec![
        make_finding("E1", Severity::Error),
        make_finding("E2", Severity::Error),
        make_finding("W1", Severity::Warn),
        make_finding("I1", Severity::Info),
        make_finding("I2", Severity::Info),
        make_finding("I3", Severity::Info),
    ];
    let counts = compute_counts(&findings);
    assert_eq!(
        counts,
        VerdictCounts {
            info: 3,
            warn: 1,
            error: 2,
            suppressed: 0,
        }
    );
}

#[test]
fn compute_counts_empty_findings() {
    let counts = compute_counts(&[]);
    assert_eq!(counts, VerdictCounts::default());
}

#[test]
fn compute_counts_all_same_severity() {
    let findings = vec![
        make_finding("W1", Severity::Warn),
        make_finding("W2", Severity::Warn),
    ];
    let counts = compute_counts(&findings);
    assert_eq!(counts.info, 0);
    assert_eq!(counts.warn, 2);
    assert_eq!(counts.error, 0);
}

#[test]
fn compute_counts_single_finding() {
    let findings = vec![make_finding("I1", Severity::Info)];
    let counts = compute_counts(&findings);
    assert_eq!(counts.info, 1);
    assert_eq!(counts.warn, 0);
    assert_eq!(counts.error, 0);
}

// ============================================================================
// finding_sort_key
// ============================================================================

#[test]
fn finding_sort_key_captures_all_fields() {
    let finding = make_finding_at("CODE1", Severity::Error, "src/lib.rs", 42);
    let key = finding_sort_key("sensor-a", &finding);
    assert_eq!(
        key,
        FindingSortKey {
            severity_rank: 0, // Error = 0
            sensor_id: "sensor-a".to_string(),
            path: "src/lib.rs".to_string(),
            line: 42,
            code: "CODE1".to_string(),
            message: "Message for CODE1".to_string(),
        }
    );
}

#[test]
fn finding_sort_key_no_location_defaults_to_empty_path_and_max_line() {
    let finding = make_finding_no_location("CODE2", Severity::Warn);
    let key = finding_sort_key("sensor-b", &finding);
    assert_eq!(key.path, "");
    assert_eq!(key.line, u32::MAX);
    assert_eq!(key.severity_rank, 1); // Warn = 1
}

#[test]
fn finding_sort_key_location_with_path_but_no_line() {
    let finding = Finding {
        severity: Severity::Info,
        check_id: None,
        code: "CODE3".to_string(),
        message: "msg".to_string(),
        location: Some(Location {
            path: Some("foo.rs".to_string()),
            line: None,
            col: None,
        }),
        help: None,
        url: None,
        fingerprint: None,
        data: None,
    };
    let key = finding_sort_key("s", &finding);
    assert_eq!(key.path, "foo.rs");
    assert_eq!(key.line, u32::MAX);
}

#[test]
fn finding_sort_key_location_with_line_but_no_path() {
    let finding = Finding {
        severity: Severity::Error,
        check_id: None,
        code: "CODE4".to_string(),
        message: "msg".to_string(),
        location: Some(Location {
            path: None,
            line: Some(99),
            col: None,
        }),
        help: None,
        url: None,
        fingerprint: None,
        data: None,
    };
    let key = finding_sort_key("s", &finding);
    assert_eq!(key.path, "");
    assert_eq!(key.line, 99);
}

#[test]
fn finding_sort_key_severity_ordering_error_before_warn_before_info() {
    let f_error = make_finding("C", Severity::Error);
    let f_warn = make_finding("C", Severity::Warn);
    let f_info = make_finding("C", Severity::Info);

    let k_error = finding_sort_key("s", &f_error);
    let k_warn = finding_sort_key("s", &f_warn);
    let k_info = finding_sort_key("s", &f_info);

    assert!(k_error < k_warn);
    assert!(k_warn < k_info);
}

// ============================================================================
// sort_findings
// ============================================================================

#[test]
fn sort_findings_orders_by_severity_then_sensor_path_line_code_message() {
    let mut findings = vec![
        make_finding_at("CODE_A", Severity::Info, "z.rs", 1),
        make_finding_at("CODE_B", Severity::Error, "a.rs", 1),
        make_finding_at("CODE_C", Severity::Warn, "m.rs", 1),
    ];
    sort_findings("sensor", &mut findings);

    assert_eq!(findings[0].severity, Severity::Error);
    assert_eq!(findings[1].severity, Severity::Warn);
    assert_eq!(findings[2].severity, Severity::Info);
}

#[test]
fn sort_findings_stable_within_same_severity_by_path_then_line() {
    let mut findings = vec![
        make_finding_at("CODE", Severity::Error, "b.rs", 20),
        make_finding_at("CODE", Severity::Error, "a.rs", 10),
        make_finding_at("CODE", Severity::Error, "a.rs", 5),
    ];
    sort_findings("sensor", &mut findings);

    // Same severity, same code: sorted by path asc, then line asc.
    assert_eq!(
        findings[0].location.as_ref().unwrap().path.as_deref(),
        Some("a.rs")
    );
    assert_eq!(findings[0].location.as_ref().unwrap().line, Some(5));
    assert_eq!(
        findings[1].location.as_ref().unwrap().path.as_deref(),
        Some("a.rs")
    );
    assert_eq!(findings[1].location.as_ref().unwrap().line, Some(10));
    assert_eq!(
        findings[2].location.as_ref().unwrap().path.as_deref(),
        Some("b.rs")
    );
}

#[test]
fn sort_findings_empty_is_noop() {
    let mut findings: Vec<Finding> = vec![];
    sort_findings("sensor", &mut findings);
    assert!(findings.is_empty());
}

#[test]
fn sort_findings_single_element_unchanged() {
    let original = make_finding("A", Severity::Error);
    let mut findings = vec![original.clone()];
    sort_findings("sensor", &mut findings);
    assert_eq!(findings[0], original);
}

#[test]
fn sort_findings_no_location_goes_after_with_location_for_same_severity() {
    let with_loc = make_finding_at("CODE", Severity::Error, "a.rs", 1);
    let no_loc = make_finding_no_location("CODE", Severity::Error);
    let mut findings = vec![no_loc.clone(), with_loc.clone()];
    sort_findings("sensor", &mut findings);

    // Empty path ("") sorts before "a.rs" lexically? Actually "" < "a.rs" so no-location comes first.
    // But no-location has line=u32::MAX which is after line=1 for same path.
    // Here path="" < "a.rs", so no-location finding comes first.
    assert_eq!(findings[0].location, None);
    assert!(findings[1].location.is_some());
}

// ============================================================================
// synthesize_missing_sensor
// ============================================================================

#[test]
fn synthesize_missing_sensor_skip_policy_produces_skip_verdict_no_highlight() {
    let (summary, highlight) = synthesize_missing_sensor(
        "mysensor",
        &make_policy(true, MissingPolicy::Skip),
        "artifacts/mysensor/report.json",
        None,
    );
    assert_eq!(summary.verdict.status, VerdictStatus::Skip);
    assert_eq!(summary.presence, Presence::Missing);
    assert!(summary.verdict.reasons.is_empty());
    assert_eq!(summary.missing_policy_applied, Some(MissingPolicy::Skip));
    assert!(highlight.is_none());
}

#[test]
fn synthesize_missing_sensor_warn_policy_produces_warn_verdict_with_highlight() {
    let (summary, highlight) = synthesize_missing_sensor(
        "mysensor",
        &make_policy(false, MissingPolicy::Warn),
        "artifacts/mysensor/report.json",
        Some("artifacts/mysensor/comment.md".to_string()),
    );
    assert_eq!(summary.verdict.status, VerdictStatus::Warn);
    assert_eq!(summary.presence, Presence::Missing);
    assert_eq!(summary.verdict.reasons, vec!["missing_receipt".to_string()]);
    assert_eq!(summary.missing_policy_applied, Some(MissingPolicy::Warn));
    assert_eq!(summary.verdict.counts.warn, 1);
    assert_eq!(summary.verdict.counts.error, 0);

    let h = highlight.unwrap();
    assert_eq!(h.sensor_id, "mysensor");
    assert_eq!(h.finding.severity, Severity::Warn);
    assert!(h.finding.message.contains("mysensor"));
    assert_eq!(h.finding.code, "cockpit.missing_receipt");
}

#[test]
fn synthesize_missing_sensor_fail_policy_produces_fail_verdict_error_severity() {
    let (summary, highlight) = synthesize_missing_sensor(
        "mysensor",
        &make_policy(true, MissingPolicy::Fail),
        "artifacts/mysensor/report.json",
        None,
    );
    assert_eq!(summary.verdict.status, VerdictStatus::Fail);
    assert_eq!(summary.verdict.counts.error, 1);
    assert_eq!(summary.policy_outcome, Some(PolicyOutcome::Blocked));

    let h = highlight.unwrap();
    assert_eq!(h.finding.severity, Severity::Error);
}

#[test]
fn synthesize_missing_sensor_non_blocking_with_fail_policy_is_informational() {
    let (summary, _highlight) = synthesize_missing_sensor(
        "s",
        &make_policy(false, MissingPolicy::Fail),
        "artifacts/s/report.json",
        None,
    );
    assert_eq!(summary.policy_outcome, Some(PolicyOutcome::Informational));
}

// ============================================================================
// synthesize_invalid_sensor
// ============================================================================

#[test]
fn synthesize_invalid_sensor_blocking_produces_blocked_outcome() {
    let (summary, highlight) = synthesize_invalid_sensor(
        "bad-sensor",
        &make_policy(true, MissingPolicy::Fail),
        "artifacts/bad-sensor/report.json",
        None,
        "unexpected EOF".to_string(),
    );
    assert_eq!(summary.verdict.status, VerdictStatus::Fail);
    assert_eq!(summary.presence, Presence::Invalid);
    assert_eq!(summary.errors, vec!["unexpected EOF".to_string()]);
    assert_eq!(summary.policy_outcome, Some(PolicyOutcome::Blocked));

    let h = highlight.unwrap();
    assert_eq!(h.finding.code, "cockpit.invalid_receipt");
    assert!(h.finding.message.contains("unexpected EOF"));
    assert!(h.finding.message.contains("bad-sensor"));
}

#[test]
fn synthesize_invalid_sensor_non_blocking_produces_informational_outcome() {
    let (summary, _highlight) = synthesize_invalid_sensor(
        "sensor",
        &make_policy(false, MissingPolicy::Skip),
        "artifacts/sensor/report.json",
        Some("comment.md".to_string()),
        "parse error".to_string(),
    );
    assert_eq!(summary.policy_outcome, Some(PolicyOutcome::Informational));
    assert_eq!(summary.comment_path, Some("comment.md".to_string()));
    assert!(!summary.blocking);
}

#[test]
fn synthesize_invalid_sensor_counts_one_error() {
    let (summary, _) = synthesize_invalid_sensor(
        "s",
        &make_policy(true, MissingPolicy::Fail),
        "p",
        None,
        "err".to_string(),
    );
    assert_eq!(summary.verdict.counts.error, 1);
    assert_eq!(summary.verdict.counts.warn, 0);
    assert_eq!(summary.verdict.counts.info, 0);
}

// ============================================================================
// synthesize_path_traversal_sensor
// ============================================================================

#[test]
fn synthesize_path_traversal_sensor_produces_blocked_fail() {
    let (summary, highlight) = synthesize_path_traversal_sensor(
        "../evil",
        &make_policy(true, MissingPolicy::Fail),
        "artifacts/../evil/report.json",
        None,
        Some("symlink escape".to_string()),
    );
    assert_eq!(summary.verdict.status, VerdictStatus::Fail);
    assert_eq!(summary.presence, Presence::Missing);
    assert_eq!(summary.policy_outcome, Some(PolicyOutcome::Blocked));
    assert_eq!(summary.verdict.reasons, vec!["path_traversal".to_string()]);
    assert!(summary.errors[0].contains("unsafe path rejected"));

    assert_eq!(highlight.sensor_id, "_cockpit");
    assert_eq!(highlight.finding.code, "cockpit.path_traversal");
    assert!(highlight.finding.message.contains("symlink escape"));
}

#[test]
fn synthesize_path_traversal_sensor_without_context_omits_detail() {
    let (summary, highlight) = synthesize_path_traversal_sensor(
        "sensor",
        &make_policy(false, MissingPolicy::Skip),
        "artifacts/sensor/report.json",
        None,
        None,
    );
    assert!(!highlight.finding.message.contains("(unsafe"));
    // Even for non-blocking sensors, path traversal always sets Blocked.
    assert_eq!(summary.policy_outcome, Some(PolicyOutcome::Blocked));
}

#[test]
fn synthesize_path_traversal_sensor_with_comment_path() {
    let (_summary, _highlight) = synthesize_path_traversal_sensor(
        "sensor",
        &make_policy(true, MissingPolicy::Fail),
        "artifacts/sensor/report.json",
        Some("artifacts/sensor/comment.md".to_string()),
        None,
    );
    assert_eq!(
        _summary.comment_path,
        Some("artifacts/sensor/comment.md".to_string())
    );
}

// ============================================================================
// synthesize_receipt_oversized_sensor
// ============================================================================

#[test]
fn synthesize_receipt_oversized_sensor_blocking_produces_blocked_outcome() {
    let (summary, highlight) = synthesize_receipt_oversized_sensor(
        "big-sensor",
        &make_policy(true, MissingPolicy::Fail),
        "artifacts/big-sensor/report.json",
        None,
        5_000_000,
        2_000_000,
    );
    assert_eq!(summary.verdict.status, VerdictStatus::Fail);
    assert_eq!(summary.presence, Presence::Invalid);
    assert_eq!(summary.policy_outcome, Some(PolicyOutcome::Blocked));
    assert!(summary.errors[0].contains("5000000"));
    assert!(summary.errors[0].contains("2000000"));

    assert_eq!(highlight.sensor_id, "_cockpit");
    assert_eq!(highlight.finding.code, "cockpit.receipt_oversized");
    assert!(highlight.finding.message.contains("5000000"));
    assert!(highlight.finding.message.contains("2000000"));
}

#[test]
fn synthesize_receipt_oversized_sensor_non_blocking_is_informational() {
    let (summary, _highlight) = synthesize_receipt_oversized_sensor(
        "sensor",
        &make_policy(false, MissingPolicy::Skip),
        "artifacts/sensor/report.json",
        None,
        100,
        50,
    );
    assert_eq!(summary.policy_outcome, Some(PolicyOutcome::Informational));
}

#[test]
fn synthesize_receipt_oversized_sensor_fingerprint_includes_size() {
    let (_summary, highlight) = synthesize_receipt_oversized_sensor(
        "sensor",
        &make_policy(true, MissingPolicy::Fail),
        "path",
        None,
        999,
        100,
    );
    let fp = highlight.finding.fingerprint.unwrap();
    assert!(fp.contains("999"), "fingerprint should include actual size");
    assert!(fp.contains("cockpit.receipt_oversized"));
}

// ============================================================================
// synthesize_receipt_inconsistent
// ============================================================================

#[test]
fn synthesize_receipt_inconsistent_includes_all_count_values() {
    let reported = VerdictCounts {
        info: 1,
        warn: 2,
        error: 3,
        suppressed: 0,
    };
    let computed = VerdictCounts {
        info: 4,
        warn: 5,
        error: 6,
        suppressed: 0,
    };
    let highlight = synthesize_receipt_inconsistent("my-sensor", &reported, &computed);

    assert_eq!(highlight.sensor_id, "my-sensor");
    assert_eq!(highlight.finding.code, "cockpit.receipt_inconsistent");
    assert_eq!(highlight.finding.severity, Severity::Info);

    let msg = &highlight.finding.message;
    // Reported values.
    assert!(msg.contains("info=1"));
    assert!(msg.contains("warn=2"));
    assert!(msg.contains("error=3"));
    // Computed values.
    assert!(msg.contains("info=4"));
    assert!(msg.contains("warn=5"));
    assert!(msg.contains("error=6"));
}

#[test]
fn synthesize_receipt_inconsistent_fingerprint_encodes_all_counts() {
    let reported = VerdictCounts {
        info: 0,
        warn: 0,
        error: 1,
        suppressed: 0,
    };
    let computed = VerdictCounts {
        info: 0,
        warn: 1,
        error: 0,
        suppressed: 0,
    };
    let highlight = synthesize_receipt_inconsistent("s", &reported, &computed);
    let fp = highlight.finding.fingerprint.unwrap();

    assert!(fp.starts_with("cockpit.receipt_inconsistent:s:"));
    // Should encode reported and computed counts.
    assert!(fp.contains(":0:0:1:0:1:0"));
}

#[test]
fn synthesize_receipt_inconsistent_has_help_text() {
    let highlight =
        synthesize_receipt_inconsistent("x", &VerdictCounts::default(), &VerdictCounts::default());
    assert!(highlight.finding.help.is_some());
}

// ============================================================================
// match_buildfix_plan
// ============================================================================

#[test]
fn match_buildfix_plan_matches_by_fingerprint() {
    let plan = BuildfixPlan {
        schema: "buildfix.plan.v1".to_string(),
        tool: tool_info(),
        fixes: vec![Fix {
            id: "fix-1".to_string(),
            safety: SafetyLevel::Safe,
            description: "Fix the thing".to_string(),
            finding_refs: vec![FindingRef {
                sensor_id: "builddiag".to_string(),
                fingerprint: Some("fp-abc".to_string()),
                code: None,
                tool: None,
                check_id: None,
            }],
            preconditions: None,
            data: None,
        }],
    };

    let highlights = vec![make_highlight_with_fingerprint(
        "builddiag",
        "E001",
        Severity::Error,
        "fp-abc",
    )];

    let summary = match_buildfix_plan("builddiag", &plan, &highlights);
    assert_eq!(summary.total_fixes, 1);
    assert_eq!(summary.matched_count, 1);
    assert_eq!(summary.unmatched_count, 0);
    assert!(!summary.fixes[0].unmatched);
    assert_eq!(summary.fixes[0].matched_findings.len(), 1);
    assert_eq!(summary.fixes[0].matched_findings[0].code, "E001");
}

#[test]
fn match_buildfix_plan_matches_by_code_when_no_fingerprint() {
    let plan = BuildfixPlan {
        schema: "buildfix.plan.v1".to_string(),
        tool: tool_info(),
        fixes: vec![Fix {
            id: "fix-2".to_string(),
            safety: SafetyLevel::Guarded,
            description: "Fix by code".to_string(),
            finding_refs: vec![FindingRef {
                sensor_id: "builddiag".to_string(),
                fingerprint: None,
                code: Some("E002".to_string()),
                tool: None,
                check_id: None,
            }],
            preconditions: None,
            data: None,
        }],
    };

    let highlights = vec![make_highlight("builddiag", "E002", Severity::Error)];
    let summary = match_buildfix_plan("builddiag", &plan, &highlights);
    assert_eq!(summary.matched_count, 1);
    assert_eq!(summary.unmatched_count, 0);
}

#[test]
fn match_buildfix_plan_unmatched_when_sensor_id_differs() {
    let plan = BuildfixPlan {
        schema: "buildfix.plan.v1".to_string(),
        tool: tool_info(),
        fixes: vec![Fix {
            id: "fix-3".to_string(),
            safety: SafetyLevel::Safe,
            description: "mismatch".to_string(),
            finding_refs: vec![FindingRef {
                sensor_id: "other-sensor".to_string(),
                fingerprint: None,
                code: Some("E003".to_string()),
                tool: None,
                check_id: None,
            }],
            preconditions: None,
            data: None,
        }],
    };

    let highlights = vec![make_highlight("builddiag", "E003", Severity::Error)];
    let summary = match_buildfix_plan("builddiag", &plan, &highlights);
    assert_eq!(summary.matched_count, 0);
    assert_eq!(summary.unmatched_count, 1);
    assert!(summary.fixes[0].unmatched);
}

#[test]
fn match_buildfix_plan_unmatched_when_fingerprint_required_but_missing_on_finding() {
    let plan = BuildfixPlan {
        schema: "buildfix.plan.v1".to_string(),
        tool: tool_info(),
        fixes: vec![Fix {
            id: "fix-fp".to_string(),
            safety: SafetyLevel::Safe,
            description: "needs fp".to_string(),
            finding_refs: vec![FindingRef {
                sensor_id: "sensor".to_string(),
                fingerprint: Some("fp-required".to_string()),
                code: None,
                tool: None,
                check_id: None,
            }],
            preconditions: None,
            data: None,
        }],
    };

    // Highlight has no fingerprint.
    let highlights = vec![make_highlight("sensor", "CODE", Severity::Error)];
    let summary = match_buildfix_plan("sensor", &plan, &highlights);
    assert_eq!(summary.unmatched_count, 1);
}

#[test]
fn match_buildfix_plan_empty_plan_produces_empty_summary() {
    let plan = BuildfixPlan {
        schema: "buildfix.plan.v1".to_string(),
        tool: tool_info(),
        fixes: vec![],
    };
    let highlights = vec![make_highlight("sensor", "CODE", Severity::Error)];
    let summary = match_buildfix_plan("sensor", &plan, &highlights);
    assert_eq!(summary.total_fixes, 0);
    assert_eq!(summary.matched_count, 0);
    assert_eq!(summary.unmatched_count, 0);
}

#[test]
fn match_buildfix_plan_sorts_by_safety_then_sensor_then_fix_id() {
    let plan = BuildfixPlan {
        schema: "buildfix.plan.v1".to_string(),
        tool: tool_info(),
        fixes: vec![
            Fix {
                id: "z-unsafe".to_string(),
                safety: SafetyLevel::Unsafe,
                description: "unsafe".to_string(),
                finding_refs: vec![],
                preconditions: None,
                data: None,
            },
            Fix {
                id: "a-safe".to_string(),
                safety: SafetyLevel::Safe,
                description: "safe".to_string(),
                finding_refs: vec![],
                preconditions: None,
                data: None,
            },
            Fix {
                id: "m-guarded".to_string(),
                safety: SafetyLevel::Guarded,
                description: "guarded".to_string(),
                finding_refs: vec![],
                preconditions: None,
                data: None,
            },
        ],
    };

    let summary = match_buildfix_plan("sensor", &plan, &[]);
    let ids: Vec<&str> = summary.fixes.iter().map(|f| f.fix_id.as_str()).collect();
    assert_eq!(ids, vec!["a-safe", "m-guarded", "z-unsafe"]);
}

#[test]
fn match_buildfix_plan_no_ref_fields_matches_all_findings_of_sensor() {
    // When FindingRef has no fingerprint and no code, any finding from the sensor matches.
    let plan = BuildfixPlan {
        schema: "buildfix.plan.v1".to_string(),
        tool: tool_info(),
        fixes: vec![Fix {
            id: "fix-all".to_string(),
            safety: SafetyLevel::Safe,
            description: "wildcard".to_string(),
            finding_refs: vec![FindingRef {
                sensor_id: "sensor".to_string(),
                fingerprint: None,
                code: None,
                tool: None,
                check_id: None,
            }],
            preconditions: None,
            data: None,
        }],
    };

    let highlights = vec![
        make_highlight("sensor", "E001", Severity::Error),
        make_highlight("sensor", "E002", Severity::Error),
        make_highlight("other", "E003", Severity::Error),
    ];
    let summary = match_buildfix_plan("sensor", &plan, &highlights);
    assert_eq!(summary.matched_count, 1);
    assert_eq!(summary.fixes[0].matched_findings.len(), 2);
}

// ============================================================================
// select_auto_apply_fixes
// ============================================================================

#[test]
fn select_auto_apply_fixes_filters_by_safety_level() {
    let summary = BuildfixSummary {
        fixes: vec![
            FixSummary {
                fix_id: "safe".to_string(),
                sensor_id: "s".to_string(),
                safety: SafetyLevel::Safe,
                description: "safe".to_string(),
                matched_findings: vec![],
                unmatched: false,
            },
            FixSummary {
                fix_id: "guarded".to_string(),
                sensor_id: "s".to_string(),
                safety: SafetyLevel::Guarded,
                description: "guarded".to_string(),
                matched_findings: vec![],
                unmatched: false,
            },
            FixSummary {
                fix_id: "unsafe".to_string(),
                sensor_id: "s".to_string(),
                safety: SafetyLevel::Unsafe,
                description: "unsafe".to_string(),
                matched_findings: vec![],
                unmatched: false,
            },
        ],
        total_fixes: 3,
        matched_count: 3,
        unmatched_count: 0,
    };

    let safe_only = select_auto_apply_fixes(&summary, SafetyLevel::Safe, false);
    assert_eq!(safe_only.len(), 1);
    assert_eq!(safe_only[0].fix_id, "safe");

    let up_to_guarded = select_auto_apply_fixes(&summary, SafetyLevel::Guarded, false);
    assert_eq!(up_to_guarded.len(), 2);

    let all = select_auto_apply_fixes(&summary, SafetyLevel::Unsafe, false);
    assert_eq!(all.len(), 3);
}

#[test]
fn select_auto_apply_fixes_excludes_unmatched_when_required() {
    let summary = BuildfixSummary {
        fixes: vec![
            FixSummary {
                fix_id: "matched".to_string(),
                sensor_id: "s".to_string(),
                safety: SafetyLevel::Safe,
                description: "matched".to_string(),
                matched_findings: vec![],
                unmatched: false,
            },
            FixSummary {
                fix_id: "unmatched".to_string(),
                sensor_id: "s".to_string(),
                safety: SafetyLevel::Safe,
                description: "unmatched".to_string(),
                matched_findings: vec![],
                unmatched: true,
            },
        ],
        total_fixes: 2,
        matched_count: 1,
        unmatched_count: 1,
    };

    let with_match_required = select_auto_apply_fixes(&summary, SafetyLevel::Unsafe, true);
    assert_eq!(with_match_required.len(), 1);
    assert_eq!(with_match_required[0].fix_id, "matched");

    let without_match_required = select_auto_apply_fixes(&summary, SafetyLevel::Unsafe, false);
    assert_eq!(without_match_required.len(), 2);
}

#[test]
fn select_auto_apply_fixes_empty_summary_returns_empty() {
    let summary = BuildfixSummary::default();
    let result = select_auto_apply_fixes(&summary, SafetyLevel::Unsafe, false);
    assert!(result.is_empty());
}

#[test]
fn select_auto_apply_fixes_all_unmatched_with_require_matched_returns_empty() {
    let summary = BuildfixSummary {
        fixes: vec![FixSummary {
            fix_id: "lone".to_string(),
            sensor_id: "s".to_string(),
            safety: SafetyLevel::Safe,
            description: "d".to_string(),
            matched_findings: vec![],
            unmatched: true,
        }],
        total_fixes: 1,
        matched_count: 0,
        unmatched_count: 1,
    };

    let result = select_auto_apply_fixes(&summary, SafetyLevel::Unsafe, true);
    assert!(result.is_empty());
}

// ============================================================================
// canonical_policy_snapshot_bytes and policy_snapshot_sha256_hex
// ============================================================================

#[test]
fn canonical_policy_snapshot_bytes_is_deterministic() {
    let cfg = CockpitConfig::default();
    let snapshot = snapshot_policy(&cfg);

    let bytes1 = canonical_policy_snapshot_bytes(&snapshot).unwrap();
    let bytes2 = canonical_policy_snapshot_bytes(&snapshot).unwrap();
    assert_eq!(bytes1, bytes2);
}

#[test]
fn canonical_policy_snapshot_bytes_is_valid_json() {
    let cfg = CockpitConfig::default();
    let snapshot = snapshot_policy(&cfg);
    let bytes = canonical_policy_snapshot_bytes(&snapshot).unwrap();
    let parsed: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(parsed.is_object());
}

#[test]
fn canonical_policy_snapshot_bytes_changes_with_policy_diff() {
    let mut cfg1 = CockpitConfig::default();
    cfg1.policy.warn_is_fail = false;
    let snapshot1 = snapshot_policy(&cfg1);

    let mut cfg2 = CockpitConfig::default();
    cfg2.policy.warn_is_fail = true;
    let snapshot2 = snapshot_policy(&cfg2);

    let bytes1 = canonical_policy_snapshot_bytes(&snapshot1).unwrap();
    let bytes2 = canonical_policy_snapshot_bytes(&snapshot2).unwrap();
    assert_ne!(bytes1, bytes2);
}

#[test]
fn policy_snapshot_sha256_hex_is_valid_hex_64_chars() {
    let cfg = CockpitConfig::default();
    let snapshot = snapshot_policy(&cfg);
    let hex = policy_snapshot_sha256_hex(&snapshot).unwrap();
    assert_eq!(hex.len(), 64);
    assert!(hex.chars().all(|c| c.is_ascii_hexdigit()));
}

#[test]
fn policy_snapshot_sha256_hex_is_deterministic() {
    let cfg = CockpitConfig::default();
    let snapshot = snapshot_policy(&cfg);
    let hex1 = policy_snapshot_sha256_hex(&snapshot).unwrap();
    let hex2 = policy_snapshot_sha256_hex(&snapshot).unwrap();
    assert_eq!(hex1, hex2);
}

#[test]
fn policy_snapshot_sha256_hex_changes_with_sensor_addition() {
    let cfg1 = CockpitConfig::default();
    let snapshot1 = snapshot_policy(&cfg1);

    let mut cfg2 = CockpitConfig::default();
    cfg2.sensors.insert(
        "new-sensor".to_string(),
        SensorPolicy {
            blocking: true,
            missing: MissingPolicy::Fail,
            section: None,
            require_label: None,
            repro: None,
        },
    );
    let snapshot2 = snapshot_policy(&cfg2);

    let hex1 = policy_snapshot_sha256_hex(&snapshot1).unwrap();
    let hex2 = policy_snapshot_sha256_hex(&snapshot2).unwrap();
    assert_ne!(hex1, hex2);
}

#[test]
fn policy_snapshot_sha256_hex_matches_manual_sha256_of_canonical_bytes() {
    use sha2::{Digest, Sha256};

    let cfg = CockpitConfig::default();
    let snapshot = snapshot_policy(&cfg);

    let bytes = canonical_policy_snapshot_bytes(&snapshot).unwrap();
    let manual_hex = hex::encode(Sha256::digest(&bytes));
    let function_hex = policy_snapshot_sha256_hex(&snapshot).unwrap();
    assert_eq!(manual_hex, function_hex);
}
