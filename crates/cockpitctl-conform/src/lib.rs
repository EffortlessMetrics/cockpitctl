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

#[cfg(test)]
mod tests {
    use super::*;
    use cockpitctl_types::*;
    use std::collections::BTreeMap;

    fn minimal_sensor_report() -> SensorReport {
        SensorReport {
            schema: "sensor.report.v1".to_string(),
            tool: ToolInfo {
                name: "test".to_string(),
                version: "1.0.0".to_string(),
                commit: None,
            },
            run: RunInfo {
                started_at: "2026-02-01T00:00:00Z".to_string(),
                ended_at: None,
                duration_ms: None,
                host: None,
                git: None,
                ci: None,
                capabilities: BTreeMap::new(),
            },
            verdict: Verdict {
                status: VerdictStatus::Pass,
                counts: VerdictCounts::default(),
                reasons: vec![],
            },
            findings: vec![],
            artifacts: vec![],
            data: None,
        }
    }

    fn all_checks() -> ConformChecks {
        ConformChecks {
            path_hygiene: true,
            ordering: true,
            reason_lint: true,
            survivability: true,
            tool_error_identity: true,
            sensor_id_format: true,
            artifact_pointers: true,
        }
    }

    // ---- Minimally valid receipt ----

    #[test]
    fn conform_minimally_valid_receipt() {
        let json = serde_json::to_string(&minimal_sensor_report()).unwrap();
        let result = conform_single(&json, "good-sensor", &all_checks()).unwrap();
        assert!(
            result.is_pass(),
            "minimal valid receipt should pass: {:?}",
            result.violations
        );
    }

    // ---- Missing required fields ----

    #[test]
    fn conform_receipt_missing_schema_field() {
        let json = serde_json::json!({
            "tool": { "name": "t", "version": "1.0.0" },
            "run": { "started_at": "2026-01-01T00:00:00Z" },
            "verdict": { "status": "pass", "counts": { "info": 0, "warn": 0, "error": 0 } },
            "findings": []
        });
        let result = conform_single(&json.to_string(), "sensor", &all_checks()).unwrap();
        assert!(!result.is_pass());
        assert!(result.violations.iter().any(|v| v.check == "schema"));
    }

    #[test]
    fn conform_receipt_missing_tool_field() {
        let json = serde_json::json!({
            "schema": "sensor.report.v1",
            "run": { "started_at": "2026-01-01T00:00:00Z" },
            "verdict": { "status": "pass", "counts": { "info": 0, "warn": 0, "error": 0 } },
            "findings": []
        });
        let result = conform_single(&json.to_string(), "sensor", &all_checks()).unwrap();
        assert!(!result.is_pass());
        assert!(result.violations.iter().any(|v| v.check == "schema"));
    }

    #[test]
    fn conform_receipt_missing_verdict_field() {
        let json = serde_json::json!({
            "schema": "sensor.report.v1",
            "tool": { "name": "t", "version": "1.0.0" },
            "run": { "started_at": "2026-01-01T00:00:00Z" },
            "findings": []
        });
        let result = conform_single(&json.to_string(), "sensor", &all_checks()).unwrap();
        assert!(!result.is_pass());
        assert!(result.violations.iter().any(|v| v.check == "schema"));
    }

    #[test]
    fn conform_receipt_missing_run_field() {
        let json = serde_json::json!({
            "schema": "sensor.report.v1",
            "tool": { "name": "t", "version": "1.0.0" },
            "verdict": { "status": "pass", "counts": { "info": 0, "warn": 0, "error": 0 } },
            "findings": []
        });
        let result = conform_single(&json.to_string(), "sensor", &all_checks()).unwrap();
        assert!(!result.is_pass());
        assert!(result.violations.iter().any(|v| v.check == "schema"));
    }

    // ---- Path hygiene traversal patterns ----

    #[test]
    fn path_hygiene_various_traversal_patterns() {
        let mut report = minimal_sensor_report();
        report.findings = vec![
            Finding {
                severity: Severity::Info,
                check_id: None,
                code: "T1".to_string(),
                message: "traversal".to_string(),
                location: Some(Location {
                    path: Some("../etc/passwd".to_string()),
                    line: None,
                    col: None,
                }),
                help: None,
                url: None,
                fingerprint: None,
                data: None,
            },
            Finding {
                severity: Severity::Info,
                check_id: None,
                code: "T2".to_string(),
                message: "mid traversal".to_string(),
                location: Some(Location {
                    path: Some("foo/../../bar".to_string()),
                    line: None,
                    col: None,
                }),
                help: None,
                url: None,
                fingerprint: None,
                data: None,
            },
            Finding {
                severity: Severity::Info,
                check_id: None,
                code: "T3".to_string(),
                message: "backslash traversal".to_string(),
                location: Some(Location {
                    path: Some("foo\\..\\bar".to_string()),
                    line: None,
                    col: None,
                }),
                help: None,
                url: None,
                fingerprint: None,
                data: None,
            },
            Finding {
                severity: Severity::Info,
                check_id: None,
                code: "T4".to_string(),
                message: "drive abs".to_string(),
                location: Some(Location {
                    path: Some("D:\\Windows\\System32".to_string()),
                    line: None,
                    col: None,
                }),
                help: None,
                url: None,
                fingerprint: None,
                data: None,
            },
        ];

        let violations = check_path_hygiene(&report);
        assert!(
            violations.len() >= 4,
            "expected >= 4 violations, got {:?}",
            violations
        );
    }

    #[test]
    fn path_hygiene_clean_paths_pass() {
        let mut report = minimal_sensor_report();
        report.findings = vec![Finding {
            severity: Severity::Info,
            check_id: None,
            code: "OK".to_string(),
            message: "clean".to_string(),
            location: Some(Location {
                path: Some("src/lib.rs".to_string()),
                line: Some(1),
                col: None,
            }),
            help: None,
            url: None,
            fingerprint: None,
            data: None,
        }];

        let violations = check_path_hygiene(&report);
        assert!(violations.is_empty());
    }

    // ---- Sensor ID format ----

    #[test]
    fn sensor_id_format_edge_cases() {
        assert!(check_sensor_id_format("a").is_empty());
        assert!(check_sensor_id_format("A-Z_0").is_empty());
        assert!(!check_sensor_id_format("").is_empty());
        assert!(!check_sensor_id_format("has.dot").is_empty());
        assert!(!check_sensor_id_format("has space").is_empty());
        assert!(!check_sensor_id_format("../traversal").is_empty());
        assert!(!check_sensor_id_format("café").is_empty());
    }

    // ---- Reason token edge cases ----

    #[test]
    fn reason_token_edge_cases() {
        assert!(is_valid_reason_token("a"));
        assert!(is_valid_reason_token("a_b_c"));
        assert!(is_valid_reason_token("abc123"));
        assert!(!is_valid_reason_token(""));
        assert!(!is_valid_reason_token("UPPER"));
        assert!(!is_valid_reason_token("has-dash"));
        assert!(!is_valid_reason_token("has space"));
        assert!(!is_valid_reason_token("has.dot"));
    }
}
