//! Focused domain catalog for cockpit finding codes and explanations.

#![warn(missing_docs)]

/// A cockpit-level code for synthesized findings.
pub mod cockpit_codes {
    /// Missing sensor receipt.
    pub const MISSING_RECEIPT: &str = "cockpit.missing_receipt";
    /// Invalid or unparseable receipt.
    pub const INVALID_RECEIPT: &str = "cockpit.invalid_receipt";
    /// Receipt failed schema validation.
    pub const SCHEMA_VIOLATION: &str = "cockpit.schema_violation";
    /// Receipt has inconsistent data.
    pub const RECEIPT_INCONSISTENT: &str = "cockpit.receipt_inconsistent";
    /// Sensor count exceeded the cap.
    pub const SENSORS_TRUNCATED: &str = "cockpit.sensors_truncated";
    /// Path traversal detected in sensor ID.
    pub const PATH_TRAVERSAL: &str = "cockpit.path_traversal";
    /// Receipt file exceeded size limit.
    pub const RECEIPT_OVERSIZED: &str = "cockpit.receipt_oversized";
}

/// Explanation of a cockpit finding code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeExplanation {
    /// The cockpit finding code.
    pub code: &'static str,
    /// Short human-readable title.
    pub title: &'static str,
    /// Detailed explanation.
    pub description: &'static str,
    /// Common cause of this finding.
    pub cause: &'static str,
    /// Suggested remediation.
    pub fix: &'static str,
}

/// Look up an explanation for a cockpit finding code.
pub fn explain_code(code: &str) -> Option<CodeExplanation> {
    all_codes().into_iter().find(|e| e.code == code)
}

/// Return all known cockpit finding codes with explanations.
pub fn all_codes() -> Vec<CodeExplanation> {
    vec![
        CodeExplanation {
            code: cockpit_codes::MISSING_RECEIPT,
            title: "Missing Receipt",
            description: "A sensor declared in cockpit.toml did not produce a receipt file.",
            cause: "The sensor either did not run, failed before writing output, or wrote to the wrong path.",
            fix: "Ensure the sensor ran and wrote artifacts/<sensor>/report.json. Check the sensor's logs for errors.",
        },
        CodeExplanation {
            code: cockpit_codes::INVALID_RECEIPT,
            title: "Invalid Receipt",
            description: "A sensor receipt file exists but could not be parsed as valid JSON.",
            cause: "The sensor wrote malformed JSON (syntax error, truncated output, or binary data).",
            fix: "Validate the receipt file with `cockpitctl validate --input <path>`. Fix the sensor's output format.",
        },
        CodeExplanation {
            code: cockpit_codes::SCHEMA_VIOLATION,
            title: "Schema Violation",
            description: "A sensor receipt is valid JSON but does not conform to the sensor.report.v1 schema.",
            cause: "The receipt is missing required fields, has wrong types, or includes disallowed properties.",
            fix: "Run `cockpitctl validate --input <path> --strict` to see specific violations. Update the sensor to match the schema.",
        },
        CodeExplanation {
            code: cockpit_codes::RECEIPT_INCONSISTENT,
            title: "Receipt Inconsistent",
            description: "The verdict counts in a receipt do not match the actual findings array.",
            cause: "The sensor reported different counts (info/warn/error) than what the findings array contains.",
            fix: "Update the sensor to compute verdict counts from the findings array, or fix the findings array.",
        },
        CodeExplanation {
            code: cockpit_codes::SENSORS_TRUNCATED,
            title: "Sensors Truncated",
            description: "More sensor directories were found than the safety limit allows.",
            cause: "The artifacts directory contains more sensor directories than max_receipts (default 100).",
            fix: "Review why so many sensors exist. Increase max_receipts if legitimate, or clean up stale sensor directories.",
        },
        CodeExplanation {
            code: cockpit_codes::PATH_TRAVERSAL,
            title: "Path Traversal Rejected",
            description: "A sensor ID or artifact path attempted to escape the artifacts root directory.",
            cause: "A sensor ID contains `..`, `/`, `\\`, or other unsafe path characters.",
            fix: "Ensure sensor IDs contain only alphanumeric characters, hyphens, and underscores.",
        },
        CodeExplanation {
            code: cockpit_codes::RECEIPT_OVERSIZED,
            title: "Receipt Oversized",
            description: "A sensor receipt exceeds the maximum allowed file size.",
            cause: "The receipt file is larger than max_receipt_size_bytes (default 2MB).",
            fix: "Reduce the receipt size (fewer findings, smaller payloads) or increase max_receipt_size_bytes in cockpit.toml.",
        },
    ]
}
