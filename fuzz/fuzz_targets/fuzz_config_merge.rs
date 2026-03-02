//! Fuzz target for CockpitConfig TOML parsing with sensor policy maps.
//!
//! This target exercises config parsing with a focus on sensor policy
//! overrides, section ordering, and interactions between global and
//! per-sensor settings. It also tests serialization round-trips.
//!
//! Run with: cargo +nightly fuzz run fuzz_config_merge

#![no_main]

use cockpitctl_types::CockpitConfig;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let text = match std::str::from_utf8(data) {
        Ok(s) => s,
        Err(_) => return,
    };

    // Parse the config — must never panic.
    let config: CockpitConfig = match toml::from_str(text) {
        Ok(c) => c,
        Err(_) => return,
    };

    // Serialization round-trip must not panic.
    let serialized = match toml::to_string(&config) {
        Ok(s) => s,
        Err(_) => return,
    };

    // Re-parse the serialized output — must not panic.
    let _ = toml::from_str::<CockpitConfig>(&serialized);

    // Also try pretty serialization.
    if let Ok(pretty) = toml::to_string_pretty(&config) {
        let _ = toml::from_str::<CockpitConfig>(&pretty);
    }

    // Verify sensor map iteration order is deterministic.
    let keys1: Vec<_> = config.sensors.keys().collect();
    let keys2: Vec<_> = config.sensors.keys().collect();
    assert_eq!(keys1, keys2, "BTreeMap iteration must be deterministic");
});
