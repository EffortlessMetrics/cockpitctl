//! Compile-time regression tests for cockpitctl-core facade reexports.
//!
//! If a reexport is removed or renamed, the corresponding test will fail to
//! compile. This guards against accidental API breakage in the facade crate.

// ============================================================================
// Flattened type reexports
// ============================================================================

#[test]
fn flattened_types_accessible() {
    // Verify every flattened type is importable through cockpitctl_core::.
    let _ = std::any::type_name::<cockpitctl_core::VerdictStatus>();
    let _ = std::any::type_name::<cockpitctl_core::CockpitReport>();
    let _ = std::any::type_name::<cockpitctl_core::SensorReport>();
    let _ = std::any::type_name::<cockpitctl_core::CockpitConfig>();
    let _ = std::any::type_name::<cockpitctl_core::ToolInfo>();
    let _ = std::any::type_name::<cockpitctl_core::Presence>();
    let _ = std::any::type_name::<cockpitctl_core::PolicyOutcome>();
    let _ = std::any::type_name::<cockpitctl_core::ArtifactPointer>();
    let _ = std::any::type_name::<cockpitctl_core::BuildfixPolicy>();
    let _ = std::any::type_name::<cockpitctl_core::BuildfixApplySummary>();
    let _ = std::any::type_name::<cockpitctl_core::PolicySigningConfig>();
    let _ = std::any::type_name::<cockpitctl_core::PolicySignatureEvidence>();
}

// ============================================================================
// Flattened ingest reexports
// ============================================================================

#[test]
fn flattened_ingest_accessible() {
    let _ = std::any::type_name::<cockpitctl_core::IngestRequest>();
    let _ = std::any::type_name::<cockpitctl_core::IngestResult>();
    let _ = std::any::type_name::<cockpitctl_core::NoOpSchemaValidator>();
}

// ============================================================================
// Flattened domain reexports
// ============================================================================

#[test]
fn flattened_domain_functions_accessible() {
    // CodeExplanation struct
    let _ = std::any::type_name::<cockpitctl_core::CodeExplanation>();

    // all_codes returns a Vec of code explanations
    let codes = cockpitctl_core::all_codes();
    assert!(
        !codes.is_empty(),
        "all_codes should return at least one code"
    );

    // explain_code returns an Option
    let _ = cockpitctl_core::explain_code("cockpit.pass");
}

#[test]
fn flattened_domain_signing_accessible() {
    // Verify the functions are importable by taking references.
    let _f1: fn(
        &cockpitctl_core::types::PolicySnapshot,
        cockpitctl_core::types::PolicySignatureAlgorithm,
        &[u8],
        Option<String>,
    ) -> anyhow::Result<cockpitctl_core::PolicySignatureEvidence> =
        cockpitctl_core::sign_policy_snapshot;
    let _f2: fn(
        &cockpitctl_core::types::PolicySnapshot,
        &[u8],
        Option<String>,
    ) -> anyhow::Result<cockpitctl_core::PolicySignatureEvidence> =
        cockpitctl_core::sign_policy_snapshot_hmac_sha256;
    let _f3: fn(&cockpitctl_core::types::PolicySnapshot) -> anyhow::Result<String> =
        cockpitctl_core::policy_snapshot_sha256_hex;
}

#[test]
fn flattened_buildfix_domain_accessible() {
    // match_buildfix_plan takes (sensor_id, plan, highlights)
    let _: fn(
        &str,
        &cockpitctl_core::types::BuildfixPlan,
        &[cockpitctl_core::types::Highlight],
    ) -> cockpitctl_core::types::BuildfixSummary = cockpitctl_core::match_buildfix_plan;
    // select_auto_apply_fixes exists
    let _ = &cockpitctl_core::select_auto_apply_fixes;
}

#[test]
fn flattened_trend_accessible() {
    let _ = &cockpitctl_core::compute_trend;
}

// ============================================================================
// Flattened render reexports
// ============================================================================

#[test]
fn flattened_render_accessible() {
    let _ = std::any::type_name::<cockpitctl_core::GitHubAnnotationResult>();
    let _: fn(&cockpitctl_core::CockpitReport, &cockpitctl_core::CockpitConfig) -> String =
        cockpitctl_core::render_comment;
    let _ = &cockpitctl_core::render_github_annotations;
    let _: fn(&str, &[(String, String)]) -> String = cockpitctl_core::append_comment_sections;
}

// ============================================================================
// Flattened SARIF reexports
// ============================================================================

#[test]
fn flattened_sarif_accessible() {
    let _ = &cockpitctl_core::cockpit_report_to_sarif;
    let _ = &cockpitctl_core::cockpit_report_to_sarif_json;
}

// ============================================================================
// Module-level reexports (sub-crate namespaces)
// ============================================================================

