//! DTOs and stable primitives for cockpitctl.
//!
//! This crate is intentionally boring:
//! - pure data structures
//! - stable IDs and enums
//! - deterministic ordering helpers
//!
//! It must not depend on filesystem, clap, or network.

#![warn(missing_docs)]

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Embedded JSON Schema for sensor.report.v1.
pub const SENSOR_REPORT_V1_SCHEMA_JSON: &str = include_str!("../schemas/sensor.report.v1.json");

/// Embedded JSON Schema for cockpit.report.v1.
pub const COCKPIT_REPORT_V1_SCHEMA_JSON: &str = include_str!("../schemas/cockpit.report.v1.json");

/// Embedded JSON Schema for buildfix.plan.v1.
pub const BUILDFIX_PLAN_V1_SCHEMA_JSON: &str = include_str!("../schemas/buildfix.plan.v1.json");

/// Embedded JSON Schema for cockpit.promote.v1.
pub const COCKPIT_PROMOTE_V1_SCHEMA_JSON: &str = include_str!("../schemas/cockpit.promote.v1.json");

/// A schema identifier string, e.g. `builddiag.report.v1`.
pub type SchemaId = String;

/// Overall status of a sensor or cockpit verdict.
///
/// # Examples
///
/// ```
/// use cockpitctl_types::VerdictStatus;
///
/// let status = VerdictStatus::Pass;
/// assert_eq!(serde_json::to_string(&status).unwrap(), "\"pass\"");
///
/// let parsed: VerdictStatus = serde_json::from_str("\"fail\"").unwrap();
/// assert_eq!(parsed, VerdictStatus::Fail);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VerdictStatus {
    /// All checks passed.
    Pass,
    /// Non-fatal warnings were raised.
    Warn,
    /// One or more checks failed.
    Fail,
    /// The sensor was skipped (e.g., missing or label-gated).
    Skip,
}

fn is_zero(v: &u64) -> bool {
    *v == 0
}

/// Counts of findings by severity.
///
/// # Examples
///
/// ```
/// use cockpitctl_types::VerdictCounts;
///
/// let counts = VerdictCounts { info: 3, warn: 1, error: 0, suppressed: 0 };
/// assert_eq!(counts.info, 3);
/// assert_eq!(counts.warn, 1);
///
/// let default = VerdictCounts::default();
/// assert_eq!(default.info, 0);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct VerdictCounts {
    /// Number of info-level findings.
    pub info: u64,
    /// Number of warn-level findings.
    pub warn: u64,
    /// Number of error-level findings.
    pub error: u64,
    /// Number of suppressed findings.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub suppressed: u64,
}

/// Combined verdict including status, counts, and reason tokens.
///
/// # Examples
///
/// ```
/// use cockpitctl_types::{Verdict, VerdictCounts, VerdictStatus};
///
/// let verdict = Verdict {
///     status: VerdictStatus::Fail,
///     counts: VerdictCounts { info: 0, warn: 1, error: 2, suppressed: 0 },
///     reasons: vec!["missing_receipt".to_string()],
/// };
/// assert_eq!(verdict.status, VerdictStatus::Fail);
/// assert_eq!(verdict.counts.error, 2);
/// assert_eq!(verdict.reasons.len(), 1);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Verdict {
    /// Overall verdict status.
    pub status: VerdictStatus,
    /// Finding counts by severity.
    pub counts: VerdictCounts,
    /// Human-readable reason tokens.
    #[serde(default)]
    pub reasons: Vec<String>,
}

/// Metadata about the tool that produced a report.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolInfo {
    /// Tool name (e.g. "builddiag").
    pub name: String,
    /// Tool version string.
    pub version: String,
    /// Optional git commit SHA.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
}

/// Host environment information captured at run time.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HostInfo {
    /// Operating system name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub os: Option<String>,
    /// CPU architecture.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arch: Option<String>,
    /// Hostname of the build machine.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hostname: Option<String>,
}

/// Git repository context captured at run time.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GitInfo {
    /// Repository URL or slug.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo: Option<String>,
    /// Base branch ref.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_ref: Option<String>,
    /// Head branch ref.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub head_ref: Option<String>,
    /// Base commit SHA.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_sha: Option<String>,
    /// Head commit SHA.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub head_sha: Option<String>,
    /// Merge base commit SHA.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merge_base: Option<String>,
}

/// CI provider context captured at run time.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CiInfo {
    /// CI provider name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    /// CI run identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    /// URL to the CI run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_url: Option<String>,
    /// CI job name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job: Option<String>,
}

/// Availability status of a declared capability.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityStatus {
    /// The capability is available and was exercised.
    Available,
    /// The capability is not available in this environment.
    Unavailable,
    /// The capability was available but intentionally skipped.
    Skipped,
}

