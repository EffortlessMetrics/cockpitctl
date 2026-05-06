//! Shared feature flag model for runtime availability and runtime-disable state.
//!
//! Enumerates features that can be conditionally present at runtime and
//! maps each feature to its CLI flag, comment marker, and sidecar file.

#![warn(missing_docs)]

/// Metadata describing how a feature maps to CLI/runtime artifacts.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct FeatureContract {
    /// The feature this contract describes.
    pub feature: Feature,
    /// Short name of the feature.
    pub name: &'static str,
    /// CLI flag to disable this feature.
    pub disable_flag: &'static str,
    /// Comment marker contributed when the feature renders markdown sections.
    pub comment_marker: Option<&'static str>,
    /// Optional report `data` key this feature emits.
    pub report_data_key: Option<&'static str>,
    /// Optional sidecar file this feature emits.
    pub sidecar_file: Option<&'static str>,
}

/// Features that can be conditionally present at runtime.
///
/// # Examples
///
/// ```
/// use cockpitctl_feature_state::Feature;
///
/// // Parse a feature by name
/// let f = Feature::from_name("hooks").unwrap();
/// assert_eq!(f.as_str(), "hooks");
///
/// // Enumerate all features
/// assert_eq!(Feature::all().len(), 3);
/// ```
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Feature {
    /// Post-processing hooks feature.
    Hooks,
    /// Automatic build-fix application feature.
    Buildfix,
    /// Policy snapshot signing feature.
    PolicySigning,
}

impl Feature {
    /// Returns a slice of all known features.
    pub const fn all() -> &'static [Feature] {
        &[Feature::Hooks, Feature::Buildfix, Feature::PolicySigning]
    }

    /// Returns the string name of this feature.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Hooks => "hooks",
            Self::Buildfix => "buildfix",
            Self::PolicySigning => "policy-signing",
        }
    }

    /// Returns the CLI flag that disables this feature.
    pub const fn disable_flag(self) -> &'static str {
        match self {
            Self::Hooks => "--disable-hooks",
            Self::Buildfix => "--disable-buildfix",
            Self::PolicySigning => "--disable-policy-signing",
        }
    }

    /// Returns whether this feature was compiled in.
    pub const fn is_available(self) -> bool {
        match self {
            Self::Hooks => cfg!(feature = "feature-hooks"),
            Self::Buildfix => cfg!(feature = "feature-buildfix"),
            Self::PolicySigning => cfg!(feature = "feature-policy-signing"),
        }
    }

    /// Returns the full contract metadata for this feature.
    pub const fn contract(self) -> FeatureContract {
        match self {
            Self::Hooks => FeatureContract {
                feature: Self::Hooks,
                name: "hooks",
                disable_flag: "--disable-hooks",
                comment_marker: Some("### Hook Notes"),
                report_data_key: None,
                sidecar_file: None,
            },
            Self::Buildfix => FeatureContract {
                feature: Self::Buildfix,
                name: "buildfix",
                disable_flag: "--disable-buildfix",
                comment_marker: Some("### Buildfix"),
                report_data_key: Some("_buildfix"),
                sidecar_file: Some("buildfix.apply.json"),
            },
            Self::PolicySigning => FeatureContract {
                feature: Self::PolicySigning,
                name: "policy-signing",
                disable_flag: "--disable-policy-signing",
                comment_marker: Some("### Policy Signature"),
                report_data_key: Some("_policy_signature"),
                sidecar_file: Some("policy.signature.json"),
            },
        }
    }

    /// Parse a feature from its string name.
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "hooks" => Some(Self::Hooks),
            "buildfix" => Some(Self::Buildfix),
            "policy-signing" => Some(Self::PolicySigning),
            _ => None,
        }
    }
}

/// Runtime feature state after compile-time and CLI disable flags are applied.
///
/// # Examples
///
/// ```
/// use cockpitctl_feature_state::{Feature, RuntimeFeatureState};
///
/// let state = RuntimeFeatureState::new(true, false, true);
/// assert!(state.hooks());
/// assert!(!state.buildfix());
/// assert!(state.is_enabled(Feature::PolicySigning));
/// ```
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct RuntimeFeatureState {
    hooks: bool,
    buildfix: bool,
    policy_signing: bool,
}

impl RuntimeFeatureState {
    /// Create a new runtime feature state from explicit flags.
    pub const fn new(hooks: bool, buildfix: bool, policy_signing: bool) -> Self {
        Self {
            hooks,
            buildfix,
            policy_signing,
        }
    }

