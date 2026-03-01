//! Feature flag isolation tests.
//!
//! Verifies that each feature flag works independently: enabling or disabling
//! one feature must not affect any other feature's state.

use cockpitctl_feature_grid::feature_runtime_present;
use cockpitctl_feature_state::{Feature, RuntimeFeatureState};

// ── Cross-feature independence ──────────────────────────────────────────────

/// Disabling feature X must not change the runtime state of feature Y.
#[test]
fn disable_hooks_does_not_affect_buildfix_or_signing() {
    let args = ["--disable-hooks"];
    // Buildfix/PolicySigning should only depend on their own disable flag
    assert_eq!(
        feature_runtime_present(Feature::Buildfix, &args),
        Feature::Buildfix.is_available(),
    );
    assert_eq!(
        feature_runtime_present(Feature::PolicySigning, &args),
        Feature::PolicySigning.is_available(),
    );
}

#[test]
fn disable_buildfix_does_not_affect_hooks_or_signing() {
    let args = ["--disable-buildfix"];
    assert_eq!(
        feature_runtime_present(Feature::Hooks, &args),
        Feature::Hooks.is_available(),
    );
    assert_eq!(
        feature_runtime_present(Feature::PolicySigning, &args),
        Feature::PolicySigning.is_available(),
    );
}

#[test]
fn disable_signing_does_not_affect_hooks_or_buildfix() {
    let args = ["--disable-policy-signing"];
    assert_eq!(
        feature_runtime_present(Feature::Hooks, &args),
        Feature::Hooks.is_available(),
    );
    assert_eq!(
        feature_runtime_present(Feature::Buildfix, &args),
        Feature::Buildfix.is_available(),
    );
}

// ── RuntimeFeatureState isolation via from_disable_flags ─────────────────────

/// Each parameter position in `from_disable_flags` controls exactly one feature.
#[test]
fn from_disable_flags_hooks_isolated() {
    let all_on = RuntimeFeatureState::from_disable_flags(true, false, true, false, true, false);
    let hooks_off = RuntimeFeatureState::from_disable_flags(true, true, true, false, true, false);
    // Only hooks should change
    assert!(all_on.hooks());
    assert!(!hooks_off.hooks());
    assert_eq!(all_on.buildfix(), hooks_off.buildfix());
    assert_eq!(all_on.policy_signing(), hooks_off.policy_signing());
}

#[test]
fn from_disable_flags_buildfix_isolated() {
    let all_on = RuntimeFeatureState::from_disable_flags(true, false, true, false, true, false);
    let buildfix_off =
        RuntimeFeatureState::from_disable_flags(true, false, true, true, true, false);
    assert!(all_on.buildfix());
    assert!(!buildfix_off.buildfix());
    assert_eq!(all_on.hooks(), buildfix_off.hooks());
    assert_eq!(all_on.policy_signing(), buildfix_off.policy_signing());
}

#[test]
fn from_disable_flags_signing_isolated() {
    let all_on = RuntimeFeatureState::from_disable_flags(true, false, true, false, true, false);
    let signing_off = RuntimeFeatureState::from_disable_flags(true, false, true, false, true, true);
    assert!(all_on.policy_signing());
    assert!(!signing_off.policy_signing());
    assert_eq!(all_on.hooks(), signing_off.hooks());
    assert_eq!(all_on.buildfix(), signing_off.buildfix());
}

// ── Compile-time unavailability overrides runtime ───────────────────────────

/// A feature not compiled in cannot be enabled at runtime, even without disable flags.
#[test]
fn uncompiled_feature_stays_off_regardless_of_args() {
    let state = RuntimeFeatureState::from_disable_flags(false, false, false, false, false, false);
    assert!(!state.hooks());
    assert!(!state.buildfix());
    assert!(!state.policy_signing());
}

/// Passing a disable flag for an already-uncompiled feature is harmless.
#[test]
fn disable_flag_on_uncompiled_feature_is_noop() {
    let without_flag =
        RuntimeFeatureState::from_disable_flags(false, false, false, false, false, false);
    let with_flag = RuntimeFeatureState::from_disable_flags(false, true, false, true, false, true);
    assert_eq!(without_flag, with_flag);
}

// ── Feature contract isolation ──────────────────────────────────────────────

/// Each feature's contract references only its own disable flag.
#[test]
fn contract_disable_flags_are_unique() {
    let flags: Vec<&str> = Feature::all().iter().map(|f| f.disable_flag()).collect();
    for (i, a) in flags.iter().enumerate() {
        for (j, b) in flags.iter().enumerate() {
            if i != j {
                assert_ne!(a, b, "feature disable flags must be unique");
            }
        }
    }
}

/// Each feature's contract name is unique.
#[test]
fn contract_names_are_unique() {
    let names: Vec<&str> = Feature::all().iter().map(|f| f.as_str()).collect();
    for (i, a) in names.iter().enumerate() {
        for (j, b) in names.iter().enumerate() {
            if i != j {
                assert_ne!(a, b, "feature names must be unique");
            }
        }
    }
}

// ── Full matrix: every single-feature toggle is independent ─────────────────

/// For each feature, toggling it from enabled to disabled (via disable flags)
/// must not change any other feature's is_enabled() result.
#[test]
fn single_feature_toggle_matrix() {
    let features = Feature::all();
    for &target in features {
        let all_enabled = RuntimeFeatureState::new(true, true, true);
        // Disable only the target
        let toggled = RuntimeFeatureState::from_disable_flags(
            true,
            target == Feature::Hooks,
            true,
            target == Feature::Buildfix,
            true,
            target == Feature::PolicySigning,
        );

        // Target must be off
        assert!(
            !toggled.is_enabled(target),
            "{} should be disabled after toggle",
            target.as_str()
        );

        // All others must remain on
        for &other in features {
            if other != target {
                assert_eq!(
                    all_enabled.is_enabled(other),
                    toggled.is_enabled(other),
                    "toggling {} should not affect {}",
                    target.as_str(),
                    other.as_str()
                );
            }
        }
    }
}

// ── from_args isolation ─────────────────────────────────────────────────────

/// Verify that from_args with only one disable flag leaves others untouched.
#[test]
fn from_args_single_disable_isolates_target() {
    for &target in Feature::all() {
        let args = vec![target.disable_flag().to_string()];
        let state = RuntimeFeatureState::from_args(true, true, true, &args);

        assert!(
            !state.is_enabled(target),
            "from_args: {} should be disabled",
            target.as_str()
        );

        for &other in Feature::all() {
            if other != target {
                assert!(
                    state.is_enabled(other),
                    "from_args: disabling {} should not affect {}",
                    target.as_str(),
                    other.as_str()
                );
            }
        }
    }
}