/// A declared runtime capability and its status.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Capability {
    /// Whether the capability was available.
    pub status: CapabilityStatus,
    /// Optional reason for the status.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Execution context for a sensor or cockpitctl run.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RunInfo {
    /// RFC 3339 timestamp when the run started.
    pub started_at: String, // RFC3339
    /// RFC 3339 timestamp when the run ended.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<String>,
    /// Wall-clock duration in milliseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    /// Host environment information.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<HostInfo>,
    /// Git repository context.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git: Option<GitInfo>,
    /// CI provider context.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ci: Option<CiInfo>,
    /// Declared capabilities (e.g., "git", "baseline", "lcov").
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub capabilities: BTreeMap<String, Capability>,
}

/// Finding severity level.
///
/// # Examples
///
/// ```
/// use cockpitctl_types::Severity;
///
/// let sev = Severity::Error;
/// assert_eq!(serde_json::to_string(&sev).unwrap(), "\"error\"");
///
/// let parsed: Severity = serde_json::from_str("\"warn\"").unwrap();
/// assert_eq!(parsed, Severity::Warn);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// Informational notice.
    Info,
    /// Non-fatal warning.
    Warn,
    /// Error that contributes to a fail verdict.
    Error,
}

/// Source location of a finding in a file.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Location {
    /// File path of the finding.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Line number in the file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    /// Column number in the file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub col: Option<u32>,
}

/// A single diagnostic finding produced by a sensor.
///
/// # Examples
///
/// ```
/// use cockpitctl_types::{Finding, Location, Severity};
///
/// let finding = Finding {
///     severity: Severity::Error,
///     check_id: Some("clippy.warning".into()),
///     code: "unused_var".into(),
///     message: "unused variable `x`".into(),
///     location: Some(Location { path: Some("src/lib.rs".into()), line: Some(42), col: None }),
///     help: Some("consider prefixing with `_`".into()),
///     url: None,
///     fingerprint: None,
///     data: None,
/// };
/// assert_eq!(finding.severity, Severity::Error);
/// assert_eq!(finding.location.as_ref().unwrap().line, Some(42));
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Finding {
    /// Severity level of the finding.
    pub severity: Severity,
    /// Optional check identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub check_id: Option<String>,
    /// Machine-readable finding code.
    pub code: String,
    /// Human-readable message.
    pub message: String,

    /// Source location of the finding.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<Location>,

    /// Suggested remediation text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub help: Option<String>,
    /// URL for more information.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Stable fingerprint for deduplication.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,

    /// Opaque tool-specific payload.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

/// The shared receipt envelope for sensors.
///
/// # Examples
///
/// ```
/// use cockpitctl_types::{SensorReport, ToolInfo, RunInfo, Verdict, VerdictCounts, VerdictStatus};
/// use std::collections::BTreeMap;
///
/// let report = SensorReport {
///     schema: "sensor.report.v1".into(),
///     tool: ToolInfo { name: "builddiag".into(), version: "1.0.0".into(), commit: None },
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
///     findings: vec![],
///     artifacts: vec![],
///     data: None,
/// };
///
/// // Round-trip through JSON
/// let json = serde_json::to_string(&report).unwrap();
/// let parsed: SensorReport = serde_json::from_str(&json).unwrap();
/// assert_eq!(parsed.schema, "sensor.report.v1");
/// assert_eq!(parsed.verdict.status, VerdictStatus::Pass);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SensorReport {
    /// Schema identifier string.
    pub schema: SchemaId,
    /// Metadata about the producing tool.
    pub tool: ToolInfo,
    /// Execution context.
    pub run: RunInfo,
    /// Aggregate verdict.
    pub verdict: Verdict,
    /// Diagnostic findings.
    #[serde(default)]
    pub findings: Vec<Finding>,
    /// Pointers to produced artifacts.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifacts: Vec<ArtifactPointer>,
    /// Opaque tool-specific payload.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

/// Missing receipt behavior (policy).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum MissingPolicy {
    /// Skip: treat missing receipt as non-blocking.
    #[default]
    Skip,
    /// Warn: report a warning for missing receipt.
    Warn,
    /// Fail: fail the cockpit verdict for missing receipt.
    Fail,
}

/// Presence state for a sensor in the cockpit report.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Presence {
    /// Sensor receipt was found.
    Present,
    /// Sensor receipt was not found.
    Missing,
    /// Sensor receipt was found but invalid.
    Invalid,
}

/// Policy outcome for a sensor after evaluation.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PolicyOutcome {
    /// Sensor blocked the merge.
    Blocked,
    /// Sensor allowed the merge.
    Allowed,
    /// Sensor is non-blocking (informational only).
    Informational,
}

/// Pointer to an artifact produced by a sensor.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArtifactPointer {
    /// Artifact identifier.
    pub id: String,
    /// Relative path to the artifact file.
    pub path: String,
    /// MIME type of the artifact.
    pub mime: String,
    /// Optional schema the artifact conforms to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
}

/// Promotion hints for cockpit (`data._cockpit`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CockpitPromoteHints {
    /// Schema identifier for the promote hints.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    /// Promoted summary cards.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cards: Vec<PromoteCard>,
    /// Suggested highlight fingerprints.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub suggested_highlights: Vec<SuggestedHighlight>,
    /// Suggested artifact pointers.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub suggested_artifacts: Vec<SuggestedArtifact>,
}

