//! Expanded property-based serde roundtrip tests for cockpitctl-types.
//!
//! Covers DTOs not already tested in `proptest_serde.rs`: compound structs,
//! buildfix types, trend types, policy signing, hooks, and promotion hints.

use cockpitctl_types::*;
use proptest::prelude::*;
use std::collections::BTreeMap;

// ============================================================================
// Reusable strategies
// ============================================================================

fn any_severity() -> impl Strategy<Value = Severity> {
    prop_oneof![
        Just(Severity::Info),
        Just(Severity::Warn),
        Just(Severity::Error),
    ]
}

fn any_verdict_status() -> impl Strategy<Value = VerdictStatus> {
    prop_oneof![
        Just(VerdictStatus::Pass),
        Just(VerdictStatus::Warn),
        Just(VerdictStatus::Fail),
        Just(VerdictStatus::Skip),
    ]
}

fn any_missing_policy() -> impl Strategy<Value = MissingPolicy> {
    prop_oneof![
        Just(MissingPolicy::Skip),
        Just(MissingPolicy::Warn),
        Just(MissingPolicy::Fail),
    ]
}

fn any_presence() -> impl Strategy<Value = Presence> {
    prop_oneof![
        Just(Presence::Present),
        Just(Presence::Missing),
        Just(Presence::Invalid),
    ]
}

fn any_policy_outcome() -> impl Strategy<Value = PolicyOutcome> {
    prop_oneof![
        Just(PolicyOutcome::Blocked),
        Just(PolicyOutcome::Allowed),
        Just(PolicyOutcome::Informational),
    ]
}

fn any_safety_level() -> impl Strategy<Value = SafetyLevel> {
    prop_oneof![
        Just(SafetyLevel::Safe),
        Just(SafetyLevel::Guarded),
        Just(SafetyLevel::Unsafe),
    ]
}

fn any_capability_status() -> impl Strategy<Value = CapabilityStatus> {
    prop_oneof![
        Just(CapabilityStatus::Available),
        Just(CapabilityStatus::Unavailable),
        Just(CapabilityStatus::Skipped),
    ]
}

fn any_hook_when() -> impl Strategy<Value = HookWhen> {
    Just(HookWhen::AfterIngest)
}

fn any_buildfix_apply_status() -> impl Strategy<Value = BuildfixApplyStatus> {
    prop_oneof![
        Just(BuildfixApplyStatus::Skipped),
        Just(BuildfixApplyStatus::Applied),
        Just(BuildfixApplyStatus::Failed),
    ]
}

fn any_trend_change() -> impl Strategy<Value = TrendChange> {
    prop_oneof![Just(TrendChange::New), Just(TrendChange::Fixed),]
}

fn any_policy_signature_algorithm() -> impl Strategy<Value = PolicySignatureAlgorithm> {
    Just(PolicySignatureAlgorithm::HmacSha256)
}

fn any_verdict_counts() -> impl Strategy<Value = VerdictCounts> {
    (0u64..100, 0u64..100, 0u64..100, 0u64..10).prop_map(|(info, warn, error, suppressed)| {
        VerdictCounts {
            info,
            warn,
            error,
            suppressed,
        }
    })
}

fn any_verdict() -> impl Strategy<Value = Verdict> {
    (
        any_verdict_status(),
        any_verdict_counts(),
        prop::collection::vec("[a-z_]{1,10}", 0..3),
    )
        .prop_map(|(status, counts, reasons)| Verdict {
            status,
            counts,
            reasons,
        })
}

fn any_tool_info() -> impl Strategy<Value = ToolInfo> {
    (
        "[a-z][a-z0-9-]{0,10}",
        "[0-9]{1,2}\\.[0-9]{1,2}\\.[0-9]{1,2}",
        prop::option::of("[a-f0-9]{7}"),
    )
        .prop_map(|(name, version, commit)| ToolInfo {
            name,
            version,
            commit,
        })
}

fn any_location() -> impl Strategy<Value = Location> {
    (
        prop::option::of("[a-z/_.-]{1,20}"),
        prop::option::of(1u32..10000),
        prop::option::of(1u32..500),
    )
        .prop_map(|(path, line, col)| Location { path, line, col })
}

