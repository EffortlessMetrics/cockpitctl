//! Comprehensive safety boundary tests for cockpitctl-io.
//!
//! These tests exercise the *edges* of every safety mechanism in the IO crate:
//! path traversal rejection, receipt size caps, sensor count limits, and output
//! containment. Each test targets a specific boundary condition or attack vector
//! that is not already covered by the unit or integration tests.

use cockpitctl_ingest::{CommentRead, OutputSink, ReceiptSource, ReportRead};
use cockpitctl_io::{FsLayout, FsOutputSink, FsReceiptSource};
use std::fs;
use tempfile::TempDir;

/// Build a minimal valid sensor receipt JSON string.
fn minimal_receipt() -> String {
    serde_json::json!({
        "schema": "sensor.report.v1",
        "tool": { "name": "boundary-test", "version": "0.1.0" },
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
// PATH TRAVERSAL REJECTION
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn traversal_parent_prefix_rejected() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    fs::create_dir_all(&artifacts).unwrap();
    let src = FsReceiptSource::new(FsLayout::new(&artifacts, tmp.path().join("c.toml")));

    assert!(matches!(
        src.read_report_bytes("../etc/passwd").unwrap(),
        ReportRead::UnsafePath
    ));
}

#[test]
fn traversal_intermediate_dotdot_rejected() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    fs::create_dir_all(&artifacts).unwrap();
    let src = FsReceiptSource::new(FsLayout::new(&artifacts, tmp.path().join("c.toml")));

    // foo/../bar contains ".." and "/" — both banned by is_valid_sensor_id
    assert!(matches!(
        src.read_report_bytes("foo/../bar").unwrap(),
        ReportRead::UnsafePath
    ));
}

#[test]
fn traversal_bare_dot_rejected() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    fs::create_dir_all(&artifacts).unwrap();
    let src = FsReceiptSource::new(FsLayout::new(&artifacts, tmp.path().join("c.toml")));

    // "." contains a dot character, which is not in [a-zA-Z0-9_-]
    assert!(matches!(
        src.read_report_bytes(".").unwrap(),
        ReportRead::UnsafePath
    ));
}

#[test]
fn traversal_bare_dotdot_rejected() {
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
fn traversal_forward_slash_rejected() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    fs::create_dir_all(&artifacts).unwrap();
    let src = FsReceiptSource::new(FsLayout::new(&artifacts, tmp.path().join("c.toml")));

    assert!(matches!(
        src.read_report_bytes("foo/bar").unwrap(),
        ReportRead::UnsafePath
    ));
}

#[test]
fn traversal_backslash_rejected() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    fs::create_dir_all(&artifacts).unwrap();
    let src = FsReceiptSource::new(FsLayout::new(&artifacts, tmp.path().join("c.toml")));

    assert!(matches!(
        src.read_report_bytes("foo\\bar").unwrap(),
        ReportRead::UnsafePath
    ));
}

#[test]
fn traversal_null_byte_rejected() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    fs::create_dir_all(&artifacts).unwrap();
    let src = FsReceiptSource::new(FsLayout::new(&artifacts, tmp.path().join("c.toml")));

    // Null byte is not in the valid ASCII set
    assert!(matches!(
        src.read_report_bytes("sensor\0id").unwrap(),
        ReportRead::UnsafePath
    ));
}

#[test]
fn traversal_unicode_tricks_rejected() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    fs::create_dir_all(&artifacts).unwrap();
    let src = FsReceiptSource::new(FsLayout::new(&artifacts, tmp.path().join("c.toml")));

    // Various Unicode tricks that could confuse path resolution
    let attacks = [
        "sensor\u{2025}",   // TWO DOT LEADER (‥)
        "sensor\u{FF0F}",   // FULLWIDTH SOLIDUS (／)
        "sensor\u{FF3C}",   // FULLWIDTH REVERSE SOLIDUS (＼)
        "\u{002E}\u{002E}", // plain ".." via Unicode escapes
        "sensor\u{2024}",   // ONE DOT LEADER (․)
        "sensor\u{00E9}",   // accented character (é)
    ];
    for attack in &attacks {
        assert!(
            matches!(
                src.read_report_bytes(attack).unwrap(),
                ReportRead::UnsafePath
            ),
            "expected UnsafePath for sensor_id {:?}",
            attack
        );
    }
}

