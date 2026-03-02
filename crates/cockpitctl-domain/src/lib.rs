//! Domain logic for cockpitctl:
//! - policy evaluation
//! - missing/invalid receipt handling rules
//! - deterministic ordering + highlight selection
//!
//! No filesystem, no clap, no network.

#![deny(missing_docs)]

pub use cockpitctl_domain_buildfix::{match_buildfix_plan, select_auto_apply_fixes};
pub use cockpitctl_domain_signing::{
    canonical_policy_snapshot_bytes, policy_snapshot_sha256_hex, sign_policy_snapshot,
    sign_policy_snapshot_hmac_sha256,
};
pub use cockpitctl_domain_trend::compute_trend;
use cockpitctl_types::{
    CockpitConfig, CockpitReport, Finding, FindingSortKey, Highlight, MissingPolicy, PolicyOutcome,
    PolicySensorSnapshot, PolicySnapshot, Presence, RunInfo, SensorPolicy, SensorReport,
    SensorSummary, Severity, ToolInfo, Verdict, VerdictCounts, VerdictStatus, severity_rank,
    verdict_status_rank,
};
use sha2::{Digest, Sha256};

/// The schema identifier for cockpit reports.
pub const COCKPIT_SCHEMA_ID: &str = "cockpit.report.v1";

/// Derive the policy outcome for a sensor given its blocking flag and verdict status.
///
/// # Examples
///
/// ```
/// use cockpitctl_domain::compute_policy_outcome;
/// use cockpitctl_types::{PolicyOutcome, VerdictStatus};
///
/// // Non-blocking sensors are always informational.
/// assert_eq!(compute_policy_outcome(false, &VerdictStatus::Fail), PolicyOutcome::Informational);
///
/// // Blocking sensor with fail → blocked.
/// assert_eq!(compute_policy_outcome(true, &VerdictStatus::Fail), PolicyOutcome::Blocked);
///
/// // Blocking sensor with pass → allowed.
/// assert_eq!(compute_policy_outcome(true, &VerdictStatus::Pass), PolicyOutcome::Allowed);
/// ```
pub fn compute_policy_outcome(blocking: bool, status: &VerdictStatus) -> PolicyOutcome {
    if !blocking {
        PolicyOutcome::Informational
    } else if matches!(status, VerdictStatus::Fail) {
        PolicyOutcome::Blocked
    } else {
        PolicyOutcome::Allowed
    }
}

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

// ============================================================================
// Code explanations for `cockpitctl explain`
// ============================================================================

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
///
/// # Examples
///
/// ```
/// use cockpitctl_domain::explain_code;
///
/// let explanation = explain_code("cockpit.missing_receipt").unwrap();
/// assert_eq!(explanation.title, "Missing Receipt");
///
/// assert!(explain_code("nonexistent.code").is_none());
/// ```
pub fn explain_code(code: &str) -> Option<CodeExplanation> {
    all_codes().into_iter().find(|e| e.code == code)
}

/// Return all known cockpit finding codes with explanations.
///
/// # Examples
///
/// ```
/// use cockpitctl_domain::all_codes;
///
/// let codes = all_codes();
/// assert!(codes.len() >= 7);
/// assert!(codes.iter().any(|c| c.code == "cockpit.missing_receipt"));
/// assert!(codes.iter().any(|c| c.code == "cockpit.path_traversal"));
/// ```
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

/// Normalize and cap a sensor report's findings for cockpit surfacing.
///
/// The original report remains the record; cockpit surfaces only up to policy limits.
///
/// # Examples
///
/// ```
/// use cockpitctl_domain::cap_findings;
/// use cockpitctl_types::{Finding, Severity};
///
/// let findings: Vec<Finding> = (0..5).map(|i| Finding {
///     severity: Severity::Info,
///     check_id: None,
///     code: format!("C{}", i),
///     message: format!("msg {}", i),
///     location: None,
///     help: None, url: None, fingerprint: None, data: None,
/// }).collect();
///
/// // Under the cap: no truncation.
/// let (capped, truncated) = cap_findings(findings.clone(), 10);
/// assert_eq!(capped.len(), 5);
/// assert!(!truncated);
///
/// // Over the cap: truncated.
/// let (capped, truncated) = cap_findings(findings, 3);
/// assert_eq!(capped.len(), 3);
/// assert!(truncated);
/// ```
pub fn cap_findings(mut findings: Vec<Finding>, max: usize) -> (Vec<Finding>, bool) {
    if findings.len() <= max {
        return (findings, false);
    }
    findings.truncate(max);
    (findings, true)
}

