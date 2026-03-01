//! Fuzz target for annotation rendering and comment section appending.
//!
//! Exercises `render_annotations`, `render_github_annotations`, and
//! `append_comment_sections` with arbitrary `CockpitReport` JSON.
//!
//! Run with: cargo +nightly fuzz run fuzz_render_annotations

#![no_main]

use libfuzzer_sys::fuzz_target;
use cockpitctl_render::{
    append_comment_sections, render_annotations, render_comment, render_github_annotations,
};
use cockpitctl_types::{CockpitConfig, CockpitReport};
use std::collections::BTreeMap;

fuzz_target!(|data: &[u8]| {
    let report: CockpitReport = match serde_json::from_slice(data) {
        Ok(r) => r,
        Err(_) => return,
    };

    let cfg = CockpitConfig::default();

    // Build sensor blocking map from report data.
    let mut blocking = BTreeMap::new();
    for s in &report.sensors {
        blocking.insert(s.id.clone(), s.blocking);
    }

    // Annotation rendering must never panic.
    let _ = render_annotations(&report.highlights, &cfg, &blocking);

    // GitHub annotation rendering must never panic.
    let _ = render_github_annotations(&report.highlights, &cfg, &blocking);

    // Full comment rendering followed by section appending must never panic.
    let comment = render_comment(&report, &cfg);
    let sections = vec![
        ("Extra".to_string(), "fuzz content".to_string()),
        ("Another".to_string(), report.verdict.status.to_string()),
    ];
    let _ = append_comment_sections(&comment, &sections);
});