#[test]
fn traversal_empty_sensor_id_rejected() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    fs::create_dir_all(&artifacts).unwrap();
    let src = FsReceiptSource::new(FsLayout::new(&artifacts, tmp.path().join("c.toml")));

    assert!(matches!(
        src.read_report_bytes("").unwrap(),
        ReportRead::UnsafePath
    ));
}

#[test]
fn valid_sensor_ids_accepted() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");

    let valid_ids = ["builddiag", "my-sensor", "sensor_123", "a", "A-B_C-d"];
    for id in &valid_ids {
        create_sensor(&artifacts, id);
    }

    let src = FsReceiptSource::new(FsLayout::new(&artifacts, tmp.path().join("c.toml")));

    for id in &valid_ids {
        match src.read_report_bytes(id).unwrap() {
            ReportRead::Bytes(b) => assert!(!b.is_empty(), "expected non-empty bytes for {id}"),
            other => panic!(
                "expected Bytes for valid sensor_id {id}, got {:?}",
                std::mem::discriminant(&other)
            ),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// COMMENT READ SAFETY
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn comment_read_rejects_traversal_sensor_ids() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    fs::create_dir_all(&artifacts).unwrap();
    let src = FsReceiptSource::new(FsLayout::new(&artifacts, tmp.path().join("c.toml")));

    let attacks = ["../escape", "foo/../bar", "..", "foo/bar", "foo\\bar", ""];
    for attack in &attacks {
        assert!(
            matches!(
                src.comment_path_if_present(attack).unwrap(),
                CommentRead::UnsafePath
            ),
            "expected UnsafePath for comment_path_if_present({:?})",
            attack
        );
    }
}

#[test]
fn comment_read_returns_missing_for_valid_sensor_without_comment() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    create_sensor(&artifacts, "sensor-a");
    // sensor-a has report.json but no comment.md
    let src = FsReceiptSource::new(FsLayout::new(&artifacts, tmp.path().join("c.toml")));

    assert!(matches!(
        src.comment_path_if_present("sensor-a").unwrap(),
        CommentRead::Missing
    ));
}

#[test]
fn comment_read_returns_present_for_sensor_with_comment() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    create_sensor(&artifacts, "sensor-b");
    fs::write(artifacts.join("sensor-b").join("comment.md"), "# Findings").unwrap();

    let src = FsReceiptSource::new(FsLayout::new(&artifacts, tmp.path().join("c.toml")));

    match src.comment_path_if_present("sensor-b").unwrap() {
        CommentRead::Present(path) => assert!(path.contains("sensor-b/comment.md")),
        other => panic!("expected Present, got {:?}", std::mem::discriminant(&other)),
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// PLAN READ SAFETY
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn plan_read_rejects_traversal_sensor_ids() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    fs::create_dir_all(&artifacts).unwrap();
    let src = FsReceiptSource::new(FsLayout::new(&artifacts, tmp.path().join("c.toml")));

    let attacks = ["../escape", "..", "foo/bar"];
    for attack in &attacks {
        // Invalid sensor IDs yield Missing for plan_read (not UnsafePath)
        assert!(
            matches!(
                src.read_plan_bytes(attack).unwrap(),
                cockpitctl_ingest::PlanRead::Missing
            ),
            "expected Missing for read_plan_bytes({:?})",
            attack
        );
    }
}

