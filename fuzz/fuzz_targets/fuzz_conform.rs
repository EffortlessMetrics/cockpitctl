//! Fuzz target for the full conformance checking suite.
//!
//! This target exercises `conform_single` with all checks enabled,
//! plus individual check functions like `check_path_hygiene`,
//! `check_ordering`, `check_reason_tokens`, and `check_artifact_pointers`.
//!
//! Run with: cargo +nightly fuzz run fuzz_conform

#![no_main]

use cockpitctl_conform::{
    check_artifact_pointers, check_ordering, check_path_hygiene, check_reason_tokens,
    check_sensor_id_format, check_tool_error_identity, conform_single, ConformChecks,
};
use cockpitctl_types::SensorReport;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let text = match std::str::from_utf8(data) {
        Ok(s) => s,
        Err(_) => return,
    };

    // Full conformance suite — must never panic.
    let all_checks = ConformChecks {
        path_hygiene: true,
        ordering: true,
        reason_lint: true,
        survivability: true,
        tool_error_identity: true,
        sensor_id_format: true,
        artifact_pointers: true,
    };
    let _ = conform_single(text, "fuzz-sensor", &all_checks);

    // Also exercise with a subset of checks disabled.
    let partial_checks = ConformChecks {
        path_hygiene: true,
        ordering: false,
        reason_lint: false,
        survivability: true,
        tool_error_identity: false,
        sensor_id_format: true,
        artifact_pointers: false,
    };
    let _ = conform_single(text, "fuzz-sensor", &partial_checks);

    // If we can parse as SensorReport, exercise individual checks.
    if let Ok(report) = serde_json::from_str::<SensorReport>(text) {
        let _ = check_path_hygiene(&report);
        let _ = check_ordering(&report, "fuzz-sensor");
        let _ = check_reason_tokens(&report);
        let _ = check_tool_error_identity(&report);
        let _ = check_artifact_pointers(&report);

        // Fuzz sensor_id_format with the tool name from the report.
        let _ = check_sensor_id_format(&report.tool.name);
    }
});