#[test]
fn module_types_accessible() {
    use cockpitctl_core::types::{
        BuildfixPlan, CiInfo, CockpitPromoteHints, CountDeltas, Finding, Fix, GitInfo, Highlight,
        HostInfo, Location, MissingPolicy, PolicySnapshot, RunInfo, SafetyLevel, SchemaValidation,
        SensorPolicy, SensorSummary, Severity, TrendDelta, Verdict, VerdictCounts,
    };

    let _ = std::any::type_name::<Severity>();
    let _ = std::any::type_name::<Finding>();
    let _ = std::any::type_name::<Location>();
    let _ = std::any::type_name::<Highlight>();
    let _ = std::any::type_name::<Verdict>();
    let _ = std::any::type_name::<VerdictCounts>();
    let _ = std::any::type_name::<SensorSummary>();
    let _ = std::any::type_name::<SensorPolicy>();
    let _ = std::any::type_name::<SchemaValidation>();
    let _ = std::any::type_name::<MissingPolicy>();
    let _ = std::any::type_name::<PolicySnapshot>();
    let _ = std::any::type_name::<SafetyLevel>();
    let _ = std::any::type_name::<Fix>();
    let _ = std::any::type_name::<BuildfixPlan>();
    let _ = std::any::type_name::<HostInfo>();
    let _ = std::any::type_name::<GitInfo>();
    let _ = std::any::type_name::<CiInfo>();
    let _ = std::any::type_name::<RunInfo>();
    let _ = std::any::type_name::<TrendDelta>();
    let _ = std::any::type_name::<CountDeltas>();
    let _ = std::any::type_name::<CockpitPromoteHints>();
}

#[test]
fn module_domain_accessible() {
    use cockpitctl_core::domain;

    let cfg = cockpitctl_core::CockpitConfig::default();
    let snapshot = domain::snapshot_policy(&cfg);
    assert!(!snapshot.warn_is_fail);
}

#[test]
fn module_ingest_accessible() {
    use cockpitctl_core::ingest;

    // SchemaValidator trait and NoOpSchemaValidator are re-exported
    let validator = ingest::NoOpSchemaValidator;
    let result = ingest::SchemaValidator::validate_receipt(&validator, b"{}").unwrap();
    assert!(matches!(result, ingest::SchemaValidationResult::Valid));
}

#[test]
fn module_render_accessible() {
    use cockpitctl_core::render;

    let _ = &render::render_comment;
}

#[test]
fn module_sarif_accessible() {
    use cockpitctl_core::sarif;

    let _ = &sarif::cockpit_report_to_sarif;
}

#[test]
fn module_io_accessible() {
    use cockpitctl_core::io;

    let _ = std::any::type_name::<io::FsReceiptSource>();
}

#[test]
fn module_domain_buildfix_accessible() {
    use cockpitctl_core::domain_buildfix;

    let _ = &domain_buildfix::match_buildfix_plan;
}

#[test]
fn module_domain_signing_accessible() {
    use cockpitctl_core::domain_signing;

    let _ = &domain_signing::sign_policy_snapshot_hmac_sha256;
}

#[test]
fn module_feature_grid_accessible() {
    use cockpitctl_core::feature_grid;

    let _ = std::any::type_name::<feature_grid::FeatureGridState>();
    let _ = std::any::type_name::<feature_grid::FeatureGridCase>();
}

#[test]
fn module_feature_state_accessible() {
    use cockpitctl_core::feature_state;

    let _ = std::any::type_name::<feature_state::RuntimeFeatureState>();
    let _ = std::any::type_name::<feature_state::Feature>();
}

// ============================================================================
// Constructability: verify key types can be instantiated through facade
// ============================================================================

#[test]
fn cockpit_config_default_through_facade() {
    let cfg = cockpitctl_core::CockpitConfig::default();
    assert!(cfg.sensors.is_empty());
    assert!(!cfg.policy.warn_is_fail);
}

#[test]
fn verdict_status_variants_through_facade() {
    let statuses = [
        cockpitctl_core::VerdictStatus::Pass,
        cockpitctl_core::VerdictStatus::Warn,
        cockpitctl_core::VerdictStatus::Fail,
        cockpitctl_core::VerdictStatus::Skip,
    ];
    assert_eq!(statuses.len(), 4);
}

#[test]
fn presence_variants_through_facade() {
    let _ = cockpitctl_core::Presence::Present;
    let _ = cockpitctl_core::Presence::Missing;
    let _ = cockpitctl_core::Presence::Invalid;
}

#[test]
fn policy_outcome_variants_through_facade() {
    let _ = cockpitctl_core::PolicyOutcome::Blocked;
    let _ = cockpitctl_core::PolicyOutcome::Allowed;
    let _ = cockpitctl_core::PolicyOutcome::Informational;
}

#[test]
fn tool_info_constructable_through_facade() {
    let tool = cockpitctl_core::ToolInfo {
        name: "test".to_string(),
        version: "0.1.0".to_string(),
        commit: None,
    };
    assert_eq!(tool.name, "test");
}
