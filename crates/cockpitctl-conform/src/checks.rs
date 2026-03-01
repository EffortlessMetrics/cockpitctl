use cockpitctl_types::{CockpitReport, FindingSortKey, Presence, SensorReport, severity_rank};

/// Check if a reason token matches `^[a-z0-9_]+$`.
///
/// # Examples
///
/// ```
/// use cockpitctl_conform::is_valid_reason_token;
///
/// assert!(is_valid_reason_token("tool_error"));
/// assert!(is_valid_reason_token("missing_receipt"));
/// assert!(!is_valid_reason_token("Bad-Token"));
/// assert!(!is_valid_reason_token(""));
/// ```
pub fn is_valid_reason_token(s: &str) -> bool {
    !s.is_empty()
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

/// Check finding location paths for hygiene violations (absolute paths, traversal, backslashes).
///
/// # Examples
///
/// ```
/// use cockpitctl_conform::check_path_hygiene;
/// use cockpitctl_types::*;
/// use std::collections::BTreeMap;
///
/// let mut report = SensorReport {
///     schema: "sensor.report.v1".into(),
///     tool: ToolInfo { name: "test".into(), version: "1.0.0".into(), commit: None },
///     run: RunInfo {
///         started_at: "2026-01-01T00:00:00Z".into(),
///         ended_at: None, duration_ms: None, host: None,
///         git: None, ci: None, capabilities: BTreeMap::new(),
///     },
///     verdict: Verdict {
///         status: VerdictStatus::Pass,
///         counts: VerdictCounts::default(),
///         reasons: vec![],
///     },
///     findings: vec![Finding {
///         severity: Severity::Info,
///         check_id: None,
///         code: "T1".into(),
///         message: "traversal".into(),
///         location: Some(Location { path: Some("../etc/passwd".into()), line: None, col: None }),
///         help: None, url: None, fingerprint: None, data: None,
///     }],
///     artifacts: vec![],
///     data: None,
/// };
///
/// let violations = check_path_hygiene(&report);
/// assert!(!violations.is_empty()); // detects path traversal
/// ```
pub fn check_path_hygiene(report: &SensorReport) -> Vec<String> {
    let mut violations = Vec::new();
    for (i, f) in report.findings.iter().enumerate() {
        if let Some(loc) = &f.location
            && let Some(path) = &loc.path
        {
            if path.starts_with('/') || path.starts_with('\\') {
                violations.push(format!(
                    "finding[{}]: absolute path (starts with / or \\): {}",
                    i, path
                ));
            } else if path.len() >= 2
                && path.as_bytes()[0].is_ascii_alphabetic()
                && path.as_bytes()[1] == b':'
            {
                violations.push(format!(
                    "finding[{}]: absolute path (drive letter): {}",
                    i, path
                ));
            }
            if path.contains("..") {
                violations.push(format!(
                    "finding[{}]: path traversal (contains ..): {}",
                    i, path
                ));
            }
            if path.contains('\\') {
                violations.push(format!("finding[{}]: backslash in path: {}", i, path));
            }
        }
    }
    violations
}

/// Check that findings are sorted in canonical order.
///
/// # Examples
///
/// ```
/// use cockpitctl_conform::check_ordering;
/// use cockpitctl_types::*;
/// use std::collections::BTreeMap;
///
/// let report = SensorReport {
///     schema: "sensor.report.v1".into(),
///     tool: ToolInfo { name: "test".into(), version: "1.0.0".into(), commit: None },
///     run: RunInfo {
///         started_at: "2026-01-01T00:00:00Z".into(),
///         ended_at: None, duration_ms: None, host: None,
///         git: None, ci: None, capabilities: BTreeMap::new(),
///     },
///     verdict: Verdict {
///         status: VerdictStatus::Pass,
///         counts: VerdictCounts::default(),
///         reasons: vec![],
///     },
///     findings: vec![
///         Finding {
///             severity: Severity::Info,
///             check_id: None,
///             code: "A".into(),
///             message: "first".into(),
///             location: None, help: None, url: None, fingerprint: None, data: None,
///         },
///         Finding {
///             severity: Severity::Error,
///             check_id: None,
///             code: "B".into(),
///             message: "second".into(),
///             location: None, help: None, url: None, fingerprint: None, data: None,
///         },
///     ],
///     artifacts: vec![],
///     data: None,
/// };
///
/// // Info before Error is out of order (Error should come first).
/// let violations = check_ordering(&report, "test");
/// assert!(!violations.is_empty());
/// ```
pub fn check_ordering(report: &SensorReport, sensor_id: &str) -> Vec<String> {
    let keys: Vec<FindingSortKey> = report
        .findings
        .iter()
        .map(|f| FindingSortKey {
            severity_rank: severity_rank(&f.severity),
            sensor_id: sensor_id.to_string(),
            path: f
                .location
                .as_ref()
                .and_then(|l| l.path.as_deref())
                .unwrap_or("")
                .to_string(),
            line: f.location.as_ref().and_then(|l| l.line).unwrap_or(0),
            code: f.code.clone(),
            message: f.message.clone(),
        })
        .collect();

    let mut violations = Vec::new();
    for i in 1..keys.len() {
        if keys[i] < keys[i - 1] {
            violations.push(format!(
                "finding[{}] is out of order (severity_rank={}, code={}) < finding[{}] (severity_rank={}, code={})",
                i, keys[i].severity_rank, keys[i].code,
                i - 1, keys[i - 1].severity_rank, keys[i - 1].code,
            ));
        }
    }
    violations
}

/// Check that reason tokens in a sensor report match `^[a-z0-9_]+$`.
pub fn check_reason_tokens(report: &SensorReport) -> Vec<String> {
    let mut violations = Vec::new();

    for (i, reason) in report.verdict.reasons.iter().enumerate() {
        if !is_valid_reason_token(reason) {
            violations.push(format!(
                "verdict.reasons[{}]: invalid token {:?}",
                i, reason
            ));
        }
    }

    for (name, cap) in &report.run.capabilities {
        if let Some(reason) = &cap.reason
            && !is_valid_reason_token(reason)
        {
            violations.push(format!(
                "capabilities.{}.reason: invalid token {:?}",
                name, reason
            ));
        }
    }

    violations
}

/// Check tool_error identity: require canonical check_id/code when verdict has tool_error reason.
pub fn check_tool_error_identity(report: &SensorReport) -> Vec<String> {
    let mut violations = Vec::new();

    if !report.verdict.reasons.iter().any(|r| r == "tool_error") {
        return violations;
    }

    let has_canonical = report
        .findings
        .iter()
        .any(|f| f.check_id.as_deref() == Some("tool.runtime") && f.code == "runtime_error");

    if !has_canonical {
        violations.push(
            "verdict.reasons contains \"tool_error\" but no finding has check_id=\"tool.runtime\" + code=\"runtime_error\""
                .to_string(),
        );
    }

    violations
}

/// Validate sensor ID matches `[a-zA-Z0-9_-]+`.
///
/// # Examples
///
/// ```
/// use cockpitctl_conform::check_sensor_id_format;
///
/// assert!(check_sensor_id_format("builddiag").is_empty());
/// assert!(check_sensor_id_format("my-sensor_v2").is_empty());
/// assert!(!check_sensor_id_format("bad.id").is_empty());
/// assert!(!check_sensor_id_format("").is_empty());
/// ```
pub fn check_sensor_id_format(sensor_id: &str) -> Vec<String> {
    let mut violations = Vec::new();
    let valid = !sensor_id.is_empty()
        && sensor_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
    if !valid {
        violations.push(format!(
            "sensor_id {:?} does not match [a-zA-Z0-9_-]+",
            sensor_id
        ));
    }
    violations
}

/// Validate artifact pointer fields and path safety.
pub fn check_artifact_pointers(report: &SensorReport) -> Vec<String> {
    let mut violations = Vec::new();
    for (i, artifact) in report.artifacts.iter().enumerate() {
        if artifact.id.is_empty() {
            violations.push(format!("artifacts[{}]: id is empty", i));
        }
        if artifact.path.is_empty() {
            violations.push(format!("artifacts[{}]: path is empty", i));
        } else {
            if artifact.path.contains("..") {
                violations.push(format!(
                    "artifacts[{}]: path contains \"..\": {}",
                    i, artifact.path
                ));
            }
            if artifact.path.starts_with('/') || artifact.path.starts_with('\\') {
                violations.push(format!(
                    "artifacts[{}]: path is absolute (starts with / or \\): {}",
                    i, artifact.path
                ));
            }
            if artifact.path.len() >= 2
                && artifact.path.as_bytes()[0].is_ascii_alphabetic()
                && artifact.path.as_bytes()[1] == b':'
            {
                violations.push(format!(
                    "artifacts[{}]: path is absolute (drive letter): {}",
                    i, artifact.path
                ));
            }
        }
        if artifact.mime.is_empty() {
            violations.push(format!("artifacts[{}]: mime is empty", i));
        }
    }
    violations
}

/// Validate presence semantics in cockpit report.
pub fn check_presence_semantics(report: &CockpitReport) -> Vec<String> {
    let mut violations = Vec::new();
    for (i, sensor) in report.sensors.iter().enumerate() {
        if sensor.missing_policy_applied.is_some() && sensor.presence != Presence::Missing {
            violations.push(format!(
                "sensors[{}] ({}): missing_policy_applied is set but presence is {:?}, expected \"missing\"",
                i, sensor.id, sensor.presence
            ));
        }
    }
    violations
}

/// Check that reason tokens in a cockpit report match `^[a-z0-9_]+$`.
pub fn check_cockpit_reason_tokens(report: &CockpitReport) -> Vec<String> {
    let mut violations = Vec::new();

    for (i, reason) in report.verdict.reasons.iter().enumerate() {
        if !is_valid_reason_token(reason) {
            violations.push(format!(
                "verdict.reasons[{}]: invalid token {:?}",
                i, reason
            ));
        }
    }

    for (si, sensor) in report.sensors.iter().enumerate() {
        for (ri, reason) in sensor.verdict.reasons.iter().enumerate() {
            if !is_valid_reason_token(reason) {
                violations.push(format!(
                    "sensors[{}].verdict.reasons[{}]: invalid token {:?}",
                    si, ri, reason
                ));
            }
        }
    }

    for (name, cap) in &report.run.capabilities {
        if let Some(reason) = &cap.reason
            && !is_valid_reason_token(reason)
        {
            violations.push(format!(
                "run.capabilities.{}.reason: invalid token {:?}",
                name, reason
            ));
        }
    }

    violations
}

/// Pure string compare for determinism checking.
///
/// # Examples
///
/// ```
/// use cockpitctl_conform::check_determinism;
///
/// assert!(check_determinism("hello", "hello").is_none());
/// assert!(check_determinism("hello", "world").is_some());
/// ```
pub fn check_determinism(actual: &str, expected: &str) -> Option<String> {
    if actual != expected {
        Some("report does not match golden file".to_string())
    } else {
        None
    }
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
                version: "0.2.0".to_string(),
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
    fn reason_token_and_sensor_id_checks() {
        assert!(is_valid_reason_token("ok_token"));
        assert!(!is_valid_reason_token("Bad-Token"));

        assert!(check_sensor_id_format("good_id").is_empty());
        assert!(!check_sensor_id_format("bad.id").is_empty());
    }

    #[test]
    fn path_hygiene_and_ordering_checks() {
        let mut report = minimal_sensor_report();
        report.findings = vec![
            Finding {
                severity: Severity::Info,
                check_id: None,
                code: "I1".to_string(),
                message: "info".to_string(),
                location: Some(Location {
                    path: Some("/abs/path".to_string()),
                    line: Some(1),
                    col: None,
                }),
                help: None,
                url: None,
                fingerprint: None,
                data: None,
            },
            Finding {
                severity: Severity::Warn,
                check_id: None,
                code: "W1".to_string(),
                message: "warn".to_string(),
                location: Some(Location {
                    path: Some("C:\\temp\\file.rs".to_string()),
                    line: Some(2),
                    col: None,
                }),
                help: None,
                url: None,
                fingerprint: None,
                data: None,
            },
            Finding {
                severity: Severity::Error,
                check_id: None,
                code: "E1".to_string(),
                message: "err".to_string(),
                location: Some(Location {
                    path: Some("src/../file.rs".to_string()),
                    line: Some(3),
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
                code: "I2".to_string(),
                message: "no path".to_string(),
                location: Some(Location {
                    path: None,
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
        assert!(violations.iter().any(|v| v.contains("absolute path")));
        assert!(violations.iter().any(|v| v.contains("drive letter")));
        assert!(violations.iter().any(|v| v.contains("path traversal")));
        assert!(violations.iter().any(|v| v.contains("backslash")));

        let mut ordering_report = minimal_sensor_report();
        ordering_report.findings = vec![
            Finding {
                severity: Severity::Info,
                check_id: None,
                code: "I1".to_string(),
                message: "info".to_string(),
                location: None,
                help: None,
                url: None,
                fingerprint: None,
                data: None,
            },
            Finding {
                severity: Severity::Error,
                check_id: None,
                code: "E1".to_string(),
                message: "err".to_string(),
                location: None,
                help: None,
                url: None,
                fingerprint: None,
                data: None,
            },
        ];
        let ordering = check_ordering(&ordering_report, "sensor");
        assert_eq!(ordering.len(), 1);
    }

    #[test]
    fn tool_error_identity_and_reason_lint_checks() {
        let mut report = minimal_sensor_report();
        assert!(check_tool_error_identity(&report).is_empty());

        report.verdict.reasons = vec!["tool_error".to_string()];
        let violations = check_tool_error_identity(&report);
        assert!(!violations.is_empty());

        report.findings.push(Finding {
            severity: Severity::Error,
            check_id: Some("tool.runtime".to_string()),
            code: "runtime_error".to_string(),
            message: "boom".to_string(),
            location: None,
            help: None,
            url: None,
            fingerprint: None,
            data: None,
        });
        assert!(check_tool_error_identity(&report).is_empty());

        report.verdict.reasons = vec!["Bad-Token".to_string()];
        report.run.capabilities.insert(
            "git".to_string(),
            Capability {
                status: CapabilityStatus::Available,
                reason: Some("Bad-Token".to_string()),
            },
        );
        let reasons = check_reason_tokens(&report);
        assert!(reasons.len() >= 2);
    }

    #[test]
    fn cockpit_reason_tokens_and_presence_semantics_checks() {
        let mut report = minimal_cockpit_report();
        report.verdict.reasons = vec!["Bad-Token".to_string()];
        report.run.capabilities.insert(
            "git".to_string(),
            Capability {
                status: CapabilityStatus::Available,
                reason: Some("Bad-Token".to_string()),
            },
        );
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
                reasons: vec!["Bad-Token".to_string()],
            },
            truncated: false,
            errors: vec![],
            missing_policy_applied: Some(MissingPolicy::Skip),
            policy_outcome: None,
        });

        let reason_violations = check_cockpit_reason_tokens(&report);
        assert!(reason_violations.len() >= 3);

        let presence_violations = check_presence_semantics(&report);
        assert_eq!(presence_violations.len(), 1);
    }

    #[test]
    fn artifact_pointer_checks() {
        let mut report = minimal_sensor_report();
        report.artifacts = vec![
            ArtifactPointer {
                id: "".to_string(),
                path: "".to_string(),
                mime: "".to_string(),
                schema: None,
            },
            ArtifactPointer {
                id: "ok".to_string(),
                path: "../bad".to_string(),
                mime: "text/plain".to_string(),
                schema: None,
            },
            ArtifactPointer {
                id: "abs".to_string(),
                path: "/abs/path.txt".to_string(),
                mime: "text/plain".to_string(),
                schema: None,
            },
            ArtifactPointer {
                id: "drive".to_string(),
                path: "C:\\abs\\path.txt".to_string(),
                mime: "text/plain".to_string(),
                schema: None,
            },
        ];
        let violations = check_artifact_pointers(&report);
        assert!(violations.len() >= 4);
    }

    #[test]
    fn determinism_check_pass_and_fail() {
        assert!(check_determinism("same", "same").is_none());
        assert!(check_determinism("a", "b").is_some());
    }
}