fn any_finding() -> impl Strategy<Value = Finding> {
    (
        any_severity(),
        prop::option::of("[A-Z][A-Z0-9_]{0,8}"),
        "[A-Z][A-Z0-9_]{0,10}",
        ".{1,30}",
        prop::option::of(any_location()),
        prop::option::of(".{0,20}"),
        prop::option::of("https://example\\.com"),
        prop::option::of("[a-f0-9]{16}"),
    )
        .prop_map(
            |(severity, check_id, code, message, location, help, url, fingerprint)| Finding {
                severity,
                check_id,
                code,
                message,
                location,
                help,
                url,
                fingerprint,
                data: None,
            },
        )
}

// ============================================================================
// Compound struct strategies
// ============================================================================

fn any_host_info() -> impl Strategy<Value = HostInfo> {
    (
        prop::option::of("[a-z]{3,8}"),
        prop::option::of("[a-z0-9_]{3,8}"),
        prop::option::of("[a-z0-9-]{3,12}"),
    )
        .prop_map(|(os, arch, hostname)| HostInfo { os, arch, hostname })
}

fn any_git_info() -> impl Strategy<Value = GitInfo> {
    (
        prop::option::of("[a-z/]{3,15}"),
        prop::option::of("[a-z/]{3,15}"),
        prop::option::of("[a-z/]{3,15}"),
        prop::option::of("[a-f0-9]{7}"),
        prop::option::of("[a-f0-9]{7}"),
        prop::option::of("[a-f0-9]{7}"),
    )
        .prop_map(
            |(repo, base_ref, head_ref, base_sha, head_sha, merge_base)| GitInfo {
                repo,
                base_ref,
                head_ref,
                base_sha,
                head_sha,
                merge_base,
            },
        )
}

fn any_ci_info() -> impl Strategy<Value = CiInfo> {
    (
        prop::option::of("[a-z]{3,10}"),
        prop::option::of("[0-9]{4,10}"),
        prop::option::of("https://ci\\.example\\.com/[0-9]{1,5}"),
        prop::option::of("[a-z-]{3,10}"),
    )
        .prop_map(|(provider, run_id, run_url, job)| CiInfo {
            provider,
            run_id,
            run_url,
            job,
        })
}

fn any_capability() -> impl Strategy<Value = Capability> {
    (any_capability_status(), prop::option::of("[a-z ]{3,15}"))
        .prop_map(|(status, reason)| Capability { status, reason })
}

fn any_run_info() -> impl Strategy<Value = RunInfo> {
    (
        "2024-0[1-9]-[012][1-9]T[01][0-9]:[0-5][0-9]:[0-5][0-9]Z",
        prop::option::of("2024-0[1-9]-[012][1-9]T[01][0-9]:[0-5][0-9]:[0-5][0-9]Z"),
        prop::option::of(0u64..600_000),
        prop::option::of(any_host_info()),
        prop::option::of(any_git_info()),
        prop::option::of(any_ci_info()),
    )
        .prop_map(
            |(started_at, ended_at, duration_ms, host, git, ci)| RunInfo {
                started_at,
                ended_at,
                duration_ms,
                host,
                git,
                ci,
                capabilities: BTreeMap::new(),
            },
        )
}

fn any_run_info_with_caps() -> impl Strategy<Value = RunInfo> {
    (
        any_run_info(),
        prop::collection::btree_map("[a-z]{3,8}", any_capability(), 0..3),
    )
        .prop_map(|(mut run, caps)| {
            run.capabilities = caps;
            run
        })
}

fn any_sensor_policy() -> impl Strategy<Value = SensorPolicy> {
    (
        any::<bool>(),
        any_missing_policy(),
        prop::option::of("[A-Z][a-z]{2,10}"),
        prop::option::of("[a-z-]{3,10}"),
        prop::option::of("[a-z ]{3,15}"),
    )
        .prop_map(
            |(blocking, missing, section, require_label, repro)| SensorPolicy {
                blocking,
                missing,
                section,
                require_label,
                repro,
            },
        )
}

