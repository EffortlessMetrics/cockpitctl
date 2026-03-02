//! Comprehensive serde roundtrip and invariant tests for cockpitctl-types.

use std::collections::BTreeMap;

use cockpitctl_types::*;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

// ---------------------------------------------------------------------------
// Helper
// ---------------------------------------------------------------------------

fn roundtrip_json<T>(value: &T) -> T
where
    T: Serialize + DeserializeOwned + PartialEq + std::fmt::Debug,
{
    let json = serde_json::to_string(value).expect("serialize");
    let back: T = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(value, &back, "roundtrip mismatch for {json}");
    back
}

// ---------------------------------------------------------------------------
// 1. Roundtrip every major type through JSON
// ---------------------------------------------------------------------------

#[test]
fn roundtrip_verdict_status() {
    for v in [
        VerdictStatus::Pass,
        VerdictStatus::Warn,
        VerdictStatus::Fail,
        VerdictStatus::Skip,
    ] {
        roundtrip_json(&v);
    }
}

#[test]
fn roundtrip_severity() {
    for s in [Severity::Info, Severity::Warn, Severity::Error] {
        roundtrip_json(&s);
    }
}

#[test]
fn roundtrip_verdict_counts() {
    roundtrip_json(&VerdictCounts {
        info: 10,
        warn: 5,
        error: 1,
        suppressed: 3,
    });
    roundtrip_json(&VerdictCounts::default());
}

#[test]
fn roundtrip_verdict() {
    roundtrip_json(&Verdict {
        status: VerdictStatus::Fail,
        counts: VerdictCounts {
            info: 0,
            warn: 1,
            error: 2,
            suppressed: 0,
        },
        reasons: vec!["missing_receipt".into()],
    });
}

#[test]
fn roundtrip_tool_info() {
    roundtrip_json(&ToolInfo {
        name: "builddiag".into(),
        version: "1.0.0".into(),
        commit: Some("abc123".into()),
    });
    roundtrip_json(&ToolInfo {
        name: "t".into(),
        version: "0.1.0".into(),
        commit: None,
    });
}

#[test]
fn roundtrip_host_info() {
    roundtrip_json(&HostInfo {
        os: Some("linux".into()),
        arch: Some("x86_64".into()),
        hostname: Some("ci-runner-1".into()),
    });
    roundtrip_json(&HostInfo {
        os: None,
        arch: None,
        hostname: None,
    });
}

#[test]
fn roundtrip_git_info() {
    roundtrip_json(&GitInfo {
        repo: Some("org/repo".into()),
        base_ref: Some("main".into()),
        head_ref: Some("feature".into()),
        base_sha: Some("aaa".into()),
        head_sha: Some("bbb".into()),
        merge_base: Some("ccc".into()),
    });
    roundtrip_json(&GitInfo {
        repo: None,
        base_ref: None,
        head_ref: None,
        base_sha: None,
        head_sha: None,
        merge_base: None,
    });
}

#[test]
fn roundtrip_ci_info() {
    roundtrip_json(&CiInfo {
        provider: Some("github".into()),
        run_id: Some("12345".into()),
        run_url: Some("https://github.com/run/1".into()),
        job: Some("build".into()),
    });
    roundtrip_json(&CiInfo {
        provider: None,
        run_id: None,
        run_url: None,
        job: None,
    });
}

#[test]
fn roundtrip_capability_status() {
    for cs in [
        CapabilityStatus::Available,
        CapabilityStatus::Unavailable,
        CapabilityStatus::Skipped,
    ] {
        roundtrip_json(&cs);
    }
}

#[test]
fn roundtrip_capability() {
    roundtrip_json(&Capability {
        status: CapabilityStatus::Available,
        reason: Some("found git".into()),
    });
    roundtrip_json(&Capability {
        status: CapabilityStatus::Unavailable,
        reason: None,
    });
}

fn make_full_run_info() -> RunInfo {
    let mut caps = BTreeMap::new();
    caps.insert(
        "git".into(),
        Capability {
            status: CapabilityStatus::Available,
            reason: None,
        },
    );
    RunInfo {
        started_at: "2026-01-01T00:00:00Z".into(),
        ended_at: Some("2026-01-01T00:01:00Z".into()),
        duration_ms: Some(60000),
        host: Some(HostInfo {
            os: Some("linux".into()),
            arch: Some("x86_64".into()),
            hostname: None,
        }),
        git: Some(GitInfo {
            repo: Some("org/repo".into()),
            base_ref: None,
            head_ref: None,
            base_sha: None,
            head_sha: None,
            merge_base: None,
        }),
        ci: Some(CiInfo {
            provider: Some("github".into()),
            run_id: None,
            run_url: None,
            job: None,
        }),
        capabilities: caps,
    }
}

fn make_minimal_run_info() -> RunInfo {
    RunInfo {
        started_at: "2026-01-01T00:00:00Z".into(),
        ended_at: None,
        duration_ms: None,
        host: None,
        git: None,
        ci: None,
        capabilities: BTreeMap::new(),
    }
}

#[test]
fn roundtrip_run_info_full() {
    roundtrip_json(&make_full_run_info());
}

#[test]
fn roundtrip_run_info_minimal() {
    roundtrip_json(&make_minimal_run_info());
}

#[test]
fn roundtrip_location() {
    roundtrip_json(&Location {
        path: Some("src/lib.rs".into()),
        line: Some(42),
        col: Some(10),
    });
    roundtrip_json(&Location {
        path: None,
        line: None,
        col: None,
    });
}

fn make_full_finding() -> Finding {
    Finding {
        severity: Severity::Error,
        check_id: Some("clippy.warning".into()),
        code: "unused_var".into(),
        message: "unused variable `x`".into(),
        location: Some(Location {
            path: Some("src/lib.rs".into()),
            line: Some(42),
            col: None,
        }),
        help: Some("consider prefixing with `_`".into()),
        url: Some("https://example.com".into()),
        fingerprint: Some("fp123".into()),
        data: Some(serde_json::json!({"extra": true})),
    }
}

fn make_minimal_finding() -> Finding {
    Finding {
        severity: Severity::Info,
        check_id: None,
        code: "ok".into(),
        message: "all good".into(),
        location: None,
        help: None,
        url: None,
        fingerprint: None,
        data: None,
    }
}

