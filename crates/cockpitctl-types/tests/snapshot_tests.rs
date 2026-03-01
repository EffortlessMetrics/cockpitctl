use std::collections::BTreeMap;

use cockpitctl_types::*;
use serde_json::json;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn sample_tool() -> ToolInfo {
    ToolInfo {
        name: "builddiag".into(),
        version: "1.2.3".into(),
        commit: Some("abc1234".into()),
    }
}

fn sample_run() -> RunInfo {
    RunInfo {
        started_at: "2025-01-15T10:00:00Z".into(),
        ended_at: Some("2025-01-15T10:01:30Z".into()),
        duration_ms: Some(90_000),
        host: Some(HostInfo {
            os: Some("linux".into()),
            arch: Some("x86_64".into()),
            hostname: Some("ci-runner-42".into()),
        }),
        git: Some(GitInfo {
            repo: Some("octocat/hello-world".into()),
            base_ref: Some("main".into()),
            head_ref: Some("feature/foo".into()),
            base_sha: Some("aaa1111".into()),
            head_sha: Some("bbb2222".into()),
            merge_base: Some("ccc3333".into()),
        }),
        ci: Some(CiInfo {
            provider: Some("github".into()),
            run_id: Some("12345".into()),
            run_url: Some("https://github.com/octocat/hello-world/actions/runs/12345".into()),
            job: Some("build".into()),
        }),
        capabilities: BTreeMap::from([
            (
                "git".into(),
                Capability {
                    status: CapabilityStatus::Available,
                    reason: None,
                },
            ),
            (
                "baseline".into(),
                Capability {
                    status: CapabilityStatus::Unavailable,
                    reason: Some("no baseline found".into()),
                },
            ),
        ]),
    }
}

fn sample_verdict() -> Verdict {
    Verdict {
        status: VerdictStatus::Fail,
        counts: VerdictCounts {
            info: 1,
            warn: 2,
            error: 3,
            suppressed: 0,
        },
        reasons: vec!["3 errors found".into()],
    }
}

fn sample_finding_full() -> Finding {
    Finding {
        severity: Severity::Error,
        check_id: Some("CK001".into()),
        code: "null-deref".into(),
        message: "Potential null dereference".into(),
        location: Some(Location {
            path: Some("src/main.rs".into()),
            line: Some(42),
            col: Some(10),
        }),
        help: Some("Add a null check before accessing".into()),
        url: Some("https://example.com/rules/null-deref".into()),
        fingerprint: Some("fp-abc123".into()),
        data: Some(json!({"extra": "detail"})),
    }
}

// ---------------------------------------------------------------------------
// VerdictStatus variants
// ---------------------------------------------------------------------------

