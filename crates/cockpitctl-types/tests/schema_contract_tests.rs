//! Contract tests for embedded JSON schemas in cockpitctl-types.
//!
//! Verifies that every embedded schema is valid JSON Schema (Draft 2020-12),
//! contains the expected meta-fields, and can validate conforming / non-conforming
//! documents.

use serde_json::Value;

// ═══════════════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════════════

fn parse_schema(raw: &str) -> Value {
    serde_json::from_str(raw).expect("embedded schema must be valid JSON")
}

fn compile_validator(schema: &Value) -> jsonschema::Validator {
    jsonschema::validator_for(schema).expect("embedded schema must compile as JSON Schema")
}

fn validates(validator: &jsonschema::Validator, doc: &Value) -> bool {
    validator.validate(doc).is_ok()
}

// ═══════════════════════════════════════════════════════════════════════
// 1–4. Each embedded schema is valid JSON Schema
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn embedded_sensor_report_v1_is_valid_json_schema() {
    let schema = parse_schema(cockpitctl_types::SENSOR_REPORT_V1_SCHEMA_JSON);
    let _ = compile_validator(&schema);
}

#[test]
fn embedded_cockpit_report_v1_is_valid_json_schema() {
    let schema = parse_schema(cockpitctl_types::COCKPIT_REPORT_V1_SCHEMA_JSON);
    let _ = compile_validator(&schema);
}

#[test]
fn embedded_buildfix_plan_v1_is_valid_json_schema() {
    let schema = parse_schema(cockpitctl_types::BUILDFIX_PLAN_V1_SCHEMA_JSON);
    let _ = compile_validator(&schema);
}

#[test]
fn embedded_cockpit_promote_v1_is_valid_json_schema() {
    let schema = parse_schema(cockpitctl_types::COCKPIT_PROMOTE_V1_SCHEMA_JSON);
    let _ = compile_validator(&schema);
}

// ═══════════════════════════════════════════════════════════════════════
// 5. Each schema has required "$schema" and "type" fields
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn sensor_report_v1_has_schema_and_type_fields() {
    let schema = parse_schema(cockpitctl_types::SENSOR_REPORT_V1_SCHEMA_JSON);
    assert_eq!(
        schema["$schema"],
        "https://json-schema.org/draft/2020-12/schema"
    );
    assert_eq!(schema["type"], "object");
}

#[test]
fn cockpit_report_v1_has_schema_and_type_fields() {
    let schema = parse_schema(cockpitctl_types::COCKPIT_REPORT_V1_SCHEMA_JSON);
    assert_eq!(
        schema["$schema"],
        "https://json-schema.org/draft/2020-12/schema"
    );
    assert_eq!(schema["type"], "object");
}

#[test]
fn buildfix_plan_v1_has_schema_and_type_fields() {
    let schema = parse_schema(cockpitctl_types::BUILDFIX_PLAN_V1_SCHEMA_JSON);
    assert_eq!(
        schema["$schema"],
        "https://json-schema.org/draft/2020-12/schema"
    );
    assert_eq!(schema["type"], "object");
}

#[test]
fn cockpit_promote_v1_has_schema_and_type_fields() {
    let schema = parse_schema(cockpitctl_types::COCKPIT_PROMOTE_V1_SCHEMA_JSON);
    assert_eq!(
        schema["$schema"],
        "https://json-schema.org/draft/2020-12/schema"
    );
    assert_eq!(schema["type"], "object");
}

// ═══════════════════════════════════════════════════════════════════════
// 6. Schemas can validate conforming documents
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn sensor_report_v1_accepts_conforming_document() {
    let schema = parse_schema(cockpitctl_types::SENSOR_REPORT_V1_SCHEMA_JSON);
    let v = compile_validator(&schema);
    let doc: Value = serde_json::from_str(
        r#"{
          "schema": "sensor.report.v1",
          "tool": { "name": "clippy", "version": "0.1.0" },
          "run": { "started_at": "2026-01-01T00:00:00Z" },
          "verdict": { "status": "pass", "counts": { "info": 0, "warn": 0, "error": 0 } },
          "findings": []
        }"#,
    )
    .unwrap();
    assert!(validates(&v, &doc), "minimal sensor report must validate");
}

#[test]
fn cockpit_report_v1_accepts_conforming_document() {
    let schema = parse_schema(cockpitctl_types::COCKPIT_REPORT_V1_SCHEMA_JSON);
    let v = compile_validator(&schema);
    let doc: Value = serde_json::from_str(
        r#"{
          "schema": "cockpit.report.v1",
          "tool": { "name": "cockpitctl", "version": "0.1.0" },
          "run": { "started_at": "2026-01-01T00:00:00Z" },
          "verdict": { "status": "pass", "counts": { "info": 0, "warn": 0, "error": 0 } },
          "sensors": [],
          "highlights": [],
          "policy": {
            "warn_is_fail": false,
            "max_highlights": 5,
            "max_per_sensor_findings": 10,
            "section_order": [],
            "sensors": []
          }
        }"#,
    )
    .unwrap();
    assert!(validates(&v, &doc), "minimal cockpit report must validate");
}