/// A card promoted to the cockpit summary from a sensor.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PromoteCard {
    /// Card identifier.
    pub id: String,
    /// Display label.
    pub label: String,
    /// Display value.
    pub value: String,
    /// Optional severity for visual styling.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub severity: Option<Severity>,
}

/// A suggested highlight from a sensor via promotion.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SuggestedHighlight {
    /// Fingerprint of the finding to highlight.
    pub finding_fingerprint: String,
}

/// A suggested artifact from a sensor via promotion.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SuggestedArtifact {
    /// Identifier of the suggested artifact.
    pub artifact_id: String,
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
    /// Whether this sensor blocks the merge.
    #[serde(default)]
    pub blocking: bool,
    /// Policy for missing receipts.
    #[serde(default)]
    pub missing: MissingPolicy,
    /// Comment section to render this sensor under.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub section: Option<String>,
    /// PR label required to activate this sensor.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub require_label: Option<String>,
    /// Reproduction command for failures.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repro: Option<String>,
}

/// Buildfix auto-apply policy in cockpit.toml.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BuildfixPolicy {
    /// Enable automatic fix application after ingest.
    #[serde(default)]
    pub auto_apply: bool,
    /// Maximum safety level allowed for auto-apply.
    #[serde(default = "default_buildfix_max_auto_apply_safety")]
    pub max_auto_apply_safety: SafetyLevel,
    /// Require each selected fix to match at least one surfaced finding.
    #[serde(default = "default_buildfix_require_matched_finding")]
    pub require_matched_finding: bool,
    /// Optional external actuator command used to apply selected fixes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actuator: Option<BuildfixActuatorConfig>,
}

impl Default for BuildfixPolicy {
    fn default() -> Self {
        Self {
            auto_apply: false,
            max_auto_apply_safety: default_buildfix_max_auto_apply_safety(),
            require_matched_finding: default_buildfix_require_matched_finding(),
            actuator: None,
        }
    }
}

/// External command configuration for buildfix actuation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BuildfixActuatorConfig {
    /// Shell command to invoke.
    pub command: String,
    /// Timeout in milliseconds.
    #[serde(default = "default_buildfix_actuator_timeout_ms")]
    pub timeout_ms: u64,
}

/// Global policy settings from `cockpit.toml`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Policy {
    /// Treat warn verdicts as fail.
    #[serde(default)]
    pub warn_is_fail: bool,
    /// Maximum number of highlights in the comment.
    #[serde(default = "default_max_highlights")]
    pub max_highlights: usize,
    /// Maximum findings per sensor before truncation.
    #[serde(default = "default_max_per_sensor_findings")]
    pub max_per_sensor_findings: usize,
    /// Maximum number of CI annotations.
    #[serde(default = "default_max_annotations")]
    pub max_annotations: usize,
    /// Ordered list of comment section names.
    #[serde(default = "default_section_order")]
    pub section_order: Vec<String>,
    /// Schema validation mode: "lax" (default) skips schema validation; "strict"
    /// validates receipts against the embedded sensor.report.v1 schema.
    #[serde(default)]
    pub schema_validation: SchemaValidation,
    /// Maximum receipt file size in bytes (default 2MB). Receipts exceeding this
    /// limit are rejected with a `cockpit.receipt_oversized` finding.
    #[serde(default = "default_max_receipt_size_bytes")]
    pub max_receipt_size_bytes: usize,
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
fn default_max_receipt_size_bytes() -> usize {
    2 * 1024 * 1024 // 2MB
}
fn default_buildfix_max_auto_apply_safety() -> SafetyLevel {
    SafetyLevel::Safe
}
fn default_buildfix_require_matched_finding() -> bool {
    true
}
fn default_buildfix_actuator_timeout_ms() -> u64 {
    30_000 // 30 seconds
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
            max_receipt_size_bytes: default_max_receipt_size_bytes(),
        }
    }
}

/// Top-level cockpit configuration deserialized from `cockpit.toml`.
///
/// # Examples
///
/// ```
/// use cockpitctl_types::CockpitConfig;
///
/// // Default config provides sensible defaults.
/// let cfg = CockpitConfig::default();
/// assert_eq!(cfg.policy.max_highlights, 7);
/// assert_eq!(cfg.policy.max_per_sensor_findings, 20);
/// assert!(!cfg.policy.warn_is_fail);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct CockpitConfig {
    /// Global policy settings.
    #[serde(default)]
    pub policy: Policy,
    /// Buildfix auto-apply policy.
    #[serde(default)]
    pub buildfix: BuildfixPolicy,
    /// Policy snapshot signing config.
    #[serde(default)]
    pub policy_signing: PolicySigningConfig,
    /// Per-sensor policy overrides.
    #[serde(default)]
    pub sensors: std::collections::BTreeMap<String, SensorPolicy>,
    /// Post-processing hooks.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hooks: Vec<HookConfig>,
}

