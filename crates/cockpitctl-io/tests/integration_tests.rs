//! Integration tests for cockpitctl-io filesystem adapters.

use cockpitctl_ingest::{OutputSink, ReceiptSource, ReportRead};
use cockpitctl_io::{FsLayout, FsOutputSink, FsReceiptSource};
use std::fs;
use tempfile::TempDir;

/// Build a minimal valid sensor receipt JSON string.
fn minimal_receipt_json() -> String {
    serde_json::json!({
        "schema": "sensor.report.v1",
        "tool": { "name": "test-sensor", "version": "0.1.0" },
        "run": { "started_at": "2025-01-01T00:00:00Z" },
        "verdict": {
            "status": "pass",
            "counts": { "info": 0, "warn": 0, "error": 0 }
        },
        "findings": []
    })
    .to_string()
}

/// Helper: create a sensor directory with a report.json inside a temp artifacts dir.
fn create_sensor(artifacts: &std::path::Path, sensor_id: &str, json: &str) {
    let dir = artifacts.join(sensor_id);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("report.json"), json).unwrap();
}

// ── Receipt discovery ──────────────────────────────────────────────────────

#[test]
fn discovery_returns_sensors_in_lexical_order() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    fs::create_dir_all(&artifacts).unwrap();

    // Create sensors in non-lexical order.
    for name in &["zebra", "alpha", "mango"] {
        create_sensor(&artifacts, name, &minimal_receipt_json());
    }

    let layout = FsLayout::new(&artifacts, tmp.path().join("cockpit.toml"));
    let source = FsReceiptSource::new(layout);
    let discovered = source.discovered_sensors().unwrap();

    assert_eq!(discovered.sensors, vec!["alpha", "mango", "zebra"]);
    assert!(!discovered.truncated);
    assert_eq!(discovered.total_found, 3);
}

#[test]
fn discovery_skips_dirs_without_report_json() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    fs::create_dir_all(&artifacts).unwrap();

    // One valid sensor, one directory without report.json.
    create_sensor(&artifacts, "valid", &minimal_receipt_json());
    fs::create_dir_all(artifacts.join("empty-dir")).unwrap();

    let layout = FsLayout::new(&artifacts, tmp.path().join("cockpit.toml"));
    let source = FsReceiptSource::new(layout);
    let discovered = source.discovered_sensors().unwrap();

    assert_eq!(discovered.sensors, vec!["valid"]);
}

#[test]
fn discovery_skips_cockpit_output_dir() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    fs::create_dir_all(&artifacts).unwrap();

    create_sensor(&artifacts, "sensor-a", &minimal_receipt_json());
    // "cockpit" is the reserved output dir — it must not appear as a sensor.
    create_sensor(&artifacts, "cockpit", &minimal_receipt_json());

    let layout = FsLayout::new(&artifacts, tmp.path().join("cockpit.toml"));
    let source = FsReceiptSource::new(layout);
    let discovered = source.discovered_sensors().unwrap();

    assert_eq!(discovered.sensors, vec!["sensor-a"]);
}

// ── Receipt reading ────────────────────────────────────────────────────────

#[test]
fn read_report_bytes_returns_valid_json() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    fs::create_dir_all(&artifacts).unwrap();

    let json = minimal_receipt_json();
    create_sensor(&artifacts, "my-sensor", &json);

    let layout = FsLayout::new(&artifacts, tmp.path().join("cockpit.toml"));
    let source = FsReceiptSource::new(layout);

    match source.read_report_bytes("my-sensor").unwrap() {
        ReportRead::Bytes(bytes) => {
            let parsed: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(parsed["tool"]["name"], "test-sensor");
        }
        other => panic!("expected Bytes, got {:?}", variant_name(&other)),
    }
}

#[test]
fn read_report_bytes_missing_sensor() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    fs::create_dir_all(&artifacts).unwrap();

    let layout = FsLayout::new(&artifacts, tmp.path().join("cockpit.toml"));
    let source = FsReceiptSource::new(layout);

    assert!(matches!(
        source.read_report_bytes("nonexistent").unwrap(),
        ReportRead::Missing
    ));
}

// ── Size cap enforcement ───────────────────────────────────────────────────