/// Compute counts from findings.
///
/// # Examples
///
/// ```
/// use cockpitctl_domain::compute_counts;
/// use cockpitctl_types::{Finding, Severity};
///
/// let findings = vec![
///     Finding {
///         severity: Severity::Error,
///         check_id: None,
///         code: "E1".to_string(),
///         message: "err".to_string(),
///         location: None,
///         help: None,
///         url: None,
///         fingerprint: None,
///         data: None,
///     },
///     Finding {
///         severity: Severity::Warn,
///         check_id: None,
///         code: "W1".to_string(),
///         message: "warn".to_string(),
///         location: None,
///         help: None,
///         url: None,
///         fingerprint: None,
///         data: None,
///     },
/// ];
/// let counts = compute_counts(&findings);
/// assert_eq!(counts.error, 1);
/// assert_eq!(counts.warn, 1);
/// assert_eq!(counts.info, 0);
/// ```
pub fn compute_counts(findings: &[Finding]) -> VerdictCounts {
    let mut c = VerdictCounts::default();
    for f in findings {
        match f.severity {
            Severity::Info => c.info += 1,
            Severity::Warn => c.warn += 1,
            Severity::Error => c.error += 1,
        }
    }
    c
}

/// Derive a stable fingerprint for a finding when absent.
///
/// # Examples
///
/// ```
/// use cockpitctl_domain::derive_fingerprint;
/// use cockpitctl_types::{Finding, Severity};
///
/// let finding = Finding {
///     severity: Severity::Error,
///     check_id: None,
///     code: "E001".into(),
///     message: "something broke".into(),
///     location: None,
///     help: None, url: None, fingerprint: None, data: None,
/// };
///
/// let fp = derive_fingerprint("builddiag", &finding);
/// assert_eq!(fp.len(), 64); // SHA-256 hex
///
/// // Same input produces the same fingerprint.
/// assert_eq!(fp, derive_fingerprint("builddiag", &finding));
/// ```
pub fn derive_fingerprint(sensor_id: &str, finding: &Finding) -> String {
    let mut h = Sha256::new();
    h.update(sensor_id.as_bytes());
    h.update(b"\n");
    h.update(finding.code.as_bytes());
    h.update(b"\n");
    h.update(finding.message.as_bytes());
    h.update(b"\n");
    if let Some(loc) = &finding.location {
        if let Some(path) = &loc.path {
            h.update(path.as_bytes());
            h.update(b"\n");
        }
        if let Some(line) = loc.line {
            h.update(line.to_string().as_bytes());
            h.update(b"\n");
        }
    }
    let out = h.finalize();
    hex::encode(out)
}

/// Build a deterministic sort key for a finding, enabling stable ordering.
///
/// # Examples
///
/// ```
/// use cockpitctl_domain::finding_sort_key;
/// use cockpitctl_types::{Finding, Location, Severity};
///
/// let finding = Finding {
///     severity: Severity::Error,
///     check_id: None,
///     code: "E001".into(),
///     message: "fail".into(),
///     location: Some(Location { path: Some("src/main.rs".into()), line: Some(10), col: None }),
///     help: None, url: None, fingerprint: None, data: None,
/// };
///
/// let key = finding_sort_key("builddiag", &finding);
/// assert_eq!(key.severity_rank, 0); // Error is most severe
/// assert_eq!(key.path, "src/main.rs");
/// assert_eq!(key.line, 10);
/// ```
pub fn finding_sort_key(sensor_id: &str, f: &Finding) -> FindingSortKey {
    let (path, line) = match &f.location {
        Some(loc) => (
            loc.path.clone().unwrap_or_default(),
            loc.line.unwrap_or(u32::MAX),
        ),
        None => (String::new(), u32::MAX),
    };
    FindingSortKey {
        severity_rank: severity_rank(&f.severity),
        sensor_id: sensor_id.to_string(),
        path,
        line,
        code: f.code.clone(),
        message: f.message.clone(),
    }
}

/// Sort findings deterministically: severity desc → sensor_id → path → line → code → message.
///
/// # Examples
///
/// ```
/// use cockpitctl_domain::sort_findings;
/// use cockpitctl_types::{Finding, Severity};
///
/// let mut findings = vec![
///     Finding {
///         severity: Severity::Info,
///         check_id: None,
///         code: "I1".into(),
///         message: "info".into(),
///         location: None, help: None, url: None, fingerprint: None, data: None,
///     },
///     Finding {
///         severity: Severity::Error,
///         check_id: None,
///         code: "E1".into(),
///         message: "error".into(),
///         location: None, help: None, url: None, fingerprint: None, data: None,
///     },
/// ];
///
/// sort_findings("sensor", &mut findings);
/// // Errors sort before info (lower severity_rank = more severe).
/// assert_eq!(findings[0].severity, Severity::Error);
/// assert_eq!(findings[1].severity, Severity::Info);
/// ```
pub fn sort_findings(sensor_id: &str, findings: &mut [Finding]) {
    findings.sort_by_key(|f| finding_sort_key(sensor_id, f));
}

