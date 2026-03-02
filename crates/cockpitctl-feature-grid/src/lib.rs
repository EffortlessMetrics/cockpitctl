//! Shared feature-grid definitions for BDD and feature flag parity.
//!
//! Defines the expected presence/absence of features per CLI argument
//! combination, used by BDD scenario expansion and feature-flag tests.

#![warn(missing_docs)]

use cockpitctl_feature_state::Feature;

pub use cockpitctl_feature_runtime::{feature_runtime_present, parse_feature_state};

/// Expected feature presence in a BDD matrix cell.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum FeatureGridState {
    /// Feature is expected to be present.
    Present,
    /// Feature is expected to be absent.
    Absent,
}

impl FeatureGridState {
    /// Returns `true` if the state is `Present`.
    pub const fn is_present(self) -> bool {
        match self {
            Self::Present => true,
            Self::Absent => false,
        }
    }
}

/// A single row in the feature runtime matrix.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct FeatureGridCase {
    /// The feature under test.
    pub feature: Feature,
    /// CLI arguments for this test case.
    pub args: &'static [&'static str],
    /// Expected presence state.
    pub expected: FeatureGridState,
}

impl FeatureGridCase {
    /// Create a new grid case.
    pub const fn new(
        feature: Feature,
        args: &'static [&'static str],
        expected: FeatureGridState,
    ) -> Self {
        Self {
            feature,
            args,
            expected,
        }
    }

    /// Returns whether the runtime state matches the expected state.
    pub fn expected_present<S: AsRef<str>>(self, cli_args: &[S]) -> bool {
        let runtime = feature_runtime_present(self.feature, cli_args);
        self.expected.is_present() == runtime
    }

    /// Alias for [`expected_present`](Self::expected_present).
    pub fn matches_row<S: AsRef<str>>(self, cli_args: &[S]) -> bool {
        self.expected_present(cli_args)
    }
}

const NO_ARGS: &[&str] = &[];
const DISABLE_HOOKS: &[&str] = &["--disable-hooks"];
const DISABLE_BUILDFIX: &[&str] = &["--disable-buildfix"];
const DISABLE_POLICY_SIGNING: &[&str] = &["--disable-policy-signing"];

