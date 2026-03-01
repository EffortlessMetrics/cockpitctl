//! Fuzz target for sensor ID validation and path traversal checks.
//!
//! This target exercises `is_valid_sensor_id` and `check_sensor_id_format`
//! with arbitrary byte strings. The goal is to ensure these safety-critical
//! functions never panic on any input.
//!
//! Run with: cargo +nightly fuzz run fuzz_sensor_id

#![no_main]

use cockpitctl_types::is_valid_sensor_id;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Sensor ID validation must never panic on any input.
    if let Ok(s) = std::str::from_utf8(data) {
        // is_valid_sensor_id is the primary safety gate for path traversal
        let _ = is_valid_sensor_id(s);

        // check_sensor_id_format in the conform crate must agree: never panic
        let _ = cockpitctl_conform::check_sensor_id_format(s);

        // Also fuzz the path-hygiene helper with a synthetic single-finding report
        if s.len() < 512 {
            let json = format!(
                r#"{{
                    "schema": "sensor.report.v1",
                    "tool": {{ "name": "fuzz", "version": "0.0.1" }},
                    "run": {{ "started_at": "2026-01-01T00:00:00Z" }},
                    "verdict": {{ "status": "pass", "counts": {{ "info": 0, "warn": 0, "error": 0 }} }},
                    "findings": [{{
                        "severity": "info",
                        "code": "FUZZ",
                        "message": "fuzz",
                        "location": {{ "path": {} }}
                    }}],
                    "artifacts": []
                }}"#,
                serde_json::to_string(s).unwrap_or_default()
            );
            if let Ok(report) = serde_json::from_str::<cockpitctl_types::SensorReport>(&json) {
                let _ = cockpitctl_conform::check_path_hygiene(&report);
            }
        }
    }
});
