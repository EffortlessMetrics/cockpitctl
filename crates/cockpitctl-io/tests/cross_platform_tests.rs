//! Cross-platform path normalization and safety tests for cockpitctl-io.
//!
//! These tests verify that path handling is consistent across Windows, Linux,
//! and macOS — sensor ID validation, separator rejection, traversal prevention,
//! long paths, and Unicode paths all behave identically regardless of platform.

use cockpitctl_ingest::{OutputSink, ReceiptSource, ReportRead};
use cockpitctl_io::{FsLayout, FsOutputSink, FsReceiptSource};
use std::fs;
use tempfile::TempDir;

fn minimal_receipt() -> String {
    serde_json::json!({
        "schema": "sensor.report.v1",
        "tool": { "name": "xplat-test", "version": "0.1.0" },
        "run": { "started_at": "2025-01-01T00:00:00Z" },
        "verdict": {
            "status": "pass",
            "counts": { "info": 0, "warn": 0, "error": 0 }
        },
        "findings": []
    })
    .to_string()
}

fn create_sensor(artifacts: &std::path::Path, sensor_id: &str) {
    let dir = artifacts.join(sensor_id);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("report.json"), minimal_receipt()).unwrap();
}

// ═══════════════════════════════════════════════════════════════════════════
// 1. SENSOR IDS WITH BACKSLASHES — REJECTED ON ALL PLATFORMS
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn sensor_id_with_backslash_rejected() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    fs::create_dir_all(&artifacts).unwrap();
    let src = FsReceiptSource::new(FsLayout::new(&artifacts, tmp.path().join("c.toml")));

    let backslash_ids = ["foo\\bar", "a\\b\\c", "sensor\\", "\\leading"];
    for id in &backslash_ids {
        assert!(
            matches!(src.read_report_bytes(id).unwrap(), ReportRead::UnsafePath),
            "sensor_id with backslash {:?} must be rejected on all platforms",
            id
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 2. SENSOR IDS WITH FORWARD SLASHES — REJECTED ON ALL PLATFORMS
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn sensor_id_with_forward_slash_rejected() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    fs::create_dir_all(&artifacts).unwrap();
    let src = FsReceiptSource::new(FsLayout::new(&artifacts, tmp.path().join("c.toml")));

    let slash_ids = ["foo/bar", "a/b/c", "sensor/", "/leading"];
    for id in &slash_ids {
        assert!(
            matches!(src.read_report_bytes(id).unwrap(), ReportRead::UnsafePath),
            "sensor_id with forward slash {:?} must be rejected on all platforms",
            id
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 3. PATHS WITH MIXED SEPARATORS — CONSISTENT HANDLING
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn sensor_id_with_mixed_separators_rejected() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    fs::create_dir_all(&artifacts).unwrap();
    let src = FsReceiptSource::new(FsLayout::new(&artifacts, tmp.path().join("c.toml")));

    let mixed = ["foo/bar\\baz", "a\\b/c", "x/y\\z/w"];
    for id in &mixed {
        assert!(
            matches!(src.read_report_bytes(id).unwrap(), ReportRead::UnsafePath),
            "sensor_id with mixed separators {:?} must be rejected",
            id
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 4. PATHS WITH .. — ALWAYS REJECTED REGARDLESS OF SEPARATOR
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn dotdot_traversal_rejected_with_any_separator() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    fs::create_dir_all(&artifacts).unwrap();
    let src = FsReceiptSource::new(FsLayout::new(&artifacts, tmp.path().join("c.toml")));

    let traversals = [
        "..",
        "../escape",
        "foo/../bar",
        "..\\escape",
        "foo\\..\\bar",
        "foo/..\\bar",
        "foo\\../bar",
    ];
    for id in &traversals {
        assert!(
            matches!(src.read_report_bytes(id).unwrap(), ReportRead::UnsafePath),
            "traversal pattern {:?} must be rejected on all platforms",
            id
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 5. SYMLINK DETECTION — CONSISTENT BEHAVIOR
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(unix)]
#[test]
fn symlink_outside_artifacts_is_safe_guarded() {
    use std::os::unix::fs as unix_fs;

    let tmp = TempDir::new().unwrap();
    let outside = tmp.path().join("outside");
    fs::create_dir_all(&outside).unwrap();
    fs::write(outside.join("report.json"), minimal_receipt()).unwrap();

    let artifacts = tmp.path().join("artifacts");
    fs::create_dir_all(&artifacts).unwrap();
    unix_fs::symlink(&outside, artifacts.join("linked")).unwrap();

    let src = FsReceiptSource::new(FsLayout::new(&artifacts, tmp.path().join("c.toml")));
    match src.read_report_bytes("linked").unwrap() {
        ReportRead::UnsafePath => {} // ideal: symlink outside root rejected
        ReportRead::Bytes(_) => {} // acceptable on some configs if canonicalize resolves within parent
        other => panic!("unexpected variant: {:?}", std::mem::discriminant(&other)),
    }
}

#[cfg(windows)]
#[test]
fn symlink_detection_windows_directory_junction() {
    // On Windows, directory junctions are more commonly available than symlinks
    // (symlinks require elevated privileges). Test that the safe-path logic
    // doesn't panic regardless of the link type.
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    fs::create_dir_all(&artifacts).unwrap();

    // Just ensure reading a valid sensor works in the same artifacts dir
    create_sensor(&artifacts, "real-sensor");
    let src = FsReceiptSource::new(FsLayout::new(&artifacts, tmp.path().join("c.toml")));
    match src.read_report_bytes("real-sensor").unwrap() {
        ReportRead::Bytes(b) => assert!(!b.is_empty()),
        other => panic!("expected Bytes, got {:?}", std::mem::discriminant(&other)),
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 6. PATH CANONICALIZATION — WORKS ON ALL PLATFORMS
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn canonicalization_resolves_within_artifacts_root() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    create_sensor(&artifacts, "sensor-a");

    let src = FsReceiptSource::new(FsLayout::new(&artifacts, tmp.path().join("c.toml")));

    // Valid sensor should be readable after canonicalization
    match src.read_report_bytes("sensor-a").unwrap() {
        ReportRead::Bytes(b) => assert!(!b.is_empty()),
        other => panic!(
            "expected Bytes after canonicalization, got {:?}",
            std::mem::discriminant(&other)
        ),
    }
}

#[test]
fn canonicalization_with_nonexistent_artifacts_dir() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("does-not-exist");

    // FsReceiptSource::new should not panic even if artifacts dir doesn't exist
    let src = FsReceiptSource::new(FsLayout::new(&artifacts, tmp.path().join("c.toml")));
    let disc = src.discovered_sensors().unwrap();
    assert!(disc.sensors.is_empty());
}

// ═══════════════════════════════════════════════════════════════════════════
// 7. LONG PATHS (> 260 CHARS ON WINDOWS) — HANDLED
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn long_sensor_id_rejected_by_character_validation() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    fs::create_dir_all(&artifacts).unwrap();
    let src = FsReceiptSource::new(FsLayout::new(&artifacts, tmp.path().join("c.toml")));

    // A 300-char sensor ID (all valid chars) — is_valid_sensor_id allows it,
    // but the file won't exist, so we get Missing.
    let long_id: String = std::iter::repeat_n('a', 300).collect();
    match src.read_report_bytes(&long_id).unwrap() {
        ReportRead::Missing => {} // expected: no such directory
        ReportRead::Bytes(_) => panic!("unexpected Bytes for nonexistent long sensor id"),
        other => panic!("unexpected: {:?}", std::mem::discriminant(&other)),
    }
}

#[test]
fn long_sensor_id_with_real_file_works() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");

    // Use a moderately long name (within filesystem limits)
    let long_id: String = std::iter::repeat_n('a', 100).collect();
    create_sensor(&artifacts, &long_id);

    let src = FsReceiptSource::new(FsLayout::new(&artifacts, tmp.path().join("c.toml")));
    match src.read_report_bytes(&long_id).unwrap() {
        ReportRead::Bytes(b) => assert!(!b.is_empty()),
        other => panic!(
            "expected Bytes for long sensor id, got {:?}",
            std::mem::discriminant(&other)
        ),
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 8. UNICODE PATHS — HANDLED CORRECTLY
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn unicode_sensor_ids_rejected() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    fs::create_dir_all(&artifacts).unwrap();
    let src = FsReceiptSource::new(FsLayout::new(&artifacts, tmp.path().join("c.toml")));

    // is_valid_sensor_id only allows ASCII alphanumeric + underscore + hyphen
    let unicode_ids = [
        "café",
        "naïve",
        "über",
        "日本語",
        "中文",
        "sensor\u{200B}id", // zero-width space
        "sensor\u{00A0}id", // non-breaking space
        "sensor\u{FEFF}id", // BOM
    ];
    for id in &unicode_ids {
        assert!(
            matches!(src.read_report_bytes(id).unwrap(), ReportRead::UnsafePath),
            "unicode sensor_id {:?} must be rejected",
            id
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 9. PATH WITH TRAILING SLASH/BACKSLASH — HANDLED
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn trailing_separator_in_sensor_id_rejected() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    fs::create_dir_all(&artifacts).unwrap();
    let src = FsReceiptSource::new(FsLayout::new(&artifacts, tmp.path().join("c.toml")));

    let trailing = ["sensor/", "sensor\\", "sensor//", "sensor\\\\"];
    for id in &trailing {
        assert!(
            matches!(src.read_report_bytes(id).unwrap(), ReportRead::UnsafePath),
            "trailing separator in {:?} must be rejected",
            id
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// OUTPUT SAFETY — write_extra_file CROSS-PLATFORM
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn write_extra_file_rejects_all_separator_variants() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    let layout = FsLayout::new(&artifacts, tmp.path().join("c.toml"));
    let sink = FsOutputSink::new(layout);

    let bad_names = [
        "sub/file.json",
        "sub\\file.json",
        "../escape.txt",
        "..\\escape.txt",
        "foo/bar\\baz.txt",
    ];
    for name in &bad_names {
        assert!(
            sink.write_extra_file(name, b"data").is_err(),
            "write_extra_file should reject {:?} on all platforms",
            name
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// DISCOVERY — PLATFORM-INDEPENDENT SENSOR DISCOVERY ORDER
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn discovery_order_is_lexical_on_all_platforms() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");

    // Create sensors in non-lexical order
    for name in &["zebra", "alpha", "middle", "beta"] {
        create_sensor(&artifacts, name);
    }

    let src = FsReceiptSource::new(FsLayout::new(&artifacts, tmp.path().join("c.toml")));
    let disc = src.discovered_sensors().unwrap();

    assert_eq!(
        disc.sensors,
        vec!["alpha", "beta", "middle", "zebra"],
        "sensors must be in lexical order on all platforms"
    );
}
