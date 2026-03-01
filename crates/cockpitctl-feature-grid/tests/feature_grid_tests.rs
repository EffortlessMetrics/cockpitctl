//! Integration tests for the `cockpitctl-feature-grid` public API.
//!
//! Exercises the crate from an external consumer's perspective:
//! constructing the feature grid, enumerating combinations,
//! and querying feature state.

use cockpitctl_feature_grid::{
    FEATURE_TOGGLE_GRID, FeatureGridCase, FeatureGridState, feature_runtime_present,
    parse_feature_state,
};
use cockpitctl_feature_state::Feature;

// ── Default feature grid is constructible ───────────────────────────────

#[test]
fn feature_grid_is_non_empty() {
    assert!(
        !FEATURE_TOGGLE_GRID.is_empty(),
        "FEATURE_TOGGLE_GRID must not be empty"
    );
}

#[test]
fn feature_grid_contains_expected_row_count() {
    // 3 features × 2 states = 6 rows
    assert_eq!(FEATURE_TOGGLE_GRID.len(), 6);
}

// ── Feature combinations are enumerable ─────────────────────────────────

#[test]
fn all_three_features_appear_in_grid() {
    let features: Vec<Feature> = FEATURE_TOGGLE_GRID.iter().map(|c| c.feature).collect();
    assert!(features.contains(&Feature::Hooks));
    assert!(features.contains(&Feature::Buildfix));
    assert!(features.contains(&Feature::PolicySigning));
}

#[test]
fn each_feature_has_both_present_and_absent_rows() {
    for feature in Feature::all() {
        let states: Vec<FeatureGridState> = FEATURE_TOGGLE_GRID
            .iter()
            .filter(|c| c.feature == *feature)
            .map(|c| c.expected)
            .collect();

        assert!(
            states.contains(&FeatureGridState::Present),
            "{feature:?} must have a Present row"
        );
        assert!(
            states.contains(&FeatureGridState::Absent),
            "{feature:?} must have an Absent row"
        );
    }
}

#[test]
fn grid_cases_are_self_consistent_for_absent_rows() {
    // Absent rows always pass: feature not available → runtime absent → matches Absent.
    // Present rows only pass when feature flags are compiled in.
    for case in FEATURE_TOGGLE_GRID {
        if case.expected == FeatureGridState::Absent {
            assert!(
                case.matches_row(case.args),
                "absent grid case for {:?} should always be self-consistent",
                case.feature,
            );
        }
    }
}

// ── Feature state queries ───────────────────────────────────────────────
//
// Note: without feature-* Cargo features enabled (standalone crate test),
// `Feature::is_available()` returns false for all features. These tests
// verify the runtime helper behaves correctly in that context: disabled
// features always report absent regardless of CLI args.

#[test]
fn feature_runtime_respects_availability() {
    let empty: &[&str] = &[];
    for feature in Feature::all() {
        let result = feature_runtime_present(*feature, empty);
        // Result must match compile-time availability
        assert_eq!(
            result,
            feature.is_available(),
            "{feature:?} runtime should match is_available()"
        );
    }
}

#[test]
fn feature_runtime_disabled_by_own_flag() {
    // With the disable flag, the feature is always absent (whether compiled or not)
    let cases = [
        (Feature::Hooks, "--disable-hooks"),
        (Feature::Buildfix, "--disable-buildfix"),
        (Feature::PolicySigning, "--disable-policy-signing"),
    ];
    for (feature, flag) in &cases {
        assert!(
            !feature_runtime_present(*feature, &[*flag]),
            "{feature:?} should be absent when {flag} is passed"
        );
    }
}

#[test]
fn feature_runtime_disable_flag_only_affects_own_feature() {
    // Disabling one feature should not affect another's availability
    for feature in Feature::all() {
        let other_flags: Vec<&str> = Feature::all()
            .iter()
            .filter(|f| **f != *feature)
            .map(|f| f.disable_flag())
            .collect();
        let result = feature_runtime_present(*feature, &other_flags);
        assert_eq!(
            result,
            feature.is_available(),
            "{feature:?} should not be affected by other features' disable flags"
        );
    }
}

// ── FeatureGridCase API ─────────────────────────────────────────────────

#[test]
fn feature_grid_case_new_round_trips_fields() {
    let case = FeatureGridCase::new(
        Feature::Buildfix,
        &["--disable-buildfix"],
        FeatureGridState::Absent,
    );
    assert_eq!(case.feature, Feature::Buildfix);
    assert_eq!(case.args, &["--disable-buildfix"]);
    assert_eq!(case.expected, FeatureGridState::Absent);
}

#[test]
fn feature_grid_case_expected_present_absent_case_matches() {
    // An Absent case with the disable flag: runtime is absent → matches Absent expectation
    let absent_case = FeatureGridCase::new(
        Feature::Hooks,
        &["--disable-hooks"],
        FeatureGridState::Absent,
    );
    assert!(absent_case.expected_present(&["--disable-hooks"]));
}

#[test]
fn feature_grid_case_expected_present_mismatch_when_absent() {
    // If runtime says absent (no feature flag) but case expects Present → mismatch
    let case = FeatureGridCase::new(Feature::Hooks, &[], FeatureGridState::Present);
    if !Feature::Hooks.is_available() {
        // Without feature flag compiled in, runtime is always absent → mismatch with Present
        assert!(!case.expected_present::<&str>(&[]));
    }
}

// ── parse_feature_state ─────────────────────────────────────────────────

#[test]
fn parse_feature_state_positive_tokens() {
    for token in &["present", "enabled", "on", "PRESENT", "Enabled", "ON"] {
        assert_eq!(
            parse_feature_state(token),
            Some(true),
            "token {token:?} should parse as true"
        );
    }
}

#[test]
fn parse_feature_state_negative_tokens() {
    for token in &["absent", "disabled", "off", "ABSENT", "Disabled", "OFF"] {
        assert_eq!(
            parse_feature_state(token),
            Some(false),
            "token {token:?} should parse as false"
        );
    }
}

#[test]
fn parse_feature_state_invalid_tokens_return_none() {
    for token in &["yes", "no", "true", "false", "1", "0", "", "maybe"] {
        assert_eq!(
            parse_feature_state(token),
            None,
            "token {token:?} should parse as None"
        );
    }
}

// ── FeatureGridState API ────────────────────────────────────────────────

#[test]
fn feature_grid_state_is_present() {
    assert!(FeatureGridState::Present.is_present());
    assert!(!FeatureGridState::Absent.is_present());
}

#[test]
fn feature_grid_state_equality() {
    assert_eq!(FeatureGridState::Present, FeatureGridState::Present);
    assert_ne!(FeatureGridState::Present, FeatureGridState::Absent);
}
