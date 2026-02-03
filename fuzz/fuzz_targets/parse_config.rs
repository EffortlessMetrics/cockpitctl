//! Fuzz target for CockpitConfig TOML parsing.
//!
//! This target attempts to deserialize arbitrary bytes as a CockpitConfig.
//! The goal is to find any panics or crashes in the TOML parsing logic.
//!
//! Run with: cargo +nightly fuzz run parse_config

#![no_main]

use libfuzzer_sys::fuzz_target;
use cockpitctl_types::CockpitConfig;

fuzz_target!(|data: &[u8]| {
    // Config parsing must never panic on any input.
    // We only care that it doesn't crash; errors are expected for invalid input.

    // Try parsing as UTF-8 first, then as TOML
    if let Ok(text) = std::str::from_utf8(data) {
        let _ = toml::from_str::<CockpitConfig>(text);

        // If parsing succeeded, also verify serialization round-trip doesn't panic
        if let Ok(config) = toml::from_str::<CockpitConfig>(text) {
            // Serialization should never panic on valid data
            let _ = toml::to_string(&config);
            let _ = toml::to_string_pretty(&config);
        }
    }
});
