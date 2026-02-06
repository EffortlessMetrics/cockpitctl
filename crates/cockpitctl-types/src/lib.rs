//! DTOs and stable primitives for cockpitctl.
//!
//! This crate is intentionally boring:
//! - pure data structures
//! - stable IDs and enums
//! - deterministic ordering helpers
//!
//! It must not depend on filesystem, clap, or network.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Embedded JSON Schema for sensor.report.v1.
pub const SENSOR_REPORT_V1_SCHEMA_JSON: &str = include_str!("../schemas/sensor.report.v1.json");

/// Embedded JSON Schema for cockpit.report.v1.
pub const COCKPIT_REPORT_V1_SCHEMA_JSON: &str = include_str!("../schemas/cockpit.report.v1.json");

/// Embedded JSON Schema for buildfix.plan.v1.
pub const BUILDFIX_PLAN_V1_SCHEMA_JSON: &str = include_str!("../schemas/buildfix.plan.v1.json");

/// A schema identifier string, e.g. `builddiag.report.v1`.
pub type SchemaId = String;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VerdictStatus {
    Pass,
    Warn,
    Fail,
    Skip,
}

fn is_zero(v: &u64) -> bool {
    *v == 0
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct VerdictCounts {
    pub info: u64,
    pub warn: u64,
    pub error: u64,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub suppressed: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Verdict {
    pub status: VerdictStatus,
    pub counts: VerdictCounts,
    #[serde(default)]
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolInfo {
    pub name: String,
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HostInfo {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub os: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hostname: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GitInfo {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub head_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_sha: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub head_sha: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merge_base: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CiInfo {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityStatus {
    Available,
    Unavailable,
    Skipped,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Capability {
    pub status: CapabilityStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RunInfo {
    pub started_at: String, // RFC3339
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<HostInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git: Option<GitInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ci: Option<CiInfo>,
    /// Declared capabilities (e.g., "git", "baseline", "lcov").
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub capabilities: BTreeMap<String, Capability>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Location {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub col: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Finding {
    pub severity: Severity,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub check_id: Option<String>,
    pub code: String,
    pub message: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<Location>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub help: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

/// The shared receipt envelope for sensors.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SensorReport {
    pub schema: SchemaId,
    pub tool: ToolInfo,
    pub run: RunInfo,
    pub verdict: Verdict,
    #[serde(default)]
    pub findings: Vec<Finding>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

/// Missing receipt behavior (policy).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum MissingPolicy {
    #[default]
    Skip,
    Warn,
    Fail,
}

/// Schema validation mode for sensor receipts.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum SchemaValidation {
    /// Skip schema validation; only parse as JSON (default).
    #[default]
    Lax,
    /// Validate receipts against the JSON schema.
    Strict,
}

/// A per-sensor policy in cockpit.toml.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct SensorPolicy {
    #[serde(default)]
    pub blocking: bool,
    #[serde(default)]
    pub missing: MissingPolicy,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub section: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub require_label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repro: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Policy {
    #[serde(default)]
    pub warn_is_fail: bool,
    #[serde(default = "default_max_highlights")]
    pub max_highlights: usize,
    #[serde(default = "default_max_per_sensor_findings")]
    pub max_per_sensor_findings: usize,
    #[serde(default = "default_max_annotations")]
    pub max_annotations: usize,
    #[serde(default = "default_section_order")]
    pub section_order: Vec<String>,
    /// Schema validation mode: "lax" (default) skips schema validation; "strict"
    /// validates receipts against the embedded sensor.report.v1 schema.
    #[serde(default)]
    pub schema_validation: SchemaValidation,
}

fn default_max_highlights() -> usize {
    7
}
fn default_max_per_sensor_findings() -> usize {
    20
}
fn default_max_annotations() -> usize {
    25
}
fn default_section_order() -> Vec<String> {
    vec![
        "Highlights".into(),
        "Repo contract".into(),
        "Dependencies".into(),
        "Policy".into(),
        "Tests".into(),
        "Diagnostics".into(),
        "Performance".into(),
        "Environment".into(),
        "Other".into(),
    ]
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            warn_is_fail: false,
            max_highlights: default_max_highlights(),
            max_per_sensor_findings: default_max_per_sensor_findings(),
            max_annotations: default_max_annotations(),
            section_order: default_section_order(),
            schema_validation: SchemaValidation::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct CockpitConfig {
    #[serde(default)]
    pub policy: Policy,
    #[serde(default)]
    pub sensors: std::collections::BTreeMap<String, SensorPolicy>,
}

/// A single sensor row in the cockpit aggregate report.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SensorSummary {
    pub id: String,
    pub blocking: bool,
    pub missing: MissingPolicy,
    pub present: bool,
    pub report_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment_path: Option<String>,
    pub verdict: Verdict,
    #[serde(default)]
    pub truncated: bool,
    #[serde(default)]
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Highlight {
    pub sensor_id: String,
    pub finding: Finding,
}

/// The director output: cockpit.report.v1.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CockpitReport {
    pub schema: SchemaId,
    pub tool: ToolInfo,
    pub run: RunInfo,
    pub verdict: Verdict,
    pub sensors: Vec<SensorSummary>,
    pub highlights: Vec<Highlight>,
    pub policy: PolicySnapshot,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PolicySnapshot {
    pub warn_is_fail: bool,
    pub max_highlights: usize,
    pub max_per_sensor_findings: usize,
    pub max_annotations: usize,
    pub section_order: Vec<String>,
    pub sensors: Vec<PolicySensorSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PolicySensorSnapshot {
    pub id: String,
    pub blocking: bool,
    pub missing: MissingPolicy,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub section: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub require_label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repro: Option<String>,
}

/// A stable key used for sorting findings deterministically.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct FindingSortKey {
    pub severity_rank: u8,
    pub sensor_id: String,
    pub path: String,
    pub line: u32,
    pub code: String,
    pub message: String,
}

pub fn severity_rank(s: &Severity) -> u8 {
    match s {
        Severity::Error => 0,
        Severity::Warn => 1,
        Severity::Info => 2,
    }
}

pub fn verdict_status_rank(s: &VerdictStatus) -> u8 {
    match s {
        VerdictStatus::Fail => 0,
        VerdictStatus::Warn => 1,
        VerdictStatus::Pass => 2,
        VerdictStatus::Skip => 3,
    }
}

/// Validate a sensor ID for safe path usage.
pub fn is_valid_sensor_id(id: &str) -> bool {
    !id.is_empty() && !id.contains("..") && !id.contains('/') && !id.contains('\\')
}

// ============================================================================
// Buildfix types (actuator protocol)
// ============================================================================

/// Safety level for a fix.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SafetyLevel {
    /// No side effects; safe to apply automatically.
    Safe,
    /// Requires confirmation before applying.
    Guarded,
    /// May break things; use with caution.
    Unsafe,
}

/// Reference to a finding that a fix addresses.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FindingRef {
    pub sensor_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub check_id: Option<String>,
}

/// Preconditions that must hold before applying a fix.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Preconditions {
    pub repo_head: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub receipt_digests: Vec<String>,
}

/// A single fix in a buildfix plan.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Fix {
    pub id: String,
    pub safety: SafetyLevel,
    pub description: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub finding_refs: Vec<FindingRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preconditions: Option<Preconditions>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

/// A buildfix plan: a set of fixes to apply.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BuildfixPlan {
    pub schema: SchemaId,
    pub tool: ToolInfo,
    pub fixes: Vec<Fix>,
}
