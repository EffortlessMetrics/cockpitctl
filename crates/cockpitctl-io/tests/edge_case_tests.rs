//! Edge-case tests for cockpitctl-io filesystem adapters.
//!
//! These tests exercise FsLayout defaults/overrides, discovery boundary
//! conditions at the default cap, size-cap boundary values, sensor ID
//! validation edge cases, output sink safety, and deterministic ordering.

use cockpitctl_ingest::{OutputSink, ReceiptSource, ReportRead};
use cockpitctl_io::{DEFAULT_MAX_RECEIPTS, FsLayout, FsOutputSink, FsReceiptSource};
use std::fs;
use tempfile::TempDir;

/// Build a minimal valid sensor receipt JSON string.
fn minimal_receipt() -> String {
    serde_json::json!({
        "schema": "sensor.report.v1",
        "tool": { "name": "edge-test", "version": "0.1.0" },
        "run": { "started_at": "2025-01-01T00:00:00Z" },
        "verdict": {
            "status": "pass",
            "counts": { "info": 0, "warn": 0, "error": 0 }
        },
        "findings": []
    })
    .to_string()
}

/// Helper: create a sensor directory with a report.json.
fn create_sensor(artifacts: &std::path::Path, sensor_id: &str) {
    let dir = artifacts.join(sensor_id);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("report.json"), minimal_receipt()).unwrap();
}

// ═══════════════════════════════════════════════════════════════════════════
// 1–2. FsLayout DEFAULTS AND CUSTOM OVERRIDES
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn fs_layout_default_values() {
    let layout = FsLayout::new("artifacts", "cockpit.toml");

    assert_eq!(layout.max_receipt_bytes, 2 * 1024 * 1024, "default 2MB");
    assert_eq!(layout.max_receipts, DEFAULT_MAX_RECEIPTS);
    assert_eq!(layout.max_receipts, 100);
    assert_eq!(
        layout.out_dir,
        std::path::PathBuf::from("artifacts").join("cockpit")
    );
    assert_eq!(layout.config_path, std::path::PathBuf::from("cockpit.toml"));
}

#[test]
fn fs_layout_custom_overrides() {
    let layout = FsLayout::new("art", "cfg.toml")
        .with_max_receipt_bytes(512)
        .with_max_receipts(10);

    assert_eq!(layout.max_receipt_bytes, 512);
    assert_eq!(layout.max_receipts, 10);
    // Paths unchanged by overrides
    assert_eq!(layout.artifacts_dir, std::path::PathBuf::from("art"));
    assert_eq!(
        layout.out_dir,
        std::path::PathBuf::from("art").join("cockpit")
    );
}

#[test]
fn fs_layout_derived_path_helpers() {
    let layout = FsLayout::new("artifacts", "cockpit.toml");

    assert_eq!(
        layout.sensor_dir("builddiag"),
        std::path::PathBuf::from("artifacts").join("builddiag")
    );
    assert_eq!(
        layout.report_file("builddiag"),
        std::path::PathBuf::from("artifacts")
            .join("builddiag")
            .join("report.json")
    );
    assert_eq!(
        layout.comment_file("builddiag"),
        std::path::PathBuf::from("artifacts")
            .join("builddiag")
            .join("comment.md")
    );
    assert_eq!(
        layout.plan_file("builddiag"),
        std::path::PathBuf::from("artifacts")
            .join("builddiag")
            .join("plan.json")
    );
    assert_eq!(
        layout.cockpit_report_file(),
        std::path::PathBuf::from("artifacts")
            .join("cockpit")
            .join("report.json")
    );
    assert_eq!(
        layout.cockpit_comment_file(),
        std::path::PathBuf::from("artifacts")
            .join("cockpit")
            .join("comment.md")
    );
    assert_eq!(
        layout.sarif_report_file(),
        std::path::PathBuf::from("artifacts")
            .join("cockpit")
            .join("sarif.json")
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// 3–4. DISCOVER EMPTY DIR AND SINGLE SENSOR
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn discover_empty_artifacts_dir() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    fs::create_dir_all(&artifacts).unwrap();

    let src = FsReceiptSource::new(FsLayout::new(&artifacts, tmp.path().join("c.toml")));
    let disc = src.discovered_sensors().unwrap();

    assert!(disc.sensors.is_empty());
    assert_eq!(disc.total_found, 0);
    assert!(!disc.truncated);
    assert!(disc.invalid_sensor_ids.is_empty());
}

#[test]
fn discover_single_valid_sensor() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    create_sensor(&artifacts, "only-one");

    let src = FsReceiptSource::new(FsLayout::new(&artifacts, tmp.path().join("c.toml")));
    let disc = src.discovered_sensors().unwrap();

    assert_eq!(disc.sensors, vec!["only-one"]);
    assert_eq!(disc.total_found, 1);
    assert!(!disc.truncated);
}