#[test]
fn plan_read_oversized_rejected() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    let sensor_dir = artifacts.join("planner");
    fs::create_dir_all(&sensor_dir).unwrap();
    fs::write(sensor_dir.join("report.json"), minimal_receipt()).unwrap();

    let cap = 256;
    let oversized = vec![b'x'; cap + 1];
    fs::write(sensor_dir.join("plan.json"), &oversized).unwrap();

    let layout = FsLayout::new(&artifacts, tmp.path().join("c.toml")).with_max_receipt_bytes(cap);
    let src = FsReceiptSource::new(layout);

    match src.read_plan_bytes("planner").unwrap() {
        cockpitctl_ingest::PlanRead::Oversized { size, cap: c } => {
            assert_eq!(size as usize, cap + 1);
            assert_eq!(c, cap);
        }
        other => panic!(
            "expected Oversized, got {:?}",
            std::mem::discriminant(&other)
        ),
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// SIZE CAP ENFORCEMENT (BOUNDARY VALUES)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn size_cap_custom_override_exact_boundary() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    let sensor_dir = artifacts.join("sized");
    fs::create_dir_all(&sensor_dir).unwrap();

    let cap = 512;

    // Exactly at cap → accepted
    fs::write(sensor_dir.join("report.json"), vec![b'a'; cap]).unwrap();
    let layout = FsLayout::new(&artifacts, tmp.path().join("c.toml")).with_max_receipt_bytes(cap);
    let src = FsReceiptSource::new(layout);
    match src.read_report_bytes("sized").unwrap() {
        ReportRead::Bytes(b) => assert_eq!(b.len(), cap),
        other => panic!(
            "expected Bytes at exact cap, got {:?}",
            std::mem::discriminant(&other)
        ),
    }

    // One byte over → rejected
    fs::write(sensor_dir.join("report.json"), vec![b'a'; cap + 1]).unwrap();
    let layout2 = FsLayout::new(&artifacts, tmp.path().join("c.toml")).with_max_receipt_bytes(cap);
    let src2 = FsReceiptSource::new(layout2);
    match src2.read_report_bytes("sized").unwrap() {
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
fn zero_byte_receipt_handled_gracefully() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    let sensor_dir = artifacts.join("empty-receipt");
    fs::create_dir_all(&sensor_dir).unwrap();
    fs::write(sensor_dir.join("report.json"), b"").unwrap();

    let layout = FsLayout::new(&artifacts, tmp.path().join("c.toml"));
    let src = FsReceiptSource::new(layout);

    // 0 bytes is under every cap → returns Bytes (empty)
    match src.read_report_bytes("empty-receipt").unwrap() {
        ReportRead::Bytes(b) => assert!(b.is_empty()),
        other => panic!(
            "expected Bytes (empty), got {:?}",
            std::mem::discriminant(&other)
        ),
    }
}

#[test]
fn size_cap_one_byte_cap() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    let sensor_dir = artifacts.join("tiny");
    fs::create_dir_all(&sensor_dir).unwrap();

    // With a 1-byte cap, only single-byte receipts pass
    fs::write(sensor_dir.join("report.json"), b"x").unwrap();
    let layout = FsLayout::new(&artifacts, tmp.path().join("c.toml")).with_max_receipt_bytes(1);
    let src = FsReceiptSource::new(layout);
    assert!(matches!(
        src.read_report_bytes("tiny").unwrap(),
        ReportRead::Bytes(_)
    ));

    // Two bytes should be rejected
    fs::write(sensor_dir.join("report.json"), b"xx").unwrap();
    let layout2 = FsLayout::new(&artifacts, tmp.path().join("c.toml")).with_max_receipt_bytes(1);
    let src2 = FsReceiptSource::new(layout2);
    assert!(matches!(
        src2.read_report_bytes("tiny").unwrap(),
        ReportRead::Oversized { .. }
    ));
}

// ═══════════════════════════════════════════════════════════════════════════
// SENSOR COUNT CAP (BOUNDARY VALUES)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn exactly_at_max_receipts_not_truncated() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");

    let max = 4;
    for i in 0..max {
        create_sensor(&artifacts, &format!("s{i:02}"));
    }

    let layout = FsLayout::new(&artifacts, tmp.path().join("c.toml")).with_max_receipts(max);
    let src = FsReceiptSource::new(layout);
    let disc = src.discovered_sensors().unwrap();

    assert_eq!(disc.sensors.len(), max);
    assert_eq!(disc.total_found, max);
    assert!(!disc.truncated, "exactly at cap should NOT be truncated");
}