/// Sort sensor summaries by section order (from config), then by sensor ID.
///
/// Sensors whose section appears in [`CockpitConfig::policy::section_order`] are ranked
/// by position; unknown sections sort after all known ones. Within the same section,
/// sensors are ordered lexically by ID to guarantee deterministic output.
///
/// # Examples
///
/// ```
/// use cockpitctl_domain::sort_sensor_summaries;
/// use cockpitctl_types::*;
///
/// let mut summaries = vec![
///     SensorSummary {
///         id: "z-sensor".into(), blocking: false, missing: MissingPolicy::Skip,
///         presence: Presence::Present, report_path: "artifacts/z-sensor/report.json".into(),
///         comment_path: None, verdict: Verdict { status: VerdictStatus::Pass,
///         counts: VerdictCounts::default(), reasons: vec![] },
///         truncated: false, errors: vec![], missing_policy_applied: None,
///         policy_outcome: Some(PolicyOutcome::Informational),
///     },
///     SensorSummary {
///         id: "a-sensor".into(), blocking: false, missing: MissingPolicy::Skip,
///         presence: Presence::Present, report_path: "artifacts/a-sensor/report.json".into(),
///         comment_path: None, verdict: Verdict { status: VerdictStatus::Pass,
///         counts: VerdictCounts::default(), reasons: vec![] },
///         truncated: false, errors: vec![], missing_policy_applied: None,
///         policy_outcome: Some(PolicyOutcome::Informational),
///     },
/// ];
///
/// let cfg = CockpitConfig::default();
/// sort_sensor_summaries(&mut summaries, &cfg);
/// assert_eq!(summaries[0].id, "a-sensor");
/// assert_eq!(summaries[1].id, "z-sensor");
/// ```
pub fn sort_sensor_summaries(summaries: &mut [SensorSummary], cfg: &CockpitConfig) {
    // Order by section order, then by id.
    let mut section_rank = std::collections::BTreeMap::<String, usize>::new();
    for (i, s) in cfg.policy.section_order.iter().enumerate() {
        section_rank.insert(s.to_string(), i);
    }

    summaries.sort_by(|a, b| {
        let a_section = cfg
            .sensors
            .get(&a.id)
            .and_then(|p| p.section.clone())
            .unwrap_or_else(|| "Other".to_string());
        let b_section = cfg
            .sensors
            .get(&b.id)
            .and_then(|p| p.section.clone())
            .unwrap_or_else(|| "Other".to_string());

        let ra = section_rank.get(&a_section).cloned().unwrap_or(usize::MAX);
        let rb = section_rank.get(&b_section).cloned().unwrap_or(usize::MAX);

        (ra, a.id.clone()).cmp(&(rb, b.id.clone()))
    });
}

/// Select, deduplicate, sort, and cap highlights for the cockpit report.
///
/// Highlights are deduplicated by fingerprint (derived if absent), then sorted
/// deterministically: severity descending (error first), blocking sensors first,
/// then by sensor_id / path / line / code. The result is capped at
/// [`CockpitConfig::policy::max_highlights`].
///
/// # Examples
///
/// ```
/// use cockpitctl_domain::select_highlights;
/// use cockpitctl_types::*;
/// use std::collections::BTreeMap;
///
/// let candidates = vec![
///     Highlight {
///         sensor_id: "s1".into(),
///         finding: Finding {
///             severity: Severity::Info, check_id: None,
///             code: "I1".into(), message: "info".into(),
///             location: None, help: None, url: None, fingerprint: None, data: None,
///         },
///     },
///     Highlight {
///         sensor_id: "s1".into(),
///         finding: Finding {
///             severity: Severity::Error, check_id: None,
///             code: "E1".into(), message: "error".into(),
///             location: None, help: None, url: None, fingerprint: None, data: None,
///         },
///     },
/// ];
///
/// let cfg = CockpitConfig::default();
/// let blocking = BTreeMap::from([("s1".to_string(), true)]);
/// let selected = select_highlights(candidates, &cfg, &blocking);
/// // Errors sort before info.
/// assert_eq!(selected[0].finding.severity, Severity::Error);
/// assert_eq!(selected[1].finding.severity, Severity::Info);
/// ```
pub fn select_highlights(
    mut candidates: Vec<Highlight>,
    cfg: &CockpitConfig,
    sensor_blocking: &std::collections::BTreeMap<String, bool>,
) -> Vec<Highlight> {
    // Dedupe by fingerprint if present; else derived key.
    let mut seen = std::collections::BTreeSet::<String>::new();
    let mut deduped = Vec::new();

    for mut h in candidates.drain(..) {
        let fp = h
            .finding
            .fingerprint
            .clone()
            .unwrap_or_else(|| derive_fingerprint(&h.sensor_id, &h.finding));
        if seen.insert(fp.clone()) {
            // Normalize by ensuring fingerprint is present for later stages.
            if h.finding.fingerprint.is_none() {
                h.finding.fingerprint = Some(fp);
            }
            deduped.push(h);
        }
    }

    // Sort deterministically: severity desc (error first), blocking sensors first, then sensor_id/path/line/code.
    deduped.sort_by(|a, b| {
        let a_block = sensor_blocking.get(&a.sensor_id).cloned().unwrap_or(false);
        let b_block = sensor_blocking.get(&b.sensor_id).cloned().unwrap_or(false);

        let a_key = (
            severity_rank(&a.finding.severity),
            if a_block { 0u8 } else { 1u8 },
            a.sensor_id.clone(),
            a.finding
                .location
                .as_ref()
                .and_then(|l| l.path.clone())
                .unwrap_or_default(),
            a.finding
                .location
                .as_ref()
                .and_then(|l| l.line)
                .unwrap_or(u32::MAX),
            a.finding.code.clone(),
            a.finding.message.clone(),
        );
        let b_key = (
            severity_rank(&b.finding.severity),
            if b_block { 0u8 } else { 1u8 },
            b.sensor_id.clone(),
            b.finding
                .location
                .as_ref()
                .and_then(|l| l.path.clone())
                .unwrap_or_default(),
            b.finding
                .location
                .as_ref()
                .and_then(|l| l.line)
                .unwrap_or(u32::MAX),
            b.finding.code.clone(),
            b.finding.message.clone(),
        );

        a_key.cmp(&b_key)
    });

    deduped.truncate(cfg.policy.max_highlights);
    deduped
}