fn any_highlight() -> impl Strategy<Value = Highlight> {
    ("[a-z_][a-z0-9_]{0,8}", any_finding())
        .prop_map(|(sensor_id, finding)| Highlight { sensor_id, finding })
}

fn any_sensor_summary() -> impl Strategy<Value = SensorSummary> {
    (
        "[a-z_][a-z0-9_]{0,8}",
        any::<bool>(),
        any_missing_policy(),
        any_presence(),
        any_verdict(),
        any::<bool>(),
        prop::option::of(any_missing_policy()),
        prop::option::of(any_policy_outcome()),
    )
        .prop_map(
            |(id, blocking, missing, presence, verdict, truncated, missing_applied, outcome)| {
                SensorSummary {
                    id: id.clone(),
                    blocking,
                    missing,
                    presence,
                    report_path: format!("artifacts/{id}/report.json"),
                    comment_path: None,
                    verdict,
                    truncated,
                    errors: vec![],
                    missing_policy_applied: missing_applied,
                    policy_outcome: outcome,
                }
            },
        )
}

fn any_policy_sensor_snapshot() -> impl Strategy<Value = PolicySensorSnapshot> {
    (
        "[a-z_][a-z0-9_]{0,8}",
        any::<bool>(),
        any_missing_policy(),
        prop::option::of("[A-Z][a-z]{2,8}"),
        prop::option::of("[a-z-]{3,8}"),
        prop::option::of("[a-z ]{3,10}"),
    )
        .prop_map(|(id, blocking, missing, section, require_label, repro)| {
            PolicySensorSnapshot {
                id,
                blocking,
                missing,
                section,
                require_label,
                repro,
            }
        })
}

fn any_policy_snapshot() -> impl Strategy<Value = PolicySnapshot> {
    (
        any::<bool>(),
        1usize..20,
        1usize..50,
        1usize..50,
        prop::collection::vec("[A-Z][a-z]{2,10}", 0..4),
        prop::collection::vec(any_policy_sensor_snapshot(), 0..4),
    )
        .prop_map(
            |(warn_is_fail, max_hl, max_psf, max_ann, section_order, sensors)| PolicySnapshot {
                warn_is_fail,
                max_highlights: max_hl,
                max_per_sensor_findings: max_psf,
                max_annotations: max_ann,
                section_order,
                sensors,
            },
        )
}

fn any_artifact_pointer() -> impl Strategy<Value = ArtifactPointer> {
    (
        "[a-z][a-z0-9_]{0,8}",
        "[a-z/._]{1,15}",
        Just("application/json".to_string()),
        prop::option::of("[a-z._]{1,8}"),
    )
        .prop_map(|(id, path, mime, schema)| ArtifactPointer {
            id,
            path,
            mime,
            schema,
        })
}

fn any_finding_ref() -> impl Strategy<Value = FindingRef> {
    (
        "[a-z_][a-z0-9_]{0,8}",
        prop::option::of("[a-f0-9]{16}"),
        prop::option::of("[A-Z][A-Z0-9_]{0,8}"),
        prop::option::of("[a-z]{3,8}"),
        prop::option::of("[A-Z][A-Z0-9_]{0,6}"),
    )
        .prop_map(
            |(sensor_id, fingerprint, code, tool, check_id)| FindingRef {
                sensor_id,
                fingerprint,
                code,
                tool,
                check_id,
            },
        )
}

fn any_preconditions() -> impl Strategy<Value = Preconditions> {
    ("[a-f0-9]{40}", prop::collection::vec("[a-f0-9]{64}", 0..3)).prop_map(
        |(repo_head, receipt_digests)| Preconditions {
            repo_head,
            receipt_digests,
        },
    )
}

fn any_fix() -> impl Strategy<Value = Fix> {
    (
        "[a-z][a-z0-9-]{0,10}",
        any_safety_level(),
        ".{5,30}",
        prop::collection::vec(any_finding_ref(), 0..3),
        prop::option::of(any_preconditions()),
    )
        .prop_map(
            |(id, safety, description, finding_refs, preconditions)| Fix {
                id,
                safety,
                description,
                finding_refs,
                preconditions,
                data: None,
            },
        )
}

