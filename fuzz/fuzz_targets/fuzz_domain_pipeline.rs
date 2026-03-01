//! Fuzz target for the domain pipeline: summarize → select → build report.
//!
//! Exercises `summarize_sensor_report`, `select_highlights`, and
//! `build_cockpit_report` with arbitrary `SensorReport` JSON.
//!
//! Run with: cargo +nightly fuzz run fuzz_domain_pipeline

#![no_main]

use libfuzzer_sys::fuzz_target;
use cockpitctl_domain::{
    build_cockpit_report, select_highlights, sort_sensor_summaries, summarize_sensor_report,
};
use cockpitctl_types::{CockpitConfig, RunInfo, SensorPolicy, SensorReport, ToolInfo};
use std::collections::BTreeMap;

fuzz_target!(|data: &[u8]| {
    // Parse arbitrary bytes as a SensorReport.
    let report: SensorReport = match serde_json::from_slice(data) {
        Ok(r) => r,
        Err(_) => return,
    };

    let cfg = CockpitConfig::default();
    let policy = SensorPolicy::default();

    // Summarize with bounded findings cap.
    let (summary, highlights) =
        summarize_sensor_report("fuzz-sensor", "artifacts/fuzz-sensor/report.json", None, &policy, report, 20);

    // Select highlights with blocking map.
    let mut blocking = BTreeMap::new();
    blocking.insert("fuzz-sensor".to_string(), true);
    let selected = select_highlights(highlights, &cfg, &blocking);

    // Sort summaries.
    let mut summaries = vec![summary];
    sort_sensor_summaries(&mut summaries, &cfg);

    // Build final report — must never panic.
    let tool = ToolInfo {
        name: "cockpitctl".to_string(),
        version: "0.0.0-fuzz".to_string(),
    };
    let run = RunInfo {
        started_at: "2026-01-01T00:00:00Z".to_string(),
    };
    let cockpit_report = build_cockpit_report(&cfg, tool, run, summaries, selected);

    // Serialization round-trip must not panic.
    let _ = serde_json::to_string(&cockpit_report);
});