/// Capture the current policy configuration as a snapshot for the report.
///
/// # Examples
///
/// ```
/// use cockpitctl_domain::snapshot_policy;
/// use cockpitctl_types::CockpitConfig;
///
/// let cfg = CockpitConfig::default();
/// let snapshot = snapshot_policy(&cfg);
/// assert_eq!(snapshot.max_highlights, 7);
/// assert!(!snapshot.warn_is_fail);
/// ```
pub fn snapshot_policy(cfg: &CockpitConfig) -> PolicySnapshot {
    let mut sensors = Vec::new();
    for (id, p) in cfg.sensors.iter() {
        sensors.push(PolicySensorSnapshot {
            id: id.clone(),
            blocking: p.blocking,
            missing: p.missing,
            section: p.section.clone(),
            require_label: p.require_label.clone(),
            repro: p.repro.clone(),
        });
    }
    PolicySnapshot {
        warn_is_fail: cfg.policy.warn_is_fail,
        max_highlights: cfg.policy.max_highlights,
        max_per_sensor_findings: cfg.policy.max_per_sensor_findings,
        max_annotations: cfg.policy.max_annotations,
        section_order: cfg.policy.section_order.clone(),
        sensors,
    }
}

/// Derive the overall cockpit verdict from blocking sensor summaries.
///
/// # Examples
///
/// ```
/// use cockpitctl_domain::overall_verdict;
/// use cockpitctl_types::*;
///
/// let summaries = vec![SensorSummary {
///     id: "builddiag".into(),
///     blocking: true,
///     missing: MissingPolicy::Skip,
///     presence: Presence::Present,
///     report_path: "artifacts/builddiag/report.json".into(),
///     comment_path: None,
///     verdict: Verdict {
///         status: VerdictStatus::Fail,
///         counts: VerdictCounts { info: 0, warn: 0, error: 1, suppressed: 0 },
///         reasons: vec![],
///     },
///     truncated: false,
///     errors: vec![],
///     missing_policy_applied: None,
///     policy_outcome: Some(PolicyOutcome::Blocked),
/// }];
///
/// let cfg = CockpitConfig::default();
/// let verdict = overall_verdict(&summaries, &cfg);
/// assert_eq!(verdict.status, VerdictStatus::Fail);
/// ```
pub fn overall_verdict(sensor_summaries: &[SensorSummary], cfg: &CockpitConfig) -> Verdict {
    // Overall verdict is derived from blocking sensors only.
    // Status ordering: fail > warn > pass > skip
    let mut worst = VerdictStatus::Pass;
    let mut counts = VerdictCounts::default();
    let mut reasons: Vec<String> = Vec::new();

    for s in sensor_summaries {
        counts.info += s.verdict.counts.info;
        counts.warn += s.verdict.counts.warn;
        counts.error += s.verdict.counts.error;

        if !s.blocking {
            continue;
        }

        // warn-as-fail mapping:
        let mut effective_status = s.verdict.status.clone();
        if cfg.policy.warn_is_fail && matches!(effective_status, VerdictStatus::Warn) {
            effective_status = VerdictStatus::Fail;
            if !reasons.contains(&"warn_is_fail".to_string()) {
                reasons.push("warn_is_fail".to_string());
            }
        }

        if verdict_status_rank(&effective_status) < verdict_status_rank(&worst) {
            worst = effective_status;
        }
    }

    Verdict {
        status: worst,
        counts,
        reasons,
    }
}

