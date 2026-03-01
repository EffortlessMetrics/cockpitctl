//! Ingest use case (hexagonal boundary).
//!
//! This crate defines the core application flow in terms of ports (traits).
//! It does not touch filesystem directly; adapters provide IO.
//!
//! # Examples
//!
//! ```
//! use cockpitctl_ingest::{NoOpSchemaValidator, SchemaValidator, SchemaValidationResult};
//!
//! // The no-op validator always returns Valid (for lax mode).
//! let validator = NoOpSchemaValidator;
//! let result = validator.validate_receipt(b"{}").unwrap();
//! assert!(matches!(result, SchemaValidationResult::Valid));
//! ```

use anyhow::{Context, Result};
use cockpitctl_domain::{
    build_cockpit_report, match_buildfix_plan, select_highlights, sort_sensor_summaries,
    summarize_sensor_report, synthesize_invalid_sensor, synthesize_missing_sensor,
    synthesize_path_traversal_highlight, synthesize_path_traversal_sensor,
    synthesize_receipt_oversized_sensor, synthesize_schema_violation_sensor,
    synthesize_sensors_truncated,
};
use cockpitctl_types::{
    BuildfixSummary, CockpitConfig, CockpitReport, MissingPolicy, RunInfo, SchemaValidation,
    ToolInfo, is_valid_sensor_id,
};

/// Result of sensor discovery, including any truncation info.
pub struct DiscoveredSensors {
    /// Discovered sensor IDs (sorted lexically).
    pub sensors: Vec<String>,
    /// True if the number of sensors was capped due to max_receipts limit.
    pub truncated: bool,
    /// Total number of sensors found before truncation (if truncated).
    pub total_found: usize,
    /// Sensor IDs that were rejected due to invalid path traversal.
    pub invalid_sensor_ids: Vec<String>,
}

/// Result of reading a sensor report.
pub enum ReportRead {
    Missing,
    Bytes(Vec<u8>),
    Oversized { size: u64, cap: usize },
    UnsafePath,
}

/// Result of checking for a sensor comment.
pub enum CommentRead {
    Missing,
    Present(String),
    UnsafePath,
}

/// Result of reading a buildfix plan.
pub enum PlanRead {
    Missing,
    Bytes(Vec<u8>),
    Oversized { size: u64, cap: usize },
}

/// Ports: where receipts come from.
pub trait ReceiptSource {
    /// Return a stable list of discovered sensor IDs that have a receipt file present.
    /// May be truncated if the number of sensors exceeds a safety limit.
    fn discovered_sensors(&self) -> Result<DiscoveredSensors>;

    /// Read the report.json bytes for a sensor if present.
    fn read_report_bytes(&self, sensor_id: &str) -> Result<ReportRead>;

    /// Return canonical relative path to the sensor's report.json.
    fn report_path(&self, sensor_id: &str) -> String;

    /// Return canonical relative path to the sensor's comment.md if present.
    fn comment_path_if_present(&self, sensor_id: &str) -> Result<CommentRead>;

    /// Read the plan.json bytes for a sensor if present (buildfix integration).
    fn read_plan_bytes(&self, _sensor_id: &str) -> Result<PlanRead> {
        Ok(PlanRead::Missing)
    }
}

/// Ports: policy source (cockpit.toml).
pub trait PolicySource {
    fn load_config(&self) -> Result<Option<CockpitConfig>>;
}

/// Ports: where outputs are written.
pub trait OutputSink {
    fn write_cockpit_report(&self, json: &str) -> Result<()>;
    fn write_cockpit_comment(&self, md: &str) -> Result<()>;

    /// Write an extra file (e.g. from a post-processor hook). Default is a no-op.
    fn write_extra_file(&self, _name: &str, _content: &[u8]) -> Result<()> {
        Ok(())
    }
}

/// Result of JSON Schema validation.
pub enum SchemaValidationResult {
    /// Receipt conforms to the schema.
    Valid,
    /// Receipt violates the schema. Contains a list of validation error messages.
    Invalid(Vec<String>),
}

/// Ports: schema validation for receipts.
pub trait SchemaValidator {
    /// Validate raw receipt bytes against the sensor.report.v1 JSON schema.
    /// Returns validation result with detailed errors if invalid.
    fn validate_receipt(&self, bytes: &[u8]) -> Result<SchemaValidationResult>;
}

/// A no-op schema validator that always returns Valid (for lax mode).
pub struct NoOpSchemaValidator;