#[test]
fn one_over_max_receipts_truncated() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");

    let max = 4;
    for i in 0..=max {
        create_sensor(&artifacts, &format!("s{i:02}"));
    }

    let layout = FsLayout::new(&artifacts, tmp.path().join("c.toml")).with_max_receipts(max);
    let src = FsReceiptSource::new(layout);
    let disc = src.discovered_sensors().unwrap();

    assert_eq!(disc.sensors.len(), max);
    assert_eq!(disc.total_found, max + 1);
    assert!(disc.truncated, "one over cap should be truncated");
    // The first `max` sensors in lexical order are kept
    assert_eq!(disc.sensors, vec!["s00", "s01", "s02", "s03"]);
}

#[test]
fn max_receipts_one_allows_exactly_one_sensor() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    create_sensor(&artifacts, "alpha");
    create_sensor(&artifacts, "beta");

    let layout = FsLayout::new(&artifacts, tmp.path().join("c.toml")).with_max_receipts(1);
    let src = FsReceiptSource::new(layout);
    let disc = src.discovered_sensors().unwrap();

    assert_eq!(disc.sensors.len(), 1);
    assert_eq!(disc.sensors[0], "alpha"); // lexical first
    assert!(disc.truncated);
    assert_eq!(disc.total_found, 2);
}

#[test]
fn zero_sensors_with_cap_returns_empty() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    fs::create_dir_all(&artifacts).unwrap();

    let layout = FsLayout::new(&artifacts, tmp.path().join("c.toml")).with_max_receipts(5);
    let src = FsReceiptSource::new(layout);
    let disc = src.discovered_sensors().unwrap();

    assert!(disc.sensors.is_empty());
    assert!(!disc.truncated);
    assert_eq!(disc.total_found, 0);
}

// ═══════════════════════════════════════════════════════════════════════════
// OUTPUT SAFETY (write_extra_file)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn extra_file_valid_name_succeeds() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    let layout = FsLayout::new(&artifacts, tmp.path().join("c.toml"));
    let sink = FsOutputSink::new(layout.clone());

    sink.write_extra_file("sarif.json", b"{}").unwrap();
    let content = fs::read_to_string(layout.out_dir.join("sarif.json")).unwrap();
    assert_eq!(content, "{}");
}

#[test]
fn extra_file_dotdot_escape_rejected() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    let layout = FsLayout::new(&artifacts, tmp.path().join("c.toml"));
    let sink = FsOutputSink::new(layout);

    assert!(sink.write_extra_file("../escape.txt", b"pwned").is_err());
}

#[test]
fn extra_file_forward_slash_rejected() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    let layout = FsLayout::new(&artifacts, tmp.path().join("c.toml"));
    let sink = FsOutputSink::new(layout);

    assert!(sink.write_extra_file("sub/file.json", b"pwned").is_err());
}

#[test]
fn extra_file_backslash_rejected() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    let layout = FsLayout::new(&artifacts, tmp.path().join("c.toml"));
    let sink = FsOutputSink::new(layout);

    assert!(sink.write_extra_file("sub\\file.json", b"pwned").is_err());
}

#[test]
fn extra_file_dotdot_only_rejected() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    let layout = FsLayout::new(&artifacts, tmp.path().join("c.toml"));
    let sink = FsOutputSink::new(layout);

    assert!(sink.write_extra_file("..", b"pwned").is_err());
}

#[test]
fn extra_file_embedded_dotdot_rejected() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    let layout = FsLayout::new(&artifacts, tmp.path().join("c.toml"));
    let sink = FsOutputSink::new(layout);

    // "foo..bar" contains ".." substring — rejected by the contains check
    assert!(sink.write_extra_file("foo..bar", b"data").is_err());
}

