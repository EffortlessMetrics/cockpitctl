//! Fuzz target for SensorReport JSON parsing.
//!
//! This target attempts to deserialize arbitrary bytes as a SensorReport.
//! The goal is to find any panics or crashes in the parsing logic.
//!
//! Run with: cargo +nightly fuzz run parse_receipt

#![no_main]

use libfuzzer_sys::fuzz_target;
use cockpitctl_types::SensorReport;

fuzz_target!(|data: &[u8]| {
    // Receipt parsing must never panic on any input.
    // We only care that it doesn't crash; errors are expected for invalid input.
    let _ = serde_json::from_slice::<SensorReport>(data);

    // If parsing succeeded, also verify serialization round-trip doesn't panic
    if let Ok(report) = serde_json::from_slice::<SensorReport>(data) {
        // Serialization should never panic on valid data
        let _ = serde_json::to_string(&report);
        let _ = serde_json::to_vec(&report);
    }
});
