//! Runtime feature-toggle evaluation helpers.

#![warn(missing_docs)]

use cockpitctl_feature_state::Feature;

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
    fn runtime_present_no_args() {
        assert!(feature_runtime_present::<&str>(Feature::Hooks, &[]));
        assert!(feature_runtime_present::<&str>(Feature::Buildfix, &[]));
        assert!(feature_runtime_present::<&str>(Feature::PolicySigning, &[]));
    }

    #[test]
    fn runtime_present_with_own_disable_flag() {
        assert!(!feature_runtime_present(
            Feature::Hooks,
            &["--disable-hooks"]
        ));
        assert!(!feature_runtime_present(
            Feature::Buildfix,
            &["--disable-buildfix"]
        ));
        assert!(!feature_runtime_present(
            Feature::PolicySigning,
            &["--disable-policy-signing"]
        ));
    }

    #[test]
    fn parse_feature_state_variants() {
        assert_eq!(parse_feature_state("present"), Some(true));
        assert_eq!(parse_feature_state("enabled"), Some(true));
        assert_eq!(parse_feature_state("on"), Some(true));
        assert_eq!(parse_feature_state("absent"), Some(false));
        assert_eq!(parse_feature_state("disabled"), Some(false));
        assert_eq!(parse_feature_state("off"), Some(false));
    }

    #[test]
    fn parse_feature_state_unknown_returns_none() {
        assert_eq!(parse_feature_state("weird"), None);
    }
}