/// A single sensor row in the cockpit aggregate report.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SensorSummary {
    /// Sensor identifier.
    pub id: String,
    /// Whether this sensor is blocking.
    pub blocking: bool,
    /// Policy for missing receipts.
    pub missing: MissingPolicy,
    /// Whether the sensor receipt was found.
    pub presence: Presence,
    /// Path to the sensor report file.
    pub report_path: String,
    /// Optional path to a sensor comment file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment_path: Option<String>,
    /// The sensor's verdict.
    pub verdict: Verdict,
    /// Whether findings were truncated.
    #[serde(default)]
    pub truncated: bool,
    /// Errors encountered while processing.
    #[serde(default)]
    pub errors: Vec<String>,
    /// Missing policy that was actually applied.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub missing_policy_applied: Option<MissingPolicy>,
    /// Evaluated policy outcome.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_outcome: Option<PolicyOutcome>,
}

/// A finding surfaced as a highlight in the cockpit report.
///
/// # Examples
///
/// ```
/// use cockpitctl_types::{Finding, Highlight, Severity};
///
/// let highlight = Highlight {
///     sensor_id: "builddiag".into(),
///     finding: Finding {
///         severity: Severity::Error,
///         check_id: None,
///         code: "build_error".into(),
///         message: "compilation failed".into(),
///         location: None,
///         help: None, url: None, fingerprint: None, data: None,
///     },
/// };
/// assert_eq!(highlight.sensor_id, "builddiag");
/// assert_eq!(highlight.finding.severity, Severity::Error);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Highlight {
    /// Sensor that produced the finding.
    pub sensor_id: String,
    /// The highlighted finding.
    pub finding: Finding,
}

/// The director output: cockpit.report.v1.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CockpitReport {
    /// Schema identifier string.
    pub schema: SchemaId,
    /// Metadata about the cockpitctl tool.
    pub tool: ToolInfo,
    /// Execution context.
    pub run: RunInfo,
    /// Aggregate cockpit verdict.
    pub verdict: Verdict,
    /// Sensor summaries.
    pub sensors: Vec<SensorSummary>,
    /// Highlighted findings.
    pub highlights: Vec<Highlight>,
    /// Snapshot of the evaluated policy.
    pub policy: PolicySnapshot,
    /// Opaque extension data.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

/// Snapshot of the policy configuration used during evaluation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PolicySnapshot {
    /// Whether warn was treated as fail.
    pub warn_is_fail: bool,
    /// Maximum highlights allowed.
    pub max_highlights: usize,
    /// Maximum findings per sensor.
    pub max_per_sensor_findings: usize,
    /// Maximum CI annotations.
    pub max_annotations: usize,
    /// Ordered section names.
    pub section_order: Vec<String>,
    /// Per-sensor policy snapshots.
    pub sensors: Vec<PolicySensorSnapshot>,
}

/// Per-sensor policy snapshot embedded in the cockpit report.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PolicySensorSnapshot {
    /// Sensor identifier.
    pub id: String,
    /// Whether the sensor is blocking.
    pub blocking: bool,
    /// Missing receipt policy.
    pub missing: MissingPolicy,
    /// Comment section name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub section: Option<String>,
    /// Required PR label.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub require_label: Option<String>,
    /// Reproduction command.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repro: Option<String>,
}

/// A stable key used for sorting findings deterministically.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct FindingSortKey {
    /// Numeric severity rank (lower = more severe).
    pub severity_rank: u8,
    /// Sensor that produced the finding.
    pub sensor_id: String,
    /// File path of the finding.
    pub path: String,
    /// Line number.
    pub line: u32,
    /// Finding code.
    pub code: String,
    /// Finding message.
    pub message: String,
}

/// Returns a numeric rank for a [`Severity`], where lower means more severe.
///
/// # Examples
///
/// ```
/// use cockpitctl_types::{Severity, severity_rank};
///
/// assert_eq!(severity_rank(&Severity::Error), 0);
/// assert_eq!(severity_rank(&Severity::Warn), 1);
/// assert_eq!(severity_rank(&Severity::Info), 2);
/// assert!(severity_rank(&Severity::Error) < severity_rank(&Severity::Warn));
/// ```
pub fn severity_rank(s: &Severity) -> u8 {
    match s {
        Severity::Error => 0,
        Severity::Warn => 1,
        Severity::Info => 2,
    }
}

/// Returns a numeric rank for a [`VerdictStatus`], where lower means worse.
///
/// # Examples
///
/// ```
/// use cockpitctl_types::{VerdictStatus, verdict_status_rank};
///
/// assert_eq!(verdict_status_rank(&VerdictStatus::Fail), 0);
/// assert_eq!(verdict_status_rank(&VerdictStatus::Pass), 2);
/// assert!(verdict_status_rank(&VerdictStatus::Fail) < verdict_status_rank(&VerdictStatus::Pass));
/// ```
pub fn verdict_status_rank(s: &VerdictStatus) -> u8 {
    match s {
        VerdictStatus::Fail => 0,
        VerdictStatus::Warn => 1,
        VerdictStatus::Pass => 2,
        VerdictStatus::Skip => 3,
    }
}