fn any_matched_finding() -> impl Strategy<Value = MatchedFinding> {
    (
        "[a-z_][a-z0-9_]{0,8}",
        "[A-Z][A-Z0-9_]{0,8}",
        prop::option::of("[a-f0-9]{16}"),
    )
        .prop_map(|(sensor_id, code, fingerprint)| MatchedFinding {
            sensor_id,
            code,
            fingerprint,
        })
}

fn any_fix_summary() -> impl Strategy<Value = FixSummary> {
    (
        "[a-z][a-z0-9-]{0,8}",
        "[a-z_][a-z0-9_]{0,8}",
        any_safety_level(),
        ".{5,20}",
        prop::collection::vec(any_matched_finding(), 0..3),
        any::<bool>(),
    )
        .prop_map(
            |(fix_id, sensor_id, safety, description, matched_findings, unmatched)| FixSummary {
                fix_id,
                sensor_id,
                safety,
                description,
                matched_findings,
                unmatched,
            },
        )
}

fn any_verdict_change() -> impl Strategy<Value = VerdictChange> {
    (any_verdict_status(), any_verdict_status())
        .prop_map(|(before, after)| VerdictChange { before, after })
}

fn any_count_deltas() -> impl Strategy<Value = CountDeltas> {
    (-50i64..50, -50i64..50, -50i64..50).prop_map(|(info_delta, warn_delta, error_delta)| {
        CountDeltas {
            info_delta,
            warn_delta,
            error_delta,
        }
    })
}

fn any_trend_finding() -> impl Strategy<Value = TrendFinding> {
    (
        "[a-z_][a-z0-9_]{0,8}",
        "[A-Z][A-Z0-9_]{0,8}",
        ".{1,20}",
        prop::option::of("[a-z/_.-]{1,15}"),
        prop::option::of(1u32..10000),
        prop::option::of("[a-f0-9]{16}"),
        any_severity(),
    )
        .prop_map(
            |(sensor_id, code, message, path, line, fingerprint, severity)| TrendFinding {
                sensor_id,
                code,
                message,
                path,
                line,
                fingerprint,
                severity,
            },
        )
}

fn any_hook_config() -> impl Strategy<Value = HookConfig> {
    (
        "[a-z][a-z0-9-]{0,10}",
        "[a-z/. ]{3,15}",
        any_hook_when(),
        1000u64..120_000,
    )
        .prop_map(|(name, command, when, timeout_ms)| HookConfig {
            name,
            command,
            when,
            timeout_ms,
        })
}

fn any_promote_card() -> impl Strategy<Value = PromoteCard> {
    (
        "[a-z][a-z0-9-]{0,8}",
        "[A-Z][a-z ]{2,12}",
        "[0-9]{1,5}",
        prop::option::of(any_severity()),
    )
        .prop_map(|(id, label, value, severity)| PromoteCard {
            id,
            label,
            value,
            severity,
        })
}

fn any_suggested_highlight() -> impl Strategy<Value = SuggestedHighlight> {
    "[a-f0-9]{16}".prop_map(|fp| SuggestedHighlight {
        finding_fingerprint: fp,
    })
}

fn any_suggested_artifact() -> impl Strategy<Value = SuggestedArtifact> {
    "[a-z][a-z0-9_]{0,8}".prop_map(|id| SuggestedArtifact { artifact_id: id })
}

fn any_policy_signing_config() -> impl Strategy<Value = PolicySigningConfig> {
    (
        any::<bool>(),
        any_policy_signature_algorithm(),
        prop::option::of("[a-z/._]{3,15}"),
        prop::option::of("[A-Z_]{3,12}"),
        prop::option::of("[a-z0-9-]{3,10}"),
    )
        .prop_map(
            |(enabled, algorithm, key_path, key_env, key_id)| PolicySigningConfig {
                enabled,
                algorithm,
                key_path,
                key_env,
                key_id,
            },
        )
}

// ============================================================================
// Enum serde roundtrips (types not in proptest_serde.rs)
// ============================================================================