#[test]
fn oversized_receipt_is_rejected() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    fs::create_dir_all(&artifacts).unwrap();

    // Create a receipt that exceeds a small cap.
    let sensor_dir = artifacts.join("big-sensor");
    fs::create_dir_all(&sensor_dir).unwrap();
    let oversized = "x".repeat(1024);
    fs::write(sensor_dir.join("report.json"), &oversized).unwrap();

    let layout =
        FsLayout::new(&artifacts, tmp.path().join("cockpit.toml")).with_max_receipt_bytes(512); // 512 byte cap
    let source = FsReceiptSource::new(layout);

    match source.read_report_bytes("big-sensor").unwrap() {
        ReportRead::Oversized { size, cap } => {
            assert_eq!(size, 1024);
            assert_eq!(cap, 512);
        }
        other => panic!("expected Oversized, got {:?}", variant_name(&other)),
    }
}

// ── Path traversal protection ──────────────────────────────────────────────

#[test]
fn path_traversal_sensor_id_is_rejected_on_read() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    fs::create_dir_all(&artifacts).unwrap();

    let layout = FsLayout::new(&artifacts, tmp.path().join("cockpit.toml"));
    let source = FsReceiptSource::new(layout);

    // ".." in sensor ID must yield UnsafePath.
    assert!(matches!(
        source.read_report_bytes("..\\etc").unwrap(),
        ReportRead::UnsafePath
    ));
    assert!(matches!(
        source.read_report_bytes("../etc").unwrap(),
        ReportRead::UnsafePath
    ));
}

#[test]
fn path_traversal_sensor_id_excluded_from_discovery() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    fs::create_dir_all(&artifacts).unwrap();

    create_sensor(&artifacts, "good", &minimal_receipt_json());

    let layout = FsLayout::new(&artifacts, tmp.path().join("cockpit.toml"));
    let source = FsReceiptSource::new(layout);
    let discovered = source.discovered_sensors().unwrap();

    // Only the valid sensor appears.
    assert_eq!(discovered.sensors, vec!["good"]);
}

// ── Output writing ─────────────────────────────────────────────────────────

#[test]
fn write_cockpit_report_creates_file_with_content() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    fs::create_dir_all(&artifacts).unwrap();

    let layout = FsLayout::new(&artifacts, tmp.path().join("cockpit.toml"));
    let sink = FsOutputSink::new(layout.clone());

    let report_json = r#"{"schema":"cockpit.report.v1"}"#;
    sink.write_cockpit_report(report_json).unwrap();

    let written = fs::read_to_string(layout.cockpit_report_file()).unwrap();
    assert_eq!(written, report_json);
}

#[test]
fn write_cockpit_comment_creates_file_with_content() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    fs::create_dir_all(&artifacts).unwrap();

    let layout = FsLayout::new(&artifacts, tmp.path().join("cockpit.toml"));
    let sink = FsOutputSink::new(layout.clone());

    let comment = "# Cockpit Summary\nAll green.";
    sink.write_cockpit_comment(comment).unwrap();

    let written = fs::read_to_string(layout.cockpit_comment_file()).unwrap();
    assert_eq!(written, comment);
}

#[test]
fn write_extra_file_rejects_path_traversal() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    fs::create_dir_all(&artifacts).unwrap();

    let layout = FsLayout::new(&artifacts, tmp.path().join("cockpit.toml"));
    let sink = FsOutputSink::new(layout);

    assert!(sink.write_extra_file("../evil.txt", b"pwned").is_err());
    assert!(sink.write_extra_file("sub\\evil.txt", b"pwned").is_err());
    assert!(sink.write_extra_file("sub/evil.txt", b"pwned").is_err());
}

#[test]
fn output_creates_cockpit_dir_if_missing() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    // Don't create artifacts/cockpit/ — the sink should create it.

    let layout = FsLayout::new(&artifacts, tmp.path().join("cockpit.toml"));
    let sink = FsOutputSink::new(layout.clone());

    sink.write_cockpit_report("{}").unwrap();
    assert!(layout.cockpit_report_file().exists());
}

// ── Missing directories ────────────────────────────────────────────────────