/// Validate a sensor ID for safe path usage.
///
/// Returns `true` if the ID is non-empty, contains no path separators or
/// traversal sequences, and consists only of ASCII alphanumerics, hyphens,
/// and underscores.
///
/// # Examples
///
/// ```
/// use cockpitctl_types::is_valid_sensor_id;
///
/// assert!(is_valid_sensor_id("builddiag"));
/// assert!(is_valid_sensor_id("my-sensor_v2"));
/// assert!(!is_valid_sensor_id(""));
/// assert!(!is_valid_sensor_id("../escape"));
/// assert!(!is_valid_sensor_id("bad/path"));
/// ```
pub fn is_valid_sensor_id(id: &str) -> bool {
    !id.is_empty()
        && !id.contains("..")
        && !id.contains('/')
        && !id.contains('\\')
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
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

/// Rank a safety level for deterministic ordering and gating.
/// Lower is safer.
///
/// # Examples
///
/// ```
/// use cockpitctl_types::{SafetyLevel, safety_level_rank};
///
/// assert_eq!(safety_level_rank(&SafetyLevel::Safe), 0);
/// assert_eq!(safety_level_rank(&SafetyLevel::Guarded), 1);
/// assert_eq!(safety_level_rank(&SafetyLevel::Unsafe), 2);
/// assert!(safety_level_rank(&SafetyLevel::Safe) < safety_level_rank(&SafetyLevel::Unsafe));
/// ```
pub fn safety_level_rank(s: &SafetyLevel) -> u8 {
    match s {
        SafetyLevel::Safe => 0,
        SafetyLevel::Guarded => 1,
        SafetyLevel::Unsafe => 2,
    }
}

/// Reference to a finding that a fix addresses.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FindingRef {
    /// Sensor that produced the referenced finding.
    pub sensor_id: String,
    /// Fingerprint of the referenced finding.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
    /// Code of the referenced finding.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    /// Tool name that produced the finding.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    /// Check identifier of the finding.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub check_id: Option<String>,
}

/// Preconditions that must hold before applying a fix.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Preconditions {
    /// Required repository HEAD commit SHA.
    pub repo_head: String,
    /// Expected receipt content digests.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub receipt_digests: Vec<String>,
}

/// A single fix in a buildfix plan.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Fix {
    /// Unique fix identifier.
    pub id: String,
    /// Safety classification of the fix.
    pub safety: SafetyLevel,
    /// Human-readable description.
    pub description: String,
    /// Findings this fix addresses.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub finding_refs: Vec<FindingRef>,
    /// Preconditions required before applying.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preconditions: Option<Preconditions>,
    /// Opaque tool-specific fix payload.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

/// A buildfix plan: a set of fixes to apply.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BuildfixPlan {
    /// Schema identifier.
    pub schema: SchemaId,
    /// Metadata about the producing tool.
    pub tool: ToolInfo,
    /// List of fixes in the plan.
    pub fixes: Vec<Fix>,
}

// ============================================================================
// Trend tracking types
// ============================================================================

/// Change in the overall verdict between baseline and current.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VerdictChange {
    /// Verdict status before the change.
    pub before: VerdictStatus,
    /// Verdict status after the change.
    pub after: VerdictStatus,
}

/// Delta counts for findings.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct CountDeltas {
    /// Change in info-level finding count.
    pub info_delta: i64,
    /// Change in warn-level finding count.
    pub warn_delta: i64,
    /// Change in error-level finding count.
    pub error_delta: i64,
}

/// A finding that changed between baseline and current.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TrendFinding {
    /// Sensor that produced the finding.
    pub sensor_id: String,
    /// Finding code.
    pub code: String,
    /// Human-readable message.
    pub message: String,
    /// Optional file path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Optional line number.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    /// Stable fingerprint for deduplication.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
    /// Severity level.
    pub severity: Severity,
}

/// Category of change for a trending finding.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TrendChange {
    /// Finding is new in the current report.
    New,
    /// Finding was fixed since the baseline.
    Fixed,
}

/// Full trend delta between a baseline and current report.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TrendDelta {
    /// Change in the overall verdict.
    pub verdict_change: Option<VerdictChange>,
    /// Aggregate count changes.
    pub count_deltas: CountDeltas,
    /// Findings that are new.
    pub new_findings: Vec<TrendFinding>,
    /// Findings that were fixed.
    pub fixed_findings: Vec<TrendFinding>,
    /// Sensors added since baseline.
    pub sensors_added: Vec<String>,
    /// Sensors removed since baseline.
    pub sensors_removed: Vec<String>,
}

// ============================================================================
// Buildfix summary types (for cockpit report surfacing)
// ============================================================================