// ═══════════════════════════════════════════════════════════════════════════
// 5–6. DISCOVER AT DEFAULT CAP (100) AND ONE OVER (101)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn discover_100_sensors_at_default_cap() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");

    for i in 0..100 {
        create_sensor(&artifacts, &format!("sensor-{i:03}"));
    }

    // Use default layout (max_receipts = 100)
    let src = FsReceiptSource::new(FsLayout::new(&artifacts, tmp.path().join("c.toml")));
    let disc = src.discovered_sensors().unwrap();

    assert_eq!(disc.sensors.len(), 100);
    assert_eq!(disc.total_found, 100);
    assert!(!disc.truncated, "exactly at cap should NOT be truncated");
}

#[test]
fn discover_101_sensors_exceeds_default_cap() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");

    for i in 0..101 {
        create_sensor(&artifacts, &format!("sensor-{i:03}"));
    }

    // Use default layout (max_receipts = 100)
    let src = FsReceiptSource::new(FsLayout::new(&artifacts, tmp.path().join("c.toml")));
    let disc = src.discovered_sensors().unwrap();

    assert_eq!(disc.sensors.len(), 100, "capped at 100");
    assert_eq!(disc.total_found, 101);
    assert!(disc.truncated, "one over cap should be truncated");
    // First 100 in lexical order are kept
    assert_eq!(disc.sensors[0], "sensor-000");
    assert_eq!(disc.sensors[99], "sensor-099");
}

// ═══════════════════════════════════════════════════════════════════════════
// 7. READ REPORT THAT DOESN'T EXIST
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn read_nonexistent_report_returns_missing() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    fs::create_dir_all(&artifacts).unwrap();

    let src = FsReceiptSource::new(FsLayout::new(&artifacts, tmp.path().join("c.toml")));

    // Sensor dir doesn't exist at all
    assert!(matches!(
        src.read_report_bytes("no-such-sensor").unwrap(),
        ReportRead::Missing
    ));
}

#[test]
fn read_report_sensor_dir_exists_but_no_report_json() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    let sensor_dir = artifacts.join("half-baked");
    fs::create_dir_all(&sensor_dir).unwrap();
    fs::write(sensor_dir.join("other.txt"), "not a report").unwrap();

    let src = FsReceiptSource::new(FsLayout::new(&artifacts, tmp.path().join("c.toml")));

    assert!(matches!(
        src.read_report_bytes("half-baked").unwrap(),
        ReportRead::Missing
    ));
}

// ═══════════════════════════════════════════════════════════════════════════
// 8–9. SIZE CAP BOUNDARY: OVER 2MB REJECTED, EXACTLY 2MB ACCEPTED
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn read_report_over_2mb_rejected() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    let sensor_dir = artifacts.join("oversized");
    fs::create_dir_all(&sensor_dir).unwrap();

    let cap = 2 * 1024 * 1024;
    let data = vec![b'x'; cap + 1];
    fs::write(sensor_dir.join("report.json"), &data).unwrap();

    let src = FsReceiptSource::new(FsLayout::new(&artifacts, tmp.path().join("c.toml")));

    match src.read_report_bytes("oversized").unwrap() {
        ReportRead::Oversized { size, cap: c } => {
            assert_eq!(size as usize, cap + 1);
            assert_eq!(c, cap);
        }
        other => panic!(
            "expected Oversized, got {:?}",
            std::mem::discriminant(&other)
        ),
    }
}

#[test]
fn read_report_at_exactly_2mb_accepted() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    let sensor_dir = artifacts.join("exact-2mb");
    fs::create_dir_all(&sensor_dir).unwrap();

    let cap = 2 * 1024 * 1024;
    let data = vec![b'x'; cap];
    fs::write(sensor_dir.join("report.json"), &data).unwrap();

    let src = FsReceiptSource::new(FsLayout::new(&artifacts, tmp.path().join("c.toml")));

    match src.read_report_bytes("exact-2mb").unwrap() {
        ReportRead::Bytes(b) => assert_eq!(b.len(), cap),
        other => panic!(
            "expected Bytes at exact 2MB, got {:?}",
            std::mem::discriminant(&other)
        ),
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 10–11. PATH TRAVERSAL IN SENSOR IDS
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn path_traversal_bare_dotdot_rejected() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    fs::create_dir_all(&artifacts).unwrap();
    let src = FsReceiptSource::new(FsLayout::new(&artifacts, tmp.path().join("c.toml")));

    assert!(matches!(
        src.read_report_bytes("..").unwrap(),
        ReportRead::UnsafePath
    ));
}