    /// Create state from compile-time availability and disable flags.
    pub const fn from_disable_flags(
        hooks_compiled: bool,
        hooks_disabled: bool,
        buildfix_compiled: bool,
        buildfix_disabled: bool,
        policy_signing_compiled: bool,
        policy_signing_disabled: bool,
    ) -> Self {
        Self::new(
            hooks_compiled && !hooks_disabled,
            buildfix_compiled && !buildfix_disabled,
            policy_signing_compiled && !policy_signing_disabled,
        )
    }

    /// Create state by scanning CLI args for disable flags.
    pub fn from_args(
        hooks_compiled: bool,
        buildfix_compiled: bool,
        policy_signing_compiled: bool,
        args: &[String],
    ) -> Self {
        Self::from_disable_flags(
            hooks_compiled,
            has_arg(args, Feature::Hooks.disable_flag()),
            buildfix_compiled,
            has_arg(args, Feature::Buildfix.disable_flag()),
            policy_signing_compiled,
            has_arg(args, Feature::PolicySigning.disable_flag()),
        )
    }

    /// Returns whether hooks are enabled.
    pub const fn hooks(self) -> bool {
        self.hooks
    }
    /// Returns whether buildfix is enabled.
    pub const fn buildfix(self) -> bool {
        self.buildfix
    }
    /// Returns whether policy signing is enabled.
    pub const fn policy_signing(self) -> bool {
        self.policy_signing
    }

    /// Returns whether the given feature is enabled.
    pub const fn is_enabled(self, feature: Feature) -> bool {
        match feature {
            Feature::Hooks => self.hooks,
            Feature::Buildfix => self.buildfix,
            Feature::PolicySigning => self.policy_signing,
        }
    }
}

