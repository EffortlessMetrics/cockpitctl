//! Conformance checking library for cockpitctl sensor receipts.
//!
//! Provides structured validation of sensor reports against the protocol,
//! including schema validation, path hygiene, ordering, reason tokens, and more.

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
