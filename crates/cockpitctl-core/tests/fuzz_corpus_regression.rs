//! Corpus regression tests — exercise fuzz corpus seeds in CI without cargo-fuzz.
//!
//! Each test walks the corresponding `fuzz/corpus/<target>/` directory and feeds
//! every seed file through the same logic the fuzz target uses. This ensures
//! corpus seeds stay valid and that the code never panics on them.

use cockpitctl_conform::{
    ConformChecks, check_artifact_pointers, check_ordering, check_path_hygiene,
    check_reason_tokens, check_sensor_id_format, check_tool_error_identity, conform_single,
};
use cockpitctl_core::domain::{
    build_cockpit_report, cap_findings, compute_counts, derive_fingerprint, finding_sort_key,
    select_highlights, sort_findings, sort_sensor_summaries, summarize_sensor_report,
};
use cockpitctl_core::render::render_annotations;
use cockpitctl_core::types::{Finding, Policy, RunInfo, SensorPolicy};
use cockpitctl_core::{
    CockpitConfig, CockpitReport, SensorReport, ToolInfo, append_comment_sections,
    cockpit_report_to_sarif, cockpit_report_to_sarif_json, render_comment,
    render_github_annotations,
};
use std::collections::BTreeMap;
use std::path::Path;

/// Collect all files in a corpus directory.
fn corpus_files(target: &str) -> Vec<std::path::PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("fuzz")
        .join("corpus")
        .join(target);
    if !dir.exists() {
        panic!("corpus dir not found: {}", dir.display());
    }
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| {
            let e = e.ok()?;
            if e.file_type().ok()?.is_file() {
                Some(e.path())
            } else {
                None
            }
        })
        .collect();
    files.sort();
    assert!(!files.is_empty(), "no corpus files in {}", dir.display());
    files
}

#[test]
fn corpus_parse_receipt() {
    for path in corpus_files("parse_receipt") {
        let data = std::fs::read(&path).unwrap();
        let _ = serde_json::from_slice::<SensorReport>(&data);
        if let Ok(report) = serde_json::from_slice::<SensorReport>(&data) {
            let _ = serde_json::to_string(&report);
            let _ = serde_json::to_vec(&report);
        }
    }
}

#[test]
fn corpus_parse_config() {
    for path in corpus_files("parse_config") {
        let data = std::fs::read(&path).unwrap();
        if let Ok(text) = std::str::from_utf8(&data) {
            let _ = toml::from_str::<CockpitConfig>(text);
            if let Ok(config) = toml::from_str::<CockpitConfig>(text) {
                let _ = toml::to_string(&config);
                let _ = toml::to_string_pretty(&config);
            }
        }
    }
}

#[test]
fn corpus_sarif_convert() {
    for path in corpus_files("sarif_convert") {
        let data = std::fs::read(&path).unwrap();
        if let Ok(report) = serde_json::from_slice::<CockpitReport>(&data) {
            let sarif = cockpit_report_to_sarif(&report);
            let _ = serde_json::to_string(&sarif);
            let _ = cockpit_report_to_sarif_json(&report);
        }
    }
}

#[test]
fn corpus_render_comment() {
    for path in corpus_files("render_comment") {
        let data = std::fs::read(&path).unwrap();
        if let Ok(report) = serde_json::from_slice::<CockpitReport>(&data) {
            let cfg = CockpitConfig::default();
            let _ = render_comment(&report, &cfg);
        }
    }
}

#[test]
fn corpus_fuzz_sensor_id() {
    for path in corpus_files("fuzz_sensor_id") {
        let data = std::fs::read(&path).unwrap();
        if let Ok(s) = std::str::from_utf8(&data) {
            let _ = cockpitctl_core::types::is_valid_sensor_id(s);
            let _ = check_sensor_id_format(s);
        }
    }
}

#[test]
fn corpus_fuzz_schema_validate() {
    let checks = ConformChecks {
        path_hygiene: true,
        ordering: true,
        reason_lint: true,
        survivability: true,
        tool_error_identity: true,
        sensor_id_format: true,
        artifact_pointers: true,
    };
    for path in corpus_files("fuzz_schema_validate") {
        let data = std::fs::read(&path).unwrap();
        if let Ok(text) = std::str::from_utf8(&data) {
            let _ = conform_single(text, "fuzz-sensor", &checks);
            let _ = cockpitctl_conform::validate_cockpit_schema(text);
        }
    }
}

