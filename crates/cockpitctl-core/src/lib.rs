//! cockpitctl compiler library — portable, no CLI dependencies.
//!
//! Re-exports the public API from each microcrate so downstream consumers
//! (including the substrate-bridge / in-memory path) can depend on a single
//! `cockpitctl-core` crate instead of wiring individual microcrates.

pub use cockpitctl_domain as domain;
pub use cockpitctl_ingest as ingest;
pub use cockpitctl_io as io;
pub use cockpitctl_render as render;
pub use cockpitctl_types as types;

// Flatten the most-used ingest items.
pub use cockpitctl_ingest::{
    IngestRequest, IngestResult, IngestUseCase, NoOpSchemaValidator, OutputSink, PolicySource,
    ReceiptSource, SchemaValidator,
};

// Flatten the most-used types.
pub use cockpitctl_types::{
    ArtifactPointer, CockpitConfig, CockpitReport, PolicyOutcome, Presence, SensorReport, ToolInfo,
    VerdictStatus,
};

// Flatten the renderer.
pub use cockpitctl_render::render_comment;