#[test]
fn roundtrip_finding_full() {
    roundtrip_json(&make_full_finding());
}

#[test]
fn roundtrip_finding_minimal() {
    roundtrip_json(&make_minimal_finding());
}

#[test]
fn roundtrip_artifact_pointer() {
    roundtrip_json(&ArtifactPointer {
        id: "coverage".into(),
        path: "artifacts/cov/lcov.info".into(),
        mime: "text/plain".into(),
        schema: Some("coverage.v1".into()),
    });
    roundtrip_json(&ArtifactPointer {
        id: "log".into(),
        path: "out.log".into(),
        mime: "text/plain".into(),
        schema: None,
    });
}

#[test]
fn roundtrip_missing_policy() {
    for mp in [
        MissingPolicy::Skip,
        MissingPolicy::Warn,
        MissingPolicy::Fail,
    ] {
        roundtrip_json(&mp);
    }
}

#[test]
fn roundtrip_presence() {
    for p in [Presence::Present, Presence::Missing, Presence::Invalid] {
        roundtrip_json(&p);
    }
}

#[test]
fn roundtrip_policy_outcome() {
    for po in [
        PolicyOutcome::Blocked,
        PolicyOutcome::Allowed,
        PolicyOutcome::Informational,
    ] {
        roundtrip_json(&po);
    }
}

#[test]
fn roundtrip_schema_validation() {
    for sv in [SchemaValidation::Lax, SchemaValidation::Strict] {
        roundtrip_json(&sv);
    }
}

#[test]
fn roundtrip_safety_level() {
    for sl in [SafetyLevel::Safe, SafetyLevel::Guarded, SafetyLevel::Unsafe] {
        roundtrip_json(&sl);
    }
}