#[test]
fn verdict_status_pass() {
    insta::assert_json_snapshot!(VerdictStatus::Pass, @r#""pass""#);
}

#[test]
fn verdict_status_warn() {
    insta::assert_json_snapshot!(VerdictStatus::Warn, @r#""warn""#);
}

#[test]
fn verdict_status_fail() {
    insta::assert_json_snapshot!(VerdictStatus::Fail, @r#""fail""#);
}

#[test]
fn verdict_status_skip() {
    insta::assert_json_snapshot!(VerdictStatus::Skip, @r#""skip""#);
}

// ---------------------------------------------------------------------------
// Severity variants
// ---------------------------------------------------------------------------

#[test]
fn severity_variants() {
    insta::assert_debug_snapshot!("severity_info", Severity::Info);
    insta::assert_debug_snapshot!("severity_warn", Severity::Warn);
    insta::assert_debug_snapshot!("severity_error", Severity::Error);
}

// ---------------------------------------------------------------------------
// VerdictCounts default
// ---------------------------------------------------------------------------

#[test]
fn verdict_counts_default() {
    insta::assert_json_snapshot!(VerdictCounts::default());
}

// ---------------------------------------------------------------------------
// Finding – all optional fields populated
// ---------------------------------------------------------------------------

#[test]
fn finding_full() {
    insta::assert_json_snapshot!(sample_finding_full());
}

// ---------------------------------------------------------------------------
// Finding – minimal (no optional fields)
// ---------------------------------------------------------------------------

#[test]
fn finding_minimal() {
    let f = Finding {
        severity: Severity::Info,
        check_id: None,
        code: "note".into(),
        message: "Just a note".into(),
        location: None,
        help: None,
        url: None,
        fingerprint: None,
        data: None,
    };
    insta::assert_json_snapshot!(f);
}

// ---------------------------------------------------------------------------
// Highlight
// ---------------------------------------------------------------------------

#[test]
fn highlight_snapshot() {
    let h = Highlight {
        sensor_id: "builddiag".into(),
        finding: sample_finding_full(),
    };
    insta::assert_json_snapshot!(h);
}

// ---------------------------------------------------------------------------
// SensorReport – all fields
// ---------------------------------------------------------------------------

#[test]
fn sensor_report_full() {
    let report = SensorReport {
        schema: "sensor.report.v1".into(),
        tool: sample_tool(),
        run: sample_run(),
        verdict: sample_verdict(),
        findings: vec![sample_finding_full()],
        artifacts: vec![ArtifactPointer {
            id: "coverage-lcov".into(),
            path: "artifacts/builddiag/coverage.lcov".into(),
            mime: "text/plain".into(),
            schema: None,
        }],
        data: Some(json!({"custom_key": "custom_value"})),
    };
    insta::assert_json_snapshot!(report);
}

// ---------------------------------------------------------------------------
// SensorReport – empty/minimal
// ---------------------------------------------------------------------------

#[test]
fn sensor_report_minimal() {
    let report = SensorReport {
        schema: "sensor.report.v1".into(),
        tool: ToolInfo {
            name: "minimal-sensor".into(),
            version: "0.1.0".into(),
            commit: None,
        },
        run: RunInfo {
            started_at: "2025-01-15T10:00:00Z".into(),
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
    };
    insta::assert_json_snapshot!(report);
}

// ---------------------------------------------------------------------------
// CockpitReport – typical data
// ---------------------------------------------------------------------------

#[test]
fn cockpit_report_typical() {
    let report = CockpitReport {
        schema: "cockpit.report.v1".into(),
        tool: ToolInfo {
            name: "cockpitctl".into(),
            version: "0.5.0".into(),
            commit: Some("def5678".into()),
        },
        run: sample_run(),
        verdict: sample_verdict(),
        sensors: vec![SensorSummary {
            id: "builddiag".into(),
            blocking: true,
            missing: MissingPolicy::Fail,
            presence: Presence::Present,
            report_path: "artifacts/builddiag/report.json".into(),
            comment_path: None,
            verdict: sample_verdict(),
            truncated: false,
            errors: vec![],
            missing_policy_applied: None,
            policy_outcome: Some(PolicyOutcome::Blocked),
        }],
        highlights: vec![Highlight {
            sensor_id: "builddiag".into(),
            finding: sample_finding_full(),
        }],
        policy: PolicySnapshot {
            warn_is_fail: false,
            max_highlights: 7,
            max_per_sensor_findings: 20,
            max_annotations: 25,
            section_order: vec!["Highlights".into(), "Diagnostics".into()],
            sensors: vec![PolicySensorSnapshot {
                id: "builddiag".into(),
                blocking: true,
                missing: MissingPolicy::Fail,
                section: Some("Diagnostics".into()),
                require_label: None,
                repro: Some("cargo build 2>&1".into()),
            }],
        },
        data: None,
    };
    insta::assert_json_snapshot!(report);
}

// ---------------------------------------------------------------------------
// Enums – MissingPolicy, Presence, PolicyOutcome
// ---------------------------------------------------------------------------

#[test]
fn missing_policy_variants() {
    insta::assert_json_snapshot!("missing_skip", MissingPolicy::Skip);
    insta::assert_json_snapshot!("missing_warn", MissingPolicy::Warn);
    insta::assert_json_snapshot!("missing_fail", MissingPolicy::Fail);
}

#[test]
fn presence_variants() {
    insta::assert_json_snapshot!("presence_present", Presence::Present);
    insta::assert_json_snapshot!("presence_missing", Presence::Missing);
    insta::assert_json_snapshot!("presence_invalid", Presence::Invalid);
}

#[test]
fn policy_outcome_variants() {
    insta::assert_json_snapshot!("policy_blocked", PolicyOutcome::Blocked);
    insta::assert_json_snapshot!("policy_allowed", PolicyOutcome::Allowed);
    insta::assert_json_snapshot!("policy_informational", PolicyOutcome::Informational);
}

// ---------------------------------------------------------------------------
// SafetyLevel variants
// ---------------------------------------------------------------------------

#[test]
fn safety_level_variants() {
    insta::assert_json_snapshot!("safety_safe", SafetyLevel::Safe);
    insta::assert_json_snapshot!("safety_guarded", SafetyLevel::Guarded);
    insta::assert_json_snapshot!("safety_unsafe", SafetyLevel::Unsafe);
}

// ---------------------------------------------------------------------------
// SchemaValidation variants
// ---------------------------------------------------------------------------

#[test]
fn schema_validation_variants() {
    insta::assert_json_snapshot!("schema_lax", SchemaValidation::Lax);
    insta::assert_json_snapshot!("schema_strict", SchemaValidation::Strict);
}

// ---------------------------------------------------------------------------
// Policy default
// ---------------------------------------------------------------------------

#[test]
fn policy_default() {
    insta::assert_json_snapshot!(Policy::default());
}

// ---------------------------------------------------------------------------
// SensorPolicy default
// ---------------------------------------------------------------------------

#[test]
fn sensor_policy_default() {
    insta::assert_json_snapshot!(SensorPolicy::default());
}

// ---------------------------------------------------------------------------
// Location debug
// ---------------------------------------------------------------------------

#[test]
fn location_debug() {
    let loc = Location {
        path: Some("src/lib.rs".into()),
        line: Some(10),
        col: Some(5),
    };
    insta::assert_debug_snapshot!(loc);
}

// ---------------------------------------------------------------------------
// ArtifactPointer with schema
// ---------------------------------------------------------------------------

#[test]
fn artifact_pointer_with_schema() {
    let ap = ArtifactPointer {
        id: "coverage".into(),
        path: "artifacts/cov/lcov.info".into(),
        mime: "text/plain".into(),
        schema: Some("coverage.lcov.v1".into()),
    };
    insta::assert_json_snapshot!(ap);
}

// ---------------------------------------------------------------------------
// BuildfixPlan
// ---------------------------------------------------------------------------

#[test]
fn buildfix_plan_snapshot() {
    let plan = BuildfixPlan {
        schema: "buildfix.plan.v1".into(),
        tool: sample_tool(),
        fixes: vec![Fix {
            id: "fix-1".into(),
            safety: SafetyLevel::Safe,
            description: "Add missing import".into(),
            finding_refs: vec![FindingRef {
                sensor_id: "builddiag".into(),
                fingerprint: Some("fp-abc123".into()),
                code: Some("E0432".into()),
                tool: None,
                check_id: None,
            }],
            preconditions: Some(Preconditions {
                repo_head: "abc1234".into(),
                receipt_digests: vec!["sha256:deadbeef".into()],
            }),
            data: None,
        }],
    };
    insta::assert_json_snapshot!(plan);
}

// ---------------------------------------------------------------------------
// TrendDelta
// ---------------------------------------------------------------------------

#[test]
fn trend_delta_snapshot() {
    let delta = TrendDelta {
        verdict_change: Some(VerdictChange {
            before: VerdictStatus::Pass,
            after: VerdictStatus::Fail,
        }),
        count_deltas: CountDeltas {
            info_delta: 0,
            warn_delta: 1,
            error_delta: 2,
        },
        new_findings: vec![TrendFinding {
            sensor_id: "builddiag".into(),
            code: "null-deref".into(),
            message: "Potential null dereference".into(),
            path: Some("src/main.rs".into()),
            line: Some(42),
            fingerprint: Some("fp-new".into()),
            severity: Severity::Error,
        }],
        fixed_findings: vec![],
        sensors_added: vec!["new-sensor".into()],
        sensors_removed: vec![],
    };
    insta::assert_json_snapshot!(delta);
}

// ---------------------------------------------------------------------------
// BuildfixSummary default
// ---------------------------------------------------------------------------

#[test]
fn buildfix_summary_default() {
    insta::assert_json_snapshot!(BuildfixSummary::default());
}

// ---------------------------------------------------------------------------
// CountDeltas default
// ---------------------------------------------------------------------------

#[test]
fn count_deltas_default() {
    insta::assert_json_snapshot!(CountDeltas::default());
}

// ---------------------------------------------------------------------------
// New expanded snapshot scenarios
// ---------------------------------------------------------------------------

#[test]
fn cockpit_config_default() {
    insta::assert_json_snapshot!(CockpitConfig::default());
}

#[test]
fn run_info_full() {
    insta::assert_json_snapshot!(sample_run());
}

#[test]
fn sensor_summary_full() {
    let summary = SensorSummary {
        id: "builddiag".into(),
        blocking: true,
        missing: MissingPolicy::Fail,
        presence: Presence::Present,
        report_path: "artifacts/builddiag/report.json".into(),
        comment_path: Some("artifacts/builddiag/comment.md".into()),
        verdict: sample_verdict(),
        truncated: true,
        errors: vec!["some warning".into()],
        missing_policy_applied: None,
        policy_outcome: Some(PolicyOutcome::Blocked),
    };
    insta::assert_json_snapshot!(summary);
}

#[test]
fn verdict_with_reasons() {
    let v = Verdict {
        status: VerdictStatus::Fail,
        counts: VerdictCounts {
            info: 2,
            warn: 3,
            error: 5,
            suppressed: 1,
        },
        reasons: vec![
            "blocking_sensor_failed".into(),
            "warn_is_fail".into(),
            "schema_violation".into(),
        ],
    };
    insta::assert_json_snapshot!(v);
}

#[test]
fn capability_status_variants() {
    insta::assert_json_snapshot!("cap_available", CapabilityStatus::Available);
    insta::assert_json_snapshot!("cap_unavailable", CapabilityStatus::Unavailable);
    insta::assert_json_snapshot!("cap_skipped", CapabilityStatus::Skipped);
}

#[test]
fn host_info_snapshot() {
    let h = HostInfo {
        os: Some("linux".into()),
        arch: Some("aarch64".into()),
        hostname: Some("runner-1".into()),
    };
    insta::assert_json_snapshot!(h);
}

#[test]
fn git_info_snapshot() {
    let g = GitInfo {
        repo: Some("org/repo".into()),
        base_ref: Some("main".into()),
        head_ref: Some("feature/x".into()),
        base_sha: Some("aaa".into()),
        head_sha: Some("bbb".into()),
        merge_base: Some("ccc".into()),
    };
    insta::assert_json_snapshot!(g);
}

#[test]
fn ci_info_snapshot() {
    let ci = CiInfo {
        provider: Some("github".into()),
        run_id: Some("99999".into()),
        run_url: Some("https://github.com/org/repo/actions/runs/99999".into()),
        job: Some("test".into()),
    };
    insta::assert_json_snapshot!(ci);
}

#[test]
fn cockpit_promote_hints_snapshot() {
    let hints = CockpitPromoteHints {
        schema: Some("cockpit.promote.v1".into()),
        cards: vec![PromoteCard {
            id: "cov-percent".into(),
            label: "Coverage".into(),
            value: "85.2%".into(),
            severity: Some(Severity::Warn),
        }],
        suggested_highlights: vec![SuggestedHighlight {
            finding_fingerprint: "fp-cov-drop".into(),
        }],
        suggested_artifacts: vec![SuggestedArtifact {
            artifact_id: "lcov".into(),
        }],
    };
    insta::assert_json_snapshot!(hints);
}

#[test]
fn fix_summary_snapshot() {
    let fs = FixSummary {
        fix_id: "fix-1".into(),
        sensor_id: "builddiag".into(),
        safety: SafetyLevel::Safe,
        description: "Add missing import".into(),
        matched_findings: vec![MatchedFinding {
            sensor_id: "builddiag".into(),
            code: "E0432".into(),
            fingerprint: Some("fp-123".into()),
        }],
        unmatched: false,
    };
    insta::assert_json_snapshot!(fs);
}

#[test]
fn buildfix_apply_summary_full() {
    let summary = BuildfixApplySummary {
        status: BuildfixApplyStatus::Applied,
        auto_apply_enabled: true,
        max_auto_apply_safety: SafetyLevel::Safe,
        require_matched_finding: true,
        candidate_fix_ids: vec!["fix-1".into(), "fix-2".into()],
        selected_fix_ids: vec!["fix-1".into()],
        applied_fix_ids: vec!["fix-1".into()],
        skipped_fix_ids: vec!["fix-2".into()],
        errors: vec![],
        reason: Some("auto-applied safe fix".into()),
        actuator_command: Some("buildfix-actuator apply".into()),
    };
    insta::assert_json_snapshot!(summary);
}

#[test]
fn policy_signature_evidence_snapshot() {
    let evidence = PolicySignatureEvidence {
        schema: "policy.signature.v1".into(),
        algorithm: PolicySignatureAlgorithm::HmacSha256,
        policy_sha256: "abcdef1234567890".into(),
        signature: "sig-hex-value".into(),
        key_id: Some("key-1".into()),
    };
    insta::assert_json_snapshot!(evidence);
}
