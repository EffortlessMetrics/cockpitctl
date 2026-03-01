//! Integration tests for cockpitctl-feature-state.
//!
//! Exercises the public feature enumeration, contract metadata, runtime state,
//! and CLI flag parsing through the crate's public API.

use cockpitctl_feature_state::{Feature, RuntimeFeatureState};

// ── default state (all off) ──────────────────────────────────────────

#[test]
fn default_state_all_off() {
    let state = RuntimeFeatureState::new(false, false, false);
    assert!(!state.hooks());
    assert!(!state.buildfix());
    assert!(!state.policy_signing());
}

#[test]
fn default_state_is_enabled_returns_false_for_all() {
    let state = RuntimeFeatureState::new(false, false, false);
    for &f in Feature::all() {
        assert!(!state.is_enabled(f), "{:?} should be disabled", f);
    }
}

// ── enable individual features ───────────────────────────────────────

#[test]
fn enable_hooks_only() {
    let state = RuntimeFeatureState::new(true, false, false);
    assert!(state.hooks());
    assert!(!state.buildfix());
    assert!(!state.policy_signing());
}

#[test]
fn enable_buildfix_only() {
    let state = RuntimeFeatureState::new(false, true, false);
    assert!(!state.hooks());
    assert!(state.buildfix());
    assert!(!state.policy_signing());
}

#[test]
fn enable_policy_signing_only() {
    let state = RuntimeFeatureState::new(false, false, true);
    assert!(state.is_enabled(Feature::PolicySigning));
    assert!(!state.is_enabled(Feature::Hooks));
    assert!(!state.is_enabled(Feature::Buildfix));
}

// ── enable all features ──────────────────────────────────────────────

#[test]
fn all_features_enabled() {
    let state = RuntimeFeatureState::new(true, true, true);
    for &f in Feature::all() {
        assert!(state.is_enabled(f), "{:?} should be enabled", f);
    }
}

// ── state serialization / deserialization (equality roundtrip) ───────

#[test]
fn state_clone_is_equal() {
    let state = RuntimeFeatureState::new(true, false, true);
    let cloned = state;
    assert_eq!(state, cloned);
}

#[test]
fn state_debug_representation_is_non_empty() {
    let state = RuntimeFeatureState::new(true, true, true);
    let debug = format!("{state:?}");
    assert!(!debug.is_empty());
    assert!(debug.contains("RuntimeFeatureState"));
}

// ── feature query methods ────────────────────────────────────────────

#[test]
fn feature_from_name_roundtrip() {
    for &f in Feature::all() {
        let name = f.as_str();
        let parsed = Feature::from_name(name).unwrap();
        assert_eq!(parsed, f);
    }
}

#[test]
fn feature_from_name_unknown_returns_none() {
    assert!(Feature::from_name("nonexistent").is_none());
    assert!(Feature::from_name("").is_none());
}

#[test]
fn feature_disable_flag_matches_convention() {
    for &f in Feature::all() {
        let flag = f.disable_flag();
        assert!(
            flag.starts_with("--disable-"),
            "flag should start with --disable-: {flag}"
        );
        assert!(
            flag.contains(f.as_str()),
            "flag should contain feature name"
        );
    }
}

#[test]
fn feature_contract_name_matches_as_str() {
    for &f in Feature::all() {
        let contract = f.contract();
        assert_eq!(contract.name, f.as_str());
        assert_eq!(contract.feature, f);
        assert_eq!(contract.disable_flag, f.disable_flag());
    }
}

#[test]
fn all_features_have_comment_markers() {
    for &f in Feature::all() {
        let contract = f.contract();
        assert!(
            contract.comment_marker.is_some(),
            "{:?} should have a comment marker",
            f
        );
    }
}

// ── from_disable_flags ───────────────────────────────────────────────

#[test]
fn from_disable_flags_compiled_but_disabled() {
    let state = RuntimeFeatureState::from_disable_flags(
        true, true, // hooks compiled, disabled
        true, false, // buildfix compiled, not disabled
        false, false, // policy-signing not compiled, not disabled
    );
    assert!(!state.hooks());
    assert!(state.buildfix());
    assert!(!state.policy_signing());
}

// ── from_args ────────────────────────────────────────────────────────

#[test]
fn from_args_with_disable_flag() {
    let args: Vec<String> = vec!["--disable-hooks".into()];
    let state = RuntimeFeatureState::from_args(true, true, true, &args);
    assert!(!state.hooks());
    assert!(state.buildfix());
    assert!(state.policy_signing());
}

#[test]
fn from_args_empty_enables_all_compiled() {
    let state = RuntimeFeatureState::from_args(true, true, true, &[]);
    assert!(state.hooks());
    assert!(state.buildfix());
    assert!(state.policy_signing());
}

#[test]
fn from_args_multiple_disable_flags() {
    let args: Vec<String> = vec![
        "--disable-hooks".into(),
        "--disable-buildfix".into(),
        "--disable-policy-signing".into(),
    ];
    let state = RuntimeFeatureState::from_args(true, true, true, &args);
    assert!(!state.hooks());
    assert!(!state.buildfix());
    assert!(!state.policy_signing());
}

// ── feature catalog exhaustiveness ───────────────────────────────────

#[test]
fn feature_all_returns_exactly_three() {
    assert_eq!(Feature::all().len(), 3);
}

#[test]
fn feature_names_are_unique() {
    let names: Vec<&str> = Feature::all().iter().map(|f| f.as_str()).collect();
    let mut deduped = names.clone();
    deduped.sort();
    deduped.dedup();
    assert_eq!(names.len(), deduped.len());
}
