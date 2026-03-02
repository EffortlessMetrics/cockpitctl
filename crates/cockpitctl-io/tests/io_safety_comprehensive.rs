//! Comprehensive safety and boundary tests for cockpitctl-io adapters.
//!
//! Covers path traversal rejection, symlink handling, file size limits,
//! lexical ordering, FsLayout customisation, FsOutputSink safety, and
//! controlled error handling for malformed inputs.

use cockpitctl_ingest::{CommentRead, OutputSink, PlanRead, ReceiptSource, ReportRead};
use cockpitctl_io::{FsLayout, FsOutputSink, FsReceiptSource};
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn setup_artifacts(tmp: &TempDir) -> PathBuf {
    let artifacts = tmp.path().join("artifacts");
    fs::create_dir_all(&artifacts).unwrap();
    artifacts
}

fn add_sensor(artifacts: &std::path::Path, name: &str) {
    let d = artifacts.join(name);
    fs::create_dir_all(&d).unwrap();
    fs::write(d.join("report.json"), r#"{"ok":true}"#).unwrap();
}

fn make_source(artifacts: &std::path::Path, tmp: &TempDir) -> FsReceiptSource {
    let layout = FsLayout::new(artifacts, tmp.path().join("cockpit.toml"));
    FsReceiptSource::new(layout)
}

// ---------------------------------------------------------------------------
// 1. Path traversal rejection
// ---------------------------------------------------------------------------

#[test]
fn traversal_bare_dotdot_rejected() {
    let tmp = TempDir::new().unwrap();
    let artifacts = setup_artifacts(&tmp);
    let src = make_source(&artifacts, &tmp);
    match src.read_report_bytes("..").unwrap() {
        ReportRead::UnsafePath => {}
        other => panic!(
            "expected UnsafePath, got {:?}",
            std::mem::discriminant(&other)
        ),
    }
}

#[test]
fn traversal_parent_prefix_rejected() {
    let tmp = TempDir::new().unwrap();
    let artifacts = setup_artifacts(&tmp);
    let src = make_source(&artifacts, &tmp);
    match src.read_report_bytes("../etc/passwd").unwrap() {
        ReportRead::UnsafePath => {}
        other => panic!(
            "expected UnsafePath, got {:?}",
            std::mem::discriminant(&other)
        ),
    }
}

#[test]
fn traversal_forward_slash_rejected() {
    let tmp = TempDir::new().unwrap();
    let artifacts = setup_artifacts(&tmp);
    let src = make_source(&artifacts, &tmp);
    match src.read_report_bytes("a/b").unwrap() {
        ReportRead::UnsafePath => {}
        other => panic!(
            "expected UnsafePath, got {:?}",
            std::mem::discriminant(&other)
        ),
    }
}

#[test]
fn traversal_backslash_rejected() {
    let tmp = TempDir::new().unwrap();
    let artifacts = setup_artifacts(&tmp);
    let src = make_source(&artifacts, &tmp);
    match src.read_report_bytes(r"a\b").unwrap() {
        ReportRead::UnsafePath => {}
        other => panic!(
            "expected UnsafePath, got {:?}",
            std::mem::discriminant(&other)
        ),
    }
}

#[test]
fn traversal_intermediate_dotdot_rejected() {
    let tmp = TempDir::new().unwrap();
    let artifacts = setup_artifacts(&tmp);
    let src = make_source(&artifacts, &tmp);
    match src.read_report_bytes("foo/../bar").unwrap() {
        ReportRead::UnsafePath => {}
        other => panic!(
            "expected UnsafePath, got {:?}",
            std::mem::discriminant(&other)
        ),
    }
}

#[test]
fn traversal_empty_sensor_id_rejected() {
    let tmp = TempDir::new().unwrap();
    let artifacts = setup_artifacts(&tmp);
    let src = make_source(&artifacts, &tmp);
    match src.read_report_bytes("").unwrap() {
        ReportRead::UnsafePath => {}
        other => panic!(
            "expected UnsafePath, got {:?}",
            std::mem::discriminant(&other)
        ),
    }
}

#[test]
fn traversal_unicode_tricks_rejected() {
    let tmp = TempDir::new().unwrap();
    let artifacts = setup_artifacts(&tmp);
    let src = make_source(&artifacts, &tmp);
    // Fullwidth dot + dot => not ASCII alphanumeric
    match src.read_report_bytes("\u{FF0E}\u{FF0E}").unwrap() {
        ReportRead::UnsafePath => {}
        other => panic!(
            "expected UnsafePath, got {:?}",
            std::mem::discriminant(&other)
        ),
    }
}

#[test]
fn traversal_null_byte_rejected() {
    let tmp = TempDir::new().unwrap();
    let artifacts = setup_artifacts(&tmp);
    let src = make_source(&artifacts, &tmp);
    match src.read_report_bytes("sensor\0evil").unwrap() {
        ReportRead::UnsafePath => {}
        other => panic!(
            "expected UnsafePath, got {:?}",
            std::mem::discriminant(&other)
        ),
    }
}

#[test]
fn traversal_bare_dot_rejected() {
    let tmp = TempDir::new().unwrap();
    let artifacts = setup_artifacts(&tmp);
    let src = make_source(&artifacts, &tmp);
    // Single dot is not a valid sensor ID (contains non-alphanumeric/hyphen/underscore)
    match src.read_report_bytes(".").unwrap() {
        ReportRead::UnsafePath => {}
        other => panic!(
            "expected UnsafePath, got {:?}",
            std::mem::discriminant(&other)
        ),
    }
}

// ---------------------------------------------------------------------------
// 2. Comment / Plan traversal rejection
// ---------------------------------------------------------------------------

#[test]
fn comment_read_rejects_traversal_sensor_ids() {
    let tmp = TempDir::new().unwrap();
    let artifacts = setup_artifacts(&tmp);
    let src = make_source(&artifacts, &tmp);
    for bad in &["..", "../etc", "a/b", r"a\b", "", ".", "sensor\0x"] {
        match src.comment_path_if_present(bad).unwrap() {
            CommentRead::UnsafePath | CommentRead::Missing => {}
            other => panic!(
                "expected UnsafePath/Missing for {:?}, got {:?}",
                bad,
                std::mem::discriminant(&other)
            ),
        }
    }
}

#[test]
fn plan_read_rejects_traversal_sensor_ids() {
    let tmp = TempDir::new().unwrap();
    let artifacts = setup_artifacts(&tmp);
    let src = make_source(&artifacts, &tmp);
    for bad in &["..", "../etc", "a/b", r"a\b", ""] {
        match src.read_plan_bytes(bad).unwrap() {
            PlanRead::Missing => {}
            other => panic!(
                "expected Missing for {:?}, got {:?}",
                bad,
                std::mem::discriminant(&other)
            ),
        }
    }
}

// ---------------------------------------------------------------------------
// 3. File size limits
// ---------------------------------------------------------------------------

#[test]
fn size_cap_custom_override_exact_boundary() {
    let tmp = TempDir::new().unwrap();
    let artifacts = setup_artifacts(&tmp);
    let cap = 128;
    add_sensor(&artifacts, "sensor1");

    // Exactly at cap: accepted (uses > not >=)
    let exact = vec![b'x'; cap];
    fs::write(artifacts.join("sensor1").join("report.json"), &exact).unwrap();
    let layout = FsLayout::new(&artifacts, tmp.path().join("c.toml")).with_max_receipt_bytes(cap);
    let src = FsReceiptSource::new(layout);
    match src.read_report_bytes("sensor1").unwrap() {
        ReportRead::Bytes(b) => assert_eq!(b.len(), cap),
        other => panic!("expected Bytes, got {:?}", std::mem::discriminant(&other)),
    }

    // One byte over: rejected
    let over = vec![b'x'; cap + 1];
    fs::write(artifacts.join("sensor1").join("report.json"), &over).unwrap();
    let layout2 = FsLayout::new(&artifacts, tmp.path().join("c.toml")).with_max_receipt_bytes(cap);
    let src2 = FsReceiptSource::new(layout2);
    match src2.read_report_bytes("sensor1").unwrap() {
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
fn size_cap_one_byte_cap() {
    let tmp = TempDir::new().unwrap();
    let artifacts = setup_artifacts(&tmp);
    add_sensor(&artifacts, "s1");

    // 1-byte cap: single byte accepted
    fs::write(artifacts.join("s1").join("report.json"), b"x").unwrap();
    let layout = FsLayout::new(&artifacts, tmp.path().join("c.toml")).with_max_receipt_bytes(1);
    let src = FsReceiptSource::new(layout);
    match src.read_report_bytes("s1").unwrap() {
        ReportRead::Bytes(b) => assert_eq!(b.len(), 1),
        other => panic!("expected Bytes, got {:?}", std::mem::discriminant(&other)),
    }

    // 2 bytes with 1-byte cap: rejected
    fs::write(artifacts.join("s1").join("report.json"), b"xy").unwrap();
    let layout2 = FsLayout::new(&artifacts, tmp.path().join("c.toml")).with_max_receipt_bytes(1);
    let src2 = FsReceiptSource::new(layout2);
    match src2.read_report_bytes("s1").unwrap() {
        ReportRead::Oversized { size, cap } => {
            assert_eq!(size, 2);
            assert_eq!(cap, 1);
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
    let artifacts = setup_artifacts(&tmp);
    add_sensor(&artifacts, "empty-sensor");

    fs::write(artifacts.join("empty-sensor").join("report.json"), b"").unwrap();
    let src = make_source(&artifacts, &tmp);
    match src.read_report_bytes("empty-sensor").unwrap() {
        ReportRead::Bytes(b) => assert!(b.is_empty()),
        other => panic!("expected Bytes, got {:?}", std::mem::discriminant(&other)),
    }
}

#[test]
fn plan_read_oversized_rejected() {
    let tmp = TempDir::new().unwrap();
    let artifacts = setup_artifacts(&tmp);
    let cap = 64;

    let d = artifacts.join("sensor1");
    fs::create_dir_all(&d).unwrap();
    fs::write(d.join("report.json"), "{}").unwrap();
    let over = vec![b'p'; cap + 1];
    fs::write(d.join("plan.json"), &over).unwrap();

    let layout = FsLayout::new(&artifacts, tmp.path().join("c.toml")).with_max_receipt_bytes(cap);
    let src = FsReceiptSource::new(layout);
    match src.read_plan_bytes("sensor1").unwrap() {
        PlanRead::Oversized { size, cap: c } => {
            assert_eq!(size as usize, cap + 1);
            assert_eq!(c, cap);
        }
        other => panic!(
            "expected Oversized, got {:?}",
            std::mem::discriminant(&other)
        ),
    }
}

// ---------------------------------------------------------------------------
// 4. FsLayout defaults and builder
// ---------------------------------------------------------------------------

#[test]
fn layout_defaults_are_sane() {
    let layout = FsLayout::new("/tmp/arts", "/tmp/cockpit.toml");
    assert_eq!(layout.artifacts_dir, PathBuf::from("/tmp/arts"));
    assert_eq!(layout.out_dir, PathBuf::from("/tmp/arts/cockpit"));
    assert_eq!(layout.config_path, PathBuf::from("/tmp/cockpit.toml"));
    assert_eq!(layout.max_receipt_bytes, 2 * 1024 * 1024);
    assert_eq!(layout.max_receipts, cockpitctl_io::DEFAULT_MAX_RECEIPTS);
}

#[test]
fn layout_builder_chaining_works() {
    let layout = FsLayout::new("/a", "/b")
        .with_max_receipt_bytes(999)
        .with_max_receipts(5);
    assert_eq!(layout.max_receipt_bytes, 999);
    assert_eq!(layout.max_receipts, 5);
}

// ---------------------------------------------------------------------------
// 5. FsOutputSink safety
// ---------------------------------------------------------------------------

#[test]
fn extra_file_valid_name_succeeds() {
    let tmp = TempDir::new().unwrap();
    let layout = FsLayout::new(tmp.path().join("artifacts"), tmp.path().join("c.toml"));
    let sink = FsOutputSink::new(layout);

    sink.write_extra_file("sarif.json", b"{}").unwrap();
    let written = fs::read(
        tmp.path()
            .join("artifacts")
            .join("cockpit")
            .join("sarif.json"),
    )
    .unwrap();
    assert_eq!(written, b"{}");
}

#[test]
fn extra_file_dotdot_escape_rejected() {
    let tmp = TempDir::new().unwrap();
    let layout = FsLayout::new(tmp.path().join("artifacts"), tmp.path().join("c.toml"));
    let sink = FsOutputSink::new(layout);
    assert!(sink.write_extra_file("../escape.txt", b"x").is_err());
}

#[test]
fn extra_file_dotdot_only_rejected() {
    let tmp = TempDir::new().unwrap();
    let layout = FsLayout::new(tmp.path().join("artifacts"), tmp.path().join("c.toml"));
    let sink = FsOutputSink::new(layout);
    assert!(sink.write_extra_file("..", b"x").is_err());
}

#[test]
fn extra_file_embedded_dotdot_rejected() {
    let tmp = TempDir::new().unwrap();
    let layout = FsLayout::new(tmp.path().join("artifacts"), tmp.path().join("c.toml"));
    let sink = FsOutputSink::new(layout);
    assert!(sink.write_extra_file("foo..bar", b"x").is_err());
}

#[test]
fn extra_file_forward_slash_rejected() {
    let tmp = TempDir::new().unwrap();
    let layout = FsLayout::new(tmp.path().join("artifacts"), tmp.path().join("c.toml"));
    let sink = FsOutputSink::new(layout);
    assert!(sink.write_extra_file("sub/file.txt", b"x").is_err());
}

#[test]
fn extra_file_backslash_rejected() {
    let tmp = TempDir::new().unwrap();
    let layout = FsLayout::new(tmp.path().join("artifacts"), tmp.path().join("c.toml"));
    let sink = FsOutputSink::new(layout);
    assert!(sink.write_extra_file(r"sub\file.txt", b"x").is_err());
}

// ---------------------------------------------------------------------------
// 6. Discovery ordering and filtering
// ---------------------------------------------------------------------------

#[test]
fn non_directory_entries_skipped_in_discovery() {
    let tmp = TempDir::new().unwrap();
    let artifacts = setup_artifacts(&tmp);
    // File at top level (not a directory)
    fs::write(artifacts.join("stray-file.json"), "{}").unwrap();
    add_sensor(&artifacts, "real-sensor");

    let src = make_source(&artifacts, &tmp);
    let disc = src.discovered_sensors().unwrap();
    assert_eq!(disc.sensors, vec!["real-sensor"]);
}

#[test]
fn hidden_directories_filtered_from_discovery() {
    let tmp = TempDir::new().unwrap();
    let artifacts = setup_artifacts(&tmp);
    // Hidden dirs are filtered by is_valid_sensor_id (dot not allowed)
    let hidden = artifacts.join(".hidden");
    fs::create_dir_all(&hidden).unwrap();
    fs::write(hidden.join("report.json"), "{}").unwrap();

    add_sensor(&artifacts, "visible");
    let src = make_source(&artifacts, &tmp);
    let disc = src.discovered_sensors().unwrap();
    assert_eq!(disc.sensors, vec!["visible"]);
    // .hidden should appear in invalid_sensor_ids
    assert!(disc.invalid_sensor_ids.contains(&".hidden".to_string()));
}

#[test]
fn sensor_dir_without_report_json_not_discovered() {
    let tmp = TempDir::new().unwrap();
    let artifacts = setup_artifacts(&tmp);
    fs::create_dir_all(artifacts.join("no-report")).unwrap();
    add_sensor(&artifacts, "has-report");

    let src = make_source(&artifacts, &tmp);
    let disc = src.discovered_sensors().unwrap();
    assert_eq!(disc.sensors, vec!["has-report"]);
}

#[test]
fn cockpit_reserved_dir_always_excluded() {
    let tmp = TempDir::new().unwrap();
    let artifacts = setup_artifacts(&tmp);
    // Create cockpit dir with report.json
    let cockpit = artifacts.join("cockpit");
    fs::create_dir_all(&cockpit).unwrap();
    fs::write(cockpit.join("report.json"), "{}").unwrap();
    add_sensor(&artifacts, "alpha");

    let src = make_source(&artifacts, &tmp);
    let disc = src.discovered_sensors().unwrap();
    assert_eq!(disc.sensors, vec!["alpha"]);
    assert!(!disc.invalid_sensor_ids.contains(&"cockpit".to_string()));
}

// ---------------------------------------------------------------------------
// 7. Max receipts truncation
// ---------------------------------------------------------------------------

#[test]
fn max_receipts_one_allows_exactly_one_sensor() {
    let tmp = TempDir::new().unwrap();
    let artifacts = setup_artifacts(&tmp);
    add_sensor(&artifacts, "aaa");
    add_sensor(&artifacts, "bbb");
    add_sensor(&artifacts, "ccc");

    let layout = FsLayout::new(&artifacts, tmp.path().join("c.toml")).with_max_receipts(1);
    let src = FsReceiptSource::new(layout);
    let disc = src.discovered_sensors().unwrap();
    assert_eq!(disc.sensors.len(), 1);
    assert_eq!(disc.sensors[0], "aaa"); // lexically first
    assert!(disc.truncated);
    assert_eq!(disc.total_found, 3);
}

#[test]
fn exactly_at_max_receipts_not_truncated() {
    let tmp = TempDir::new().unwrap();
    let artifacts = setup_artifacts(&tmp);
    add_sensor(&artifacts, "s1");
    add_sensor(&artifacts, "s2");

    let layout = FsLayout::new(&artifacts, tmp.path().join("c.toml")).with_max_receipts(2);
    let src = FsReceiptSource::new(layout);
    let disc = src.discovered_sensors().unwrap();
    assert_eq!(disc.sensors.len(), 2);
    assert!(!disc.truncated);
}

#[test]
fn one_over_max_receipts_truncated() {
    let tmp = TempDir::new().unwrap();
    let artifacts = setup_artifacts(&tmp);
    add_sensor(&artifacts, "s1");
    add_sensor(&artifacts, "s2");
    add_sensor(&artifacts, "s3");

    let layout = FsLayout::new(&artifacts, tmp.path().join("c.toml")).with_max_receipts(2);
    let src = FsReceiptSource::new(layout);
    let disc = src.discovered_sensors().unwrap();
    assert_eq!(disc.sensors.len(), 2);
    assert!(disc.truncated);
    assert_eq!(disc.total_found, 3);
}

#[test]
fn zero_sensors_with_cap_returns_empty() {
    let tmp = TempDir::new().unwrap();
    let artifacts = setup_artifacts(&tmp);

    let layout = FsLayout::new(&artifacts, tmp.path().join("c.toml")).with_max_receipts(0);
    let src = FsReceiptSource::new(layout);
    let disc = src.discovered_sensors().unwrap();
    assert!(disc.sensors.is_empty());
}

// ---------------------------------------------------------------------------
// 8. Invalid sensor IDs collected and sorted
// ---------------------------------------------------------------------------

#[test]
fn invalid_sensor_ids_collected_and_sorted() {
    let tmp = TempDir::new().unwrap();
    let artifacts = setup_artifacts(&tmp);

    // Create dirs with invalid names
    for name in &["bad name", "colon:id", ".dotstart"] {
        let d = artifacts.join(name);
        fs::create_dir_all(&d).unwrap();
        fs::write(d.join("report.json"), "{}").unwrap();
    }
    add_sensor(&artifacts, "valid");

    let src = make_source(&artifacts, &tmp);
    let disc = src.discovered_sensors().unwrap();
    assert_eq!(disc.sensors, vec!["valid"]);
    assert_eq!(disc.invalid_sensor_ids.len(), 3);
    // Should be sorted
    let sorted = disc.invalid_sensor_ids.windows(2).all(|w| w[0] <= w[1]);
    assert!(sorted, "invalid_sensor_ids not sorted");
}

#[test]
fn valid_sensor_ids_accepted() {
    let tmp = TempDir::new().unwrap();
    let artifacts = setup_artifacts(&tmp);

    for name in &[
        "alpha",
        "beta-1",
        "gamma_2",
        "UPPER",
        "MiXeD-CaSe_123",
        "a",
        "1",
    ] {
        add_sensor(&artifacts, name);
    }

    let src = make_source(&artifacts, &tmp);
    let disc = src.discovered_sensors().unwrap();
    assert_eq!(disc.sensors.len(), 7);
    assert!(disc.invalid_sensor_ids.is_empty());
}

// ---------------------------------------------------------------------------
// 9. Comment read present/missing
// ---------------------------------------------------------------------------

#[test]
fn comment_read_returns_present_for_sensor_with_comment() {
    let tmp = TempDir::new().unwrap();
    let artifacts = setup_artifacts(&tmp);
    add_sensor(&artifacts, "sensor1");
    fs::write(
        artifacts.join("sensor1").join("comment.md"),
        "# Comment",
    )
    .unwrap();

    let src = make_source(&artifacts, &tmp);
    match src.comment_path_if_present("sensor1").unwrap() {
        CommentRead::Present(path) => {
            assert!(path.contains("sensor1"));
            assert!(path.contains("comment.md"));
        }
        other => panic!("expected Present, got {:?}", std::mem::discriminant(&other)),
    }
}

#[test]
fn comment_read_returns_missing_for_valid_sensor_without_comment() {
    let tmp = TempDir::new().unwrap();
    let artifacts = setup_artifacts(&tmp);
    add_sensor(&artifacts, "sensor1");
    // No comment.md written

    let src = make_source(&artifacts, &tmp);
    match src.comment_path_if_present("sensor1").unwrap() {
        CommentRead::Missing => {}
        other => panic!(
            "expected Missing, got {:?}",
            std::mem::discriminant(&other)
        ),
    }
}
