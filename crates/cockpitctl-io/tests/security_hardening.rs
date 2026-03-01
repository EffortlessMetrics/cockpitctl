//! Advanced security hardening tests for cockpitctl-io.
//!
//! These tests go beyond basic boundary testing to exercise adversarial input
//! scenarios: null byte injection, resource exhaustion, race conditions,
//! permission errors, and filesystem edge cases. Safety means controlled
//! findings, not crashes.

use cockpitctl_ingest::{ReceiptSource, ReportRead};
use cockpitctl_io::{FsLayout, FsReceiptSource};
use std::fs;
use tempfile::TempDir;

/// Build a minimal valid sensor receipt JSON string.
fn minimal_receipt() -> String {
    serde_json::json!({
        "schema": "sensor.report.v1",
        "tool": { "name": "security-test", "version": "0.1.0" },
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
// NULL BYTE INJECTION
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn null_byte_in_sensor_id_rejected() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    fs::create_dir_all(&artifacts).unwrap();
    let src = FsReceiptSource::new(FsLayout::new(&artifacts, tmp.path().join("c.toml")));

    // Null byte at various positions
    let attacks = ["sensor\0id", "\0sensor", "sensor\0", "\0"];
    for attack in &attacks {
        assert!(
            matches!(
                src.read_report_bytes(attack).unwrap(),
                ReportRead::UnsafePath
            ),
            "expected UnsafePath for null-byte sensor_id {:?}",
            attack
        );
    }
}

#[test]
fn null_byte_not_in_discovered_sensors() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    create_sensor(&artifacts, "valid-sensor");
    // Cannot create a directory with null bytes on most OS, but verify
    // discovery does not panic even with unusual entries.
    let src = FsReceiptSource::new(FsLayout::new(&artifacts, tmp.path().join("c.toml")));
    let disc = src.discovered_sensors().unwrap();
    for id in &disc.sensors {
        assert!(
            !id.contains('\0'),
            "discovered sensor ID should not contain null bytes"
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// VERY LONG SENSOR ID
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn very_long_sensor_id_handled_without_oom() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    fs::create_dir_all(&artifacts).unwrap();
    let src = FsReceiptSource::new(FsLayout::new(&artifacts, tmp.path().join("c.toml")));

    // 10,000-char sensor ID: only [a-zA-Z0-9_-] are valid, so this is technically
    // valid format-wise but the filesystem will reject creating such paths.
    let long_id: String = "a".repeat(10_000);
    // Should not OOM or panic — either UnsafePath or Missing is acceptable.
    let result = src.read_report_bytes(&long_id).unwrap();
    assert!(
        matches!(result, ReportRead::Missing | ReportRead::UnsafePath),
        "very long sensor ID should be Missing or UnsafePath, not crash"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// DEEPLY NESTED JSON (STACK OVERFLOW RESISTANCE)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn deeply_nested_json_does_not_stack_overflow() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    let sensor_dir = artifacts.join("deep-json");
    fs::create_dir_all(&sensor_dir).unwrap();

    // Create 1000-level nested JSON: {"a":{"a":{"a":...}}}
    let depth = 1000;
    let mut json = String::with_capacity(depth * 6);
    for _ in 0..depth {
        json.push_str("{\"a\":");
    }
    json.push_str("null");
    for _ in 0..depth {
        json.push('}');
    }

    fs::write(sensor_dir.join("report.json"), &json).unwrap();

    let layout = FsLayout::new(&artifacts, tmp.path().join("c.toml"));
    let src = FsReceiptSource::new(layout);

    // Should read bytes without stack overflow — content is invalid but reading is safe
    match src.read_report_bytes("deep-json").unwrap() {
        ReportRead::Bytes(b) => assert!(!b.is_empty()),
        other => panic!(
            "expected Bytes for deeply nested JSON, got {:?}",
            std::mem::discriminant(&other)
        ),
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// JSON BOMB (ZIP-BOMB EQUIVALENT)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn json_bomb_rejected_by_size_cap() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    let sensor_dir = artifacts.join("json-bomb");
    fs::create_dir_all(&sensor_dir).unwrap();

    // Create a 1MB JSON payload (exceeds a small cap)
    let big_value: String = "a".repeat(1_000_000);
    let bomb = format!("{{\"a\":\"{}\"}}", big_value);
    fs::write(sensor_dir.join("report.json"), &bomb).unwrap();

    // Set a 512KB cap — the bomb should be rejected
    let cap = 512 * 1024;
    let layout = FsLayout::new(&artifacts, tmp.path().join("c.toml")).with_max_receipt_bytes(cap);
    let src = FsReceiptSource::new(layout);

    match src.read_report_bytes("json-bomb").unwrap() {
        ReportRead::Oversized { size, cap: c } => {
            assert!(size as usize > cap, "oversized size should exceed cap");
            assert_eq!(c, cap);
        }
        other => panic!(
            "expected Oversized for JSON bomb, got {:?}",
            std::mem::discriminant(&other)
        ),
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// CIRCULAR SYMLINKS
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(unix)]
#[test]
fn circular_symlinks_no_infinite_loop() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    fs::create_dir_all(&artifacts).unwrap();

    // Create a symlink loop: a -> b, b -> a
    let a = artifacts.join("loop-a");
    let b = artifacts.join("loop-b");
    std::os::unix::fs::symlink(&b, &a).unwrap();
    std::os::unix::fs::symlink(&a, &b).unwrap();

    let src = FsReceiptSource::new(FsLayout::new(&artifacts, tmp.path().join("c.toml")));

    // Discovery should complete without infinite loop
    let disc = src.discovered_sensors().unwrap();
    // Symlinks should be excluded or produce invalid IDs, not hang
    assert!(
        !disc.sensors.contains(&"loop-a".to_string())
            || !disc.sensors.contains(&"loop-b".to_string()),
        "circular symlinks should not both appear as valid sensors"
    );
}

#[cfg(windows)]
#[test]
fn symlink_like_junction_no_crash() {
    // On Windows, creating symlinks requires elevated privileges.
    // Instead, test that a non-directory entry doesn't cause infinite loop.
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    fs::create_dir_all(&artifacts).unwrap();

    // Create a file where a directory is expected
    fs::write(artifacts.join("not-a-dir"), "I am a file").unwrap();

    let src = FsReceiptSource::new(FsLayout::new(&artifacts, tmp.path().join("c.toml")));
    let disc = src.discovered_sensors().unwrap();
    // File entries should not appear as sensors
    assert!(
        !disc.sensors.contains(&"not-a-dir".to_string()),
        "non-directory should not be discovered as a sensor"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// RACE CONDITION RESISTANCE
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn race_condition_file_deleted_during_read() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    create_sensor(&artifacts, "ephemeral");

    let src = FsReceiptSource::new(FsLayout::new(&artifacts, tmp.path().join("c.toml")));

    // Discover sensors first
    let disc = src.discovered_sensors().unwrap();
    assert!(disc.sensors.contains(&"ephemeral".to_string()));

    // Delete the file between discovery and read (simulating TOCTOU)
    fs::remove_file(artifacts.join("ephemeral").join("report.json")).unwrap();

    // Read should gracefully return Missing, not panic
    match src.read_report_bytes("ephemeral").unwrap() {
        ReportRead::Missing => {} // expected
        other => panic!(
            "expected Missing after deletion, got {:?}",
            std::mem::discriminant(&other)
        ),
    }
}

#[test]
fn race_condition_directory_deleted_during_read() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    create_sensor(&artifacts, "vanishing");

    let src = FsReceiptSource::new(FsLayout::new(&artifacts, tmp.path().join("c.toml")));

    // Delete entire sensor directory after source creation
    fs::remove_dir_all(artifacts.join("vanishing")).unwrap();

    // Should not crash
    let result = src.read_report_bytes("vanishing").unwrap();
    assert!(
        matches!(result, ReportRead::Missing),
        "deleted directory should yield Missing"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// BINARY / NON-UTF8 CONTENT
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn binary_content_in_receipt_returns_bytes() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    let sensor_dir = artifacts.join("binary-sensor");
    fs::create_dir_all(&sensor_dir).unwrap();

    // Write raw binary (non-UTF8) bytes
    let binary: Vec<u8> = vec![0xFF, 0xFE, 0x00, 0x01, 0x80, 0x90, 0xAB, 0xCD];
    fs::write(sensor_dir.join("report.json"), &binary).unwrap();

    let layout = FsLayout::new(&artifacts, tmp.path().join("c.toml"));
    let src = FsReceiptSource::new(layout);

    // IO layer reads raw bytes — it should not crash on non-UTF8
    match src.read_report_bytes("binary-sensor").unwrap() {
        ReportRead::Bytes(b) => assert_eq!(b, binary),
        other => panic!(
            "expected Bytes for binary content, got {:?}",
            std::mem::discriminant(&other)
        ),
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// EMPTY FILE
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn empty_receipt_file_returns_empty_bytes() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    let sensor_dir = artifacts.join("empty-file");
    fs::create_dir_all(&sensor_dir).unwrap();
    fs::write(sensor_dir.join("report.json"), b"").unwrap();

    let layout = FsLayout::new(&artifacts, tmp.path().join("c.toml"));
    let src = FsReceiptSource::new(layout);

    // Empty file is under size cap → returns empty Bytes, not a crash
    match src.read_report_bytes("empty-file").unwrap() {
        ReportRead::Bytes(b) => assert!(b.is_empty(), "empty file should produce empty bytes"),
        other => panic!(
            "expected empty Bytes, got {:?}",
            std::mem::discriminant(&other)
        ),
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// DIRECTORY WHERE FILE EXPECTED
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn directory_where_report_json_expected_is_error() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    let sensor_dir = artifacts.join("dir-as-file");
    fs::create_dir_all(&sensor_dir).unwrap();

    // Create report.json as a directory instead of a file
    fs::create_dir_all(sensor_dir.join("report.json")).unwrap();

    let layout = FsLayout::new(&artifacts, tmp.path().join("c.toml"));
    let src = FsReceiptSource::new(layout);

    // Should produce an error or Missing, not crash
    let result = src.read_report_bytes("dir-as-file");
    match result {
        Ok(ReportRead::Bytes(_)) => {
            panic!("reading a directory as a file should not succeed with Bytes")
        }
        // Any other result (Missing, error, etc.) is acceptable
        Ok(_) => {}
        Err(_) => {} // IO error is fine
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// PERMISSION ERRORS (PLATFORM-DEPENDENT)
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(unix)]
#[test]
fn read_only_file_handled_gracefully() {
    use std::os::unix::fs::PermissionsExt;

    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    let sensor_dir = artifacts.join("no-read");
    fs::create_dir_all(&sensor_dir).unwrap();

    let report_path = sensor_dir.join("report.json");
    fs::write(&report_path, minimal_receipt()).unwrap();

    // Remove read permission
    fs::set_permissions(&report_path, fs::Permissions::from_mode(0o000)).unwrap();

    let layout = FsLayout::new(&artifacts, tmp.path().join("c.toml"));
    let src = FsReceiptSource::new(layout);

    // Should return an error, not panic
    let result = src.read_report_bytes("no-read");
    // Restore permissions for cleanup
    fs::set_permissions(&report_path, fs::Permissions::from_mode(0o644)).unwrap();

    assert!(
        result.is_err()
            || matches!(
                result.unwrap(),
                ReportRead::Missing | ReportRead::UnsafePath
            ),
        "permission error should be handled gracefully"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// MULTIPLE ATTACK VECTORS COMBINED
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn combined_attack_vectors_all_rejected() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    fs::create_dir_all(&artifacts).unwrap();
    let src = FsReceiptSource::new(FsLayout::new(&artifacts, tmp.path().join("c.toml")));

    let attacks = [
        "../../../etc/passwd",
        "..\\..\\windows\\system32",
        "sensor\0/../../etc",
        "sensor\r\nid",
        "sensor\tid",
        ".hidden",
        " leading-space",
        "trailing-space ",
        "sensor id", // space in name
    ];

    for attack in &attacks {
        let result = src.read_report_bytes(attack).unwrap();
        assert!(
            matches!(result, ReportRead::UnsafePath | ReportRead::Missing),
            "attack vector {:?} should be rejected, got {:?}",
            attack,
            std::mem::discriminant(&result)
        );
    }
}