/// Synthesize a sensor summary and optional highlight for a missing receipt.
///
/// The sensor's [`MissingPolicy`] controls the resulting verdict: `Skip` produces
/// no highlight, `Warn` emits a warning-level finding, and `Fail` emits an error.
/// The returned highlight (if any) carries code `cockpit.missing_receipt`.
///
/// # Examples
///
/// ```
/// use cockpitctl_domain::synthesize_missing_sensor;
/// use cockpitctl_types::*;
///
/// let policy = SensorPolicy { blocking: true, missing: MissingPolicy::Fail, ..Default::default() };
/// let (summary, highlight) = synthesize_missing_sensor(
///     "builddiag", &policy, "artifacts/builddiag/report.json", None,
/// );
/// assert_eq!(summary.presence, Presence::Missing);
/// assert_eq!(summary.verdict.status, VerdictStatus::Fail);
/// assert!(highlight.is_some());
///
/// // Skip produces no highlight.
/// let skip_policy = SensorPolicy { missing: MissingPolicy::Skip, ..Default::default() };
/// let (_, highlight) = synthesize_missing_sensor("s", &skip_policy, "path", None);
/// assert!(highlight.is_none());
/// ```
pub fn synthesize_missing_sensor(
    sensor_id: &str,
    policy: &SensorPolicy,
    report_path: &str,
    comment_path: Option<String>,
) -> (SensorSummary, Option<Highlight>) {
    let (status, severity, emit_finding) = match policy.missing {
        MissingPolicy::Skip => (VerdictStatus::Skip, Severity::Info, false),
        MissingPolicy::Warn => (VerdictStatus::Warn, Severity::Warn, true),
        MissingPolicy::Fail => (VerdictStatus::Fail, Severity::Error, true),
    };

    let finding = if emit_finding {
        Some(Finding {
            severity,
            check_id: Some("cockpit.missing_receipt".to_string()),
            code: cockpit_codes::MISSING_RECEIPT.to_string(),
            message: format!(
                "Expected receipt for sensor `{}` but it was not found at `{}`.",
                sensor_id, report_path
            ),
            location: None,
            help: Some(
                "Ensure the sensor ran and wrote artifacts/<sensor>/report.json.".to_string(),
            ),
            url: None,
            fingerprint: None,
            data: None,
        })
    } else {
        None
    };

    let verdict = Verdict {
        status,
        counts: finding
            .as_ref()
            .map(|f| compute_counts(std::slice::from_ref(f)))
            .unwrap_or_default(),
        reasons: if emit_finding {
            vec!["missing_receipt".to_string()]
        } else {
            vec![]
        },
    };

    let policy_outcome = compute_policy_outcome(policy.blocking, &verdict.status);

    let summary = SensorSummary {
        id: sensor_id.to_string(),
        blocking: policy.blocking,
        missing: policy.missing,
        presence: Presence::Missing,
        report_path: report_path.to_string(),
        comment_path,
        verdict,
        truncated: false,
        errors: vec![],
        missing_policy_applied: Some(policy.missing),
        policy_outcome: Some(policy_outcome),
    };

    let highlight = finding.map(|finding| Highlight {
        sensor_id: sensor_id.to_string(),
        finding,
    });

    (summary, highlight)
}

/// Synthesize a sensor summary and highlight for an unparseable (invalid JSON) receipt.
///
/// Always produces a `Fail` verdict with code `cockpit.invalid_receipt`. The raw
/// parse error is preserved in both the finding message and `SensorSummary::errors`.
///
/// # Examples
///
/// ```
/// use cockpitctl_domain::synthesize_invalid_sensor;
/// use cockpitctl_types::*;
///
/// let policy = SensorPolicy::default();
/// let (summary, highlight) = synthesize_invalid_sensor(
///     "bad", &policy, "artifacts/bad/report.json", None, "unexpected EOF".into(),
/// );
/// assert_eq!(summary.presence, Presence::Invalid);
/// assert_eq!(summary.verdict.status, VerdictStatus::Fail);
/// assert!(highlight.is_some());
/// assert!(summary.errors[0].contains("unexpected EOF"));
/// ```
pub fn synthesize_invalid_sensor(
    sensor_id: &str,
    policy: &SensorPolicy,
    report_path: &str,
    comment_path: Option<String>,
    error: String,
) -> (SensorSummary, Option<Highlight>) {
    let finding = Finding {
        severity: Severity::Error,
        check_id: Some("cockpit.invalid_receipt".to_string()),
        code: cockpit_codes::INVALID_RECEIPT.to_string(),
        message: format!(
            "Invalid receipt for sensor `{}` at `{}`: {}",
            sensor_id, report_path, error
        ),
        location: None,
        help: Some("Validate that the sensor wrote JSON matching sensor.report.v1.".to_string()),
        url: None,
        fingerprint: None,
        data: None,
    };

    let verdict = Verdict {
        status: VerdictStatus::Fail,
        counts: compute_counts(std::slice::from_ref(&finding)),
        reasons: vec!["invalid_receipt".to_string()],
    };

    let policy_outcome = compute_policy_outcome(policy.blocking, &VerdictStatus::Fail);

    let summary = SensorSummary {
        id: sensor_id.to_string(),
        blocking: policy.blocking,
        missing: policy.missing,
        presence: Presence::Invalid,
        report_path: report_path.to_string(),
        comment_path,
        verdict,
        truncated: false,
        errors: vec![error],
        missing_policy_applied: None,
        policy_outcome: Some(policy_outcome),
    };

    let highlight = Some(Highlight {
        sensor_id: sensor_id.to_string(),
        finding,
    });

    (summary, highlight)
}