#[test]
fn missing_artifacts_dir_returns_empty_sensors() {
    let tmp = TempDir::new().unwrap();
    let nonexistent = tmp.path().join("does-not-exist");

    let layout = FsLayout::new(&nonexistent, tmp.path().join("cockpit.toml"));
    let source = FsReceiptSource::new(layout);
    let discovered = source.discovered_sensors().unwrap();

    assert!(discovered.sensors.is_empty());
    assert_eq!(discovered.total_found, 0);
    assert!(!discovered.truncated);
}

// ── Empty artifacts ────────────────────────────────────────────────────────

#[test]
fn empty_artifacts_dir_returns_empty_sensors() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    fs::create_dir_all(&artifacts).unwrap();

    let layout = FsLayout::new(&artifacts, tmp.path().join("cockpit.toml"));
    let source = FsReceiptSource::new(layout);
    let discovered = source.discovered_sensors().unwrap();

    assert!(discovered.sensors.is_empty());
    assert_eq!(discovered.total_found, 0);
}

// ── Multiple sensors ───────────────────────────────────────────────────────

#[test]
fn multiple_sensors_correct_count_and_order() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    fs::create_dir_all(&artifacts).unwrap();

    let names = ["clippy", "builddiag", "coverage", "audit", "test-results"];
    for name in &names {
        create_sensor(&artifacts, name, &minimal_receipt_json());
    }

    let layout = FsLayout::new(&artifacts, tmp.path().join("cockpit.toml"));
    let source = FsReceiptSource::new(layout);
    let discovered = source.discovered_sensors().unwrap();

    assert_eq!(discovered.sensors.len(), 5);
    assert_eq!(discovered.total_found, 5);
    assert_eq!(
        discovered.sensors,
        vec!["audit", "builddiag", "clippy", "coverage", "test-results"]
    );
}

// ── Receipt cap (max_receipts) ─────────────────────────────────────────────

#[test]
fn max_receipts_cap_truncates_discovery() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    fs::create_dir_all(&artifacts).unwrap();

    for i in 0..5 {
        create_sensor(
            &artifacts,
            &format!("sensor-{i:02}"),
            &minimal_receipt_json(),
        );
    }

    let layout = FsLayout::new(&artifacts, tmp.path().join("cockpit.toml")).with_max_receipts(3);
    let source = FsReceiptSource::new(layout);
    let discovered = source.discovered_sensors().unwrap();

    assert_eq!(discovered.sensors.len(), 3);
    assert!(discovered.truncated);
    assert_eq!(discovered.total_found, 5);
    // Lexical first three.
    assert_eq!(
        discovered.sensors,
        vec!["sensor-00", "sensor-01", "sensor-02"]
    );
}

// ── Symlink handling ───────────────────────────────────────────────────────

// On Windows, creating symlinks requires elevated privileges.
// We conditionally test symlink protection on Unix only.
#[cfg(unix)]
#[test]
fn symlink_outside_artifacts_is_rejected() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    fs::create_dir_all(&artifacts).unwrap();

    // Create an external directory with a receipt.
    let external = tmp.path().join("external");
    fs::create_dir_all(&external).unwrap();
    fs::write(external.join("report.json"), minimal_receipt_json()).unwrap();

    // Symlink artifacts/evil -> ../external
    std::os::unix::fs::symlink(&external, artifacts.join("evil")).unwrap();

    let layout = FsLayout::new(&artifacts, tmp.path().join("cockpit.toml"));
    let source = FsReceiptSource::new(layout);

    // The symlinked sensor may appear in discovery (it's a dir with report.json),
    // but reading it should yield UnsafePath because the canonical path is outside artifacts.
    match source.read_report_bytes("evil").unwrap() {
        ReportRead::UnsafePath => {} // expected
        other => panic!("expected UnsafePath, got {:?}", variant_name(&other)),
    }
}

// ── Helpers ────────────────────────────────────────────────────────────────

fn variant_name(read: &ReportRead) -> &'static str {
    match read {
        ReportRead::Missing => "Missing",
        ReportRead::Bytes(_) => "Bytes",
        ReportRead::Oversized { .. } => "Oversized",
        ReportRead::UnsafePath => "UnsafePath",
    }
}
