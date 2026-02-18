//! cockpitctl compiler library — portable, no CLI dependencies.
//!
//! Re-exports the public API from each microcrate so downstream consumers
//! (including the substrate-bridge / in-memory path) can depend on a single
//! `cockpitctl-core` crate instead of wiring individual microcrates.

pub use cockpitctl_domain as domain;
pub use cockpitctl_ingest as ingest;
pub use cockpitctl_io as io;
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
pub use cockpitctl_domain::{
    CodeExplanation, all_codes, explain_code, policy_snapshot_sha256_hex, select_auto_apply_fixes,
    sign_policy_snapshot, sign_policy_snapshot_hmac_sha256,
};

// Flatten SARIF export.
pub use cockpitctl_sarif::{cockpit_report_to_sarif, cockpit_report_to_sarif_json};

// Flatten the renderer.
pub use cockpitctl_render::{
    GitHubAnnotationResult, append_comment_sections, render_comment, render_github_annotations,
};
