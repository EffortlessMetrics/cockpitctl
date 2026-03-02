//! Runtime feature-toggle evaluation and BDD token parsing helpers.

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
    fn runtime_present_extra_noise_args_ignored() {
        let args = ["--verbose", "--config=foo.toml", "--disable-hooks"];
        assert!(!feature_runtime_present(Feature::Hooks, &args));
        assert!(feature_runtime_present(Feature::Buildfix, &args));
    }

    #[test]
    fn parse_feature_state_case_insensitive() {
        assert_eq!(parse_feature_state("PRESENT"), Some(true));
        assert_eq!(parse_feature_state("Absent"), Some(false));
    }
}