/// Synthesize a sensor summary when the receipt violates the JSON schema.
///
/// This is distinct from `synthesize_invalid_sensor` (JSON parse error): here the
/// receipt is valid JSON but does not conform to the `sensor.report.v1` schema
/// (e.g., missing required fields). All validation errors are collected into a single
/// finding with code `cockpit.schema_violation`.
pub fn synthesize_schema_violation_sensor(
    sensor_id: &str,
    policy: &SensorPolicy,
    report_path: &str,
    comment_path: Option<String>,
    validation_errors: Vec<String>,
) -> (SensorSummary, Option<Highlight>) {
    let error_summary = if validation_errors.len() == 1 {
        validation_errors[0].clone()
    } else {
        format!(
            "{} schema violations: {}",
            validation_errors.len(),
            validation_errors.join("; ")
        )
    };

    let finding = Finding {
        severity: Severity::Error,
        check_id: Some("cockpit.schema_violation".to_string()),
        code: cockpit_codes::SCHEMA_VIOLATION.to_string(),
        message: format!(
            "Receipt for sensor `{}` at `{}` does not conform to sensor.report.v1 schema: {}",
            sensor_id, report_path, error_summary
        ),
        location: None,
        help: Some(
            "Ensure the sensor output matches the JSON schema at contracts/schemas/sensor.report.v1.json."
                .to_string(),
        ),
        url: None,
        fingerprint: None,
        data: None,
    };

    let verdict = Verdict {
        status: VerdictStatus::Fail,
        counts: compute_counts(std::slice::from_ref(&finding)),
        reasons: vec!["schema_violation".to_string()],
    };

    let policy_outcome = compute_policy_outcome(policy.blocking, &VerdictStatus::Fail);

    let summary = SensorSummary {
        id: sensor_id.to_string(),
        blocking: policy.blocking,
        missing: policy.missing,
        presence: Presence::Invalid,
        report_path: report_path.to_string(),
        comment_path,
        verdict,
        truncated: false,
        errors: validation_errors,
        missing_policy_applied: None,
        policy_outcome: Some(policy_outcome),
    };

    let highlight = Some(Highlight {
        sensor_id: sensor_id.to_string(),
        finding,
    });

    (summary, highlight)
}

/// Synthesize a cockpit-level highlight for an unsafe path traversal attempt.
///
/// Emits an error-severity finding with code `cockpit.path_traversal`, attributed
/// to the `_cockpit` pseudo-sensor. The fingerprint is deterministic:
/// `cockpit.path_traversal:<sensor_id>:<path>`.
///
/// # Examples
///
/// ```
/// use cockpitctl_domain::synthesize_path_traversal_highlight;
/// use cockpitctl_types::Severity;
///
/// let h = synthesize_path_traversal_highlight("../evil", "artifacts/../evil/report.json", None);
/// assert_eq!(h.sensor_id, "_cockpit");
/// assert_eq!(h.finding.severity, Severity::Error);
/// assert!(h.finding.code.contains("path_traversal"));
/// ```
pub fn synthesize_path_traversal_highlight(
    sensor_id: &str,
    path: &str,
    context: Option<String>,
) -> Highlight {
    let detail = context
        .map(|c| format!(" (unsafe {})", c))
        .unwrap_or_default();
    let finding = Finding {
        severity: Severity::Error,
        check_id: Some("cockpit.path_traversal".to_string()),
        code: cockpit_codes::PATH_TRAVERSAL.to_string(),
        message: format!(
            "Rejected unsafe path for sensor `{}` at `{}`{}.",
            sensor_id, path, detail
        ),
        location: None,
        help: Some(
            "Ensure sensor IDs and artifact paths do not contain path traversal or escape the artifacts root.".to_string(),
        ),
        url: None,
        fingerprint: Some(format!("cockpit.path_traversal:{}:{}", sensor_id, path)),
        data: None,
    };
    Highlight {
        sensor_id: "_cockpit".to_string(),
        finding,
    }
}