/// Canonical feature matrix used by the BDD suite.
pub const FEATURE_TOGGLE_GRID: &[FeatureGridCase] = &[
    FeatureGridCase::new(Feature::Hooks, NO_ARGS, FeatureGridState::Present),
    FeatureGridCase::new(Feature::Hooks, DISABLE_HOOKS, FeatureGridState::Absent),
    FeatureGridCase::new(Feature::Buildfix, NO_ARGS, FeatureGridState::Present),
    FeatureGridCase::new(
        Feature::Buildfix,
        DISABLE_BUILDFIX,
        FeatureGridState::Absent,
    ),
    FeatureGridCase::new(Feature::PolicySigning, NO_ARGS, FeatureGridState::Present),
    FeatureGridCase::new(
        Feature::PolicySigning,
        DISABLE_POLICY_SIGNING,
        FeatureGridState::Absent,
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    // ── FeatureGridState ────────────────────────────────────────────

    #[test]
    fn state_present_is_present() {
        assert!(FeatureGridState::Present.is_present());
    }

    #[test]
    fn state_absent_is_not_present() {
        assert!(!FeatureGridState::Absent.is_present());
    }

    #[test]
    fn state_equality() {
        assert_eq!(FeatureGridState::Present, FeatureGridState::Present);
        assert_eq!(FeatureGridState::Absent, FeatureGridState::Absent);
        assert_ne!(FeatureGridState::Present, FeatureGridState::Absent);
    }

    #[test]
    fn state_clone() {
        let s = FeatureGridState::Present;
        let s2 = s;
        assert_eq!(s, s2);
    }

    #[test]
    fn state_debug_format() {
        assert_eq!(format!("{:?}", FeatureGridState::Present), "Present");
        assert_eq!(format!("{:?}", FeatureGridState::Absent), "Absent");
    }

    // ── FeatureGridCase construction ────────────────────────────────

    #[test]
    fn case_new_stores_fields() {
        let case = FeatureGridCase::new(
            Feature::Hooks,
            &["--disable-hooks"],
            FeatureGridState::Absent,
        );
        assert_eq!(case.feature, Feature::Hooks);
        assert_eq!(case.args, &["--disable-hooks"]);
        assert_eq!(case.expected, FeatureGridState::Absent);
    }

    #[test]
    fn case_new_empty_args() {
        let case = FeatureGridCase::new(Feature::Buildfix, &[], FeatureGridState::Present);
        assert!(case.args.is_empty());
        assert_eq!(case.expected, FeatureGridState::Present);
    }

    #[test]
    fn case_clone_and_eq() {
        let a = FeatureGridCase::new(Feature::PolicySigning, &[], FeatureGridState::Present);
        let b = a;
        assert_eq!(a, b);
    }

    // ── FeatureGridCase::expected_present / matches_row ─────────────

    #[test]
    fn case_expected_present_with_matching_args() {
        let case = FeatureGridCase::new(Feature::Hooks, &[], FeatureGridState::Present);
        // No disable flag → feature present → matches Present expectation
        assert!(case.expected_present::<&str>(&[]));
    }

    #[test]
    fn case_expected_present_disabled() {
        let case = FeatureGridCase::new(
            Feature::Hooks,
            &["--disable-hooks"],
            FeatureGridState::Absent,
        );
        assert!(case.expected_present(&["--disable-hooks"]));
    }

    #[test]
    fn case_expected_present_mismatch() {
        // Expect Present, but runtime is disabled → should return false
        let case = FeatureGridCase::new(Feature::Hooks, &[], FeatureGridState::Present);
        assert!(!case.expected_present(&["--disable-hooks"]));
    }

    #[test]
    fn matches_row_delegates_to_expected_present() {
        let case = FeatureGridCase::new(Feature::Buildfix, &[], FeatureGridState::Present);
        assert_eq!(
            case.matches_row::<&str>(&[]),
            case.expected_present::<&str>(&[]),
        );
    }

    #[test]
    fn matches_row_with_unrelated_flag() {
        // An unrelated flag should not disable Hooks
        let case = FeatureGridCase::new(Feature::Hooks, &[], FeatureGridState::Present);
        assert!(case.matches_row(&["--disable-buildfix"]));
    }

    // ── FEATURE_TOGGLE_GRID ─────────────────────────────────────────

    #[test]
    fn feature_grid_grid_rows_are_self_consistent() {
        for case in FEATURE_TOGGLE_GRID {
            assert!(
                case.matches_row(case.args),
                "feature grid case mismatch for {:?}",
                case.feature
            );
        }
    }

    #[test]
    fn grid_has_expected_row_count() {
        // 3 features × 2 states (present + absent) = 6 rows
        assert_eq!(FEATURE_TOGGLE_GRID.len(), 6);
    }

    #[test]
    fn grid_covers_all_features() {
        let features: Vec<Feature> = FEATURE_TOGGLE_GRID.iter().map(|c| c.feature).collect();
        assert!(features.contains(&Feature::Hooks));
        assert!(features.contains(&Feature::Buildfix));
        assert!(features.contains(&Feature::PolicySigning));
    }

    #[test]
    fn grid_each_feature_has_present_and_absent() {
        for feature in [Feature::Hooks, Feature::Buildfix, Feature::PolicySigning] {
            let states: Vec<FeatureGridState> = FEATURE_TOGGLE_GRID
                .iter()
                .filter(|c| c.feature == feature)
                .map(|c| c.expected)
                .collect();
            assert!(
                states.contains(&FeatureGridState::Present),
                "{feature:?} missing Present row"
            );
            assert!(
                states.contains(&FeatureGridState::Absent),
                "{feature:?} missing Absent row"
            );
        }
    }

    #[test]
    fn grid_absent_rows_carry_matching_disable_flag() {
        for case in FEATURE_TOGGLE_GRID {
            if case.expected == FeatureGridState::Absent {
                assert!(
                    case.args.contains(&case.feature.disable_flag()),
                    "{:?} absent row should carry its disable flag",
                    case.feature
                );
            }
        }
    }

    #[test]
    fn grid_present_rows_have_empty_args() {
        for case in FEATURE_TOGGLE_GRID {
            if case.expected == FeatureGridState::Present {
                assert!(
                    case.args.is_empty(),
                    "{:?} present row should have empty args",
                    case.feature
                );
            }
        }
    }
}
