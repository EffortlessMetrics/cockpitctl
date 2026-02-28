//! Fuzz target for PR comment rendering from CockpitReport.
//!
//! This target attempts to deserialize arbitrary bytes as a CockpitReport,
//! then renders a markdown comment. The goal is to find any panics or crashes
//! in the rendering logic.
//!
//! Run with: cargo +nightly fuzz run render_comment

#![no_main]

use libfuzzer_sys::fuzz_target;
use cockpitctl_types::{CockpitConfig, CockpitReport};
use cockpitctl_render::render_comment;

fuzz_target!(|data: &[u8]| {
    // Comment rendering must never panic on any valid CockpitReport.
    // We only care that it doesn't crash; errors are expected for invalid input.
    if let Ok(report) = serde_json::from_slice::<CockpitReport>(data) {
        // Use default config for deterministic fuzzing
        let cfg = CockpitConfig::default();
        // Rendering should never panic
        let _ = render_comment(&report, &cfg);
    }
});
