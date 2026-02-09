use anyhow::{Context, Result};
use cockpitctl_types::{
    COCKPIT_REPORT_V1_SCHEMA_JSON, CockpitReport, SENSOR_REPORT_V1_SCHEMA_JSON, SensorReport,
    VerdictStatus,
};

use crate::checks;

/// Which conformance checks to run.
pub struct ConformChecks {
    pub path_hygiene: bool,
    pub ordering: bool,
    pub reason_lint: bool,
    pub survivability: bool,
    pub tool_error_identity: bool,
    pub sensor_id_format: bool,
    pub artifact_pointers: bool,
}

/// A single conformance violation.
#[derive(Debug)]
pub struct Violation {
    /// Check name, e.g. "path_hygiene", "schema", "survivability".
    pub check: String,
    /// Human-readable description.
    pub message: String,
}

/// Result of running conformance checks on a single report.
#[derive(Debug)]
pub struct ConformResult {
    pub violations: Vec<Violation>,
}

impl ConformResult {
    pub fn is_pass(&self) -> bool {
        self.violations.is_empty()
    }
}

/// Validate a single sensor report from its already-read content.
///
/// `Err` = infrastructure failure (can't parse JSON, can't compile schema).
/// `Ok(result)` = checks ran; `result.violations` may be non-empty.
pub fn conform_single(
    content: &str,
    sensor_id: &str,
    checks_cfg: &ConformChecks,
) -> Result<ConformResult> {
    let mut violations = Vec::new();

    // Parse as JSON
    let value: serde_json::Value = serde_json::from_str(content).context("parse JSON")?;

    // Schema validation
    let schema: serde_json::Value = serde_json::from_str(SENSOR_REPORT_V1_SCHEMA_JSON)
        .context("parse embedded sensor.report.v1 schema")?;

    let validator =
        jsonschema::validator_for(&schema).context("compile sensor.report.v1 schema")?;

    let errors: Vec<_> = validator.iter_errors(&value).collect();
    if !errors.is_empty() {
        for e in &errors {
            violations.push(Violation {
                check: "schema".to_string(),
                message: format!("{}", e),
            });
        }
        return Ok(ConformResult { violations });
    }

    // Parse for extended checks
    let parsed: SensorReport = serde_json::from_value(value).context("parse as SensorReport")?;

    // Survivability check
    if checks_cfg.survivability && parsed.verdict.status == VerdictStatus::Fail {
        let has_explanatory = !parsed.findings.is_empty() || !parsed.verdict.reasons.is_empty();
        if !has_explanatory {
            violations.push(Violation {
                check: "survivability".to_string(),
                message: "status=fail but no findings or reasons".to_string(),
            });
        }
    }

    // Path hygiene check
    if checks_cfg.path_hygiene {
        for msg in checks::check_path_hygiene(&parsed) {
            violations.push(Violation {
                check: "path_hygiene".to_string(),
                message: msg,
            });
        }
    }

    // Ordering check
    if checks_cfg.ordering {
        for msg in checks::check_ordering(&parsed, sensor_id) {
            violations.push(Violation {
                check: "ordering".to_string(),
                message: msg,
            });
        }
    }

    // Reason token lint
    if checks_cfg.reason_lint {
        for msg in checks::check_reason_tokens(&parsed) {
            violations.push(Violation {
                check: "reason_lint".to_string(),
                message: msg,
            });
        }
    }

    // Tool error identity check
    if checks_cfg.tool_error_identity {
        for msg in checks::check_tool_error_identity(&parsed) {
            violations.push(Violation {
                check: "tool_error_identity".to_string(),
                message: msg,
            });
        }
    }

    // Sensor ID format check
    if checks_cfg.sensor_id_format {
        for msg in checks::check_sensor_id_format(sensor_id) {
            violations.push(Violation {
                check: "sensor_id_format".to_string(),
                message: msg,
            });
        }
    }

    // Artifact pointers check
    if checks_cfg.artifact_pointers {
        for msg in checks::check_artifact_pointers(&parsed) {
            violations.push(Violation {
                check: "artifact_pointers".to_string(),
                message: msg,
            });
        }
    }

    Ok(ConformResult { violations })
}

/// Validate cockpit/report.json against cockpit.report.v1 schema.
/// Returns schema errors as violations.
pub fn validate_cockpit_schema(content: &str) -> Result<Vec<Violation>> {
    let value: serde_json::Value = serde_json::from_str(content).context("parse JSON")?;

    let schema: serde_json::Value = serde_json::from_str(COCKPIT_REPORT_V1_SCHEMA_JSON)
        .context("parse embedded cockpit.report.v1 schema")?;

    let validator =
        jsonschema::validator_for(&schema).context("compile cockpit.report.v1 schema")?;

    let errors: Vec<_> = validator.iter_errors(&value).collect();
    let mut violations = Vec::new();
    for e in &errors {
        violations.push(Violation {
            check: "schema".to_string(),
            message: format!("{}", e),
        });
    }

    Ok(violations)
}