fn has_arg(args: &[String], flag: &str) -> bool {
    args.iter().any(|arg| arg == flag)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feature_catalog_contains_expected_features() {
        let all = Feature::all();
        assert_eq!(all.len(), 3);
        assert!(all.contains(&Feature::Hooks));
        assert!(all.contains(&Feature::Buildfix));
        assert!(all.contains(&Feature::PolicySigning));
    }

    #[test]
    fn feature_contract_fields_are_populated() {
        let hooks = Feature::Hooks.contract();
        assert_eq!(hooks.name, "hooks");
        assert_eq!(hooks.disable_flag, "--disable-hooks");
        assert_eq!(hooks.comment_marker, Some("### Hook Notes"));
        assert!(hooks.report_data_key.is_none());
        assert!(hooks.sidecar_file.is_none());

        let buildfix = Feature::Buildfix.contract();
        assert_eq!(buildfix.report_data_key, Some("_buildfix"));
        assert_eq!(buildfix.sidecar_file, Some("buildfix.apply.json"));

        let policy = Feature::PolicySigning.contract();
        assert_eq!(policy.report_data_key, Some("_policy_signature"));
        assert_eq!(policy.sidecar_file, Some("policy.signature.json"));
    }

    #[test]
    fn parse_feature_by_name() {
        assert_eq!(Feature::from_name("hooks"), Some(Feature::Hooks));
        assert_eq!(
            Feature::from_name("policy-signing"),
            Some(Feature::PolicySigning)
        );
        assert_eq!(Feature::from_name("buildfix"), Some(Feature::Buildfix));
        assert_eq!(Feature::from_name("unknown"), None);
    }

    #[test]
    fn runtime_state_applies_disable_flags() {
        let state = RuntimeFeatureState::from_disable_flags(true, false, false, false, true, true);
        assert!(state.hooks());
        assert!(!state.buildfix());
        assert!(!state.policy_signing());
    }

    #[test]
    fn runtime_state_parses_cli_args() {
        let args = vec![
            "--format".to_string(),
            "cockpit".to_string(),
            "--disable-buildfix".to_string(),
            "--disable-policy-signing".to_string(),
        ];
        let state = RuntimeFeatureState::from_args(true, true, true, &args);
        assert!(state.hooks());
        assert!(!state.buildfix());
        assert!(!state.policy_signing());
    }

    // ── Feature::as_str ──────────────────────────────────────────

    #[test]
    fn feature_as_str_returns_expected_names() {
        assert_eq!(Feature::Hooks.as_str(), "hooks");
        assert_eq!(Feature::Buildfix.as_str(), "buildfix");
        assert_eq!(Feature::PolicySigning.as_str(), "policy-signing");
    }

    // ── Feature::disable_flag ────────────────────────────────────

    #[test]
    fn feature_disable_flag_returns_expected_flags() {
        assert_eq!(Feature::Hooks.disable_flag(), "--disable-hooks");
        assert_eq!(Feature::Buildfix.disable_flag(), "--disable-buildfix");
        assert_eq!(
            Feature::PolicySigning.disable_flag(),
            "--disable-policy-signing"
        );
    }

    // ── Feature::is_available (compile-time gated) ───────────────

    #[test]
    fn feature_is_available_reflects_cargo_features() {
        // With default features enabled, all three should be available.
        assert_eq!(
            Feature::Hooks.is_available(),
            cfg!(feature = "feature-hooks")
        );
        assert_eq!(
            Feature::Buildfix.is_available(),
            cfg!(feature = "feature-buildfix")
        );
        assert_eq!(
            Feature::PolicySigning.is_available(),
            cfg!(feature = "feature-policy-signing")
        );
    }

    // ── Feature::from_name edge cases ────────────────────────────

    #[test]
    fn from_name_rejects_similar_but_wrong_names() {
        assert_eq!(Feature::from_name("Hooks"), None);
        assert_eq!(Feature::from_name("HOOKS"), None);
        assert_eq!(Feature::from_name("hook"), None);
        assert_eq!(Feature::from_name("build-fix"), None);
        assert_eq!(Feature::from_name("policysigning"), None);
        assert_eq!(Feature::from_name(""), None);
        assert_eq!(Feature::from_name(" hooks"), None);
        assert_eq!(Feature::from_name("hooks "), None);
    }

    // ── Feature round-trip: as_str → from_name ──────────────────

    #[test]
    fn feature_as_str_roundtrips_through_from_name() {
        for &f in Feature::all() {
            let name = f.as_str();
            let parsed = Feature::from_name(name);
            assert_eq!(parsed, Some(f), "round-trip failed for {name}");
        }
    }

    // ── Feature contract identity ────────────────────────────────

    #[test]
    fn contract_feature_field_matches_source_variant() {
        for &f in Feature::all() {
            let c = f.contract();
            assert_eq!(c.feature, f);
            assert_eq!(c.name, f.as_str());
            assert_eq!(c.disable_flag, f.disable_flag());
        }
    }

    // ── Feature contract comment markers ─────────────────────────

    #[test]
    fn all_contracts_have_comment_markers() {
        for &f in Feature::all() {
            let c = f.contract();
            assert!(
                c.comment_marker.is_some(),
                "{} should have a comment_marker",
                c.name
            );
        }
    }

    // ── RuntimeFeatureState::new direct construction ─────────────

    #[test]
    fn runtime_state_new_all_enabled() {
        let state = RuntimeFeatureState::new(true, true, true);
        assert!(state.hooks());
        assert!(state.buildfix());
        assert!(state.policy_signing());
    }

    #[test]
    fn runtime_state_new_all_disabled() {
        let state = RuntimeFeatureState::new(false, false, false);
        assert!(!state.hooks());
        assert!(!state.buildfix());
        assert!(!state.policy_signing());
    }

    #[test]
    fn runtime_state_new_mixed() {
        let state = RuntimeFeatureState::new(true, false, true);
        assert!(state.hooks());
        assert!(!state.buildfix());
        assert!(state.policy_signing());
    }

    // ── RuntimeFeatureState::is_enabled ──────────────────────────

    #[test]
    fn is_enabled_matches_individual_accessors() {
        let state = RuntimeFeatureState::new(true, false, true);
        assert_eq!(state.is_enabled(Feature::Hooks), state.hooks());
        assert_eq!(state.is_enabled(Feature::Buildfix), state.buildfix());
        assert_eq!(
            state.is_enabled(Feature::PolicySigning),
            state.policy_signing()
        );
    }

    #[test]
    fn is_enabled_all_features_when_all_enabled() {
        let state = RuntimeFeatureState::new(true, true, true);
        for &f in Feature::all() {
            assert!(state.is_enabled(f), "{} should be enabled", f.as_str());
        }
    }

    #[test]
    fn is_enabled_no_features_when_all_disabled() {
        let state = RuntimeFeatureState::new(false, false, false);
        for &f in Feature::all() {
            assert!(!state.is_enabled(f), "{} should be disabled", f.as_str());
        }
    }

    // ── from_disable_flags edge cases ────────────────────────────

    #[test]
    fn from_disable_flags_not_compiled_ignores_disable() {
        // Even if disable is false, not-compiled means disabled.
        let state =
            RuntimeFeatureState::from_disable_flags(false, false, false, false, false, false);
        assert!(!state.hooks());
        assert!(!state.buildfix());
        assert!(!state.policy_signing());
    }

    #[test]
    fn from_disable_flags_compiled_and_disabled() {
        let state = RuntimeFeatureState::from_disable_flags(true, true, true, true, true, true);
        assert!(!state.hooks());
        assert!(!state.buildfix());
        assert!(!state.policy_signing());
    }

    #[test]
    fn from_disable_flags_compiled_not_disabled() {
        let state = RuntimeFeatureState::from_disable_flags(true, false, true, false, true, false);
        assert!(state.hooks());
        assert!(state.buildfix());
        assert!(state.policy_signing());
    }

    // ── from_args edge cases ─────────────────────────────────────

    #[test]
    fn from_args_empty_args_enables_all_compiled() {
        let args: Vec<String> = vec![];
        let state = RuntimeFeatureState::from_args(true, true, true, &args);
        assert!(state.hooks());
        assert!(state.buildfix());
        assert!(state.policy_signing());
    }

    #[test]
    fn from_args_all_disabled() {
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

    #[test]
    fn from_args_not_compiled_with_no_disable_stays_off() {
        let args: Vec<String> = vec![];
        let state = RuntimeFeatureState::from_args(false, false, false, &args);
        assert!(!state.hooks());
        assert!(!state.buildfix());
        assert!(!state.policy_signing());
    }

    #[test]
    fn from_args_ignores_unrelated_flags() {
        let args: Vec<String> = vec!["--verbose".into(), "--output".into(), "json".into()];
        let state = RuntimeFeatureState::from_args(true, true, true, &args);
        assert!(state.hooks());
        assert!(state.buildfix());
        assert!(state.policy_signing());
    }

    #[test]
    fn from_args_partial_flag_does_not_match() {
        let args: Vec<String> = vec!["--disable-hook".into()]; // missing 's'
        let state = RuntimeFeatureState::from_args(true, true, true, &args);
        assert!(state.hooks()); // should still be enabled
    }

    // ── Trait derivations ────────────────────────────────────────

    #[test]
    fn feature_clone_and_eq() {
        let a = Feature::Hooks;
        #[expect(
            clippy::clone_on_copy,
            reason = "This test documents clone behavior for a Copy type."
        )]
        let b = a.clone();
        assert_eq!(a, b);
        assert_ne!(Feature::Hooks, Feature::Buildfix);
    }

    #[test]
    fn feature_debug_format() {
        let dbg = format!("{:?}", Feature::Hooks);
        assert!(dbg.contains("Hooks"), "Debug output: {dbg}");
    }

    #[test]
    fn runtime_state_clone_and_eq() {
        let a = RuntimeFeatureState::new(true, false, true);
        #[expect(
            clippy::clone_on_copy,
            reason = "This test documents clone behavior for a Copy type."
        )]
        let b = a.clone();
        assert_eq!(a, b);

        let c = RuntimeFeatureState::new(true, true, true);
        assert_ne!(a, c);
    }

    #[test]
    fn runtime_state_debug_format() {
        let state = RuntimeFeatureState::new(true, false, true);
        let dbg = format!("{:?}", state);
        assert!(dbg.contains("RuntimeFeatureState"), "Debug output: {dbg}");
    }

    #[test]
    fn feature_contract_debug_and_eq() {
        let a = Feature::Hooks.contract();
        let b = Feature::Hooks.contract();
        assert_eq!(a, b);

        let c = Feature::Buildfix.contract();
        assert_ne!(a, c);
    }

    // ── has_arg helper (tested indirectly) ───────────────────────

    #[test]
    fn has_arg_with_duplicates_still_matches() {
        let args: Vec<String> = vec!["--disable-hooks".into(), "--disable-hooks".into()];
        let state = RuntimeFeatureState::from_args(true, true, true, &args);
        assert!(!state.hooks());
        assert!(state.buildfix());
    }

    // ── Feature::all is exhaustive ───────────────────────────────

    #[test]
    fn feature_all_covers_every_from_name_variant() {
        let names = ["hooks", "buildfix", "policy-signing"];
        for name in &names {
            let f = Feature::from_name(name).expect("should parse");
            assert!(Feature::all().contains(&f), "{name} not in Feature::all()");
        }
        assert_eq!(Feature::all().len(), names.len());
    }

    // ── FeatureContract field coverage ───────────────────────────

    #[test]
    fn policy_signing_contract_details() {
        let c = Feature::PolicySigning.contract();
        assert_eq!(c.name, "policy-signing");
        assert_eq!(c.disable_flag, "--disable-policy-signing");
        assert_eq!(c.comment_marker, Some("### Policy Signature"));
        assert_eq!(c.report_data_key, Some("_policy_signature"));
        assert_eq!(c.sidecar_file, Some("policy.signature.json"));
    }

    #[test]
    fn hooks_contract_has_no_sidecar_or_data_key() {
        let c = Feature::Hooks.contract();
        assert!(c.report_data_key.is_none());
        assert!(c.sidecar_file.is_none());
    }
}