#[test]
fn buildfix_plan_v1_accepts_conforming_document() {
    let schema = parse_schema(cockpitctl_types::BUILDFIX_PLAN_V1_SCHEMA_JSON);
    let v = compile_validator(&schema);
    let doc: Value = serde_json::from_str(
        r#"{
          "schema": "buildfix.plan.v1",
          "tool": { "name": "buildfix", "version": "1.0.0" },
          "fixes": []
        }"#,
    )
    .unwrap();
    assert!(validates(&v, &doc), "minimal buildfix plan must validate");
}

#[test]
fn cockpit_promote_v1_accepts_conforming_document() {
    let schema = parse_schema(cockpitctl_types::COCKPIT_PROMOTE_V1_SCHEMA_JSON);
    let v = compile_validator(&schema);
    let doc: Value = serde_json::from_str(
        r#"{
          "schema": "cockpit.promote.v1",
          "cards": [{ "id": "c1", "label": "Coverage", "value": "80%" }],
          "suggested_highlights": [{ "finding_fingerprint": "abc123" }],
          "suggested_artifacts": [{ "artifact_id": "log" }]
        }"#,
    )
    .unwrap();
    assert!(validates(&v, &doc), "promote document must validate");
}

// ═══════════════════════════════════════════════════════════════════════
// 7. Schemas reject non-conforming documents
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn sensor_report_v1_rejects_empty_object() {
    let schema = parse_schema(cockpitctl_types::SENSOR_REPORT_V1_SCHEMA_JSON);
    let v = compile_validator(&schema);
    let doc: Value = serde_json::json!({});
    assert!(!validates(&v, &doc), "empty object must be rejected");
}

#[test]
fn cockpit_report_v1_rejects_missing_policy() {
    let schema = parse_schema(cockpitctl_types::COCKPIT_REPORT_V1_SCHEMA_JSON);
    let v = compile_validator(&schema);
    let doc: Value = serde_json::from_str(
        r#"{
          "schema": "cockpit.report.v1",
          "tool": { "name": "cockpitctl", "version": "0.1.0" },
          "run": { "started_at": "2026-01-01T00:00:00Z" },
          "verdict": { "status": "pass", "counts": { "info": 0, "warn": 0, "error": 0 } },
          "sensors": [],
          "highlights": []
        }"#,
    )
    .unwrap();
    assert!(!validates(&v, &doc), "missing policy must be rejected");
}

#[test]
fn buildfix_plan_v1_rejects_missing_fixes() {
    let schema = parse_schema(cockpitctl_types::BUILDFIX_PLAN_V1_SCHEMA_JSON);
    let v = compile_validator(&schema);
    let doc: Value = serde_json::from_str(
        r#"{
          "schema": "buildfix.plan.v1",
          "tool": { "name": "buildfix", "version": "1.0.0" }
        }"#,
    )
    .unwrap();
    assert!(!validates(&v, &doc), "missing fixes must be rejected");
}

#[test]
fn cockpit_promote_v1_rejects_extra_properties() {
    let schema = parse_schema(cockpitctl_types::COCKPIT_PROMOTE_V1_SCHEMA_JSON);
    let v = compile_validator(&schema);
    let doc: Value = serde_json::json!({
        "schema": "cockpit.promote.v1",
        "cards": [],
        "unknown_field": true
    });
    assert!(
        !validates(&v, &doc),
        "additionalProperties must reject unknown fields"
    );
}

// ═══════════════════════════════════════════════════════════════════════
// 8. Schema version fields match expectations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn schema_titles_match_expected_versions() {
    let sensor = parse_schema(cockpitctl_types::SENSOR_REPORT_V1_SCHEMA_JSON);
    assert_eq!(sensor["title"], "sensor.report.v1");

    let cockpit = parse_schema(cockpitctl_types::COCKPIT_REPORT_V1_SCHEMA_JSON);
    assert_eq!(cockpit["title"], "cockpit.report.v1");

    let buildfix = parse_schema(cockpitctl_types::BUILDFIX_PLAN_V1_SCHEMA_JSON);
    assert_eq!(buildfix["title"], "buildfix.plan.v1");

    let promote = parse_schema(cockpitctl_types::COCKPIT_PROMOTE_V1_SCHEMA_JSON);
    assert_eq!(promote["title"], "cockpit.promote.v1");
}

#[test]
fn cockpit_report_schema_const_enforced() {
    let schema = parse_schema(cockpitctl_types::COCKPIT_REPORT_V1_SCHEMA_JSON);
    assert_eq!(
        schema["properties"]["schema"]["const"], "cockpit.report.v1",
        "cockpit schema must enforce const on 'schema' field"
    );
}

#[test]
fn buildfix_plan_schema_const_enforced() {
    let schema = parse_schema(cockpitctl_types::BUILDFIX_PLAN_V1_SCHEMA_JSON);
    assert_eq!(
        schema["properties"]["schema"]["const"], "buildfix.plan.v1",
        "buildfix schema must enforce const on 'schema' field"
    );
}