/// Parse cockpit report content and run extended checks (reason tokens, presence semantics).
pub fn check_cockpit_extended(
    content: &str,
    reason_lint: bool,
    presence_semantics: bool,
) -> Result<Vec<Violation>> {
    let parsed: CockpitReport =
        serde_json::from_str(content).context("parse cockpit report for extended checks")?;

    let mut violations = Vec::new();

    if reason_lint {
        for msg in checks::check_cockpit_reason_tokens(&parsed) {
            violations.push(Violation {
                check: "reason_lint".to_string(),
                message: msg,
            });
        }
    }

    if presence_semantics {
        for msg in checks::check_presence_semantics(&parsed) {
            violations.push(Violation {
                check: "presence_semantics".to_string(),
                message: msg,
            });
        }
    }

    Ok(violations)
}

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

    fn minimal_sensor_report_json() -> String {
        serde_json::to_string(&minimal_sensor_report()).expect("serialize report")
    }

    fn minimal_cockpit_report() -> CockpitReport {
        let cfg = CockpitConfig::default();
        let policy = PolicySnapshot {
            warn_is_fail: cfg.policy.warn_is_fail,
            max_highlights: cfg.policy.max_highlights,
            max_per_sensor_findings: cfg.policy.max_per_sensor_findings,
            max_annotations: cfg.policy.max_annotations,
            section_order: cfg.policy.section_order.clone(),
            sensors: vec![],
        };
        CockpitReport {
            schema: "cockpit.report.v1".to_string(),
            tool: ToolInfo {
                name: "cockpitctl".to_string(),
                version: "0.1.0".to_string(),
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
            sensors: vec![],
            highlights: vec![],
            policy,
            data: None,
        }
    }

    #[test]
    fn conform_single_success_and_failure_paths() {
        let checks = ConformChecks {
            path_hygiene: false,
            ordering: false,
            reason_lint: false,
            survivability: false,
            tool_error_identity: false,
            sensor_id_format: false,
            artifact_pointers: false,
        };
        let result = conform_single(&minimal_sensor_report_json(), "sensor", &checks)
            .expect("should not error");
        assert!(result.is_pass());

        let result = conform_single("{}", "sensor", &checks).expect("should parse ok");
        assert!(!result.is_pass());
        assert!(result.violations.iter().any(|v| v.check == "schema"));

        let mut fail_report = minimal_sensor_report();
        fail_report.verdict.status = VerdictStatus::Fail;
        let fail_json = serde_json::to_string(&fail_report).expect("serialize");
        let checks_survivability = ConformChecks {
            survivability: true,
            ..ConformChecks {
                path_hygiene: false,
                ordering: false,
                reason_lint: false,
                survivability: true,
                tool_error_identity: false,
                sensor_id_format: false,
                artifact_pointers: false,
            }
        };
        let result =
            conform_single(&fail_json, "sensor", &checks_survivability).expect("should not error");
        assert!(!result.is_pass());
        assert!(result.violations.iter().any(|v| v.check == "survivability"));
    }

    #[test]
    fn conform_single_survivability_branches() {
        let checks = ConformChecks {
            path_hygiene: false,
            ordering: false,
            reason_lint: false,
            survivability: true,
            tool_error_identity: false,
            sensor_id_format: false,
            artifact_pointers: false,
        };

        // Fail with findings → passes survivability
        let mut fail_report = minimal_sensor_report();
        fail_report.verdict.status = VerdictStatus::Fail;
        fail_report.findings.push(Finding {
            severity: Severity::Error,
            check_id: None,
            code: "E1".to_string(),
            message: "fail".to_string(),
            location: None,
            help: None,
            url: None,
            fingerprint: None,
            data: None,
        });
        let fail_json = serde_json::to_string(&fail_report).expect("serialize");
        let result =
            conform_single(&fail_json, "sensor", &checks).expect("should not infrastructure-fail");
        assert!(result.is_pass());

        // Pass status → survivability not checked
        let pass_report = minimal_sensor_report_json();
        let result = conform_single(&pass_report, "sensor", &checks)
            .expect("should not infrastructure-fail");
        assert!(result.is_pass());
    }

    #[test]
    fn conform_single_ordering_passes_with_no_findings() {
        let checks = ConformChecks {
            path_hygiene: false,
            ordering: true,
            reason_lint: false,
            survivability: false,
            tool_error_identity: false,
            sensor_id_format: false,
            artifact_pointers: false,
        };
        let result = conform_single(&minimal_sensor_report_json(), "sensor", &checks)
            .expect("should not error");
        assert!(result.is_pass());
    }

    #[test]
    fn conform_single_reports_multiple_violations() {
        let mut report = minimal_sensor_report();
        report.findings = vec![Finding {
            severity: Severity::Warn,
            check_id: None,
            code: "W1".to_string(),
            message: "warn".to_string(),
            location: Some(Location {
                path: Some("../bad/path.rs".to_string()),
                line: Some(1),
                col: None,
            }),
            help: None,
            url: None,
            fingerprint: None,
            data: None,
        }];
        report.verdict.reasons = vec!["Bad-Token".to_string(), "tool_error".to_string()];
        report.artifacts = vec![ArtifactPointer {
            id: "abs".to_string(),
            path: "/abs/path.txt".to_string(),
            mime: "text/plain".to_string(),
            schema: None,
        }];

        let checks = ConformChecks {
            path_hygiene: true,
            ordering: false,
            reason_lint: true,
            survivability: false,
            tool_error_identity: true,
            sensor_id_format: true,
            artifact_pointers: true,
        };

        let json = serde_json::to_string(&report).expect("serialize");
        let result = conform_single(&json, "bad.id", &checks).expect("should not error");
        assert!(!result.is_pass());
        assert!(result.violations.len() >= 3);
    }

    #[test]
    fn conform_single_ok_branches_for_checks() {
        let mut report = minimal_sensor_report();
        report.findings = vec![Finding {
            severity: Severity::Info,
            check_id: None,
            code: "I1".to_string(),
            message: "info".to_string(),
            location: Some(Location {
                path: Some("src/main.rs".to_string()),
                line: Some(1),
                col: None,
            }),
            help: None,
            url: None,
            fingerprint: None,
            data: None,
        }];
        report.artifacts = vec![ArtifactPointer {
            id: "log".to_string(),
            path: "artifacts/log.txt".to_string(),
            mime: "text/plain".to_string(),
            schema: None,
        }];

        let checks = ConformChecks {
            path_hygiene: true,
            ordering: false,
            reason_lint: false,
            survivability: false,
            tool_error_identity: true,
            sensor_id_format: true,
            artifact_pointers: true,
        };

        let json = serde_json::to_string(&report).expect("serialize");
        let result = conform_single(&json, "good_id", &checks).expect("should not error");
        assert!(result.is_pass());
    }

    #[test]
    fn validate_cockpit_schema_pass_and_fail() {
        let report = minimal_cockpit_report();
        let json = serde_json::to_string(&report).expect("serialize");
        let violations = validate_cockpit_schema(&json).expect("should not error");
        assert!(violations.is_empty());

        let violations = validate_cockpit_schema("{}").expect("should not error");
        assert!(!violations.is_empty());
    }

    #[test]
    fn check_cockpit_extended_reason_and_presence() {
        let mut report = minimal_cockpit_report();
        report.verdict.reasons = vec!["Bad-Token".to_string()];
        report.sensors.push(SensorSummary {
            id: "sensor".to_string(),
            blocking: true,
            missing: MissingPolicy::Fail,
            presence: Presence::Present,
            report_path: "artifacts/sensor/report.json".to_string(),
            comment_path: None,
            verdict: Verdict {
                status: VerdictStatus::Pass,
                counts: VerdictCounts::default(),
                reasons: vec![],
            },
            truncated: false,
            errors: vec![],
            missing_policy_applied: Some(MissingPolicy::Skip),
            policy_outcome: None,
        });

        let json = serde_json::to_string(&report).expect("serialize");

        let violations =
            check_cockpit_extended(&json, true, true).expect("should not infrastructure-fail");
        assert!(violations.iter().any(|v| v.check == "reason_lint"));
        assert!(violations.iter().any(|v| v.check == "presence_semantics"));

        // With checks disabled
        let violations =
            check_cockpit_extended(&json, false, false).expect("should not infrastructure-fail");
        assert!(violations.is_empty());
    }

    #[test]
    fn conform_single_invalid_json_errors() {
        let checks = ConformChecks {
            path_hygiene: false,
            ordering: false,
            reason_lint: false,
            survivability: false,
            tool_error_identity: false,
            sensor_id_format: false,
            artifact_pointers: false,
        };
        let err = conform_single("{invalid", "sensor", &checks).expect_err("should error");
        assert!(format!("{:#}", err).contains("parse JSON"));
    }
}