proptest! {
    #[test]
    fn safety_level_roundtrip(s in any_safety_level()) {
        let json = serde_json::to_string(&s).unwrap();
        let parsed: SafetyLevel = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(s, parsed);
    }

    #[test]
    fn capability_status_roundtrip(s in any_capability_status()) {
        let json = serde_json::to_string(&s).unwrap();
        let parsed: CapabilityStatus = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(s, parsed);
    }

    #[test]
    fn hook_when_roundtrip(w in any_hook_when()) {
        let json = serde_json::to_string(&w).unwrap();
        let parsed: HookWhen = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(w, parsed);
    }

    #[test]
    fn buildfix_apply_status_roundtrip(s in any_buildfix_apply_status()) {
        let json = serde_json::to_string(&s).unwrap();
        let parsed: BuildfixApplyStatus = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(s, parsed);
    }

    #[test]
    fn trend_change_roundtrip(c in any_trend_change()) {
        let json = serde_json::to_string(&c).unwrap();
        let parsed: TrendChange = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(c, parsed);
    }

    #[test]
    fn policy_signature_algorithm_roundtrip(a in any_policy_signature_algorithm()) {
        let json = serde_json::to_string(&a).unwrap();
        let parsed: PolicySignatureAlgorithm = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(a, parsed);
    }
}

// ============================================================================
// Struct serde roundtrips — context / environment types
// ============================================================================

proptest! {
    #[test]
    fn host_info_roundtrip(h in any_host_info()) {
        let json = serde_json::to_string(&h).unwrap();
        let parsed: HostInfo = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(h, parsed);
    }

    #[test]
    fn git_info_roundtrip(g in any_git_info()) {
        let json = serde_json::to_string(&g).unwrap();
        let parsed: GitInfo = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(g, parsed);
    }

    #[test]
    fn ci_info_roundtrip(c in any_ci_info()) {
        let json = serde_json::to_string(&c).unwrap();
        let parsed: CiInfo = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(c, parsed);
    }

    #[test]
    fn capability_roundtrip(c in any_capability()) {
        let json = serde_json::to_string(&c).unwrap();
        let parsed: Capability = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(c, parsed);
    }

    #[test]
    fn run_info_roundtrip(r in any_run_info_with_caps()) {
        let json = serde_json::to_string(&r).unwrap();
        let parsed: RunInfo = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(r, parsed);
    }
}

// ============================================================================
// Struct serde roundtrips — policy / config types
// ============================================================================

proptest! {
    #[test]
    fn sensor_policy_roundtrip(p in any_sensor_policy()) {
        let json = serde_json::to_string(&p).unwrap();
        let parsed: SensorPolicy = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(p, parsed);
    }

    #[test]
    fn policy_sensor_snapshot_roundtrip(p in any_policy_sensor_snapshot()) {
        let json = serde_json::to_string(&p).unwrap();
        let parsed: PolicySensorSnapshot = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(p, parsed);
    }

    #[test]
    fn policy_snapshot_roundtrip(p in any_policy_snapshot()) {
        let json = serde_json::to_string(&p).unwrap();
        let parsed: PolicySnapshot = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(p, parsed);
    }

    #[test]
    fn policy_signing_config_roundtrip(c in any_policy_signing_config()) {
        let json = serde_json::to_string(&c).unwrap();
        let parsed: PolicySigningConfig = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(c, parsed);
    }

    #[test]
    fn hook_config_roundtrip(h in any_hook_config()) {
        let json = serde_json::to_string(&h).unwrap();
        let parsed: HookConfig = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(h, parsed);
    }
}

// ============================================================================
// Struct serde roundtrips — sensor summary / highlight / report
// ============================================================================