/// A matched finding for a fix.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MatchedFinding {
    /// Sensor that produced the finding.
    pub sensor_id: String,
    /// Finding code.
    pub code: String,
    /// Fingerprint for deduplication.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
}

/// Summary of a single fix for cockpit surfacing.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FixSummary {
    /// Unique fix identifier.
    pub fix_id: String,
    /// Sensor the fix applies to.
    pub sensor_id: String,
    /// Safety classification.
    pub safety: SafetyLevel,
    /// Human-readable description.
    pub description: String,
    /// Findings matched to this fix.
    pub matched_findings: Vec<MatchedFinding>,
    /// Whether the fix had unmatched finding refs.
    pub unmatched: bool,
}

/// Summary of buildfix plans for cockpit surfacing.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct BuildfixSummary {
    /// Individual fix summaries.
    pub fixes: Vec<FixSummary>,
    /// Total number of fixes.
    pub total_fixes: usize,
    /// Number of fixes with matched findings.
    pub matched_count: usize,
    /// Number of fixes without matched findings.
    pub unmatched_count: usize,
}

/// Schema identifier for buildfix apply requests sent to actuator commands.
pub const BUILDFIX_APPLY_REQUEST_SCHEMA_ID: &str = "buildfix.apply.request.v1";

/// Structured request sent to a buildfix actuator command on stdin.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BuildfixApplyRequest {
    /// Schema identifier.
    pub schema: SchemaId,
    /// Maximum safety level for auto-apply.
    pub max_auto_apply_safety: SafetyLevel,
    /// Whether matched findings are required.
    pub require_matched_finding: bool,
    /// Fix summaries to apply.
    pub fixes: Vec<FixSummary>,
}

/// Structured response returned by a buildfix actuator command on stdout.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct BuildfixActuatorResult {
    /// IDs of successfully applied fixes.
    #[serde(default)]
    pub applied_fix_ids: Vec<String>,
    /// IDs of skipped fixes.
    #[serde(default)]
    pub skipped_fix_ids: Vec<String>,
    /// Error messages from the actuator.
    #[serde(default)]
    pub errors: Vec<String>,
}

/// Result status of buildfix auto-apply.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BuildfixApplyStatus {
    /// Auto-apply was skipped.
    Skipped,
    /// Fixes were applied.
    Applied,
    /// Auto-apply failed.
    Failed,
}

/// Buildfix auto-apply evidence surfaced in `cockpit.report.v1` data.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BuildfixApplySummary {
    /// Overall apply status.
    pub status: BuildfixApplyStatus,
    /// Whether auto-apply was enabled.
    pub auto_apply_enabled: bool,
    /// Maximum safety level used.
    pub max_auto_apply_safety: SafetyLevel,
    /// Whether matched findings were required.
    pub require_matched_finding: bool,
    /// IDs of candidate fixes considered.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub candidate_fix_ids: Vec<String>,
    /// IDs of selected fixes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub selected_fix_ids: Vec<String>,
    /// IDs of successfully applied fixes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub applied_fix_ids: Vec<String>,
    /// IDs of skipped fixes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skipped_fix_ids: Vec<String>,
    /// Error messages from auto-apply.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<String>,
    /// Reason for the apply status.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// Command used for actuation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actuator_command: Option<String>,
}

// ============================================================================
// Policy snapshot signing
// ============================================================================

/// Signature algorithm used for policy snapshot evidence.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum PolicySignatureAlgorithm {
    /// HMAC-SHA256 over canonical policy snapshot JSON bytes.
    #[default]
    HmacSha256,
}

/// Policy snapshot signing config in cockpit.toml.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct PolicySigningConfig {
    /// Enable policy snapshot signing.
    #[serde(default)]
    pub enabled: bool,
    /// Signature algorithm.
    #[serde(default)]
    pub algorithm: PolicySignatureAlgorithm,
    /// Path to signing key bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_path: Option<String>,
    /// Environment variable containing signing key bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_env: Option<String>,
    /// Optional key identifier attached to produced evidence.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_id: Option<String>,
}

/// Schema identifier for policy signature evidence.
pub const POLICY_SIGNATURE_SCHEMA_ID: &str = "cockpit.policy_signature.v1";

/// Signed evidence for the policy snapshot used to compute a cockpit verdict.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PolicySignatureEvidence {
    /// Schema identifier.
    pub schema: SchemaId,
    /// Signature algorithm used.
    pub algorithm: PolicySignatureAlgorithm,
    /// SHA-256 digest (hex) of canonical policy snapshot JSON bytes.
    pub policy_sha256: String,
    /// Signature (hex) produced by the configured algorithm.
    pub signature: String,
    /// Optional key identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_id: Option<String>,
}

// ============================================================================
// Hook config types
// ============================================================================

/// When a hook should run.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum HookWhen {
    /// Run after ingest completes.
    #[default]
    AfterIngest,
}