/// Synthesize a sensor summary for an unsafe path traversal attempt.
///
/// Returns a `Fail` / `Blocked` summary and the path-traversal highlight.
/// The sensor is recorded with `Presence::Missing` since no receipt was loaded.
pub fn synthesize_path_traversal_sensor(
    sensor_id: &str,
    policy: &SensorPolicy,
    report_path: &str,
    comment_path: Option<String>,
    context: Option<String>,
) -> (SensorSummary, Highlight) {
    let highlight = synthesize_path_traversal_highlight(sensor_id, report_path, context);

    let verdict = Verdict {
        status: VerdictStatus::Fail,
        counts: compute_counts(std::slice::from_ref(&highlight.finding)),
        reasons: vec!["path_traversal".to_string()],
    };

    let summary = SensorSummary {
        id: sensor_id.to_string(),
        blocking: policy.blocking,
        missing: policy.missing,
        presence: Presence::Missing,
        report_path: report_path.to_string(),
        comment_path,
        verdict,
        truncated: false,
        errors: vec![format!("unsafe path rejected: {}", report_path)],
        missing_policy_applied: None,
        policy_outcome: Some(PolicyOutcome::Blocked),
    };

    (summary, highlight)
}

/// Synthesize a sensor summary for an oversized receipt.
///
/// Emits an error-severity finding with code `cockpit.receipt_oversized` that
/// includes the actual file size and the configured cap. The summary's
/// `PolicyOutcome` is always `Blocked`.
pub fn synthesize_receipt_oversized_sensor(
    sensor_id: &str,
    policy: &SensorPolicy,
    report_path: &str,
    comment_path: Option<String>,
    size: u64,
    cap: usize,
) -> (SensorSummary, Highlight) {
    let finding = Finding {
        severity: Severity::Error,
        check_id: Some("cockpit.receipt_oversized".to_string()),
        code: cockpit_codes::RECEIPT_OVERSIZED.to_string(),
        message: format!(
            "Receipt for sensor `{}` exceeds size limit ({} bytes > {} bytes) at `{}`.",
            sensor_id, size, cap, report_path
        ),
        location: None,
        help: Some("Reduce receipt size or increase the max receipt size limit.".to_string()),
        url: None,
        fingerprint: Some(format!("cockpit.receipt_oversized:{}:{}", sensor_id, size)),
        data: None,
    };

    let verdict = Verdict {
        status: VerdictStatus::Fail,
        counts: compute_counts(std::slice::from_ref(&finding)),
        reasons: vec!["receipt_oversized".to_string()],
    };

    let policy_outcome = compute_policy_outcome(policy.blocking, &VerdictStatus::Fail);

    let summary = SensorSummary {
        id: sensor_id.to_string(),
        blocking: policy.blocking,
        missing: policy.missing,
        presence: Presence::Invalid,
        report_path: report_path.to_string(),
        comment_path,
        verdict,
        truncated: false,
        errors: vec![format!("receipt too large: {} bytes (cap {})", size, cap)],
        missing_policy_applied: None,
        policy_outcome: Some(policy_outcome),
    };

    let highlight = Highlight {
        sensor_id: "_cockpit".to_string(),
        finding,
    };

    (summary, highlight)
}

/// Synthesize an informational highlight when receipt counts are inconsistent.
///
/// This is an `Info`-severity finding (not blocking) that alerts when a sensor's
/// self-reported verdict counts don't match the actual findings array. The
/// fingerprint encodes all six count values for deterministic deduplication.
///
/// # Examples
///
/// ```
/// use cockpitctl_domain::synthesize_receipt_inconsistent;
/// use cockpitctl_types::{Severity, VerdictCounts};
///
/// let reported = VerdictCounts { info: 5, warn: 0, error: 0, suppressed: 0 };
/// let computed = VerdictCounts { info: 3, warn: 0, error: 0, suppressed: 0 };
/// let h = synthesize_receipt_inconsistent("sensor-a", &reported, &computed);
/// assert_eq!(h.finding.severity, Severity::Info);
/// assert!(h.finding.message.contains("sensor-a"));
/// ```
pub fn synthesize_receipt_inconsistent(
    sensor_id: &str,
    reported: &VerdictCounts,
    computed: &VerdictCounts,
) -> Highlight {
    let finding = Finding {
        severity: Severity::Info,
        check_id: Some("cockpit.receipt_inconsistent".to_string()),
        code: cockpit_codes::RECEIPT_INCONSISTENT.to_string(),
        message: format!(
            "Receipt counts for sensor `{}` did not match findings: reported info={}, warn={}, error={}, computed info={}, warn={}, error={}.",
            sensor_id,
            reported.info,
            reported.warn,
            reported.error,
            computed.info,
            computed.warn,
            computed.error
        ),
        location: None,
        help: Some("Ensure receipt verdict counts match the findings array.".to_string()),
        url: None,
        fingerprint: Some(format!(
            "cockpit.receipt_inconsistent:{}:{}:{}:{}:{}:{}:{}",
            sensor_id,
            reported.info,
            reported.warn,
            reported.error,
            computed.info,
            computed.warn,
            computed.error
        )),
        data: None,
    };
    Highlight {
        sensor_id: sensor_id.to_string(),
        finding,
    }
}