proptest! {
    #[test]
    fn sensor_summary_roundtrip(s in any_sensor_summary()) {
        let json = serde_json::to_string(&s).unwrap();
        let parsed: SensorSummary = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(s, parsed);
    }

    #[test]
    fn cockpit_report_roundtrip(
        tool in any_tool_info(),
        run in any_run_info(),
        verdict in any_verdict(),
        sensors in prop::collection::vec(any_sensor_summary(), 0..4),
        highlights in prop::collection::vec(any_highlight(), 0..3),
        policy in any_policy_snapshot(),
    ) {
        let report = CockpitReport {
            schema: "cockpit.report.v1".to_string(),
            tool,
            run,
            verdict,
            sensors,
            highlights,
            policy,
            data: None,
        };
        let json = serde_json::to_string(&report).unwrap();
        let parsed: CockpitReport = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(report, parsed);
    }

    #[test]
    fn sensor_report_roundtrip(
        tool in any_tool_info(),
        run in any_run_info(),
        verdict in any_verdict(),
        findings in prop::collection::vec(any_finding(), 0..4),
        artifacts in prop::collection::vec(any_artifact_pointer(), 0..2),
    ) {
        let report = SensorReport {
            schema: "sensor.report.v1".to_string(),
            tool,
            run,
            verdict,
            findings,
            artifacts,
            data: None,
        };
        let json = serde_json::to_string(&report).unwrap();
        let parsed: SensorReport = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(report, parsed);
    }
}

// ============================================================================
// Struct serde roundtrips — buildfix types
// ============================================================================

proptest! {
    #[test]
    fn finding_ref_roundtrip(r in any_finding_ref()) {
        let json = serde_json::to_string(&r).unwrap();
        let parsed: FindingRef = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(r, parsed);
    }

    #[test]
    fn preconditions_roundtrip(p in any_preconditions()) {
        let json = serde_json::to_string(&p).unwrap();
        let parsed: Preconditions = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(p, parsed);
    }

    #[test]
    fn fix_roundtrip(f in any_fix()) {
        let json = serde_json::to_string(&f).unwrap();
        let parsed: Fix = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(f, parsed);
    }

    #[test]
    fn buildfix_plan_roundtrip(
        tool in any_tool_info(),
        fixes in prop::collection::vec(any_fix(), 0..4),
    ) {
        let plan = BuildfixPlan {
            schema: "buildfix.plan.v1".to_string(),
            tool,
            fixes,
        };
        let json = serde_json::to_string(&plan).unwrap();
        let parsed: BuildfixPlan = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(plan, parsed);
    }

    #[test]
    fn matched_finding_roundtrip(m in any_matched_finding()) {
        let json = serde_json::to_string(&m).unwrap();
        let parsed: MatchedFinding = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(m, parsed);
    }

    #[test]
    fn fix_summary_roundtrip(f in any_fix_summary()) {
        let json = serde_json::to_string(&f).unwrap();
        let parsed: FixSummary = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(f, parsed);
    }

    #[test]
    fn buildfix_summary_roundtrip(
        fixes in prop::collection::vec(any_fix_summary(), 0..4),
    ) {
        let summary = BuildfixSummary {
            total_fixes: fixes.len(),
            matched_count: fixes.iter().filter(|f| !f.unmatched).count(),
            unmatched_count: fixes.iter().filter(|f| f.unmatched).count(),
            fixes,
        };
        let json = serde_json::to_string(&summary).unwrap();
        let parsed: BuildfixSummary = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(summary, parsed);
    }

    #[test]
    fn buildfix_apply_request_roundtrip(
        safety in any_safety_level(),
        require_match in any::<bool>(),
        fixes in prop::collection::vec(any_fix_summary(), 0..3),
    ) {
        let req = BuildfixApplyRequest {
            schema: BUILDFIX_APPLY_REQUEST_SCHEMA_ID.to_string(),
            max_auto_apply_safety: safety,
            require_matched_finding: require_match,
            fixes,
        };
        let json = serde_json::to_string(&req).unwrap();
        let parsed: BuildfixApplyRequest = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(req, parsed);
    }

    #[test]
    fn buildfix_actuator_result_roundtrip(
        applied in prop::collection::vec("[a-z0-9-]{3,8}", 0..3),
        skipped in prop::collection::vec("[a-z0-9-]{3,8}", 0..3),
        errors in prop::collection::vec(".{5,20}", 0..2),
    ) {
        let result = BuildfixActuatorResult { applied_fix_ids: applied, skipped_fix_ids: skipped, errors };
        let json = serde_json::to_string(&result).unwrap();
        let parsed: BuildfixActuatorResult = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(result, parsed);
    }

    #[test]
    fn buildfix_apply_summary_roundtrip(
        status in any_buildfix_apply_status(),
        auto_apply in any::<bool>(),
        safety in any_safety_level(),
        require_match in any::<bool>(),
    ) {
        let summary = BuildfixApplySummary {
            status,
            auto_apply_enabled: auto_apply,
            max_auto_apply_safety: safety,
            require_matched_finding: require_match,
            candidate_fix_ids: vec!["fix-a".into()],
            selected_fix_ids: vec!["fix-a".into()],
            applied_fix_ids: vec![],
            skipped_fix_ids: vec![],
            errors: vec![],
            reason: None,
            actuator_command: None,
        };
        let json = serde_json::to_string(&summary).unwrap();
        let parsed: BuildfixApplySummary = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(summary, parsed);
    }
}

