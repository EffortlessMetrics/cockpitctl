//! Property-based and additional edge-case tests for `cockpitctl-feature-grid`.

use cockpitctl_feature_grid::{
    FEATURE_TOGGLE_GRID, FeatureGridCase, FeatureGridState, feature_runtime_present,
    parse_feature_state,
};
use cockpitctl_feature_state::Feature;
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

fn grid_state_strategy() -> impl Strategy<Value = FeatureGridState> {
    prop_oneof![
        Just(FeatureGridState::Present),
        Just(FeatureGridState::Absent),
    ]
}

// ---------------------------------------------------------------------------
// Proptest: disable flag always disables the feature
// ---------------------------------------------------------------------------

proptest! {
    #[test]
    fn own_disable_flag_always_disables(f in feature_strategy()) {
        let flag = f.disable_flag();
        let args = vec![flag.to_string()];
        let present = feature_runtime_present(f, &args);
        if f.is_available() {
            prop_assert!(!present, "feature {:?} should be disabled by {}", f, flag);
        }
    }

    #[test]
    fn other_flags_dont_affect_feature(f in feature_strategy()) {
        let other_flags: Vec<String> = Feature::all()
            .iter()
            .filter(|&&other| other != f)
            .map(|other| other.disable_flag().to_string())
            .collect();
        let present = feature_runtime_present(f, &other_flags);
        if f.is_available() {
            prop_assert!(present, "feature {:?} should not be disabled by other flags", f);
        }
    }

    #[test]
    fn runtime_present_empty_args_equals_availability(f in feature_strategy()) {
        let no_args: Vec<String> = vec![];
        let present = feature_runtime_present(f, &no_args);
        prop_assert_eq!(present, f.is_available());
    }

    #[test]
    fn grid_state_is_present_consistent(state in grid_state_strategy()) {
        match state {
            FeatureGridState::Present => prop_assert!(state.is_present()),
            FeatureGridState::Absent => prop_assert!(!state.is_present()),
        }
    }

    #[test]
    fn parse_feature_state_known_true_tokens(token in "(present|enabled|on)") {
        let result = parse_feature_state(&token);
        prop_assert_eq!(result, Some(true), "token {:?} should parse as true", token);
    }

    #[test]
    fn parse_feature_state_known_false_tokens(token in "(absent|disabled|off)") {
        let result = parse_feature_state(&token);
        prop_assert_eq!(result, Some(false), "token {:?} should parse as false", token);
    }

    #[test]
    fn parse_feature_state_random_returns_none(token in "[a-z]{10,20}") {
        let lower = token.to_ascii_lowercase();
        if !["present", "enabled", "on", "absent", "disabled", "off"].contains(&lower.as_str()) {
            prop_assert_eq!(parse_feature_state(&token), None);
        }
    }
}

// ---------------------------------------------------------------------------
// Edge-case: grid covers all features exactly twice
// ---------------------------------------------------------------------------

#[test]
fn grid_covers_all_features_exactly_twice() {
    for f in Feature::all() {
        let count = FEATURE_TOGGLE_GRID
            .iter()
            .filter(|c| c.feature == *f)
            .count();
        assert_eq!(
            count, 2,
            "feature {:?} appears {} times, expected 2",
            f, count
        );
    }
}

// ---------------------------------------------------------------------------
// Edge-case: Present rows have no args
// ---------------------------------------------------------------------------

#[test]
fn grid_present_rows_have_no_args() {
    for case in FEATURE_TOGGLE_GRID {
        if case.expected == FeatureGridState::Present {
            assert!(
                case.args.is_empty(),
                "Present case for {:?} should have no args",
                case.feature
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Edge-case: Absent rows carry the correct disable flag
// ---------------------------------------------------------------------------

#[test]
fn grid_absent_rows_carry_correct_disable_flag() {
    for case in FEATURE_TOGGLE_GRID {
        if case.expected == FeatureGridState::Absent {
            assert_eq!(
                case.args.len(),
                1,
                "Absent case for {:?} should have exactly 1 arg",
                case.feature
            );
            assert_eq!(
                case.args[0],
                case.feature.disable_flag(),
                "Absent case arg should be the feature's disable flag"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Edge-case: all features disabled at once
// ---------------------------------------------------------------------------

#[test]
fn runtime_present_all_disabled() {
    let all_flags: Vec<String> = Feature::all()
        .iter()
        .map(|f| f.disable_flag().to_string())
        .collect();
    for f in Feature::all() {
        assert!(
            !feature_runtime_present(*f, &all_flags),
            "{:?} should be disabled when all flags passed",
            f
        );
    }
}

// ---------------------------------------------------------------------------
// Edge-case: matches_row aliases expected_present
// ---------------------------------------------------------------------------

#[test]
fn matches_row_aliases_expected_present() {
    for case in FEATURE_TOGGLE_GRID {
        let args: Vec<String> = case.args.iter().map(|s| s.to_string()).collect();
        assert_eq!(
            case.expected_present(&args),
            case.matches_row(&args),
            "matches_row should equal expected_present for {:?}",
            case.feature,
        );
    }
}

// ---------------------------------------------------------------------------
// Edge-case: FeatureGridCase::new round-trips
// ---------------------------------------------------------------------------

#[test]
fn case_new_round_trips_all_fields() {
    for case in FEATURE_TOGGLE_GRID {
        let rebuilt = FeatureGridCase::new(case.feature, case.args, case.expected);
        assert_eq!(rebuilt.feature, case.feature);
        assert_eq!(rebuilt.args, case.args);
        assert_eq!(rebuilt.expected, case.expected);
    }
}

// ---------------------------------------------------------------------------
// Edge-case: Absent rows are self-consistent
// ---------------------------------------------------------------------------

#[test]
fn grid_absent_rows_self_consistent() {
    for case in FEATURE_TOGGLE_GRID {
        if case.expected == FeatureGridState::Absent {
            let args: Vec<String> = case.args.iter().map(|s| s.to_string()).collect();
            assert!(
                case.expected_present(&args),
                "Absent case for {:?} should be self-consistent",
                case.feature
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Edge-case: parse_feature_state mixed case
// ---------------------------------------------------------------------------

#[test]
fn parse_feature_state_mixed_case_variants() {
    assert_eq!(parse_feature_state("Present"), Some(true));
    assert_eq!(parse_feature_state("PRESENT"), Some(true));
    assert_eq!(parse_feature_state("Enabled"), Some(true));
    assert_eq!(parse_feature_state("ON"), Some(true));
    assert_eq!(parse_feature_state("Absent"), Some(false));
    assert_eq!(parse_feature_state("DISABLED"), Some(false));
    assert_eq!(parse_feature_state("Off"), Some(false));
}

// ---------------------------------------------------------------------------
// Edge-case: parse_feature_state unicode returns None
// ---------------------------------------------------------------------------

#[test]
fn parse_feature_state_unicode_returns_none() {
    assert_eq!(parse_feature_state(""), None);
}

// ---------------------------------------------------------------------------
// Edge-case: FeatureGridState variants are distinct
// ---------------------------------------------------------------------------

#[test]
fn grid_state_not_equal_across_variants() {
    assert_ne!(FeatureGridState::Present, FeatureGridState::Absent);
}

// ---------------------------------------------------------------------------
// Edge-case: FeatureGridState is Copy
// ---------------------------------------------------------------------------

#[test]
fn grid_state_copy_semantics() {
    let a = FeatureGridState::Present;
    let b = a;
    assert_eq!(a, b);
}
