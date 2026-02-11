//! cockpitctl library facade.
//!
//! Re-exports everything from `cockpitctl-core` for backward compatibility.
//! New consumers should depend on `cockpitctl-core` directly to avoid
//! inheriting CLI dependencies (clap, etc.).

pub use cockpitctl_core::*;
