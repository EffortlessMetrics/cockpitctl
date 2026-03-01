//! Conformance checking library for cockpitctl sensor receipts.
//!
//! Provides structured validation of sensor reports against the protocol,
//! including schema validation, path hygiene, ordering, reason tokens, and more.
//!
//! # Examples
//!
//! ```
//! use cockpitctl_conform::{conform_single, ConformChecks};
//!
//! let json = r#"{
//!   "schema": "sensor.report.v1",
//!   "tool": { "name": "test", "version": "1.0.0" },
//!   "run": { "started_at": "2026-01-01T00:00:00Z" },
//!   "verdict": { "status": "pass", "counts": { "info": 0, "warn": 0, "error": 0 } },
//!   "findings": []
//! }"#;
//!
//! let checks = ConformChecks {
//!     path_hygiene: true,
//!     ordering: true,
//!     reason_lint: true,
//!     survivability: true,
//!     tool_error_identity: true,
//!     sensor_id_format: true,
//!     artifact_pointers: true,
//! };
//!
//! let result = conform_single(json, "test-sensor", &checks).unwrap();
//! assert!(result.is_pass());
//! ```

pub mod checks;
pub mod single;

pub use checks::{
    check_artifact_pointers, check_cockpit_reason_tokens, check_determinism, check_ordering,
    check_path_hygiene, check_presence_semantics, check_reason_tokens, check_sensor_id_format,
    check_tool_error_identity, is_valid_reason_token,
};
pub use single::{
    ConformChecks, ConformResult, Violation, check_cockpit_extended, conform_single,
    validate_cockpit_schema,
};
