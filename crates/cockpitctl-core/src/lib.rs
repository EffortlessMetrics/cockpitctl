//! cockpitctl compiler library — portable, no CLI dependencies.
//!
//! Re-exports the public API from each microcrate so downstream consumers
//! (including the substrate-bridge / in-memory path) can depend on a single
//! `cockpitctl-core` crate instead of wiring individual microcrates.
//!
//! # Examples
//!
//! ```
//! // Access types directly via flattened re-exports.
//! use cockpitctl_core::{CockpitConfig, CockpitReport, VerdictStatus, ToolInfo};
//!
//! // Or use the namespaced module path.
//! use cockpitctl_core::domain::explain_code;
//! use cockpitctl_core::types::Severity;
//!
//! let cfg = CockpitConfig::default();
//! assert_eq!(cfg.policy.max_highlights, 7);
//!
//! let explanation = explain_code("cockpit.missing_receipt");
//! assert!(explanation.is_some());
//! ```

#![deny(missing_docs)]

/// Pure domain logic: policy evaluation, deterministic selection, normalization, and code explanations.
pub use cockpitctl_domain as domain;
/// Buildfix domain logic: plan matching and auto-apply fix selection.
pub use cockpitctl_domain_buildfix as domain_buildfix;
/// Policy-snapshot signing utilities (HMAC-SHA-256).
pub use cockpitctl_domain_signing as domain_signing;
/// Feature-grid computation for multi-sensor dashboards.
pub use cockpitctl_feature_grid as feature_grid;
/// Feature-state tracking across pipeline runs.
pub use cockpitctl_feature_state as feature_state;
/// Ingest use-case boundary: orchestration, port traits, and precedence logic.
pub use cockpitctl_ingest as ingest;
/// Filesystem adapters implementing the ingest port traits (receipt reading, output writing).
pub use cockpitctl_io as io;
/// Buildfix I/O adapters for reading and writing fix plans.
pub use cockpitctl_io_buildfix as io_buildfix;
/// Hook I/O adapters for lifecycle event handling.
pub use cockpitctl_io_hooks as io_hooks;
/// Policy-signing I/O adapter (key loading and signature persistence).
pub use cockpitctl_io_policy_signing as io_policy_signing;
/// Schema I/O adapter for loading and validating JSON schemas at runtime.
pub use cockpitctl_io_schema as io_schema;
/// PR comment renderer with stable markers, budgeting, and truncation.
pub use cockpitctl_render as render;
/// SARIF export: converts cockpit reports to SARIF v2.1.0 for GitHub code scanning.
pub use cockpitctl_sarif as sarif;
/// Stable DTOs, enums (VerdictStatus, Severity), ordering helpers, and embedded schemas.
pub use cockpitctl_types as types;

// Flatten the most-used ingest items.
pub use cockpitctl_ingest::{
    IngestRequest, IngestResult, IngestUseCase, NoOpSchemaValidator, OutputSink, PolicySource,
    ReceiptSource, SchemaValidator,
};

// Flatten the most-used types.
pub use cockpitctl_types::{
    ArtifactPointer, BuildfixApplySummary, BuildfixPolicy, CockpitConfig, CockpitReport,
    PolicyOutcome, PolicySignatureEvidence, PolicySigningConfig, Presence, SensorReport, ToolInfo,
    VerdictStatus,
};

// Flatten domain helpers.
pub use cockpitctl_domain::{CodeExplanation, all_codes, explain_code, policy_snapshot_sha256_hex};
pub use cockpitctl_domain_buildfix::{match_buildfix_plan, select_auto_apply_fixes};
pub use cockpitctl_domain_signing::{sign_policy_snapshot, sign_policy_snapshot_hmac_sha256};
pub use cockpitctl_domain_trend::compute_trend;

// Flatten SARIF export.
pub use cockpitctl_sarif::{cockpit_report_to_sarif, cockpit_report_to_sarif_json};

// Flatten the renderer.
pub use cockpitctl_render::{
    GitHubAnnotationResult, append_comment_sections, render_comment, render_github_annotations,
};