#[test]
fn corpus_fuzz_domain_pipeline() {
    for path in corpus_files("fuzz_domain_pipeline") {
        let data = std::fs::read(&path).unwrap();
        let report: SensorReport = match serde_json::from_slice(&data) {
            Ok(r) => r,
            Err(_) => continue,
        };
        let cfg = CockpitConfig::default();
        let policy = SensorPolicy::default();
        let (summary, highlights) = summarize_sensor_report(
            "fuzz-sensor",
            "artifacts/fuzz-sensor/report.json",
            None,
            &policy,
            report,
            20,
        );
        let mut blocking = BTreeMap::new();
        blocking.insert("fuzz-sensor".to_string(), true);
        let selected = select_highlights(highlights, &cfg, &blocking);
        let mut summaries = vec![summary];
        sort_sensor_summaries(&mut summaries, &cfg);
        let tool = ToolInfo {
            name: "cockpitctl".to_string(),
            version: "0.0.0-fuzz".to_string(),
            commit: None,
        };
        let run = RunInfo {
            started_at: "2026-01-01T00:00:00Z".to_string(),
            ended_at: None,
            duration_ms: None,
            host: None,
            git: None,
            ci: None,
            capabilities: BTreeMap::new(),
        };
        let cockpit_report = build_cockpit_report(&cfg, tool, run, summaries, selected);
        let _ = serde_json::to_string(&cockpit_report);
    }
}

#[test]
fn corpus_fuzz_render_annotations() {
    for path in corpus_files("fuzz_render_annotations") {
        let data = std::fs::read(&path).unwrap();
        let report: CockpitReport = match serde_json::from_slice(&data) {
            Ok(r) => r,
            Err(_) => continue,
        };
        let cfg = CockpitConfig::default();
        let mut blocking = BTreeMap::new();
        for s in &report.sensors {
            blocking.insert(s.id.clone(), s.blocking);
        }
        let _ = render_annotations(&report.highlights, &cfg, &blocking);
        let _ = render_github_annotations(&report.highlights, &cfg, &blocking);
        let comment = render_comment(&report, &cfg);
        let sections = vec![("Extra".to_string(), "fuzz content".to_string())];
        let _ = append_comment_sections(&comment, &sections);
    }
}

#[test]
fn corpus_fuzz_fingerprint() {
    for path in corpus_files("fuzz_fingerprint") {
        let data = std::fs::read(&path).unwrap();
        if let Ok(finding) = serde_json::from_slice::<Finding>(&data) {
            let fp = derive_fingerprint("fuzz-sensor", &finding);
            assert!(!fp.is_empty());
            let _ = finding_sort_key("fuzz-sensor", &finding);
            let _ = compute_counts(std::slice::from_ref(&finding));
        }
        if let Ok(mut findings) = serde_json::from_slice::<Vec<Finding>>(&data) {
            sort_findings("fuzz-sensor", &mut findings);
            let _ = cap_findings(findings.clone(), 0);
            let _ = cap_findings(findings.clone(), 1);
            let _ = cap_findings(findings, usize::MAX);
        }
    }
}

#[test]
fn corpus_fuzz_conform() {
    let all_checks = ConformChecks {
        path_hygiene: true,
        ordering: true,
        reason_lint: true,
        survivability: true,
        tool_error_identity: true,
        sensor_id_format: true,
        artifact_pointers: true,
    };
    for path in corpus_files("fuzz_conform") {
        let data = std::fs::read(&path).unwrap();
        let text = match std::str::from_utf8(&data) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let _ = conform_single(text, "fuzz-sensor", &all_checks);
        if let Ok(report) = serde_json::from_str::<SensorReport>(text) {
            let _ = check_path_hygiene(&report);
            let _ = check_ordering(&report, "fuzz-sensor");
            let _ = check_reason_tokens(&report);
            let _ = check_tool_error_identity(&report);
            let _ = check_artifact_pointers(&report);
            let _ = check_sensor_id_format(&report.tool.name);
        }
    }
}

#[test]
fn corpus_fuzz_render_budgets() {
    for path in corpus_files("fuzz_render_budgets") {
        let data = std::fs::read(&path).unwrap();
        let report: CockpitReport = match serde_json::from_slice(&data) {
            Ok(r) => r,
            Err(_) => continue,
        };
        let cfg = CockpitConfig::default();
        let _ = render_comment(&report, &cfg);

        let zero_cfg = CockpitConfig {
            policy: Policy {
                max_highlights: 0,
                max_per_sensor_findings: 0,
                max_annotations: 0,
                ..Policy::default()
            },
            ..CockpitConfig::default()
        };
        let _ = render_comment(&report, &zero_cfg);

        let strict_cfg = CockpitConfig {
            policy: Policy {
                warn_is_fail: true,
                ..Policy::default()
            },
            ..CockpitConfig::default()
        };
        let _ = render_comment(&report, &strict_cfg);
    }
}

#[test]
fn corpus_fuzz_config_merge() {
    for path in corpus_files("fuzz_config_merge") {
        let data = std::fs::read(&path).unwrap();
        let text = match std::str::from_utf8(&data) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let config: CockpitConfig = match toml::from_str(text) {
            Ok(c) => c,
            Err(_) => continue,
        };
        if let Ok(serialized) = toml::to_string(&config) {
            let _ = toml::from_str::<CockpitConfig>(&serialized);
        }
        if let Ok(pretty) = toml::to_string_pretty(&config) {
            let _ = toml::from_str::<CockpitConfig>(&pretty);
        }
    }
}
