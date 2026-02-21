//! Shared feature flag model for runtime availability and runtime-disable state.

/// Metadata describing how a feature maps to CLI/runtime artifacts.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct FeatureContract {
    pub feature: Feature,
    pub name: &'static str,
    pub disable_flag: &'static str,
    /// Comment marker contributed when the feature renders markdown sections.
    pub comment_marker: Option<&'static str>,
    /// Optional report `data` key this feature emits.
    pub report_data_key: Option<&'static str>,
    /// Optional sidecar file this feature emits.
    pub sidecar_file: Option<&'static str>,
}

/// Features that can be conditionally present at runtime.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Feature {
    Hooks,
    Buildfix,
    PolicySigning,
}

impl Feature {
    pub const fn all() -> &'static [Feature] {
        &[Feature::Hooks, Feature::Buildfix, Feature::PolicySigning]
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Hooks => "hooks",
            Self::Buildfix => "buildfix",
            Self::PolicySigning => "policy-signing",
        }
    }

    pub const fn disable_flag(self) -> &'static str {
        match self {
            Self::Hooks => "--disable-hooks",
            Self::Buildfix => "--disable-buildfix",
            Self::PolicySigning => "--disable-policy-signing",
        }
    }

    pub const fn is_available(self) -> bool {
        match self {
            Self::Hooks => cfg!(feature = "feature-hooks"),
            Self::Buildfix => cfg!(feature = "feature-buildfix"),
            Self::PolicySigning => cfg!(feature = "feature-policy-signing"),
        }
    }

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
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct RuntimeFeatureState {
    hooks: bool,
    buildfix: bool,
    policy_signing: bool,
}

impl RuntimeFeatureState {
    pub const fn new(hooks: bool, buildfix: bool, policy_signing: bool) -> Self {
        Self {
            hooks,
            buildfix,
            policy_signing,
        }
    }

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

    pub const fn hooks(self) -> bool {
        self.hooks
    }
    pub const fn buildfix(self) -> bool {
        self.buildfix
    }
    pub const fn policy_signing(self) -> bool {
        self.policy_signing
    }

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
}
