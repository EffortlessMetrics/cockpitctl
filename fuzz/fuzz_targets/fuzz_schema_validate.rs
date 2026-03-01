//! Fuzz target for JSON schema validation with arbitrary input.
//!
//! This target exercises schema validation of sensor receipts and cockpit
//! reports against the embedded JSON schemas. The goal is to ensure the
//! validation logic never panics on any input.
//!
//! Run with: cargo +nightly fuzz run fuzz_schema_validate

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Schema validation must never panic on any input.
    if let Ok(text) = std::str::from_utf8(data) {
        // Validate as a sensor receipt against sensor.report.v1 schema
        let checks = cockpitctl_conform::ConformChecks {
            path_hygiene: true,
            ordering: true,
            reason_lint: true,
            survivability: true,
            tool_error_identity: true,
            sensor_id_format: true,
            artifact_pointers: true,
        };
        let _ = cockpitctl_conform::conform_single(text, "fuzz-sensor", &checks);

        // Validate as a cockpit report against cockpit.report.v1 schema
        let _ = cockpitctl_conform::validate_cockpit_schema(text);
    }
});
