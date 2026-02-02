//! Ingest use case (hexagonal boundary).
//!
//! This crate defines the core application flow in terms of ports (traits).
//! It does not touch filesystem directly; adapters provide IO.

use anyhow::{Context, Result};
use cockpitctl_domain::{
    build_cockpit_report, select_highlights, sort_sensor_summaries, summarize_sensor_report,
    synthesize_invalid_sensor, synthesize_missing_sensor,
};
use cockpitctl_types::{
    CockpitConfig, CockpitReport, MissingPolicy, SensorPolicy, ToolInfo, RunInfo,
};
use serde_json::Value;

/// Ports: where receipts come from.
pub trait ReceiptSource {
    /// Return a stable list of discovered sensor IDs that have a receipt file present.
    fn discovered_sensors(&self) -> Result<Vec<String>>;

    /// Read the report.json bytes for a sensor if present.
    fn read_report_bytes(&self, sensor_id: &str) -> Result<Option<Vec<u8>>>;

    /// Return canonical relative path to the sensor's report.json.
    fn report_path(&self, sensor_id: &str) -> String;

    /// Return canonical relative path to the sensor's comment.md if present.
    fn comment_path_if_present(&self, sensor_id: &str) -> Result<Option<String>>;
}

/// Ports: policy source (cockpit.toml).
pub trait PolicySource {
    fn load_config(&self) -> Result<Option<CockpitConfig>>;
}

/// Ports: where outputs are written.
pub trait OutputSink {
    fn write_cockpit_report(&self, json: &str) -> Result<()>;
    fn write_cockpit_comment(&self, md: &str) -> Result<()>;
}

/// Request inputs for ingestion.
pub struct IngestRequest {
    pub labels: Vec<String>, // optional; label-gates may use this
    pub tool: ToolInfo,
    pub run: RunInfo,
}

/// Result of ingestion, including the computed report and recommended exit code.
pub struct IngestResult {
    pub report: CockpitReport,
    pub comment_md: String,
    pub exit_code: i32,
}

pub struct IngestUseCase<R, P, O, RenderFn>
where
    R: ReceiptSource,
    P: PolicySource,
    O: OutputSink,
    RenderFn: Fn(&CockpitReport, &CockpitConfig) -> String,
{
    receipts: R,
    policy: P,
    output: O,
    render: RenderFn,
}

impl<R, P, O, RenderFn> IngestUseCase<R, P, O, RenderFn>
where
    R: ReceiptSource,
    P: PolicySource,
    O: OutputSink,
    RenderFn: Fn(&CockpitReport, &CockpitConfig) -> String,
{
    pub fn new(receipts: R, policy: P, output: O, render: RenderFn) -> Self {
        Self { receipts, policy, output, render }
    }

    pub fn execute(&self, req: IngestRequest) -> Result<IngestResult> {
        let discovered = self.receipts.discovered_sensors().context("discover sensors")?;

        let mut cfg = if let Some(cfg) = self.policy.load_config().context("load cockpit.toml")? {
            cfg
        } else {
            // Default policy: all discovered sensors are blocking, missing policy = skip.
            let mut cfg = CockpitConfig::default();
            for id in &discovered {
                cfg.sensors.insert(id.clone(), SensorPolicy {
                    blocking: true,
                    missing: MissingPolicy::Skip,
                    section: None,
                    require_label: None,
                    repro: None,
                });
            }
            cfg
        };

        // Expected sensors are those declared in policy; if empty, treat discovered as expected.
        let expected: Vec<String> = if !cfg.sensors.is_empty() {
            cfg.sensors.keys().cloned().collect()
        } else {
            discovered.clone()
        };

        let mut sensor_summaries = Vec::new();
        let mut highlight_candidates = Vec::new();

        // Cache blocking status for highlight sorting.
        let mut sensor_blocking = std::collections::BTreeMap::<String, bool>::new();

        for sensor_id in expected {
            let policy = cfg.sensors.get(&sensor_id).cloned().unwrap_or_default();
            sensor_blocking.insert(sensor_id.clone(), policy.blocking);

            // Label-gate: if require_label is set and not present, treat as skipped/missing=skip.
            if let Some(label) = &policy.require_label {
                if !req.labels.iter().any(|l| l == label) {
                    // Synthesized "skipped due to missing label"
                    let report_path = self.receipts.report_path(&sensor_id);
                    let comment_path = self.receipts.comment_path_if_present(&sensor_id).unwrap_or(None);
                    let mut p = policy.clone();
                    p.missing = MissingPolicy::Skip;
                    let (summary, _) = synthesize_missing_sensor(
                        &sensor_id,
                        &p,
                        &report_path,
                        comment_path,
                    );
                    sensor_summaries.push(summary);
                    continue;
                }
            }

            let report_path = self.receipts.report_path(&sensor_id);
            let comment_path = self.receipts.comment_path_if_present(&sensor_id)?;

            let bytes_opt = self.receipts.read_report_bytes(&sensor_id)?;
            let Some(bytes) = bytes_opt else {
                let (summary, h) = synthesize_missing_sensor(&sensor_id, &policy, &report_path, comment_path);
                sensor_summaries.push(summary);
                if let Some(h) = h { highlight_candidates.push(h); }
                continue;
            };

            // Parse sensor report. Invalid JSON is not a cockpitctl runtime error; it is a surfaced finding.
            match serde_json::from_slice::<cockpitctl_types::SensorReport>(&bytes) {
                Ok(report) => {
                    let (summary, highlights) = summarize_sensor_report(
                        &sensor_id,
                        &report_path,
                        comment_path,
                        &policy,
                        report,
                        cfg.policy.max_per_sensor_findings,
                    );
                    sensor_summaries.push(summary);
                    highlight_candidates.extend(highlights);
                }
                Err(e) => {
                    let (summary, h) = synthesize_invalid_sensor(
                        &sensor_id,
                        &policy,
                        &report_path,
                        comment_path,
                        e.to_string(),
                    );
                    sensor_summaries.push(summary);
                    if let Some(h) = h { highlight_candidates.push(h); }
                }
            }
        }

        sort_sensor_summaries(&mut sensor_summaries, &cfg);

        // Select highlights and cap.
        let highlights = select_highlights(highlight_candidates, &cfg, &sensor_blocking);

        let report = build_cockpit_report(
            &cfg,
            req.tool.clone(),
            req.run.clone(),
            sensor_summaries,
            highlights,
        );

        // Render comment.
        let comment_md = (self.render)(&report, &cfg);

        // Write outputs.
        let report_json = serde_json::to_string_pretty(&report).context("serialize cockpit report")?;
        self.output.write_cockpit_report(&report_json)?;
        self.output.write_cockpit_comment(&comment_md)?;

        // Map overall verdict to exit code (0 pass/warn allowed, 2 policy fail, 1 runtime error).
        let exit_code = match report.verdict.status {
            cockpitctl_types::VerdictStatus::Fail => 2,
            _ => 0,
        };

        Ok(IngestResult { report, comment_md, exit_code })
    }
}
