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

#![warn(missing_docs)]

pub use cockpitctl_domain as domain;
pub use cockpitctl_domain_buildfix as domain_buildfix;
pub use cockpitctl_domain_signing as domain_signing;
pub use cockpitctl_feature_grid as feature_grid;
pub use cockpitctl_feature_runtime as feature_runtime;
pub use cockpitctl_feature_state as feature_state;
pub use cockpitctl_ingest as ingest;
pub use cockpitctl_io as io;
pub use cockpitctl_io_buildfix as io_buildfix;
pub use cockpitctl_io_hooks as io_hooks;
pub use cockpitctl_io_policy_signing as io_policy_signing;
pub use cockpitctl_io_schema as io_schema;
pub use cockpitctl_render as render;
pub use cockpitctl_sarif as sarif;
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
