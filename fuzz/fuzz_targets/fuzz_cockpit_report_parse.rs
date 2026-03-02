//! Fuzz target for CockpitReport JSON parsing and round-trip serialization.
//!
//! This target attempts to deserialize arbitrary bytes as a CockpitReport,
//! then verifies that serialization round-trips produce consistent output.
//! The goal is to find any panics or crashes in cockpit report parsing.
//!
//! Run with: cargo +nightly fuzz run fuzz_cockpit_report_parse

#![no_main]

use cockpitctl_types::CockpitReport;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Attempt to parse arbitrary bytes as CockpitReport
    let _ = serde_json::from_slice::<CockpitReport>(data);

    if let Ok(report) = serde_json::from_slice::<CockpitReport>(data) {
        // Round-trip: serialize and re-parse
        let json = serde_json::to_string(&report).expect("serialization must not fail");
        let reparsed: CockpitReport =
            serde_json::from_str(&json).expect("round-trip deserialization must not fail");

        // Verify deterministic serialization
        let json2 = serde_json::to_string(&reparsed).expect("re-serialization must not fail");
        assert_eq!(json, json2, "round-trip serialization must be stable");

        // Also test pretty-print path
        let _ = serde_json::to_string_pretty(&report);
        let _ = serde_json::to_vec(&report);
    }
});
