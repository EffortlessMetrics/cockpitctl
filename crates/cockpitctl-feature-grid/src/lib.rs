//! Shared feature-grid definitions for BDD and feature flag parity.

use cockpitctl_feature_state::Feature;

/// Expected feature presence in a BDD matrix cell.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum FeatureGridState {
    Present,
    Absent,
}

impl FeatureGridState {
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
    pub feature: Feature,
    pub args: &'static [&'static str],
    pub expected: FeatureGridState,
}

impl FeatureGridCase {
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

    pub fn expected_present<S: AsRef<str>>(self, cli_args: &[S]) -> bool {
        let runtime = feature_runtime_present(self.feature, cli_args);
        self.expected.is_present() == runtime
    }

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

/// Feature-toggle runtime helper for BDD and CLI assertions.
pub fn feature_runtime_present<S: AsRef<str>>(feature: Feature, cli_args: &[S]) -> bool {
    if !feature.is_available() {
        return false;
    }
    let disable_flag = feature.disable_flag();
    !cli_args.iter().any(|arg| arg.as_ref() == disable_flag)
}

/// Parse a BDD feature-state token into a boolean presence expectation.
pub fn parse_feature_state(token: &str) -> Option<bool> {
    match token.to_ascii_lowercase().as_str() {
        "present" | "enabled" | "on" => Some(true),
        "absent" | "disabled" | "off" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn parse_feature_state_is_bidirectional() {
        assert_eq!(parse_feature_state("present"), Some(true));
        assert_eq!(parse_feature_state("Absent"), Some(false));
        assert_eq!(parse_feature_state("on"), Some(true));
        assert_eq!(parse_feature_state("off"), Some(false));
        assert_eq!(parse_feature_state("weird"), None);
    }
}
