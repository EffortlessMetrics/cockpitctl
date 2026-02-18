use cockpitctl_types::{
    BuildfixPolicy, CockpitConfig, Policy, PolicySignatureAlgorithm, PolicySigningConfig,
    SafetyLevel, SchemaValidation, VerdictCounts, is_valid_sensor_id,
};

#[test]
fn sensor_id_validation_accepts_and_rejects_expected_patterns() {
    let valid = ["sensor1", "sensor_2", "sensor-3", "SENSOR4"];
    for id in valid {
        assert!(is_valid_sensor_id(id), "expected valid sensor id: {}", id);
    }

    let invalid = [
        "",
        "sensor..bad",
        "sensor/bad",
        "sensor\\bad",
        "sensor.bad",
        "sensor bad",
    ];
    for id in invalid {
        assert!(
            !is_valid_sensor_id(id),
            "expected invalid sensor id: {}",
            id
        );
    }
}

#[test]
fn policy_defaults_are_stable() {
    let policy = Policy::default();
    assert!(!policy.warn_is_fail);
    assert_eq!(policy.max_highlights, 7);
    assert_eq!(policy.max_per_sensor_findings, 20);
    assert_eq!(policy.max_annotations, 25);
    assert_eq!(policy.schema_validation, SchemaValidation::Lax);
    assert_eq!(
        policy.section_order,
        vec![
            "Highlights",
            "Repo contract",
            "Dependencies",
            "Policy",
            "Tests",
            "Diagnostics",
            "Performance",
            "Environment",
            "Other"
        ]
        .into_iter()
        .map(String::from)
        .collect::<Vec<_>>()
    );
}

#[test]
fn cockpit_config_defaults_include_empty_sensors() {
    let cfg = CockpitConfig::default();
    assert!(cfg.sensors.is_empty());
    assert_eq!(cfg.policy.schema_validation, SchemaValidation::Lax);
    assert_eq!(cfg.buildfix, BuildfixPolicy::default());
    assert!(!cfg.buildfix.auto_apply);
    assert_eq!(cfg.buildfix.max_auto_apply_safety, SafetyLevel::Safe);
    assert!(cfg.buildfix.require_matched_finding);
    assert!(cfg.buildfix.actuator.is_none());
    assert_eq!(cfg.policy_signing, PolicySigningConfig::default());
    assert!(!cfg.policy_signing.enabled);
    assert_eq!(
        cfg.policy_signing.algorithm,
        PolicySignatureAlgorithm::HmacSha256
    );
    assert!(cfg.policy_signing.key_path.is_none());
    assert!(cfg.policy_signing.key_env.is_none());
    assert!(cfg.policy_signing.key_id.is_none());
}

#[test]
fn verdict_counts_default_is_zeroed() {
    let counts = VerdictCounts::default();
    assert_eq!(counts.info, 0);
    assert_eq!(counts.warn, 0);
    assert_eq!(counts.error, 0);
    assert_eq!(counts.suppressed, 0);
}

#[test]
fn verdict_counts_serialization_skips_zero_suppressed() {
    let counts = VerdictCounts {
        info: 1,
        warn: 2,
        error: 3,
        suppressed: 0,
    };
    let json = serde_json::to_value(&counts).expect("serialize");
    assert!(
        json.get("suppressed").is_none(),
        "suppressed should be omitted"
    );
}

#[test]
fn verdict_counts_serialization_includes_nonzero_suppressed() {
    let counts = VerdictCounts {
        info: 1,
        warn: 2,
        error: 3,
        suppressed: 7,
    };
    let json = serde_json::to_value(&counts).expect("serialize");
    assert_eq!(json.get("suppressed").and_then(|v| v.as_u64()), Some(7));
}