#[test]
fn path_traversal_embedded_dotdot_rejected() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    fs::create_dir_all(&artifacts).unwrap();
    let src = FsReceiptSource::new(FsLayout::new(&artifacts, tmp.path().join("c.toml")));

    assert!(matches!(
        src.read_report_bytes("foo/../bar").unwrap(),
        ReportRead::UnsafePath
    ));
}

// ═══════════════════════════════════════════════════════════════════════════
// 12–13. SENSOR IDS WITH SPACES AND UNICODE
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn sensor_id_with_spaces_in_discovery() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");

    // Create dirs with spaces (invalid sensor IDs)
    let spaced = artifacts.join("has space");
    fs::create_dir_all(&spaced).unwrap();
    fs::write(spaced.join("report.json"), minimal_receipt()).unwrap();

    create_sensor(&artifacts, "valid-sensor");

    let src = FsReceiptSource::new(FsLayout::new(&artifacts, tmp.path().join("c.toml")));
    let disc = src.discovered_sensors().unwrap();

    assert_eq!(disc.sensors, vec!["valid-sensor"]);
    assert!(
        disc.invalid_sensor_ids.contains(&"has space".to_string()),
        "space-containing ID should be in invalid list"
    );
}

#[test]
fn sensor_id_with_spaces_rejected_on_read() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    fs::create_dir_all(&artifacts).unwrap();
    let src = FsReceiptSource::new(FsLayout::new(&artifacts, tmp.path().join("c.toml")));

    assert!(matches!(
        src.read_report_bytes("has space").unwrap(),
        ReportRead::UnsafePath
    ));
}

#[test]
fn sensor_id_with_unicode_in_discovery() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");

    // Create a dir with unicode name (invalid sensor ID)
    let unicode_dir = artifacts.join("café");
    fs::create_dir_all(&unicode_dir).unwrap();
    fs::write(unicode_dir.join("report.json"), minimal_receipt()).unwrap();

    create_sensor(&artifacts, "ascii-only");

    let src = FsReceiptSource::new(FsLayout::new(&artifacts, tmp.path().join("c.toml")));
    let disc = src.discovered_sensors().unwrap();

    assert_eq!(disc.sensors, vec!["ascii-only"]);
    assert!(
        disc.invalid_sensor_ids.contains(&"café".to_string()),
        "unicode ID should be in invalid list"
    );
}

#[test]
fn sensor_id_with_unicode_rejected_on_read() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    fs::create_dir_all(&artifacts).unwrap();
    let src = FsReceiptSource::new(FsLayout::new(&artifacts, tmp.path().join("c.toml")));

    assert!(matches!(
        src.read_report_bytes("über").unwrap(),
        ReportRead::UnsafePath
    ));
}

// ═══════════════════════════════════════════════════════════════════════════
// 14. WRITE OUTPUT TO NON-EXISTENT DIR
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn write_output_creates_missing_directory() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("deep").join("nested").join("artifacts");
    // Don't create artifacts or cockpit dir — the sink must do it

    let layout = FsLayout::new(&artifacts, tmp.path().join("c.toml"));
    let sink = FsOutputSink::new(layout.clone());

    sink.write_cockpit_report(r#"{"schema":"cockpit.report.v1"}"#)
        .unwrap();

    assert!(layout.cockpit_report_file().exists());
    assert!(layout.out_dir.exists());
}

// ═══════════════════════════════════════════════════════════════════════════
// 15–17. WRITE EXTRA FILE: VALID NAME, ".." REJECTED, "/" REJECTED
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn write_extra_file_normal_name_accepted() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    let layout = FsLayout::new(&artifacts, tmp.path().join("c.toml"));
    let sink = FsOutputSink::new(layout.clone());

    sink.write_extra_file("sarif.json", b"{\"runs\":[]}")
        .unwrap();

    let content = fs::read_to_string(layout.out_dir.join("sarif.json")).unwrap();
    assert_eq!(content, "{\"runs\":[]}");
}

#[test]
fn write_extra_file_dotdot_rejected() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    let sink = FsOutputSink::new(FsLayout::new(&artifacts, tmp.path().join("c.toml")));

    assert!(sink.write_extra_file("../escape.txt", b"pwned").is_err());
    assert!(sink.write_extra_file("..", b"pwned").is_err());
    assert!(sink.write_extra_file("foo..bar", b"data").is_err());
}

#[test]
fn write_extra_file_slash_rejected() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    let sink = FsOutputSink::new(FsLayout::new(&artifacts, tmp.path().join("c.toml")));

    assert!(sink.write_extra_file("sub/file.json", b"data").is_err());
    assert!(sink.write_extra_file("sub\\file.json", b"data").is_err());
}

