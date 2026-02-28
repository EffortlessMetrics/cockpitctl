//! Fuzz target for SARIF conversion from CockpitReport.
//!
//! This target attempts to deserialize arbitrary bytes as a CockpitReport,
//! then converts to SARIF format. The goal is to find any panics or crashes
//! in the conversion logic.
//!
//! Run with: cargo +nightly fuzz run sarif_convert

#![no_main]

use libfuzzer_sys::fuzz_target;
use cockpitctl_types::CockpitReport;
use cockpitctl_sarif::{cockpit_report_to_sarif, cockpit_report_to_sarif_json};

fuzz_target!(|data: &[u8]| {
    // SARIF conversion must never panic on any valid CockpitReport.
    // We only care that it doesn't crash; errors are expected for invalid input.
    if let Ok(report) = serde_json::from_slice::<CockpitReport>(data) {
        // Struct conversion should never panic
        let sarif = cockpit_report_to_sarif(&report);
        // JSON serialization of the SARIF output should never panic
        let _ = serde_json::to_string(&sarif);

        // The convenience JSON function should also never panic
        let _ = cockpit_report_to_sarif_json(&report);
    }
});
