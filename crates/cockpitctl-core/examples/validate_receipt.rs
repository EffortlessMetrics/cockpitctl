//! Example: validate a sensor receipt against the JSON schema.
//!
//! Shows how to load a receipt from JSON, validate it using the embedded
//! `sensor.report.v1` schema, and inspect the result.
//!
//! Run with:
//! ```sh
//! cargo run -p cockpitctl-core --example validate_receipt
//! ```

use cockpitctl_core::SchemaValidator;
use cockpitctl_core::ingest::SchemaValidationResult;
use cockpitctl_core::io_schema::JsonSchemaValidator;

fn main() {
    // Build the validator from the embedded sensor.report.v1 schema.
    let validator = JsonSchemaValidator::sensor_report_v1().expect("load embedded schema");

    // --- Valid receipt ---
    let valid_json = r#"{
      "schema": "sensor.report.v1",
      "tool": { "name": "clippy", "version": "0.1.85" },
      "run":  { "started_at": "2026-01-15T10:00:00Z" },
      "verdict": {
        "status": "warn",
        "counts": { "info": 0, "warn": 2, "error": 0 },
        "reasons": ["unused_imports"]
      },
      "findings": [
        {
          "severity": "warn",
          "code": "unused_import",
          "message": "unused import `std::io`",
          "location": { "path": "src/main.rs", "line": 3 }
        },
        {
          "severity": "warn",
          "code": "unused_import",
          "message": "unused import `std::fs`",
          "location": { "path": "src/lib.rs", "line": 1 }
        }
      ]
    }"#;

    println!("=== Validating a correct receipt ===");
    print_validation(&validator, valid_json);

    // --- Invalid receipt (missing required fields) ---
    let invalid_json = r#"{
      "schema": "sensor.report.v1",
      "tool": { "name": "oops" },
      "findings": []
    }"#;

    println!("\n=== Validating an invalid receipt ===");
    print_validation(&validator, invalid_json);

    // --- Malformed JSON ---
    let malformed = "{ not valid json }";

    println!("\n=== Validating malformed JSON ===");
    print_validation(&validator, malformed);
}

fn print_validation(validator: &JsonSchemaValidator, json: &str) {
    match validator.validate_receipt(json.as_bytes()) {
        Ok(SchemaValidationResult::Valid) => {
            println!("  Result: VALID ✅");
        }
        Ok(SchemaValidationResult::Invalid(errors)) => {
            println!("  Result: INVALID ❌  ({} error(s))", errors.len());
            for (i, err) in errors.iter().enumerate() {
                println!("    {}. {}", i + 1, err);
            }
        }
        Err(e) => {
            println!("  Result: ERROR — {:#}", e);
        }
    }
}
