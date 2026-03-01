//! Integration tests for cockpitctl-feature-grid.
//!
//! Exercises the feature toggle grid, runtime presence helpers,
//! and state parsing through the crate's public API.
//!
//! Note: `feature_runtime_present` returns false for features that are
//! not compiled in (`is_available()` is a compile-time check). Tests
//! adapt expectations based on the actual compile-time feature state.

use cockpitctl_feature_grid::{
    FEATURE_TOGGLE_GRID, FeatureGridCase, FeatureGridState, feature_runtime_present,
    parse_feature_state,
};
use cockpitctl_feature_state::Feature;

// ── grid with all features on (no disable args) ──────────────────────

#[test]
fn grid_present_rows_consistent_with_compile_time_availability() {
    let no_args: &[&str] = &[];
    for row in FEATURE_TOGGLE_GRID
        .iter()
        .filter(|r| r.expected == FeatureGridState::Present)
    {
        let runtime = feature_runtime_present(row.feature, no_args);
        // Runtime presence should equal compile-time availability.
        assert_eq!(
            runtime,
            row.feature.is_available(),
            "feature {:?}: runtime={runtime}, available={}",
            row.feature,
            row.feature.is_available(),
        );
    }
}

// ── grid with no features (all disabled) ─────────────────────────────

#[test]
fn grid_absent_rows_always_absent_regardless_of_availability() {
    for row in FEATURE_TOGGLE_GRID
        .iter()
        .filter(|r| r.expected == FeatureGridState::Absent)
    {
        // Features with a disable flag should always be absent.
        let runtime = feature_runtime_present(row.feature, row.args);
        assert!(
            !runtime,
            "feature {:?} should be absent with {:?}",
            row.feature, row.args,
        );
    }
}

// ── grid with mixed features ─────────────────────────────────────────

#[test]
fn disable_flag_always_disables_regardless_of_availability() {
    for &f in Feature::all() {
        let args: &[&str] = &[f.disable_flag()];
        assert!(
            !feature_runtime_present(f, args),
            "{f:?} should be absent with its own disable flag"
        );
    }
}

#[test]
fn disable_one_feature_does_not_affect_others() {
    let args: &[&str] = &["--disable-hooks"];
    assert!(!feature_runtime_present(Feature::Hooks, args));
    // Other features: runtime presence equals their compile-time availability.
    assert_eq!(
        feature_runtime_present(Feature::Buildfix, args),
        Feature::Buildfix.is_available(),
    );
    assert_eq!(
        feature_runtime_present(Feature::PolicySigning, args),
        Feature::PolicySigning.is_available(),
    );
}

// ── grid sorting / ordering ──────────────────────────────────────────

#[test]
fn grid_has_exactly_six_rows() {
    assert_eq!(FEATURE_TOGGLE_GRID.len(), 6);
}

#[test]
fn grid_covers_all_features_twice() {
    for &f in Feature::all() {
        let count = FEATURE_TOGGLE_GRID
            .iter()
            .filter(|r| r.feature == f)
            .count();
        assert_eq!(count, 2, "feature {:?} should have exactly 2 grid rows", f);
    }
}

#[test]
fn each_feature_has_one_present_and_one_absent_row() {
    for &f in Feature::all() {
        let rows: Vec<&FeatureGridCase> = FEATURE_TOGGLE_GRID
            .iter()
            .filter(|r| r.feature == f)
            .collect();
        let present_count = rows
            .iter()
            .filter(|r| r.expected == FeatureGridState::Present)
            .count();
        let absent_count = rows
            .iter()
            .filter(|r| r.expected == FeatureGridState::Absent)
            .count();
        assert_eq!(present_count, 1, "{f:?}: expected 1 Present row");
        assert_eq!(absent_count, 1, "{f:?}: expected 1 Absent row");
    }
}

// ── grid display / formatting (FeatureGridState) ─────────────────────

#[test]
fn feature_grid_state_is_present() {
    assert!(FeatureGridState::Present.is_present());
    assert!(!FeatureGridState::Absent.is_present());
}

#[test]
fn feature_grid_state_debug() {
    let dbg = format!("{:?}", FeatureGridState::Present);
    assert!(dbg.contains("Present"));
}

// ── parse_feature_state ──────────────────────────────────────────────

#[test]
fn parse_feature_state_true_variants() {
    for token in &["present", "enabled", "on", "PRESENT", "Enabled", "ON"] {
        assert_eq!(
            parse_feature_state(token),
            Some(true),
            "expected true for {token}"
        );
    }
}

#[test]
fn parse_feature_state_false_variants() {
    for token in &["absent", "disabled", "off", "ABSENT", "Disabled", "OFF"] {
        assert_eq!(
            parse_feature_state(token),
            Some(false),
            "expected false for {token}"
        );
    }
}

#[test]
fn parse_feature_state_invalid_returns_none() {
    assert_eq!(parse_feature_state("maybe"), None);
    assert_eq!(parse_feature_state(""), None);
    assert_eq!(parse_feature_state("yes"), None);
}

// ── feature_runtime_present ──────────────────────────────────────────

#[test]
fn runtime_present_with_disable_flag_always_false() {
    for &f in Feature::all() {
        let args: Vec<String> = vec![f.disable_flag().into()];
        assert!(
            !feature_runtime_present(f, &args),
            "{f:?} should not be runtime-present with its own disable flag"
        );
    }
}

#[test]
fn runtime_present_unrelated_args_preserve_availability() {
    let args: &[&str] = &["--verbose", "--output=json", "--config=test.toml"];
    for &f in Feature::all() {
        assert_eq!(
            feature_runtime_present(f, args),
            f.is_available(),
            "{f:?}: unrelated args should not change availability"
        );
    }
}

#[test]
fn runtime_present_empty_args_equals_compile_time_availability() {
    let empty: &[&str] = &[];
    for &f in Feature::all() {
        assert_eq!(
            feature_runtime_present(f, empty),
            f.is_available(),
            "{f:?}: no args should match compile-time availability"
        );
    }
}