fn make_empty_sensor_report() -> SensorReport {
    SensorReport {
        schema: "sensor.report.v1".into(),
        tool: ToolInfo {
            name: "t".into(),
            version: "0.1.0".into(),
            commit: None,
        },
        run: make_minimal_run_info(),
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

fn make_populated_sensor_report() -> SensorReport {
    SensorReport {
        schema: "sensor.report.v1".into(),
        tool: ToolInfo {
            name: "builddiag".into(),
            version: "2.0.0".into(),
            commit: Some("deadbeef".into()),
        },
        run: make_full_run_info(),
        verdict: Verdict {
            status: VerdictStatus::Fail,
            counts: VerdictCounts {
                info: 1,
                warn: 2,
                error: 3,
                suppressed: 1,
            },
            reasons: vec!["errors_found".into()],
        },
        findings: vec![make_full_finding(), make_minimal_finding()],
        artifacts: vec![ArtifactPointer {
            id: "log".into(),
            path: "build.log".into(),
            mime: "text/plain".into(),
            schema: None,
        }],
        data: Some(serde_json::json!({"custom": "payload"})),
    }
}

#[test]
fn roundtrip_sensor_report_empty() {
    roundtrip_json(&make_empty_sensor_report());
}

#[test]
fn roundtrip_sensor_report_populated() {
    roundtrip_json(&make_populated_sensor_report());
}

#[test]
fn roundtrip_highlight() {
    roundtrip_json(&Highlight {
        sensor_id: "builddiag".into(),
        finding: make_full_finding(),
    });
}

fn make_sensor_summary() -> SensorSummary {
    SensorSummary {
        id: "builddiag".into(),
        blocking: true,
        missing: MissingPolicy::Fail,
        presence: Presence::Present,
        report_path: "artifacts/builddiag/report.json".into(),
        comment_path: Some("artifacts/builddiag/comment.md".into()),
        verdict: Verdict {
            status: VerdictStatus::Fail,
            counts: VerdictCounts {
                info: 0,
                warn: 0,
                error: 1,
                suppressed: 0,
            },
            reasons: vec![],
        },
        truncated: false,
        errors: vec![],
        missing_policy_applied: None,
        policy_outcome: Some(PolicyOutcome::Blocked),
    }
}

#[test]
fn roundtrip_sensor_summary() {
    roundtrip_json(&make_sensor_summary());
}

#[test]
fn roundtrip_policy_snapshot() {
    roundtrip_json(&PolicySnapshot {
        warn_is_fail: true,
        max_highlights: 7,
        max_per_sensor_findings: 20,
        max_annotations: 25,
        section_order: vec!["Highlights".into(), "Other".into()],
        sensors: vec![PolicySensorSnapshot {
            id: "builddiag".into(),
            blocking: true,
            missing: MissingPolicy::Fail,
            section: Some("Diagnostics".into()),
            require_label: None,
            repro: Some("cargo build".into()),
        }],
    });
}

#[test]
fn roundtrip_cockpit_report_full() {
    roundtrip_json(&CockpitReport {
        schema: "cockpit.report.v1".into(),
        tool: ToolInfo {
            name: "cockpitctl".into(),
            version: "0.1.0".into(),
            commit: None,
        },
        run: make_full_run_info(),
        verdict: Verdict {
            status: VerdictStatus::Fail,
            counts: VerdictCounts {
                info: 1,
                warn: 2,
                error: 3,
                suppressed: 0,
            },
            reasons: vec!["blocked_sensor".into()],
        },
        sensors: vec![make_sensor_summary()],
        highlights: vec![Highlight {
            sensor_id: "builddiag".into(),
            finding: make_full_finding(),
        }],
        policy: PolicySnapshot {
            warn_is_fail: false,
            max_highlights: 7,
            max_per_sensor_findings: 20,
            max_annotations: 25,
            section_order: vec!["Highlights".into()],
            sensors: vec![],
        },
        data: Some(serde_json::json!({"_cockpit": {}})),
    });
}

#[test]
fn roundtrip_cockpit_report_empty() {
    roundtrip_json(&CockpitReport {
        schema: "cockpit.report.v1".into(),
        tool: ToolInfo {
            name: "cockpitctl".into(),
            version: "0.1.0".into(),
            commit: None,
        },
        run: make_minimal_run_info(),
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
    });
}

#[test]
fn roundtrip_sensor_policy() {
    roundtrip_json(&SensorPolicy {
        blocking: true,
        missing: MissingPolicy::Fail,
        section: Some("Diagnostics".into()),
        require_label: Some("run-tests".into()),
        repro: Some("cargo test".into()),
    });
    roundtrip_json(&SensorPolicy::default());
}

#[test]
fn roundtrip_cockpit_config() {
    let mut sensors = BTreeMap::new();
    sensors.insert(
        "builddiag".into(),
        SensorPolicy {
            blocking: true,
            missing: MissingPolicy::Fail,
            section: None,
            require_label: None,
            repro: None,
        },
    );
    roundtrip_json(&CockpitConfig {
        policy: Policy::default(),
        buildfix: BuildfixPolicy::default(),
        policy_signing: PolicySigningConfig::default(),
        sensors,
        hooks: vec![HookConfig {
            name: "notify".into(),
            command: "echo done".into(),
            when: HookWhen::AfterIngest,
            timeout_ms: 5000,
        }],
    });
    roundtrip_json(&CockpitConfig::default());
}

#[test]
fn roundtrip_buildfix_plan() {
    roundtrip_json(&BuildfixPlan {
        schema: "buildfix.plan.v1".into(),
        tool: ToolInfo {
            name: "fixbot".into(),
            version: "1.0.0".into(),
            commit: None,
        },
        fixes: vec![Fix {
            id: "fix-1".into(),
            safety: SafetyLevel::Safe,
            description: "Remove unused import".into(),
            finding_refs: vec![FindingRef {
                sensor_id: "builddiag".into(),
                fingerprint: Some("fp1".into()),
                code: Some("unused_import".into()),
                tool: None,
                check_id: None,
            }],
            preconditions: Some(Preconditions {
                repo_head: "abc123".into(),
                receipt_digests: vec!["sha256:deadbeef".into()],
            }),
            data: None,
        }],
    });
}

#[test]
fn roundtrip_trend_delta() {
    roundtrip_json(&TrendDelta {
        verdict_change: Some(VerdictChange {
            before: VerdictStatus::Pass,
            after: VerdictStatus::Fail,
        }),
        count_deltas: CountDeltas {
            info_delta: -1,
            warn_delta: 0,
            error_delta: 2,
        },
        new_findings: vec![TrendFinding {
            sensor_id: "builddiag".into(),
            code: "err1".into(),
            message: "new error".into(),
            path: Some("src/lib.rs".into()),
            line: Some(10),
            fingerprint: None,
            severity: Severity::Error,
        }],
        fixed_findings: vec![],
        sensors_added: vec!["new-sensor".into()],
        sensors_removed: vec![],
    });
}

#[test]
fn roundtrip_buildfix_summary() {
    roundtrip_json(&BuildfixSummary {
        fixes: vec![FixSummary {
            fix_id: "fix-1".into(),
            sensor_id: "builddiag".into(),
            safety: SafetyLevel::Safe,
            description: "Remove unused import".into(),
            matched_findings: vec![MatchedFinding {
                sensor_id: "builddiag".into(),
                code: "unused_import".into(),
                fingerprint: Some("fp1".into()),
            }],
            unmatched: false,
        }],
        total_fixes: 1,
        matched_count: 1,
        unmatched_count: 0,
    });
    roundtrip_json(&BuildfixSummary::default());
}

#[test]
fn roundtrip_buildfix_apply_request() {
    roundtrip_json(&BuildfixApplyRequest {
        schema: BUILDFIX_APPLY_REQUEST_SCHEMA_ID.into(),
        max_auto_apply_safety: SafetyLevel::Safe,
        require_matched_finding: true,
        fixes: vec![],
    });
}

#[test]
fn roundtrip_buildfix_actuator_result() {
    roundtrip_json(&BuildfixActuatorResult {
        applied_fix_ids: vec!["fix-1".into()],
        skipped_fix_ids: vec!["fix-2".into()],
        errors: vec![],
    });
    roundtrip_json(&BuildfixActuatorResult::default());
}

#[test]
fn roundtrip_buildfix_apply_summary() {
    roundtrip_json(&BuildfixApplySummary {
        status: BuildfixApplyStatus::Applied,
        auto_apply_enabled: true,
        max_auto_apply_safety: SafetyLevel::Safe,
        require_matched_finding: true,
        candidate_fix_ids: vec!["fix-1".into()],
        selected_fix_ids: vec!["fix-1".into()],
        applied_fix_ids: vec!["fix-1".into()],
        skipped_fix_ids: vec![],
        errors: vec![],
        reason: None,
        actuator_command: Some("./apply.sh".into()),
    });
}

#[test]
fn roundtrip_policy_signature_evidence() {
    roundtrip_json(&PolicySignatureEvidence {
        schema: POLICY_SIGNATURE_SCHEMA_ID.into(),
        algorithm: PolicySignatureAlgorithm::HmacSha256,
        policy_sha256: "abcdef0123456789".into(),
        signature: "sig_hex".into(),
        key_id: Some("key-1".into()),
    });
}

#[test]
fn roundtrip_cockpit_promote_hints() {
    roundtrip_json(&CockpitPromoteHints {
        schema: Some("cockpit.promote.v1".into()),
        cards: vec![PromoteCard {
            id: "coverage".into(),
            label: "Coverage".into(),
            value: "85%".into(),
            severity: Some(Severity::Warn),
        }],
        suggested_highlights: vec![SuggestedHighlight {
            finding_fingerprint: "fp1".into(),
        }],
        suggested_artifacts: vec![SuggestedArtifact {
            artifact_id: "coverage-report".into(),
        }],
    });
    roundtrip_json(&CockpitPromoteHints {
        schema: None,
        cards: vec![],
        suggested_highlights: vec![],
        suggested_artifacts: vec![],
    });
}

#[test]
fn roundtrip_hook_config() {
    roundtrip_json(&HookConfig {
        name: "post-ingest".into(),
        command: "echo ok".into(),
        when: HookWhen::AfterIngest,
        timeout_ms: 10_000,
    });
}

// ---------------------------------------------------------------------------
// 2. Enum variant serialization to exact snake_case strings
// ---------------------------------------------------------------------------

#[test]
fn enum_verdict_status_strings() {
    assert_eq!(serde_json::to_value(VerdictStatus::Pass).unwrap(), "pass");
    assert_eq!(serde_json::to_value(VerdictStatus::Warn).unwrap(), "warn");
    assert_eq!(serde_json::to_value(VerdictStatus::Fail).unwrap(), "fail");
    assert_eq!(serde_json::to_value(VerdictStatus::Skip).unwrap(), "skip");
}

#[test]
fn enum_severity_strings() {
    assert_eq!(serde_json::to_value(Severity::Info).unwrap(), "info");
    assert_eq!(serde_json::to_value(Severity::Warn).unwrap(), "warn");
    assert_eq!(serde_json::to_value(Severity::Error).unwrap(), "error");
}

#[test]
fn enum_missing_policy_strings() {
    assert_eq!(serde_json::to_value(MissingPolicy::Skip).unwrap(), "skip");
    assert_eq!(serde_json::to_value(MissingPolicy::Warn).unwrap(), "warn");
    assert_eq!(serde_json::to_value(MissingPolicy::Fail).unwrap(), "fail");
}

#[test]
fn enum_presence_strings() {
    assert_eq!(serde_json::to_value(Presence::Present).unwrap(), "present");
    assert_eq!(serde_json::to_value(Presence::Missing).unwrap(), "missing");
    assert_eq!(serde_json::to_value(Presence::Invalid).unwrap(), "invalid");
}

#[test]
fn enum_policy_outcome_strings() {
    assert_eq!(
        serde_json::to_value(PolicyOutcome::Blocked).unwrap(),
        "blocked"
    );
    assert_eq!(
        serde_json::to_value(PolicyOutcome::Allowed).unwrap(),
        "allowed"
    );
    assert_eq!(
        serde_json::to_value(PolicyOutcome::Informational).unwrap(),
        "informational"
    );
}

#[test]
fn enum_capability_status_strings() {
    assert_eq!(
        serde_json::to_value(CapabilityStatus::Available).unwrap(),
        "available"
    );
    assert_eq!(
        serde_json::to_value(CapabilityStatus::Unavailable).unwrap(),
        "unavailable"
    );
    assert_eq!(
        serde_json::to_value(CapabilityStatus::Skipped).unwrap(),
        "skipped"
    );
}

#[test]
fn enum_schema_validation_strings() {
    assert_eq!(serde_json::to_value(SchemaValidation::Lax).unwrap(), "lax");
    assert_eq!(
        serde_json::to_value(SchemaValidation::Strict).unwrap(),
        "strict"
    );
}

#[test]
fn enum_safety_level_strings() {
    assert_eq!(serde_json::to_value(SafetyLevel::Safe).unwrap(), "safe");
    assert_eq!(
        serde_json::to_value(SafetyLevel::Guarded).unwrap(),
        "guarded"
    );
    assert_eq!(serde_json::to_value(SafetyLevel::Unsafe).unwrap(), "unsafe");
}

#[test]
fn enum_trend_change_strings() {
    assert_eq!(serde_json::to_value(TrendChange::New).unwrap(), "new");
    assert_eq!(serde_json::to_value(TrendChange::Fixed).unwrap(), "fixed");
}

#[test]
fn enum_hook_when_strings() {
    assert_eq!(
        serde_json::to_value(HookWhen::AfterIngest).unwrap(),
        "after_ingest"
    );
}

#[test]
fn enum_buildfix_apply_status_strings() {
    assert_eq!(
        serde_json::to_value(BuildfixApplyStatus::Skipped).unwrap(),
        "skipped"
    );
    assert_eq!(
        serde_json::to_value(BuildfixApplyStatus::Applied).unwrap(),
        "applied"
    );
    assert_eq!(
        serde_json::to_value(BuildfixApplyStatus::Failed).unwrap(),
        "failed"
    );
}

#[test]
fn enum_policy_signature_algorithm_strings() {
    assert_eq!(
        serde_json::to_value(PolicySignatureAlgorithm::HmacSha256).unwrap(),
        "hmac_sha256"
    );
}

// ---------------------------------------------------------------------------
// 3. Unknown fields in JSON are ignored (serde default behavior)
// ---------------------------------------------------------------------------

#[test]
fn unknown_fields_ignored_verdict_counts() {
    let json = r#"{"info":1,"warn":2,"error":3,"suppressed":0,"unknown_field":"ignored"}"#;
    let counts: VerdictCounts = serde_json::from_str(json).expect("should ignore unknown fields");
    assert_eq!(counts.info, 1);
    assert_eq!(counts.warn, 2);
    assert_eq!(counts.error, 3);
}

#[test]
fn unknown_fields_ignored_tool_info() {
    let json = r#"{"name":"t","version":"1.0","extra":true}"#;
    let ti: ToolInfo = serde_json::from_str(json).expect("should ignore unknown fields");
    assert_eq!(ti.name, "t");
}

#[test]
fn unknown_fields_ignored_finding() {
    let json = r#"{"severity":"error","code":"c","message":"m","brand_new_field":42}"#;
    let f: Finding = serde_json::from_str(json).expect("should ignore unknown fields");
    assert_eq!(f.severity, Severity::Error);
}

#[test]
fn unknown_fields_ignored_sensor_report() {
    let json = serde_json::to_string(&make_empty_sensor_report()).unwrap();
    let mut v: Value = serde_json::from_str(&json).unwrap();
    v.as_object_mut()
        .unwrap()
        .insert("future_field".into(), Value::Bool(true));
    let _: SensorReport = serde_json::from_value(v).expect("should ignore unknown fields");
}

// ---------------------------------------------------------------------------
// 4. Unknown enum variants are rejected
// ---------------------------------------------------------------------------

#[test]
fn unknown_verdict_status_rejected() {
    let r = serde_json::from_str::<VerdictStatus>(r#""unknown""#);
    assert!(r.is_err());
}

#[test]
fn unknown_severity_rejected() {
    let r = serde_json::from_str::<Severity>(r#""critical""#);
    assert!(r.is_err());
}

#[test]
fn unknown_missing_policy_rejected() {
    let r = serde_json::from_str::<MissingPolicy>(r#""error""#);
    assert!(r.is_err());
}

#[test]
fn unknown_presence_rejected() {
    let r = serde_json::from_str::<Presence>(r#""unknown""#);
    assert!(r.is_err());
}

#[test]
fn unknown_policy_outcome_rejected() {
    let r = serde_json::from_str::<PolicyOutcome>(r#""pending""#);
    assert!(r.is_err());
}

#[test]
fn unknown_capability_status_rejected() {
    let r = serde_json::from_str::<CapabilityStatus>(r#""maybe""#);
    assert!(r.is_err());
}

#[test]
fn unknown_schema_validation_rejected() {
    let r = serde_json::from_str::<SchemaValidation>(r#""partial""#);
    assert!(r.is_err());
}

#[test]
fn unknown_safety_level_rejected() {
    let r = serde_json::from_str::<SafetyLevel>(r#""dangerous""#);
    assert!(r.is_err());
}

#[test]
fn unknown_hook_when_rejected() {
    let r = serde_json::from_str::<HookWhen>(r#""before_ingest""#);
    assert!(r.is_err());
}

#[test]
fn unknown_buildfix_apply_status_rejected() {
    let r = serde_json::from_str::<BuildfixApplyStatus>(r#""pending""#);
    assert!(r.is_err());
}

#[test]
fn unknown_trend_change_rejected() {
    let r = serde_json::from_str::<TrendChange>(r#""changed""#);
    assert!(r.is_err());
}

#[test]
fn unknown_policy_signature_algorithm_rejected() {
    let r = serde_json::from_str::<PolicySignatureAlgorithm>(r#""sha512""#);
    assert!(r.is_err());
}

// ---------------------------------------------------------------------------
// 5. Default values for optional fields
// ---------------------------------------------------------------------------

#[test]
fn default_verdict_counts_all_zero() {
    let d = VerdictCounts::default();
    assert_eq!(d.info, 0);
    assert_eq!(d.warn, 0);
    assert_eq!(d.error, 0);
    assert_eq!(d.suppressed, 0);
}

#[test]
fn default_missing_policy_is_skip() {
    assert_eq!(MissingPolicy::default(), MissingPolicy::Skip);
}

#[test]
fn default_schema_validation_is_lax() {
    assert_eq!(SchemaValidation::default(), SchemaValidation::Lax);
}

#[test]
fn default_sensor_policy() {
    let sp = SensorPolicy::default();
    assert!(!sp.blocking);
    assert_eq!(sp.missing, MissingPolicy::Skip);
    assert!(sp.section.is_none());
    assert!(sp.require_label.is_none());
    assert!(sp.repro.is_none());
}

#[test]
fn default_policy_values() {
    let p = Policy::default();
    assert!(!p.warn_is_fail);
    assert_eq!(p.max_highlights, 7);
    assert_eq!(p.max_per_sensor_findings, 20);
    assert_eq!(p.max_annotations, 25);
    assert_eq!(p.schema_validation, SchemaValidation::Lax);
    assert_eq!(p.max_receipt_size_bytes, 2 * 1024 * 1024);
    assert!(!p.section_order.is_empty());
}

#[test]
fn default_buildfix_policy() {
    let bp = BuildfixPolicy::default();
    assert!(!bp.auto_apply);
    assert_eq!(bp.max_auto_apply_safety, SafetyLevel::Safe);
    assert!(bp.require_matched_finding);
    assert!(bp.actuator.is_none());
}

#[test]
fn default_cockpit_config() {
    let cfg = CockpitConfig::default();
    assert!(cfg.sensors.is_empty());
    assert!(cfg.hooks.is_empty());
}

#[test]
fn default_hook_when_is_after_ingest() {
    assert_eq!(HookWhen::default(), HookWhen::AfterIngest);
}

#[test]
fn default_policy_signature_algorithm_is_hmac_sha256() {
    assert_eq!(
        PolicySignatureAlgorithm::default(),
        PolicySignatureAlgorithm::HmacSha256
    );
}

#[test]
fn default_buildfix_actuator_result() {
    let r = BuildfixActuatorResult::default();
    assert!(r.applied_fix_ids.is_empty());
    assert!(r.skipped_fix_ids.is_empty());
    assert!(r.errors.is_empty());
}

// ---------------------------------------------------------------------------
// 6. Ordering: severity_rank, verdict_status_rank, safety_level_rank,
//    FindingSortKey ordering
// ---------------------------------------------------------------------------

#[test]
fn severity_rank_ordering() {
    assert_eq!(severity_rank(&Severity::Error), 0);
    assert_eq!(severity_rank(&Severity::Warn), 1);
    assert_eq!(severity_rank(&Severity::Info), 2);
    assert!(severity_rank(&Severity::Error) < severity_rank(&Severity::Warn));
    assert!(severity_rank(&Severity::Warn) < severity_rank(&Severity::Info));
}

#[test]
fn verdict_status_rank_ordering() {
    assert_eq!(verdict_status_rank(&VerdictStatus::Fail), 0);
    assert_eq!(verdict_status_rank(&VerdictStatus::Warn), 1);
    assert_eq!(verdict_status_rank(&VerdictStatus::Pass), 2);
    assert_eq!(verdict_status_rank(&VerdictStatus::Skip), 3);
}

#[test]
fn safety_level_rank_ordering() {
    assert_eq!(safety_level_rank(&SafetyLevel::Safe), 0);
    assert_eq!(safety_level_rank(&SafetyLevel::Guarded), 1);
    assert_eq!(safety_level_rank(&SafetyLevel::Unsafe), 2);
}

#[test]
fn finding_sort_key_ordering() {
    let key_a = FindingSortKey {
        severity_rank: 0, // error
        sensor_id: "aaa".into(),
        path: "src/a.rs".into(),
        line: 10,
        code: "E001".into(),
        message: "msg a".into(),
    };
    let key_b = FindingSortKey {
        severity_rank: 1, // warn
        sensor_id: "aaa".into(),
        path: "src/a.rs".into(),
        line: 10,
        code: "E001".into(),
        message: "msg a".into(),
    };
    // Error (0) sorts before warn (1)
    assert!(key_a < key_b);

    // Same severity, different sensor_id
    let key_c = FindingSortKey {
        severity_rank: 0,
        sensor_id: "bbb".into(),
        path: "src/a.rs".into(),
        line: 10,
        code: "E001".into(),
        message: "msg a".into(),
    };
    assert!(key_a < key_c);

    // Same severity+sensor, different path
    let key_d = FindingSortKey {
        severity_rank: 0,
        sensor_id: "aaa".into(),
        path: "src/b.rs".into(),
        line: 1,
        code: "E001".into(),
        message: "msg a".into(),
    };
    assert!(key_a < key_d);

    // Same severity+sensor+path, different line
    let key_e = FindingSortKey {
        severity_rank: 0,
        sensor_id: "aaa".into(),
        path: "src/a.rs".into(),
        line: 20,
        code: "E001".into(),
        message: "msg a".into(),
    };
    assert!(key_a < key_e);

    // Same severity+sensor+path+line, different code
    let key_f = FindingSortKey {
        severity_rank: 0,
        sensor_id: "aaa".into(),
        path: "src/a.rs".into(),
        line: 10,
        code: "E002".into(),
        message: "msg a".into(),
    };
    assert!(key_a < key_f);

    // Same severity+sensor+path+line+code, different message
    let key_g = FindingSortKey {
        severity_rank: 0,
        sensor_id: "aaa".into(),
        path: "src/a.rs".into(),
        line: 10,
        code: "E001".into(),
        message: "msg b".into(),
    };
    assert!(key_a < key_g);
}

#[test]
fn finding_sort_key_vec_sort_stability() {
    let mut keys = [
        FindingSortKey {
            severity_rank: 2,
            sensor_id: "z".into(),
            path: "z.rs".into(),
            line: 99,
            code: "Z".into(),
            message: "z".into(),
        },
        FindingSortKey {
            severity_rank: 0,
            sensor_id: "a".into(),
            path: "a.rs".into(),
            line: 1,
            code: "A".into(),
            message: "a".into(),
        },
        FindingSortKey {
            severity_rank: 0,
            sensor_id: "a".into(),
            path: "a.rs".into(),
            line: 2,
            code: "A".into(),
            message: "a".into(),
        },
    ];
    keys.sort();
    assert_eq!(keys[0].severity_rank, 0);
    assert_eq!(keys[0].line, 1);
    assert_eq!(keys[1].severity_rank, 0);
    assert_eq!(keys[1].line, 2);
    assert_eq!(keys[2].severity_rank, 2);
}

// ---------------------------------------------------------------------------
// 7. Highlight sort stability
// ---------------------------------------------------------------------------

#[test]
fn highlight_sort_by_severity_then_sensor() {
    let h1 = Highlight {
        sensor_id: "zzz".into(),
        finding: Finding {
            severity: Severity::Error,
            check_id: None,
            code: "E1".into(),
            message: "err".into(),
            location: None,
            help: None,
            url: None,
            fingerprint: None,
            data: None,
        },
    };
    let h2 = Highlight {
        sensor_id: "aaa".into(),
        finding: Finding {
            severity: Severity::Info,
            check_id: None,
            code: "I1".into(),
            message: "info".into(),
            location: None,
            help: None,
            url: None,
            fingerprint: None,
            data: None,
        },
    };

    // Build sort keys and verify ordering
    let key1 = FindingSortKey {
        severity_rank: severity_rank(&h1.finding.severity),
        sensor_id: h1.sensor_id.clone(),
        path: String::new(),
        line: 0,
        code: h1.finding.code.clone(),
        message: h1.finding.message.clone(),
    };
    let key2 = FindingSortKey {
        severity_rank: severity_rank(&h2.finding.severity),
        sensor_id: h2.sensor_id.clone(),
        path: String::new(),
        line: 0,
        code: h2.finding.code.clone(),
        message: h2.finding.message.clone(),
    };
    // Error comes before info
    assert!(key1 < key2);
}

// ---------------------------------------------------------------------------
// 8. Embedded schema bytes are valid JSON with $schema field
// ---------------------------------------------------------------------------

#[test]
fn sensor_report_schema_is_valid_json_with_schema_field() {
    let v: Value = serde_json::from_str(SENSOR_REPORT_V1_SCHEMA_JSON)
        .expect("SENSOR_REPORT_V1_SCHEMA_JSON should be valid JSON");
    assert!(
        v.get("$schema").is_some(),
        "sensor schema should have $schema field"
    );
}

#[test]
fn cockpit_report_schema_is_valid_json_with_schema_field() {
    let v: Value = serde_json::from_str(COCKPIT_REPORT_V1_SCHEMA_JSON)
        .expect("COCKPIT_REPORT_V1_SCHEMA_JSON should be valid JSON");
    assert!(
        v.get("$schema").is_some(),
        "cockpit schema should have $schema field"
    );
}

#[test]
fn buildfix_plan_schema_is_valid_json_with_schema_field() {
    let v: Value = serde_json::from_str(BUILDFIX_PLAN_V1_SCHEMA_JSON)
        .expect("BUILDFIX_PLAN_V1_SCHEMA_JSON should be valid JSON");
    assert!(
        v.get("$schema").is_some(),
        "buildfix plan schema should have $schema field"
    );
}

#[test]
fn cockpit_promote_schema_is_valid_json_with_schema_field() {
    let v: Value = serde_json::from_str(COCKPIT_PROMOTE_V1_SCHEMA_JSON)
        .expect("COCKPIT_PROMOTE_V1_SCHEMA_JSON should be valid JSON");
    assert!(
        v.get("$schema").is_some(),
        "cockpit promote schema should have $schema field"
    );
}

// ---------------------------------------------------------------------------
// 9. Backwards-compatible deserialization (missing optional fields)
// ---------------------------------------------------------------------------

#[test]
fn verdict_counts_missing_suppressed_defaults_to_zero() {
    let json = r#"{"info":1,"warn":2,"error":3}"#;
    let counts: VerdictCounts = serde_json::from_str(json).unwrap();
    assert_eq!(counts.suppressed, 0);
}

#[test]
fn tool_info_missing_commit_defaults_to_none() {
    let json = r#"{"name":"t","version":"1.0"}"#;
    let ti: ToolInfo = serde_json::from_str(json).unwrap();
    assert!(ti.commit.is_none());
}

#[test]
fn run_info_missing_optional_fields() {
    let json = r#"{"started_at":"2026-01-01T00:00:00Z"}"#;
    let ri: RunInfo = serde_json::from_str(json).unwrap();
    assert!(ri.ended_at.is_none());
    assert!(ri.duration_ms.is_none());
    assert!(ri.host.is_none());
    assert!(ri.git.is_none());
    assert!(ri.ci.is_none());
    assert!(ri.capabilities.is_empty());
}

#[test]
fn finding_missing_optional_fields() {
    let json = r#"{"severity":"warn","code":"c","message":"m"}"#;
    let f: Finding = serde_json::from_str(json).unwrap();
    assert!(f.check_id.is_none());
    assert!(f.location.is_none());
    assert!(f.help.is_none());
    assert!(f.url.is_none());
    assert!(f.fingerprint.is_none());
    assert!(f.data.is_none());
}

#[test]
fn sensor_report_missing_optional_fields() {
    let json = r#"{
        "schema":"sensor.report.v1",
        "tool":{"name":"t","version":"1"},
        "run":{"started_at":"2026-01-01T00:00:00Z"},
        "verdict":{"status":"pass","counts":{"info":0,"warn":0,"error":0}}
    }"#;
    let sr: SensorReport = serde_json::from_str(json).unwrap();
    assert!(sr.findings.is_empty());
    assert!(sr.artifacts.is_empty());
    assert!(sr.data.is_none());
}

#[test]
fn verdict_missing_reasons_defaults_to_empty() {
    let json = r#"{"status":"pass","counts":{"info":0,"warn":0,"error":0}}"#;
    let v: Verdict = serde_json::from_str(json).unwrap();
    assert!(v.reasons.is_empty());
}

#[test]
fn sensor_summary_missing_optional_fields() {
    let json = r#"{
        "id":"s","blocking":false,"missing":"skip","presence":"present",
        "report_path":"p","verdict":{"status":"pass","counts":{"info":0,"warn":0,"error":0}}
    }"#;
    let ss: SensorSummary = serde_json::from_str(json).unwrap();
    assert!(ss.comment_path.is_none());
    assert!(!ss.truncated);
    assert!(ss.errors.is_empty());
    assert!(ss.missing_policy_applied.is_none());
    assert!(ss.policy_outcome.is_none());
}

#[test]
fn cockpit_config_from_empty_json() {
    let json = "{}";
    let cfg: CockpitConfig = serde_json::from_str(json).unwrap();
    assert!(cfg.sensors.is_empty());
    assert_eq!(cfg.policy.max_highlights, 7);
}

// ---------------------------------------------------------------------------
// 10. skip_serializing_if behavior
// ---------------------------------------------------------------------------

#[test]
fn tool_info_commit_none_omitted() {
    let ti = ToolInfo {
        name: "t".into(),
        version: "1".into(),
        commit: None,
    };
    let json = serde_json::to_string(&ti).unwrap();
    assert!(!json.contains("commit"), "commit=None should be omitted");
}

#[test]
fn verdict_counts_suppressed_zero_omitted() {
    let vc = VerdictCounts {
        info: 1,
        warn: 0,
        error: 0,
        suppressed: 0,
    };
    let json = serde_json::to_string(&vc).unwrap();
    assert!(
        !json.contains("suppressed"),
        "suppressed=0 should be omitted"
    );
}

#[test]
fn verdict_counts_suppressed_nonzero_present() {
    let vc = VerdictCounts {
        info: 0,
        warn: 0,
        error: 0,
        suppressed: 5,
    };
    let json = serde_json::to_string(&vc).unwrap();
    assert!(json.contains("suppressed"));
}

#[test]
fn sensor_report_empty_artifacts_omitted() {
    let sr = make_empty_sensor_report();
    let json = serde_json::to_string(&sr).unwrap();
    assert!(
        !json.contains("artifacts"),
        "empty artifacts vec should be omitted"
    );
}

#[test]
fn sensor_report_data_none_omitted() {
    let sr = make_empty_sensor_report();
    let json = serde_json::to_string(&sr).unwrap();
    assert!(!json.contains(r#""data""#), "data=None should be omitted");
}

#[test]
fn run_info_empty_capabilities_omitted() {
    let ri = make_minimal_run_info();
    let json = serde_json::to_string(&ri).unwrap();
    assert!(
        !json.contains("capabilities"),
        "empty capabilities should be omitted"
    );
}

#[test]
fn host_info_none_fields_omitted() {
    let hi = HostInfo {
        os: None,
        arch: None,
        hostname: None,
    };
    let json = serde_json::to_string(&hi).unwrap();
    assert_eq!(json, "{}");
}

#[test]
fn finding_optional_fields_omitted_when_none() {
    let f = make_minimal_finding();
    let json = serde_json::to_string(&f).unwrap();
    assert!(!json.contains("check_id"));
    assert!(!json.contains("location"));
    assert!(!json.contains("help"));
    assert!(!json.contains("url"));
    assert!(!json.contains("fingerprint"));
    assert!(!json.contains(r#""data""#));
}

#[test]
fn artifact_pointer_schema_none_omitted() {
    let ap = ArtifactPointer {
        id: "x".into(),
        path: "p".into(),
        mime: "m".into(),
        schema: None,
    };
    let json = serde_json::to_string(&ap).unwrap();
    assert!(!json.contains("schema"));
}

#[test]
fn buildfix_apply_summary_empty_vecs_omitted() {
    let bas = BuildfixApplySummary {
        status: BuildfixApplyStatus::Skipped,
        auto_apply_enabled: false,
        max_auto_apply_safety: SafetyLevel::Safe,
        require_matched_finding: true,
        candidate_fix_ids: vec![],
        selected_fix_ids: vec![],
        applied_fix_ids: vec![],
        skipped_fix_ids: vec![],
        errors: vec![],
        reason: None,
        actuator_command: None,
    };
    let json = serde_json::to_string(&bas).unwrap();
    assert!(!json.contains("candidate_fix_ids"));
    assert!(!json.contains("selected_fix_ids"));
    assert!(!json.contains("applied_fix_ids"));
    assert!(!json.contains("skipped_fix_ids"));
    assert!(!json.contains("errors"));
    assert!(!json.contains("reason"));
    assert!(!json.contains("actuator_command"));
}

#[test]
fn cockpit_config_empty_hooks_omitted() {
    let cfg = CockpitConfig::default();
    let json = serde_json::to_string(&cfg).unwrap();
    assert!(!json.contains("hooks"));
}

#[test]
fn cockpit_promote_hints_empty_vecs_omitted() {
    let hints = CockpitPromoteHints {
        schema: None,
        cards: vec![],
        suggested_highlights: vec![],
        suggested_artifacts: vec![],
    };
    let json = serde_json::to_string(&hints).unwrap();
    assert!(!json.contains("schema"));
    assert!(!json.contains("cards"));
    assert!(!json.contains("suggested_highlights"));
    assert!(!json.contains("suggested_artifacts"));
}

// ---------------------------------------------------------------------------
// 11. Data field with complex nested JSON roundtrips
// ---------------------------------------------------------------------------

#[test]
fn finding_data_complex_nested_json() {
    let complex = serde_json::json!({
        "nested": {
            "array": [1, 2, {"deep": true}],
            "null_val": null,
            "string": "hello",
            "number": 2.72,
            "bool": false
        }
    });
    let f = Finding {
        severity: Severity::Info,
        check_id: None,
        code: "test".into(),
        message: "test".into(),
        location: None,
        help: None,
        url: None,
        fingerprint: None,
        data: Some(complex.clone()),
    };
    let back = roundtrip_json(&f);
    assert_eq!(back.data.unwrap(), complex);
}

#[test]
fn sensor_report_data_complex_roundtrip() {
    let mut sr = make_empty_sensor_report();
    sr.data = Some(serde_json::json!({
        "_cockpit": {
            "cards": [{"id": "c1", "label": "L", "value": "V"}]
        },
        "tool_specific": [1, 2, 3]
    }));
    let back = roundtrip_json(&sr);
    assert!(back.data.is_some());
    assert_eq!(
        back.data.unwrap()["_cockpit"]["cards"][0]["id"],
        Value::String("c1".into())
    );
}

#[test]
fn cockpit_report_data_complex_roundtrip() {
    let cr = CockpitReport {
        schema: "cockpit.report.v1".into(),
        tool: ToolInfo {
            name: "cockpitctl".into(),
            version: "0.1.0".into(),
            commit: None,
        },
        run: make_minimal_run_info(),
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
        data: Some(serde_json::json!({
            "buildfix": {"status": "applied"},
            "trend": {"verdict_change": {"before": "pass", "after": "fail"}}
        })),
    };
    let back = roundtrip_json(&cr);
    assert_eq!(
        back.data.unwrap()["buildfix"]["status"],
        Value::String("applied".into())
    );
}

// ---------------------------------------------------------------------------
// 12. Constant values
// ---------------------------------------------------------------------------

#[test]
fn buildfix_apply_request_schema_id_value() {
    assert_eq!(
        BUILDFIX_APPLY_REQUEST_SCHEMA_ID,
        "buildfix.apply.request.v1"
    );
}

#[test]
fn policy_signature_schema_id_value() {
    assert_eq!(POLICY_SIGNATURE_SCHEMA_ID, "cockpit.policy_signature.v1");
}

// ---------------------------------------------------------------------------
// 13. Value → typed deserialization (from serde_json::Value)
// ---------------------------------------------------------------------------

#[test]
fn value_to_verdict_status() {
    let v = serde_json::json!("pass");
    let vs: VerdictStatus = serde_json::from_value(v).unwrap();
    assert_eq!(vs, VerdictStatus::Pass);
}

#[test]
fn value_to_severity() {
    let v = serde_json::json!("error");
    let sev: Severity = serde_json::from_value(v).unwrap();
    assert_eq!(sev, Severity::Error);
}

#[test]
fn value_to_verdict_counts() {
    let v = serde_json::json!({"info": 5, "warn": 3, "error": 1});
    let vc: VerdictCounts = serde_json::from_value(v).unwrap();
    assert_eq!(vc.info, 5);
    assert_eq!(vc.warn, 3);
    assert_eq!(vc.error, 1);
    assert_eq!(vc.suppressed, 0);
}

#[test]
fn value_to_sensor_report() {
    let v = serde_json::json!({
        "schema": "sensor.report.v1",
        "tool": {"name": "t", "version": "1"},
        "run": {"started_at": "2026-01-01T00:00:00Z"},
        "verdict": {"status": "pass", "counts": {"info": 0, "warn": 0, "error": 0}},
        "findings": [
            {"severity": "error", "code": "E1", "message": "oops"}
        ]
    });
    let sr: SensorReport = serde_json::from_value(v).unwrap();
    assert_eq!(sr.findings.len(), 1);
    assert_eq!(sr.findings[0].severity, Severity::Error);
}

#[test]
fn value_to_finding() {
    let v = serde_json::json!({
        "severity": "warn",
        "code": "W1",
        "message": "watch out",
        "data": {"key": "value"}
    });
    let f: Finding = serde_json::from_value(v).unwrap();
    assert_eq!(f.severity, Severity::Warn);
    assert_eq!(f.code, "W1");
    assert!(f.data.is_some());
    assert_eq!(f.data.unwrap()["key"], Value::String("value".into()));
}

#[test]
fn value_to_cockpit_config() {
    let v = serde_json::json!({
        "policy": {"warn_is_fail": true},
        "sensors": {
            "builddiag": {"blocking": true, "missing": "fail"}
        }
    });
    let cfg: CockpitConfig = serde_json::from_value(v).unwrap();
    assert!(cfg.policy.warn_is_fail);
    assert!(cfg.sensors.contains_key("builddiag"));
    assert!(cfg.sensors["builddiag"].blocking);
}

#[test]
fn value_to_buildfix_plan() {
    let v = serde_json::json!({
        "schema": "buildfix.plan.v1",
        "tool": {"name": "fixbot", "version": "1"},
        "fixes": [{
            "id": "f1",
            "safety": "safe",
            "description": "fix it",
            "finding_refs": [{"sensor_id": "s1"}]
        }]
    });
    let bp: BuildfixPlan = serde_json::from_value(v).unwrap();
    assert_eq!(bp.fixes.len(), 1);
    assert_eq!(bp.fixes[0].safety, SafetyLevel::Safe);
}

#[test]
fn value_to_trend_delta() {
    let v = serde_json::json!({
        "verdict_change": null,
        "count_deltas": {"info_delta": 0, "warn_delta": 0, "error_delta": 0},
        "new_findings": [],
        "fixed_findings": [],
        "sensors_added": [],
        "sensors_removed": []
    });
    let td: TrendDelta = serde_json::from_value(v).unwrap();
    assert!(td.verdict_change.is_none());
    assert!(td.new_findings.is_empty());
}

#[test]
fn value_to_policy_signature_evidence() {
    let v = serde_json::json!({
        "schema": "cockpit.policy_signature.v1",
        "algorithm": "hmac_sha256",
        "policy_sha256": "abc",
        "signature": "sig"
    });
    let pse: PolicySignatureEvidence = serde_json::from_value(v).unwrap();
    assert_eq!(pse.algorithm, PolicySignatureAlgorithm::HmacSha256);
    assert!(pse.key_id.is_none());
}