// ═══════════════════════════════════════════════════════════════════════════
// 18. OUTPUT REPORT IS VALID JSON
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn output_report_is_valid_json() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    let layout = FsLayout::new(&artifacts, tmp.path().join("c.toml"));
    let sink = FsOutputSink::new(layout.clone());

    let report = serde_json::json!({
        "schema": "cockpit.report.v1",
        "verdict": { "status": "pass" },
        "sensors": []
    });
    let json_str = serde_json::to_string_pretty(&report).unwrap();
    sink.write_cockpit_report(&json_str).unwrap();

    // Re-read and verify it parses as valid JSON
    let written = fs::read_to_string(layout.cockpit_report_file()).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&written).unwrap();
    assert_eq!(parsed["schema"], "cockpit.report.v1");
    assert_eq!(parsed["verdict"]["status"], "pass");
}

// ═══════════════════════════════════════════════════════════════════════════
// 19. MULTIPLE SENSORS IN CORRECT LEXICAL ORDER
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn multiple_sensors_strict_lexical_order() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");

    // Names chosen to exercise ASCII ordering edge cases:
    // hyphens vs underscores, numbers, lexical number ordering
    let names = [
        "Alpha",       // uppercase
        "Zulu",        // uppercase
        "a-sensor",    // lowercase with hyphen
        "build_diag",  // underscore
        "coverage-01", // with number suffix
        "coverage-10", // lexical "10" < "2" — verify this
        "coverage-2",
        "z-last",
    ];
    for name in &names {
        create_sensor(&artifacts, name);
    }

    let src = FsReceiptSource::new(FsLayout::new(&artifacts, tmp.path().join("c.toml")));
    let disc = src.discovered_sensors().unwrap();

    let mut expected: Vec<&str> = names.to_vec();
    expected.sort();
    assert_eq!(disc.sensors, expected);
    assert_eq!(disc.total_found, names.len());
    assert!(!disc.truncated);
}

// ═══════════════════════════════════════════════════════════════════════════
// ADDITIONAL EDGE CASES
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn write_comment_and_report_together() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    let layout = FsLayout::new(&artifacts, tmp.path().join("c.toml"));
    let sink = FsOutputSink::new(layout.clone());

    sink.write_cockpit_report(r#"{"schema":"cockpit.report.v1"}"#)
        .unwrap();
    sink.write_cockpit_comment("# Summary\nAll green.").unwrap();

    assert!(layout.cockpit_report_file().exists());
    assert!(layout.cockpit_comment_file().exists());

    let report = fs::read_to_string(layout.cockpit_report_file()).unwrap();
    let comment = fs::read_to_string(layout.cockpit_comment_file()).unwrap();
    assert!(report.contains("cockpit.report.v1"));
    assert!(comment.contains("All green."));
}

#[test]
fn report_path_helper_returns_expected_format() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    create_sensor(&artifacts, "my-sensor");

    let src = FsReceiptSource::new(FsLayout::new(&artifacts, tmp.path().join("c.toml")));

    assert_eq!(
        src.report_path("my-sensor"),
        "artifacts/my-sensor/report.json"
    );
    assert_eq!(
        src.report_path("builddiag"),
        "artifacts/builddiag/report.json"
    );
}

#[test]
fn overwrite_existing_output_files() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    let layout = FsLayout::new(&artifacts, tmp.path().join("c.toml"));
    let sink = FsOutputSink::new(layout.clone());

    // Write initial content
    sink.write_cockpit_report("initial").unwrap();
    assert_eq!(
        fs::read_to_string(layout.cockpit_report_file()).unwrap(),
        "initial"
    );

    // Overwrite
    sink.write_cockpit_report("updated").unwrap();
    assert_eq!(
        fs::read_to_string(layout.cockpit_report_file()).unwrap(),
        "updated"
    );
}

#[test]
fn discover_with_mixed_valid_and_invalid_sensor_ids() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");

    // Valid
    create_sensor(&artifacts, "alpha");
    create_sensor(&artifacts, "beta");

    // Invalid (dot, space, special char)
    for name in &["has.dot", "has space", "special!"] {
        let d = artifacts.join(name);
        fs::create_dir_all(&d).unwrap();
        fs::write(d.join("report.json"), minimal_receipt()).unwrap();
    }

    let src = FsReceiptSource::new(FsLayout::new(&artifacts, tmp.path().join("c.toml")));
    let disc = src.discovered_sensors().unwrap();

    assert_eq!(disc.sensors, vec!["alpha", "beta"]);
    assert_eq!(disc.invalid_sensor_ids.len(), 3);
    // Invalid IDs should be sorted
    let mut sorted = disc.invalid_sensor_ids.clone();
    sorted.sort();
    assert_eq!(disc.invalid_sensor_ids, sorted);
}