/// Construct a cockpit report from sensor reports and synthesized summaries.
///
/// # Examples
///
/// ```
/// use cockpitctl_domain::build_cockpit_report;
/// use cockpitctl_types::*;
/// use std::collections::BTreeMap;
///
/// let cfg = CockpitConfig::default();
/// let tool = ToolInfo { name: "cockpitctl".into(), version: "0.1.0".into(), commit: None };
/// let run = RunInfo {
///     started_at: "2026-01-01T00:00:00Z".into(),
///     ended_at: None, duration_ms: None, host: None,
///     git: None, ci: None, capabilities: BTreeMap::new(),
/// };
///
/// let report = build_cockpit_report(&cfg, tool, run, vec![], vec![]);
/// assert_eq!(report.schema, "cockpit.report.v1");
/// assert_eq!(report.verdict.status, VerdictStatus::Pass);
/// ```
pub fn build_cockpit_report(
    cfg: &CockpitConfig,
    tool: ToolInfo,
    run: RunInfo,
    sensor_summaries: Vec<SensorSummary>,
    highlights: Vec<Highlight>,
) -> CockpitReport {
    let policy_snapshot = snapshot_policy(cfg);
    let verdict = overall_verdict(&sensor_summaries, cfg);

    CockpitReport {
        schema: COCKPIT_SCHEMA_ID.to_string(),
        tool,
        run,
        verdict,
        sensors: sensor_summaries,
        highlights,
        policy: policy_snapshot,
        data: None,
    }
}

/// Create a warning highlight when sensor discovery was truncated due to the `max_receipts` cap.
///
/// Attributed to the `_cockpit` pseudo-sensor. Serves as a safety-limit notice so
/// operators know that not all discovered sensors were processed.
///
/// # Examples
///
/// ```
/// use cockpitctl_domain::synthesize_sensors_truncated;
/// use cockpitctl_types::Severity;
///
/// let h = synthesize_sensors_truncated(50, 120);
/// assert_eq!(h.sensor_id, "_cockpit");
/// assert_eq!(h.finding.severity, Severity::Warn);
/// assert!(h.finding.message.contains("50"));
/// assert!(h.finding.message.contains("120"));
/// ```
pub fn synthesize_sensors_truncated(processed: usize, total_found: usize) -> Highlight {
    let finding = Finding {
        severity: Severity::Warn,
        check_id: Some("cockpit.sensors_truncated".to_string()),
        code: cockpit_codes::SENSORS_TRUNCATED.to_string(),
        message: format!(
            "Sensor discovery was truncated: processed {} of {} sensors found. Increase max_receipts limit if needed.",
            processed, total_found
        ),
        location: None,
        help: Some(
            "This is a safety limit to prevent DoS. Consider reviewing why so many sensors exist."
                .to_string(),
        ),
        url: None,
        fingerprint: Some(format!(
            "cockpit.sensors_truncated:{}:{}",
            processed, total_found
        )),
        data: None,
    };
    Highlight {
        sensor_id: "_cockpit".to_string(),
        finding,
    }
}

/// Convert a parsed sensor report into a cockpit sensor summary.
///
/// Sorts findings deterministically, caps them at `max_findings`, recomputes
/// verdict counts from the capped list, and extracts candidate highlights.
/// If self-reported counts differ from computed counts a
/// `cockpit.receipt_inconsistent` highlight is added.
pub fn summarize_sensor_report(
    sensor_id: &str,
    report_path: &str,
    comment_path: Option<String>,
    policy: &SensorPolicy,
    mut report: SensorReport,
    max_findings: usize,
) -> (SensorSummary, Vec<Highlight>) {
    // Sort and cap findings for surfacing.
    sort_findings(sensor_id, &mut report.findings);
    let (surfaced, truncated) = cap_findings(report.findings.clone(), max_findings);

    // Recompute counts from surfaced findings (not the full report).
    // This matches the PR surface behavior; raw sensor report remains the record.
    let reported = report.verdict.counts.clone();
    let computed = compute_counts(&surfaced);
    let mut verdict = report.verdict.clone();
    let mut highlights = Vec::new();
    if verdict.counts != computed {
        verdict.reasons.push("receipt_inconsistent".to_string());
        verdict.counts = computed;
        highlights.push(synthesize_receipt_inconsistent(
            sensor_id,
            &reported,
            &verdict.counts,
        ));
    }

    let policy_outcome = compute_policy_outcome(policy.blocking, &verdict.status);

    let summary = SensorSummary {
        id: sensor_id.to_string(),
        blocking: policy.blocking,
        missing: policy.missing,
        presence: Presence::Present,
        report_path: report_path.to_string(),
        comment_path,
        verdict,
        truncated,
        errors: vec![],
        missing_policy_applied: None,
        policy_outcome: Some(policy_outcome),
    };

    for f in surfaced {
        highlights.push(Highlight {
            sensor_id: sensor_id.to_string(),
            finding: f,
        });
    }

    (summary, highlights)
}