// ============================================================================
// Struct serde roundtrips — trend types
// ============================================================================

proptest! {
    #[test]
    fn verdict_change_roundtrip(v in any_verdict_change()) {
        let json = serde_json::to_string(&v).unwrap();
        let parsed: VerdictChange = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(v, parsed);
    }

    #[test]
    fn count_deltas_roundtrip(d in any_count_deltas()) {
        let json = serde_json::to_string(&d).unwrap();
        let parsed: CountDeltas = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(d, parsed);
    }

    #[test]
    fn trend_finding_roundtrip(f in any_trend_finding()) {
        let json = serde_json::to_string(&f).unwrap();
        let parsed: TrendFinding = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(f, parsed);
    }

    #[test]
    fn trend_delta_roundtrip(
        verdict_change in prop::option::of(any_verdict_change()),
        count_deltas in any_count_deltas(),
        new_findings in prop::collection::vec(any_trend_finding(), 0..3),
        fixed_findings in prop::collection::vec(any_trend_finding(), 0..3),
    ) {
        let delta = TrendDelta {
            verdict_change,
            count_deltas,
            new_findings,
            fixed_findings,
            sensors_added: vec!["new-sensor".into()],
            sensors_removed: vec![],
        };
        let json = serde_json::to_string(&delta).unwrap();
        let parsed: TrendDelta = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(delta, parsed);
    }
}

// ============================================================================
// Struct serde roundtrips — promotion hints
// ============================================================================

proptest! {
    #[test]
    fn promote_card_roundtrip(c in any_promote_card()) {
        let json = serde_json::to_string(&c).unwrap();
        let parsed: PromoteCard = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(c, parsed);
    }

    #[test]
    fn suggested_highlight_roundtrip(h in any_suggested_highlight()) {
        let json = serde_json::to_string(&h).unwrap();
        let parsed: SuggestedHighlight = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(h, parsed);
    }

    #[test]
    fn suggested_artifact_roundtrip(a in any_suggested_artifact()) {
        let json = serde_json::to_string(&a).unwrap();
        let parsed: SuggestedArtifact = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(a, parsed);
    }

    #[test]
    fn cockpit_promote_hints_roundtrip(
        cards in prop::collection::vec(any_promote_card(), 0..3),
        highlights in prop::collection::vec(any_suggested_highlight(), 0..3),
        artifacts in prop::collection::vec(any_suggested_artifact(), 0..2),
    ) {
        let hints = CockpitPromoteHints {
            schema: Some("cockpit.promote.v1".to_string()),
            cards,
            suggested_highlights: highlights,
            suggested_artifacts: artifacts,
        };
        let json = serde_json::to_string(&hints).unwrap();
        let parsed: CockpitPromoteHints = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(hints, parsed);
    }
}

// ============================================================================
// Struct serde roundtrips — policy signing evidence
// ============================================================================

proptest! {
    #[test]
    fn policy_signature_evidence_roundtrip(
        alg in any_policy_signature_algorithm(),
        key_id in prop::option::of("[a-z0-9-]{3,10}"),
    ) {
        let evidence = PolicySignatureEvidence {
            schema: POLICY_SIGNATURE_SCHEMA_ID.to_string(),
            algorithm: alg,
            policy_sha256: "abcd1234".repeat(8),
            signature: "beef5678".repeat(8),
            key_id,
        };
        let json = serde_json::to_string(&evidence).unwrap();
        let parsed: PolicySignatureEvidence = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(evidence, parsed);
    }
}