/// Configuration for a post-processing hook.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HookConfig {
    /// Hook name.
    pub name: String,
    /// Shell command to run.
    pub command: String,
    /// When the hook should run.
    #[serde(default)]
    pub when: HookWhen,
    /// Timeout in milliseconds.
    #[serde(default = "default_hook_timeout_ms")]
    pub timeout_ms: u64,
}

fn default_hook_timeout_ms() -> u64 {
    default_buildfix_actuator_timeout_ms()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- is_valid_sensor_id edge cases ----

    #[test]
    fn sensor_id_empty() {
        assert!(!is_valid_sensor_id(""));
    }

    #[test]
    fn sensor_id_single_char() {
        assert!(is_valid_sensor_id("a"));
        assert!(is_valid_sensor_id("Z"));
        assert!(is_valid_sensor_id("0"));
        assert!(is_valid_sensor_id("_"));
        assert!(is_valid_sensor_id("-"));
    }

    #[test]
    fn sensor_id_long_string() {
        let long = "a".repeat(256);
        assert!(is_valid_sensor_id(&long));
    }

    #[test]
    fn sensor_id_special_chars_rejected() {
        assert!(!is_valid_sensor_id("bad/path"));
        assert!(!is_valid_sensor_id("bad\\path"));
        assert!(!is_valid_sensor_id("has space"));
        assert!(!is_valid_sensor_id("has.dot"));
        assert!(!is_valid_sensor_id("has@at"));
        assert!(!is_valid_sensor_id("has!bang"));
        assert!(!is_valid_sensor_id("tab\there"));
        assert!(!is_valid_sensor_id("new\nline"));
    }

    #[test]
    fn sensor_id_unicode_rejected() {
        assert!(!is_valid_sensor_id("café"));
        assert!(!is_valid_sensor_id("日本語"));
        assert!(!is_valid_sensor_id("emoji🚀"));
    }

    #[test]
    fn sensor_id_path_traversal_attempts() {
        assert!(!is_valid_sensor_id("../bad"));
        assert!(!is_valid_sensor_id("foo/../bar"));
        assert!(!is_valid_sensor_id(".."));
        assert!(!is_valid_sensor_id("..."));
    }

    #[test]
    fn sensor_id_dots_only() {
        assert!(!is_valid_sensor_id("."));
        assert!(!is_valid_sensor_id(".."));
        assert!(!is_valid_sensor_id("..."));
    }

    #[test]
    fn sensor_id_reserved_names() {
        // These contain only valid chars, so they should pass
        assert!(is_valid_sensor_id("CON"));
        assert!(is_valid_sensor_id("NUL"));
        assert!(is_valid_sensor_id("PRN"));
    }

    #[test]
    fn sensor_id_valid_typical() {
        assert!(is_valid_sensor_id("builddiag"));
        assert!(is_valid_sensor_id("my-sensor_v2"));
        assert!(is_valid_sensor_id("UPPER"));
        assert!(is_valid_sensor_id("MiXeD-CaSe_123"));
    }

    // ---- VerdictCounts arithmetic overflow ----

    #[test]
    fn verdict_counts_max_values() {
        let counts = VerdictCounts {
            info: u64::MAX,
            warn: u64::MAX,
            error: u64::MAX,
            suppressed: u64::MAX,
        };
        assert_eq!(counts.info, u64::MAX);
        assert_eq!(counts.warn, u64::MAX);
        assert_eq!(counts.error, u64::MAX);
        assert_eq!(counts.suppressed, u64::MAX);
    }

    #[test]
    fn verdict_counts_serializes_max_values() {
        let counts = VerdictCounts {
            info: u64::MAX,
            warn: u64::MAX,
            error: u64::MAX,
            suppressed: u64::MAX,
        };
        let json = serde_json::to_string(&counts).expect("serialize");
        let back: VerdictCounts = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(counts, back);
    }

    #[test]
    fn verdict_counts_suppressed_zero_omitted() {
        let counts = VerdictCounts {
            info: 1,
            warn: 0,
            error: 0,
            suppressed: 0,
        };
        let json = serde_json::to_string(&counts).expect("serialize");
        assert!(!json.contains("suppressed"));
    }

    #[test]
    fn verdict_counts_suppressed_nonzero_included() {
        let counts = VerdictCounts {
            info: 0,
            warn: 0,
            error: 0,
            suppressed: 1,
        };
        let json = serde_json::to_string(&counts).expect("serialize");
        assert!(json.contains("suppressed"));
    }

    // ---- Enum serde round-trips ----

    #[test]
    fn verdict_status_serde_roundtrip() {
        for status in &[
            VerdictStatus::Pass,
            VerdictStatus::Warn,
            VerdictStatus::Fail,
            VerdictStatus::Skip,
        ] {
            let json = serde_json::to_string(status).expect("serialize");
            let back: VerdictStatus = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(status, &back);
        }
    }

    #[test]
    fn verdict_status_json_values() {
        assert_eq!(
            serde_json::to_string(&VerdictStatus::Pass).unwrap(),
            "\"pass\""
        );
        assert_eq!(
            serde_json::to_string(&VerdictStatus::Warn).unwrap(),
            "\"warn\""
        );
        assert_eq!(
            serde_json::to_string(&VerdictStatus::Fail).unwrap(),
            "\"fail\""
        );
        assert_eq!(
            serde_json::to_string(&VerdictStatus::Skip).unwrap(),
            "\"skip\""
        );
    }

    #[test]
    fn severity_serde_roundtrip() {
        for sev in &[Severity::Info, Severity::Warn, Severity::Error] {
            let json = serde_json::to_string(sev).expect("serialize");
            let back: Severity = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(sev, &back);
        }
    }

    #[test]
    fn severity_json_values() {
        assert_eq!(serde_json::to_string(&Severity::Info).unwrap(), "\"info\"");
        assert_eq!(serde_json::to_string(&Severity::Warn).unwrap(), "\"warn\"");
        assert_eq!(
            serde_json::to_string(&Severity::Error).unwrap(),
            "\"error\""
        );
    }

    #[test]
    fn missing_policy_serde_roundtrip() {
        for mp in &[
            MissingPolicy::Skip,
            MissingPolicy::Warn,
            MissingPolicy::Fail,
        ] {
            let json = serde_json::to_string(mp).expect("serialize");
            let back: MissingPolicy = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(mp, &back);
        }
    }

    #[test]
    fn presence_serde_roundtrip() {
        for p in &[Presence::Present, Presence::Missing, Presence::Invalid] {
            let json = serde_json::to_string(p).expect("serialize");
            let back: Presence = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(p, &back);
        }
    }

    #[test]
    fn policy_outcome_serde_roundtrip() {
        for po in &[
            PolicyOutcome::Blocked,
            PolicyOutcome::Allowed,
            PolicyOutcome::Informational,
        ] {
            let json = serde_json::to_string(po).expect("serialize");
            let back: PolicyOutcome = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(po, &back);
        }
    }

    #[test]
    fn capability_status_serde_roundtrip() {
        for cs in &[
            CapabilityStatus::Available,
            CapabilityStatus::Unavailable,
            CapabilityStatus::Skipped,
        ] {
            let json = serde_json::to_string(cs).expect("serialize");
            let back: CapabilityStatus = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(cs, &back);
        }
    }

    #[test]
    fn safety_level_serde_roundtrip() {
        for sl in &[SafetyLevel::Safe, SafetyLevel::Guarded, SafetyLevel::Unsafe] {
            let json = serde_json::to_string(sl).expect("serialize");
            let back: SafetyLevel = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(sl, &back);
        }
    }

    #[test]
    fn schema_validation_serde_roundtrip() {
        for sv in &[SchemaValidation::Lax, SchemaValidation::Strict] {
            let json = serde_json::to_string(sv).expect("serialize");
            let back: SchemaValidation = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(sv, &back);
        }
    }

    #[test]
    fn trend_change_serde_roundtrip() {
        for tc in &[TrendChange::New, TrendChange::Fixed] {
            let json = serde_json::to_string(tc).expect("serialize");
            let back: TrendChange = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(tc, &back);
        }
    }

    #[test]
    fn hook_when_serde_roundtrip() {
        let hw = HookWhen::AfterIngest;
        let json = serde_json::to_string(&hw).expect("serialize");
        let back: HookWhen = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(hw, back);
    }

    #[test]
    fn buildfix_apply_status_serde_roundtrip() {
        for s in &[
            BuildfixApplyStatus::Skipped,
            BuildfixApplyStatus::Applied,
            BuildfixApplyStatus::Failed,
        ] {
            let json = serde_json::to_string(s).expect("serialize");
            let back: BuildfixApplyStatus = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(s, &back);
        }
    }

    #[test]
    fn policy_signature_algorithm_serde_roundtrip() {
        let alg = PolicySignatureAlgorithm::HmacSha256;
        let json = serde_json::to_string(&alg).expect("serialize");
        let back: PolicySignatureAlgorithm = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(alg, back);
    }

    // ---- Ranking helpers ----

    #[test]
    fn severity_rank_ordering() {
        assert!(severity_rank(&Severity::Error) < severity_rank(&Severity::Warn));
        assert!(severity_rank(&Severity::Warn) < severity_rank(&Severity::Info));
    }

    #[test]
    fn verdict_status_rank_ordering() {
        assert!(
            verdict_status_rank(&VerdictStatus::Fail) < verdict_status_rank(&VerdictStatus::Warn)
        );
        assert!(
            verdict_status_rank(&VerdictStatus::Warn) < verdict_status_rank(&VerdictStatus::Pass)
        );
        assert!(
            verdict_status_rank(&VerdictStatus::Pass) < verdict_status_rank(&VerdictStatus::Skip)
        );
    }

    #[test]
    fn safety_level_rank_ordering() {
        assert!(safety_level_rank(&SafetyLevel::Safe) < safety_level_rank(&SafetyLevel::Guarded));
        assert!(safety_level_rank(&SafetyLevel::Guarded) < safety_level_rank(&SafetyLevel::Unsafe));
    }
}
