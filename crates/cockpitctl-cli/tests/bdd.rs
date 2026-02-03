//! Minimal BDD runner for `features/*.feature`.
//!
//! This intentionally does *not* pull in a full cucumber runtime.
//! It keeps BDD as an executable spec, while remaining compile-stable.

#![allow(deprecated)] // Command::cargo_bin still widely used

use assert_cmd::Command;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

fn workspace_root() -> PathBuf {
    // CARGO_MANIFEST_DIR points at crates/cockpitctl-cli
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_all(&from, &to)?;
        } else if file_type.is_file() {
            fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

#[derive(Debug, Default)]
struct Scenario {
    name: String,
    fixture: String,
    expected_exit: i32,
    compare_report: bool,
    compare_comment: bool,
    expect_highlight_code: Option<String>,
}

fn parse_feature(path: &Path) -> Vec<Scenario> {
    let txt = fs::read_to_string(path).expect("read feature");
    let mut scenarios: Vec<Scenario> = Vec::new();
    let mut current: Option<Scenario> = None;

    for raw in txt.lines() {
        let line = raw.trim();
        if let Some(rest) = line.strip_prefix("Scenario:") {
            if let Some(s) = current.take() {
                scenarios.push(s);
            }
            current = Some(Scenario {
                name: rest.trim().to_string(),
                ..Scenario::default()
            });
            continue;
        }

        let Some(s) = current.as_mut() else {
            continue;
        };

        if line.starts_with("Given a fixture") {
            // Given a fixture "happy_path"
            if let Some(start) = line.find('"') {
                if let Some(end) = line[start + 1..].find('"') {
                    s.fixture = line[start + 1..start + 1 + end].to_string();
                }
            }
        } else if let Some(rest) = line.strip_prefix("Then the exit code is") {
            // Then the exit code is 0
            s.expected_exit = rest.trim().parse::<i32>().unwrap();
        } else if line.contains("cockpit report matches the golden file") {
            s.compare_report = true;
        } else if line.contains("cockpit comment matches the golden file") {
            s.compare_comment = true;
        } else if line.contains("contains a highlight") {
            // And the cockpit report contains a highlight "cockpit.missing_receipt"
            if let Some(start) = line.find('"') {
                if let Some(end) = line[start + 1..].find('"') {
                    s.expect_highlight_code = Some(line[start + 1..start + 1 + end].to_string());
                }
            }
        }
    }

    if let Some(s) = current.take() {
        scenarios.push(s);
    }
    scenarios
}

fn run_scenario(s: &Scenario) {
    let ws = workspace_root();
    let fixture_src = ws.join("fixtures").join(&s.fixture);

    let tmp = TempDir::new().expect("tempdir");
    let fixture_dst = tmp.path().join(&s.fixture);

    copy_dir_all(&fixture_src, &fixture_dst).expect("copy fixture");

    let artifacts = fixture_dst.join("artifacts");
    let config = fixture_dst.join("cockpit.toml");

    // Ensure clean outputs.
    let out_dir = artifacts.join("cockpit");
    let _ = fs::remove_dir_all(&out_dir);

    let mut cmd = Command::cargo_bin("cockpitctl").expect("binary exists");
    cmd.env("COCKPITCTL_STARTED_AT", "2026-02-02T12:00:00Z");
    cmd.args([
        "ingest",
        "--artifacts",
        artifacts.to_string_lossy().as_ref(),
        "--config",
        config.to_string_lossy().as_ref(),
    ]);

    cmd.assert().code(s.expected_exit);

    let report_path = out_dir.join("report.json");
    let comment_path = out_dir.join("comment.md");

    if s.compare_report {
        let got = fs::read_to_string(&report_path).expect("read report");
        let exp = fs::read_to_string(fixture_dst.join("expected").join("report.json"))
            .expect("read expected report");
        pretty_assertions::assert_eq!(got, exp, "report mismatch for scenario {}", s.name);
    }

    if s.compare_comment {
        let got = fs::read_to_string(&comment_path).expect("read comment");
        let exp = fs::read_to_string(fixture_dst.join("expected").join("comment.md"))
            .expect("read expected comment");
        pretty_assertions::assert_eq!(got, exp, "comment mismatch for scenario {}", s.name);
    }

    if let Some(code) = &s.expect_highlight_code {
        let got = fs::read_to_string(&report_path).expect("read report");
        let v: Value = serde_json::from_str(&got).expect("parse report json");
        let highlights = v
            .get("highlights")
            .and_then(|x| x.as_array())
            .cloned()
            .unwrap_or_default();

        let mut found = false;
        for h in highlights {
            let c = h
                .get("finding")
                .and_then(|f| f.get("code"))
                .and_then(|x| x.as_str())
                .unwrap_or("");
            if c == code {
                found = true;
                break;
            }
        }
        assert!(
            found,
            "expected highlight code {} not found in scenario {}",
            code, s.name
        );
    }
}

#[test]
fn bdd_features() {
    let ws = workspace_root();
    let feature = ws.join("features").join("ingest.feature");
    let scenarios = parse_feature(&feature);

    assert!(
        !scenarios.is_empty(),
        "no scenarios parsed from {}",
        feature.display()
    );
    for s in scenarios {
        run_scenario(&s);
    }
}
