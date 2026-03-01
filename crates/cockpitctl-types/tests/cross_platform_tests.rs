//! Cross-platform serialization tests for cockpitctl-types.
//!
//! Verifies that Finding/Location structs with platform-specific path formats
//! serialize and deserialize correctly, preserving the original path format
//! byte-for-byte through JSON roundtrips.

use cockpitctl_types::*;

fn make_finding(path: &str) -> Finding {
    Finding {
        severity: Severity::Error,
        check_id: Some("test.rule".to_string()),
        code: "T001".to_string(),
        message: "test finding".to_string(),
        location: Some(Location {
            path: Some(path.to_string()),
            line: Some(42),
            col: Some(10),
        }),
        help: None,
        url: None,
        fingerprint: None,
        data: None,
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 1. FINDING WITH WINDOWS PATH — SERIALIZES CORRECTLY
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn finding_with_windows_path_serializes() {
    let finding = make_finding("src\\lib.rs");
    let json = serde_json::to_string(&finding).unwrap();

    // Backslash must be escaped in JSON as \\
    assert!(
        json.contains("src\\\\lib.rs"),
        "Windows path backslash must be escaped in JSON: {}",
        json
    );

    // Deserialize back and verify path is preserved
    let parsed: Finding = serde_json::from_str(&json).unwrap();
    assert_eq!(
        parsed.location.as_ref().unwrap().path.as_deref(),
        Some("src\\lib.rs")
    );
}

#[test]
fn finding_with_windows_drive_path_serializes() {
    let finding = make_finding("C:\\Users\\dev\\project\\file.rs");
    let json = serde_json::to_string(&finding).unwrap();

    let parsed: Finding = serde_json::from_str(&json).unwrap();
    assert_eq!(
        parsed.location.as_ref().unwrap().path.as_deref(),
        Some("C:\\Users\\dev\\project\\file.rs")
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// 2. FINDING WITH UNIX PATH — SERIALIZES CORRECTLY
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn finding_with_unix_path_serializes() {
    let finding = make_finding("src/lib.rs");
    let json = serde_json::to_string(&finding).unwrap();

    assert!(
        json.contains("src/lib.rs"),
        "Unix path must appear verbatim in JSON: {}",
        json
    );

    let parsed: Finding = serde_json::from_str(&json).unwrap();
    assert_eq!(
        parsed.location.as_ref().unwrap().path.as_deref(),
        Some("src/lib.rs")
    );
}

#[test]
fn finding_with_absolute_unix_path_serializes() {
    let finding = make_finding("/home/user/project/src/main.rs");
    let json = serde_json::to_string(&finding).unwrap();

    let parsed: Finding = serde_json::from_str(&json).unwrap();
    assert_eq!(
        parsed.location.as_ref().unwrap().path.as_deref(),
        Some("/home/user/project/src/main.rs")
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// 3. ROUNDTRIP PRESERVES ORIGINAL PATH FORMAT
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn roundtrip_preserves_path_format() {
    let paths = [
        "src/lib.rs",
        "src\\lib.rs",
        "C:\\Users\\dev\\file.rs",
        "/home/user/file.rs",
        "crates/io/src/lib.rs",
        "crates\\io\\src\\lib.rs",
        "mixed/path\\style",
    ];

    for original_path in &paths {
        let finding = make_finding(original_path);
        let json = serde_json::to_string(&finding).unwrap();
        let parsed: Finding = serde_json::from_str(&json).unwrap();

        assert_eq!(
            parsed.location.as_ref().unwrap().path.as_deref(),
            Some(*original_path),
            "roundtrip must preserve path {:?} byte-for-byte",
            original_path
        );
    }
}

#[test]
fn roundtrip_preserves_none_path() {
    let finding = Finding {
        severity: Severity::Warn,
        check_id: None,
        code: "W001".to_string(),
        message: "no location".to_string(),
        location: Some(Location {
            path: None,
            line: None,
            col: None,
        }),
        help: None,
        url: None,
        fingerprint: None,
        data: None,
    };

    let json = serde_json::to_string(&finding).unwrap();
    let parsed: Finding = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.location.as_ref().unwrap().path, None);
}

// ═══════════════════════════════════════════════════════════════════════════
// SENSOR ID VALIDATION — PLATFORM-INDEPENDENT (types-level)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn is_valid_sensor_id_rejects_separators_on_all_platforms() {
    // Forward slash
    assert!(!is_valid_sensor_id("foo/bar"));
    assert!(!is_valid_sensor_id("/leading"));
    assert!(!is_valid_sensor_id("trailing/"));

    // Backslash
    assert!(!is_valid_sensor_id("foo\\bar"));
    assert!(!is_valid_sensor_id("\\leading"));
    assert!(!is_valid_sensor_id("trailing\\"));

    // Dot-dot
    assert!(!is_valid_sensor_id(".."));
    assert!(!is_valid_sensor_id("foo..bar"));

    // Mixed
    assert!(!is_valid_sensor_id("a/b\\c"));
}

#[test]
fn is_valid_sensor_id_accepts_valid_ids_on_all_platforms() {
    let valid = ["a", "sensor", "my-sensor", "sensor_v2", "ABC", "a1-B2_c3"];
    for id in &valid {
        assert!(
            is_valid_sensor_id(id),
            "{:?} should be valid on all platforms",
            id
        );
    }
}
