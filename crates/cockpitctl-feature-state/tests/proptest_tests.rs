//! Property-based and additional edge-case tests for `cockpitctl-feature-state`.

use cockpitctl_feature_state::{Feature, RuntimeFeatureState};
use proptest::prelude::*;

// ---------------------------------------------------------------------------
// Strategies
// ---------------------------------------------------------------------------

fn feature_strategy() -> impl Strategy<Value = Feature> {
    prop_oneof![
        Just(Feature::Hooks),
        Just(Feature::Buildfix),
        Just(Feature::PolicySigning),
    ]
}

// ---------------------------------------------------------------------------
// Proptest: as_str -> from_name roundtrip
// ---------------------------------------------------------------------------

proptest! {
    #[test]
    fn as_str_from_name_roundtrip(f in feature_strategy()) {
        let name = f.as_str();
        let parsed = Feature::from_name(name);
        prop_assert_eq!(parsed, Some(f));
    }

    #[test]
    fn contract_feature_identity(f in feature_strategy()) {
        let c = f.contract();
        prop_assert_eq!(c.feature, f);
        prop_assert_eq!(c.name, f.as_str());
        prop_assert_eq!(c.disable_flag, f.disable_flag());
    }

    #[test]
    fn is_enabled_matches_accessor(
        h in proptest::bool::ANY,
        b in proptest::bool::ANY,
        p in proptest::bool::ANY,
    ) {
        let state = RuntimeFeatureState::new(h, b, p);
        prop_assert_eq!(state.is_enabled(Feature::Hooks), state.hooks());
        prop_assert_eq!(state.is_enabled(Feature::Buildfix), state.buildfix());
        prop_assert_eq!(state.is_enabled(Feature::PolicySigning), state.policy_signing());
    }

    #[test]
    fn from_disable_flags_correct(
        hc in proptest::bool::ANY, hd in proptest::bool::ANY,
        bc in proptest::bool::ANY, bd in proptest::bool::ANY,
        pc in proptest::bool::ANY, pd in proptest::bool::ANY,
    ) {
        let state = RuntimeFeatureState::from_disable_flags(hc, hd, bc, bd, pc, pd);
        prop_assert_eq!(state.hooks(), hc && !hd);
        prop_assert_eq!(state.buildfix(), bc && !bd);
        prop_assert_eq!(state.policy_signing(), pc && !pd);
    }

    #[test]
    fn new_roundtrip(
        h in proptest::bool::ANY,
        b in proptest::bool::ANY,
        p in proptest::bool::ANY,
    ) {
        let state = RuntimeFeatureState::new(h, b, p);
        prop_assert_eq!(state.hooks(), h);
        prop_assert_eq!(state.buildfix(), b);
        prop_assert_eq!(state.policy_signing(), p);
    }

    #[test]
    fn not_compiled_always_disabled(f in feature_strategy()) {
        let state = RuntimeFeatureState::from_disable_flags(
            f != Feature::Hooks,
            false,
            f != Feature::Buildfix,
            false,
            f != Feature::PolicySigning,
            false,
        );
        prop_assert!(!state.is_enabled(f));
    }
}

// ---------------------------------------------------------------------------
// Edge-case: from_name rejects invalid inputs
// ---------------------------------------------------------------------------

#[test]
fn from_name_rejects_invalid_inputs() {
    assert_eq!(Feature::from_name(""), None);
    assert_eq!(Feature::from_name("Hooks"), None);
    assert_eq!(Feature::from_name("BUILDFIX"), None);
    assert_eq!(Feature::from_name("policy_signing"), None);
    assert_eq!(Feature::from_name("unknown"), None);
    assert_eq!(Feature::from_name(" hooks"), None);
}

// ---------------------------------------------------------------------------
// Edge-case: all disable flags start with "--disable-"
// ---------------------------------------------------------------------------

#[test]
fn all_disable_flags_start_with_prefix() {
    for f in Feature::all() {
        assert!(
            f.disable_flag().starts_with("--disable-"),
            "{:?} disable flag doesn't start with --disable-",
            f
        );
    }
}

// ---------------------------------------------------------------------------
// Edge-case: disable flags are globally unique
// ---------------------------------------------------------------------------

#[test]
fn disable_flags_are_globally_unique() {
    let flags: Vec<&str> = Feature::all().iter().map(|f| f.disable_flag()).collect();
    let mut deduped = flags.clone();
    deduped.sort();
    deduped.dedup();
    assert_eq!(flags.len(), deduped.len(), "duplicate disable flags found");
}

// ---------------------------------------------------------------------------
// Edge-case: feature names are lowercase kebab-case
// ---------------------------------------------------------------------------

#[test]
fn feature_names_are_lowercase_kebab() {
    for f in Feature::all() {
        let name = f.as_str();
        assert!(
            name.chars().all(|c| c.is_ascii_lowercase() || c == '-'),
            "feature name {:?} is not lowercase kebab-case",
            name
        );
    }
}

// ---------------------------------------------------------------------------
// Edge-case: from_args with substring doesn't match
// ---------------------------------------------------------------------------

#[test]
fn from_args_substring_does_not_match() {
    let args: Vec<String> = vec!["--disable-hooks-extra".into()];
    let state = RuntimeFeatureState::from_args(true, true, true, &args);
    assert!(state.hooks());
}

// ---------------------------------------------------------------------------
// Edge-case: from_args with duplicate flags
// ---------------------------------------------------------------------------

#[test]
fn from_args_duplicate_flags_still_disables() {
    let args: Vec<String> = vec![
        "--disable-hooks".into(),
        "--disable-hooks".into(),
        "--disable-buildfix".into(),
    ];
    let state = RuntimeFeatureState::from_args(true, true, true, &args);
    assert!(!state.hooks());
    assert!(!state.buildfix());
    assert!(state.policy_signing());
}

// ---------------------------------------------------------------------------
// Edge-case: RuntimeFeatureState is Copy
// ---------------------------------------------------------------------------

#[test]
fn runtime_state_copy_semantics() {
    let a = RuntimeFeatureState::new(true, false, true);
    let b = a;
    assert_eq!(a, b);
    assert!(a.hooks());
    assert!(b.hooks());
}

// ---------------------------------------------------------------------------
// Edge-case: Feature is Copy
// ---------------------------------------------------------------------------

#[test]
fn feature_copy_semantics() {
    let a = Feature::Hooks;
    let b = a;
    assert_eq!(a, b);
    assert_eq!(a.as_str(), b.as_str());
}
