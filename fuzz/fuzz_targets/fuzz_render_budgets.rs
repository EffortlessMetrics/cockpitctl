//! Fuzz target for PR comment rendering with varied budget configurations.
//!
//! This target deserializes arbitrary bytes as a CockpitReport, then
//! renders comments with various budget settings (max_highlights,
//! max_per_sensor_findings, max_annotations). The goal is to find panics
//! in the rendering logic under extreme or unusual budget constraints.
//!
//! Run with: cargo +nightly fuzz run fuzz_render_budgets

#![no_main]

use cockpitctl_render::render_comment;
use cockpitctl_types::{CockpitConfig, CockpitReport, Policy};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let report: CockpitReport = match serde_json::from_slice(data) {
        Ok(r) => r,
        Err(_) => return,
    };

    // Render with default config.
    let cfg = CockpitConfig::default();
    let _ = render_comment(&report, &cfg);

    // Render with zero budgets — must never panic.
    let zero_cfg = CockpitConfig {
        policy: Policy {
            max_highlights: 0,
            max_per_sensor_findings: 0,
            max_annotations: 0,
            ..Policy::default()
        },
        ..CockpitConfig::default()
    };
    let _ = render_comment(&report, &zero_cfg);

    // Render with very large budgets.
    let large_cfg = CockpitConfig {
        policy: Policy {
            max_highlights: 10_000,
            max_per_sensor_findings: 10_000,
            max_annotations: 10_000,
            ..Policy::default()
        },
        ..CockpitConfig::default()
    };
    let _ = render_comment(&report, &large_cfg);

    // Render with warn_is_fail enabled.
    let strict_cfg = CockpitConfig {
        policy: Policy {
            warn_is_fail: true,
            ..Policy::default()
        },
        ..CockpitConfig::default()
    };
    let _ = render_comment(&report, &strict_cfg);
});