impl SchemaValidator for NoOpSchemaValidator {
    fn validate_receipt(&self, _bytes: &[u8]) -> Result<SchemaValidationResult> {
        Ok(SchemaValidationResult::Valid)
    }
}

/// Request inputs for ingestion.
pub struct IngestRequest {
    pub labels: Vec<String>, // optional; label-gates may use this
    pub tool: ToolInfo,
    pub run: RunInfo,
    pub schema_validation_override: Option<SchemaValidation>,
}

/// Result of ingestion, including the computed report and recommended exit code.
pub struct IngestResult {
    pub report: CockpitReport,
    pub comment_md: String,
    pub exit_code: i32,
    pub buildfix: Option<BuildfixSummary>,
}

pub struct IngestUseCase<R, P, O, S, RenderFn>
where
    R: ReceiptSource,
    P: PolicySource,
    O: OutputSink,
    S: SchemaValidator,
    RenderFn: Fn(&CockpitReport, &CockpitConfig) -> String,
{
    receipts: R,
    policy: P,
    output: O,
    schema_validator: S,
    render: RenderFn,
}

impl<R, P, O, S, RenderFn> IngestUseCase<R, P, O, S, RenderFn>
where
    R: ReceiptSource,
    P: PolicySource,
    O: OutputSink,
    S: SchemaValidator,
    RenderFn: Fn(&CockpitReport, &CockpitConfig) -> String,
{
    pub fn new(receipts: R, policy: P, output: O, schema_validator: S, render: RenderFn) -> Self {
        Self {
            receipts,
            policy,
            output,
            schema_validator,
            render,
        }
    }

    pub fn execute(&self, req: IngestRequest) -> Result<IngestResult> {
        let discovery = self
            .receipts
            .discovered_sensors()
            .context("discover sensors")?;
        let discovered = discovery.sensors;

        // Default policy: no expected sensors; discovered sensors are informational.
        let cfg: CockpitConfig = self
            .policy
            .load_config()
            .context("load cockpit.toml")?
            .unwrap_or_default();

        let effective_schema_validation = req
            .schema_validation_override
            .unwrap_or(cfg.policy.schema_validation);

        // Expected sensors are those declared in policy; if empty, treat discovered as expected.
        let expected: Vec<String> = if !cfg.sensors.is_empty() {
            cfg.sensors.keys().cloned().collect()
        } else {
            discovered.clone()
        };

        let mut sensor_summaries = Vec::new();
        let mut highlight_candidates = Vec::new();

        for invalid in &discovery.invalid_sensor_ids {
            let report_path = self.receipts.report_path(invalid);
            highlight_candidates.push(synthesize_path_traversal_highlight(
                invalid,
                &report_path,
                None,
            ));
        }

        // Add warning if sensor discovery was truncated.
        if discovery.truncated {
            highlight_candidates.push(synthesize_sensors_truncated(
                discovered.len(),
                discovery.total_found,
            ));
        }

        // Cache blocking status for highlight sorting.
        let mut sensor_blocking = std::collections::BTreeMap::<String, bool>::new();

        for sensor_id in expected {
            let policy = cfg.sensors.get(&sensor_id).cloned().unwrap_or_default();
            sensor_blocking.insert(sensor_id.clone(), policy.blocking);

            if !is_valid_sensor_id(&sensor_id) {
                let report_path = self.receipts.report_path(&sensor_id);
                let (summary, highlight) =
                    synthesize_path_traversal_sensor(&sensor_id, &policy, &report_path, None, None);
                sensor_summaries.push(summary);
                highlight_candidates.push(highlight);
                continue;
            }

            let mut comment_path: Option<String> = None;
            match self.receipts.comment_path_if_present(&sensor_id)? {
                CommentRead::Present(p) => comment_path = Some(p),
                CommentRead::Missing => {}
                CommentRead::UnsafePath => {
                    let unsafe_path = format!("artifacts/{}/comment.md", sensor_id);
                    highlight_candidates.push(synthesize_path_traversal_highlight(
                        &sensor_id,
                        &unsafe_path,
                        Some("comment.md".to_string()),
                    ));
                }
            }

            // Label-gate: if require_label is set and not present, treat as skipped/missing=skip.
            if let Some(label) = &policy.require_label
                && !req.labels.iter().any(|l| l == label)
            {
                // Synthesized "skipped due to missing label"
                let report_path = self.receipts.report_path(&sensor_id);
                let mut p = policy.clone();
                p.missing = MissingPolicy::Skip;
                let (summary, _) =
                    synthesize_missing_sensor(&sensor_id, &p, &report_path, comment_path);
                sensor_summaries.push(summary);
                continue;
            }

            let report_path = self.receipts.report_path(&sensor_id);

            let bytes = match self.receipts.read_report_bytes(&sensor_id)? {
                ReportRead::Missing => {
                    let (summary, h) =
                        synthesize_missing_sensor(&sensor_id, &policy, &report_path, comment_path);
                    sensor_summaries.push(summary);
                    if let Some(h) = h {
                        highlight_candidates.push(h);
                    }
                    continue;
                }
                ReportRead::UnsafePath => {
                    let (summary, highlight) = synthesize_path_traversal_sensor(
                        &sensor_id,
                        &policy,
                        &report_path,
                        comment_path,
                        Some("report.json".to_string()),
                    );
                    sensor_summaries.push(summary);
                    highlight_candidates.push(highlight);
                    continue;
                }
                ReportRead::Oversized { size, cap } => {
                    let (summary, highlight) = synthesize_receipt_oversized_sensor(
                        &sensor_id,
                        &policy,
                        &report_path,
                        comment_path,
                        size,
                        cap,
                    );
                    sensor_summaries.push(summary);
                    highlight_candidates.push(highlight);
                    continue;
                }
                ReportRead::Bytes(bytes) => bytes,
            };

            // Schema validation (if enabled).
            if matches!(effective_schema_validation, SchemaValidation::Strict) {
                match self.schema_validator.validate_receipt(&bytes)? {
                    SchemaValidationResult::Valid => {}
                    SchemaValidationResult::Invalid(errors) => {
                        let (summary, h) = synthesize_schema_violation_sensor(
                            &sensor_id,
                            &policy,
                            &report_path,
                            comment_path,
                            errors,
                        );
                        sensor_summaries.push(summary);
                        highlight_candidates
                            .push(h.expect("schema violation always yields a highlight"));
                        continue;
                    }
                }
            }

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
                    highlight_candidates
                        .push(h.expect("invalid receipt always yields a highlight"));
                }
            }
        }

        sort_sensor_summaries(&mut sensor_summaries, &cfg);

        // Select highlights and cap.
        let highlights = select_highlights(highlight_candidates, &cfg, &sensor_blocking);

        // Buildfix plan ingestion: read plan.json for each discovered sensor.
        let mut buildfix_summary: Option<BuildfixSummary> = None;
        let mut all_fixes = Vec::new();
        for sensor_id in &discovered {
            if let PlanRead::Bytes(bytes) = self.receipts.read_plan_bytes(sensor_id)?
                && let Ok(plan) = serde_json::from_slice::<cockpitctl_types::BuildfixPlan>(&bytes)
            {
                let summary = match_buildfix_plan(sensor_id, &plan, &highlights);
                all_fixes.extend(summary.fixes);
            }
        }
        if !all_fixes.is_empty() {
            let total_fixes = all_fixes.len();
            let unmatched_count = all_fixes.iter().filter(|f| f.unmatched).count();
            let matched_count = total_fixes - unmatched_count;
            buildfix_summary = Some(BuildfixSummary {
                fixes: all_fixes,
                total_fixes,
                matched_count,
                unmatched_count,
            });
        }

        let mut report = build_cockpit_report(
            &cfg,
            req.tool.clone(),
            req.run.clone(),
            sensor_summaries,
            highlights,
        );

        // Store buildfix summary in report.data if present.
        if let Some(ref bf) = buildfix_summary {
            let bf_value = serde_json::to_value(bf).ok();
            if let Some(val) = bf_value {
                let data = report
                    .data
                    .get_or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
                if let Some(obj) = data.as_object_mut() {
                    obj.insert("_buildfix".to_string(), val);
                }
            }
        }

        // Render comment.
        let comment_md = (self.render)(&report, &cfg);

        // Write outputs.
        let mut report_json =
            serde_json::to_string_pretty(&report).context("serialize cockpit report")?;
        report_json.push('\n'); // Ensure trailing newline for text file convention.
        self.output.write_cockpit_report(&report_json)?;
        self.output.write_cockpit_comment(&comment_md)?;

        // Map overall verdict to exit code (0 pass/warn allowed, 2 policy fail, 1 runtime error).
        let exit_code = match report.verdict.status {
            cockpitctl_types::VerdictStatus::Fail => 2,
            _ => 0,
        };

        Ok(IngestResult {
            report,
            comment_md,
            exit_code,
            buildfix: buildfix_summary,
        })
    }
}
