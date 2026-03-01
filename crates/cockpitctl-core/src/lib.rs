//! cockpitctl compiler library — portable, no CLI dependencies.
//!
//! Re-exports the public API from each microcrate so downstream consumers
//! (including the substrate-bridge / in-memory path) can depend on a single
//! `cockpitctl-core` crate instead of wiring individual microcrates.

pub use cockpitctl_buildfix as buildfix;
pub use cockpitctl_domain as domain;
pub use cockpitctl_ingest as ingest;
pub use cockpitctl_io as io;
pub use cockpitctl_policy as policy;
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
pub use cockpitctl_domain::{CodeExplanation, all_codes, explain_code};

// Flatten buildfix helpers.
pub use cockpitctl_buildfix::{match_buildfix_plan, select_auto_apply_fixes};

// Flatten policy signing helpers.
pub use cockpitctl_policy::{
    canonical_policy_snapshot_bytes, policy_snapshot_sha256_hex, sign_policy_snapshot,
    sign_policy_snapshot_hmac_sha256,
};

// Flatten SARIF export.
pub use cockpitctl_sarif::{cockpit_report_to_sarif, cockpit_report_to_sarif_json};

// Flatten the renderer.
pub use cockpitctl_render::{
    GitHubAnnotationResult, append_comment_sections, render_comment, render_github_annotations,
};
