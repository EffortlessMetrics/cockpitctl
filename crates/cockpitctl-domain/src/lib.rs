//! Domain logic for cockpitctl:
//! - policy evaluation
//! - missing/invalid receipt handling rules
//! - deterministic ordering + highlight selection
//!
//! No filesystem, no clap, no network.

use cockpitctl_types::{
    CockpitConfig, CockpitReport, Finding, FindingSortKey, Highlight, MissingPolicy,
    PolicySnapshot, PolicySensorSnapshot, SensorPolicy, SensorReport, SensorSummary, Severity,
    ToolInfo, Verdict, VerdictCounts, VerdictStatus, RunInfo, severity_rank, verdict_status_rank,
};
use sha2::{Digest, Sha256};

pub const COCKPIT_SCHEMA_ID: &str = "cockpit.report.v1";

/// A cockpit-level code for synthesized findings.
pub mod cockpit_codes {
    pub const MISSING_RECEIPT: &str = "cockpit.missing_receipt";
    pub const INVALID_RECEIPT: &str = "cockpit.invalid_receipt";
    pub const RECEIPT_INCONSISTENT: &str = "cockpit.receipt_inconsistent";
}

/// Normalize and cap a sensor report's findings for cockpit surfacing.
///
/// The original report remains the record; cockpit surfaces only up to policy limits.
pub fn cap_findings(mut findings: Vec<Finding>, max: usize) -> (Vec<Finding>, bool) {
    if findings.len() <= max {
        return (findings, false);
    }
    findings.truncate(max);
    (findings, true)
}

/// Compute counts from findings.
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

pub fn sort_findings(sensor_id: &str, findings: &mut [Finding]) {
    findings.sort_by(|a, b| finding_sort_key(sensor_id, a).cmp(&finding_sort_key(sensor_id, b)));
}

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
            a.finding.location.as_ref().and_then(|l| l.path.clone()).unwrap_or_default(),
            a.finding.location.as_ref().and_then(|l| l.line).unwrap_or(u32::MAX),
            a.finding.code.clone(),
            a.finding.message.clone(),
        );
        let b_key = (
            severity_rank(&b.finding.severity),
            if b_block { 0u8 } else { 1u8 },
            b.sensor_id.clone(),
            b.finding.location.as_ref().and_then(|l| l.path.clone()).unwrap_or_default(),
            b.finding.location.as_ref().and_then(|l| l.line).unwrap_or(u32::MAX),
            b.finding.code.clone(),
            b.finding.message.clone(),
        );

        a_key.cmp(&b_key)
    });

    deduped.truncate(cfg.policy.max_highlights);
    deduped
}

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

pub fn overall_verdict(
    sensor_summaries: &[SensorSummary],
    cfg: &CockpitConfig,
) -> Verdict {
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
            reasons.push(format!("warn_is_fail:{}",
                s.id
            ));
        }

        if verdict_status_rank(&effective_status) < verdict_status_rank(&worst) {
            worst = effective_status;
        }
    }

    Verdict { status: worst, counts, reasons }
}

pub fn synthesize_missing_sensor(
    sensor_id: &str,
    policy: &SensorPolicy,
    report_path: &str,
    comment_path: Option<String>,
) -> (SensorSummary, Option<Highlight>) {
    let (status, severity, code) = match policy.missing {
        MissingPolicy::Skip => (VerdictStatus::Skip, Severity::Info, cockpit_codes::MISSING_RECEIPT),
        MissingPolicy::Warn => (VerdictStatus::Warn, Severity::Warn, cockpit_codes::MISSING_RECEIPT),
        MissingPolicy::Fail => (VerdictStatus::Fail, Severity::Error, cockpit_codes::MISSING_RECEIPT),
    };

    let finding = Finding {
        severity,
        check_id: Some("cockpit.missing_receipt".to_string()),
        code: code.to_string(),
        message: format!("Expected receipt for sensor `{}` but it was not found at `{}`.", sensor_id, report_path),
        location: None,
        help: Some("Ensure the sensor ran and wrote artifacts/<sensor>/report.json.".to_string()),
        url: None,
        fingerprint: None,
        data: None,
    };

    let verdict = Verdict {
        status,
        counts: compute_counts(&[finding.clone()]),
        reasons: vec!["missing_receipt".to_string()],
    };

    let summary = SensorSummary {
        id: sensor_id.to_string(),
        blocking: policy.blocking,
        missing: policy.missing,
        present: false,
        report_path: report_path.to_string(),
        comment_path,
        verdict,
        truncated: false,
        errors: vec![],
    };

    let highlight = Some(Highlight { sensor_id: sensor_id.to_string(), finding });

    (summary, highlight)
}

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
        message: format!("Invalid receipt for sensor `{}` at `{}`: {}", sensor_id, report_path, error),
        location: None,
        help: Some("Validate that the sensor wrote JSON matching sensor.report.v1.".to_string()),
        url: None,
        fingerprint: None,
        data: None,
    };

    let verdict = Verdict {
        status: VerdictStatus::Fail,
        counts: compute_counts(&[finding.clone()]),
        reasons: vec!["invalid_receipt".to_string()],
    };

    let summary = SensorSummary {
        id: sensor_id.to_string(),
        blocking: policy.blocking,
        missing: policy.missing,
        present: false,
        report_path: report_path.to_string(),
        comment_path,
        verdict,
        truncated: false,
        errors: vec![error],
    };

    let highlight = Some(Highlight { sensor_id: sensor_id.to_string(), finding });

    (summary, highlight)
}

/// Construct a cockpit report from sensor reports and synthesized summaries.
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

/// Convert a parsed sensor report into a cockpit sensor summary.
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
    let computed = compute_counts(&surfaced);
    let mut verdict = report.verdict.clone();
    if verdict.counts != computed {
        verdict.reasons.push(cockpit_codes::RECEIPT_INCONSISTENT.to_string());
        verdict.counts = computed;
    }

    let summary = SensorSummary {
        id: sensor_id.to_string(),
        blocking: policy.blocking,
        missing: policy.missing,
        present: true,
        report_path: report_path.to_string(),
        comment_path,
        verdict,
        truncated,
        errors: vec![],
    };

    let mut highlights = Vec::new();
    for f in surfaced {
        highlights.push(Highlight { sensor_id: sensor_id.to_string(), finding: f });
    }

    (summary, highlights)
}
