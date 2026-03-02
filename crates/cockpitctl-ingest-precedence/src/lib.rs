//! Precedence helpers used by ingest orchestration.
//!
//! This crate is intentionally small and pure: it computes effective values
//! for policy/CLI precedence without touching IO.

#![warn(missing_docs)]

use cockpitctl_types::{CockpitConfig, SchemaValidation};

/// Resolve schema validation mode using the precedence contract.
///
/// `cockpit.toml` provides the default; CLI overrides only when explicitly set.
pub const fn effective_schema_validation(
    config_default: SchemaValidation,
    cli_override: Option<SchemaValidation>,
) -> SchemaValidation {
    match cli_override {
        Some(override_mode) => override_mode,
        None => config_default,
    }
}

/// Resolve expected sensors for ingest.
///
/// If policy declares sensors, those are authoritative. If not, discovered
/// sensors are treated as expected.
pub fn expected_sensors(cfg: &CockpitConfig, discovered: &[String]) -> Vec<String> {
    if cfg.sensors.is_empty() {
        return discovered.to_vec();
    }

    cfg.sensors.keys().cloned().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use cockpitctl_types::{CockpitConfig, SchemaValidation, SensorPolicy};

    #[test]
    fn schema_validation_cli_override_wins() {
        let mode =
            effective_schema_validation(SchemaValidation::Lax, Some(SchemaValidation::Strict));
        assert!(matches!(mode, SchemaValidation::Strict));
    }

    #[test]
    fn schema_validation_config_default_used_when_no_cli_override() {
        let mode = effective_schema_validation(SchemaValidation::Strict, None);
        assert!(matches!(mode, SchemaValidation::Strict));
    }

    #[test]
    fn expected_sensors_falls_back_to_discovered_when_policy_empty() {
        let cfg = CockpitConfig::default();
        let discovered = vec!["zeta".to_string(), "alpha".to_string()];
        assert_eq!(expected_sensors(&cfg, &discovered), discovered);
    }

    #[test]
    fn expected_sensors_uses_policy_declared_set_when_present() {
        let mut cfg = CockpitConfig::default();
        cfg.sensors
            .insert("alpha".to_string(), SensorPolicy::default());
        cfg.sensors
            .insert("beta".to_string(), SensorPolicy::default());

        let discovered = vec!["other".to_string()];
        let expected = expected_sensors(&cfg, &discovered);
        assert_eq!(expected, vec!["alpha".to_string(), "beta".to_string()]);
    }
}