// ═══════════════════════════════════════════════════════════════════════════
// DISCOVERY EDGE CASES
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn hidden_directories_filtered_from_discovery() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    fs::create_dir_all(&artifacts).unwrap();

    // .hidden has a dot → not a valid sensor_id (only [a-zA-Z0-9_-])
    let hidden = artifacts.join(".hidden");
    fs::create_dir_all(&hidden).unwrap();
    fs::write(hidden.join("report.json"), minimal_receipt()).unwrap();

    // A valid sensor for comparison
    create_sensor(&artifacts, "visible");

    let src = FsReceiptSource::new(FsLayout::new(&artifacts, tmp.path().join("c.toml")));
    let disc = src.discovered_sensors().unwrap();

    assert_eq!(disc.sensors, vec!["visible"]);
    assert!(
        disc.invalid_sensor_ids.contains(&".hidden".to_string()),
        "hidden dir should appear in invalid_sensor_ids"
    );
}

#[test]
fn non_directory_entries_skipped_in_discovery() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    fs::create_dir_all(&artifacts).unwrap();

    // File at top-level of artifacts (not a directory)
    fs::write(artifacts.join("stray-file"), "data").unwrap();
    create_sensor(&artifacts, "real-sensor");

    let src = FsReceiptSource::new(FsLayout::new(&artifacts, tmp.path().join("c.toml")));
    let disc = src.discovered_sensors().unwrap();

    assert_eq!(disc.sensors, vec!["real-sensor"]);
}

#[test]
fn sensor_dir_without_report_json_not_discovered() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    fs::create_dir_all(&artifacts).unwrap();

    // Directory with files but no report.json
    let sensor_dir = artifacts.join("incomplete");
    fs::create_dir_all(&sensor_dir).unwrap();
    fs::write(sensor_dir.join("other.txt"), "not a report").unwrap();

    create_sensor(&artifacts, "complete");

    let src = FsReceiptSource::new(FsLayout::new(&artifacts, tmp.path().join("c.toml")));
    let disc = src.discovered_sensors().unwrap();

    assert_eq!(disc.sensors, vec!["complete"]);
}

#[test]
fn cockpit_reserved_dir_always_excluded() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");
    create_sensor(&artifacts, "cockpit");
    create_sensor(&artifacts, "real");

    let src = FsReceiptSource::new(FsLayout::new(&artifacts, tmp.path().join("c.toml")));
    let disc = src.discovered_sensors().unwrap();

    assert_eq!(disc.sensors, vec!["real"]);
    // "cockpit" is filtered by name, not by validity — should not appear in invalid list
    assert!(!disc.invalid_sensor_ids.contains(&"cockpit".to_string()));
}

#[test]
fn invalid_sensor_ids_collected_and_sorted() {
    let tmp = TempDir::new().unwrap();
    let artifacts = tmp.path().join("artifacts");

    // Create directories with various invalid names
    for name in &["has.dot", "has space", "special!", "100%"] {
        let d = artifacts.join(name);
        fs::create_dir_all(&d).unwrap();
        fs::write(d.join("report.json"), minimal_receipt()).unwrap();
    }
    create_sensor(&artifacts, "valid");

    let src = FsReceiptSource::new(FsLayout::new(&artifacts, tmp.path().join("c.toml")));
    let disc = src.discovered_sensors().unwrap();

    assert_eq!(disc.sensors, vec!["valid"]);
    // All invalid IDs should be present
    assert_eq!(disc.invalid_sensor_ids.len(), 4);
    // Should be sorted
    let mut sorted = disc.invalid_sensor_ids.clone();
    sorted.sort();
    assert_eq!(disc.invalid_sensor_ids, sorted);
}

// ═══════════════════════════════════════════════════════════════════════════
// LAYOUT BUILDER CHAINING
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn layout_builder_chaining_works() {
    let layout = FsLayout::new("artifacts", "cockpit.toml")
        .with_max_receipts(10)
        .with_max_receipt_bytes(4096);

    assert_eq!(layout.max_receipts, 10);
    assert_eq!(layout.max_receipt_bytes, 4096);
}

#[test]
fn layout_defaults_are_sane() {
    let layout = FsLayout::new("artifacts", "cockpit.toml");

    assert_eq!(layout.max_receipt_bytes, 2 * 1024 * 1024); // 2MB
    assert_eq!(layout.max_receipts, 100);
}
