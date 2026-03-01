//! Property-based serde roundtrip tests for individual cockpitctl-types.
//!
//! Validates that all core enums and structs survive JSON serialization
//! and deserialization without data loss.

use cockpitctl_types::*;
use proptest::prelude::*;

// ============================================================================
// Strategies
// ============================================================================

fn any_severity() -> impl Strategy<Value = Severity> {
    prop_oneof![
        Just(Severity::Info),
        Just(Severity::Warn),
        Just(Severity::Error),
    ]
}

fn any_verdict_status() -> impl Strategy<Value = VerdictStatus> {
    prop_oneof![
        Just(VerdictStatus::Pass),
        Just(VerdictStatus::Warn),
        Just(VerdictStatus::Fail),
        Just(VerdictStatus::Skip),
    ]
}

fn any_missing_policy() -> impl Strategy<Value = MissingPolicy> {
    prop_oneof![
        Just(MissingPolicy::Skip),
        Just(MissingPolicy::Warn),
        Just(MissingPolicy::Fail),
    ]
}

fn any_presence() -> impl Strategy<Value = Presence> {
    prop_oneof![
        Just(Presence::Present),
        Just(Presence::Missing),
        Just(Presence::Invalid),
    ]
}

fn any_policy_outcome() -> impl Strategy<Value = PolicyOutcome> {
    prop_oneof![
        Just(PolicyOutcome::Blocked),
        Just(PolicyOutcome::Allowed),
        Just(PolicyOutcome::Informational),
    ]
}

fn any_schema_validation() -> impl Strategy<Value = SchemaValidation> {
    prop_oneof![Just(SchemaValidation::Lax), Just(SchemaValidation::Strict),]
}

fn any_verdict_counts() -> impl Strategy<Value = VerdictCounts> {
    (0u64..500, 0u64..500, 0u64..500, 0u64..50).prop_map(|(info, warn, error, suppressed)| {
        VerdictCounts {
            info,
            warn,
            error,
            suppressed,
        }
    })
}

fn any_verdict() -> impl Strategy<Value = Verdict> {
    (
        any_verdict_status(),
        any_verdict_counts(),
        prop::collection::vec("[a-z_]{1,15}", 0..4),
    )
        .prop_map(|(status, counts, reasons)| Verdict {
            status,
            counts,
            reasons,
        })
}

fn any_tool_info() -> impl Strategy<Value = ToolInfo> {
    (
        "[a-z][a-z0-9-]{0,15}",
        "[0-9]{1,2}\\.[0-9]{1,2}\\.[0-9]{1,2}",
        prop::option::of("[a-f0-9]{7}"),
    )
        .prop_map(|(name, version, commit)| ToolInfo {
            name,
            version,
            commit,
        })
}

fn any_location() -> impl Strategy<Value = Location> {
    (
        prop::option::of("[a-z/_.-]{1,30}"),
        prop::option::of(1u32..10000),
        prop::option::of(1u32..500),
    )
        .prop_map(|(path, line, col)| Location { path, line, col })
}

fn any_finding() -> impl Strategy<Value = Finding> {
    (
        any_severity(),
        prop::option::of("[A-Z][A-Z0-9_]{0,10}"),
        "[A-Z][A-Z0-9_]{0,15}",
        ".{1,50}",
        prop::option::of(any_location()),
        prop::option::of(".{0,30}"),
        prop::option::of("https://example\\.com"),
        prop::option::of("[a-f0-9]{64}"),
    )
        .prop_map(
            |(severity, check_id, code, message, location, help, url, fingerprint)| Finding {
                severity,
                check_id,
                code,
                message,
                location,
                help,
                url,
                fingerprint,
                data: None,
            },
        )
}

fn any_highlight() -> impl Strategy<Value = Highlight> {
    ("[a-z_][a-z0-9_]{0,10}", any_finding())
        .prop_map(|(sensor_id, finding)| Highlight { sensor_id, finding })
}

fn any_artifact_pointer() -> impl Strategy<Value = ArtifactPointer> {
    (
        "[a-z][a-z0-9_]{0,10}",
        "[a-z/._]{1,20}",
        Just("application/json".to_string()),
        prop::option::of("[a-z._]{1,10}"),
    )
        .prop_map(|(id, path, mime, schema)| ArtifactPointer {
            id,
            path,
            mime,
            schema,
        })
}

// ============================================================================
// Enum serde roundtrips
// ============================================================================

proptest! {
    #[test]
    fn severity_serde_roundtrip(s in any_severity()) {
        let json = serde_json::to_string(&s).unwrap();
        let parsed: Severity = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(s, parsed);
    }

    #[test]
    fn verdict_status_serde_roundtrip(s in any_verdict_status()) {
        let json = serde_json::to_string(&s).unwrap();
        let parsed: VerdictStatus = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(s, parsed);
    }

    #[test]
    fn missing_policy_serde_roundtrip(p in any_missing_policy()) {
        let json = serde_json::to_string(&p).unwrap();
        let parsed: MissingPolicy = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(p, parsed);
    }

    #[test]
    fn presence_serde_roundtrip(p in any_presence()) {
        let json = serde_json::to_string(&p).unwrap();
        let parsed: Presence = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(p, parsed);
    }

    #[test]
    fn policy_outcome_serde_roundtrip(p in any_policy_outcome()) {
        let json = serde_json::to_string(&p).unwrap();
        let parsed: PolicyOutcome = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(p, parsed);
    }

    #[test]
    fn schema_validation_serde_roundtrip(v in any_schema_validation()) {
        let json = serde_json::to_string(&v).unwrap();
        let parsed: SchemaValidation = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(v, parsed);
    }
}

// ============================================================================
// Struct serde roundtrips
// ============================================================================

proptest! {
    #[test]
    fn verdict_counts_serde_roundtrip(c in any_verdict_counts()) {
        let json = serde_json::to_string(&c).unwrap();
        let parsed: VerdictCounts = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(c, parsed);
    }

    #[test]
    fn verdict_serde_roundtrip(v in any_verdict()) {
        let json = serde_json::to_string(&v).unwrap();
        let parsed: Verdict = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(v, parsed);
    }

    #[test]
    fn tool_info_serde_roundtrip(t in any_tool_info()) {
        let json = serde_json::to_string(&t).unwrap();
        let parsed: ToolInfo = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(t, parsed);
    }

    #[test]
    fn location_serde_roundtrip(l in any_location()) {
        let json = serde_json::to_string(&l).unwrap();
        let parsed: Location = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(l, parsed);
    }

    #[test]
    fn finding_serde_roundtrip(f in any_finding()) {
        let json = serde_json::to_string(&f).unwrap();
        let parsed: Finding = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(f, parsed);
    }

    #[test]
    fn highlight_serde_roundtrip(h in any_highlight()) {
        let json = serde_json::to_string(&h).unwrap();
        let parsed: Highlight = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(h, parsed);
    }

    #[test]
    fn artifact_pointer_serde_roundtrip(a in any_artifact_pointer()) {
        let json = serde_json::to_string(&a).unwrap();
        let parsed: ArtifactPointer = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(a, parsed);
    }
}

// ============================================================================
// Sensor ID validation roundtrip property
// ============================================================================

proptest! {
    /// Valid sensor IDs are accepted by is_valid_sensor_id.
    #[test]
    fn valid_sensor_id_accepted(id in "[a-zA-Z0-9_-]{1,30}") {
        prop_assert!(is_valid_sensor_id(&id), "valid sensor ID {:?} must pass", id);
    }

    /// Sensor IDs with path separators are rejected.
    #[test]
    fn sensor_id_with_slash_rejected(
        prefix in "[a-z]{1,5}",
        suffix in "[a-z]{1,5}",
    ) {
        let bad_id = format!("{}/{}", prefix, suffix);
        prop_assert!(!is_valid_sensor_id(&bad_id));
    }

    /// Sensor IDs with traversal are rejected.
    #[test]
    fn sensor_id_with_traversal_rejected(
        prefix in "[a-z]{0,5}",
        suffix in "[a-z]{0,5}",
    ) {
        let bad_id = format!("{}..{}", prefix, suffix);
        prop_assert!(!is_valid_sensor_id(&bad_id));
    }
}

// ============================================================================
// Serde produces expected JSON shapes for enums
// ============================================================================

proptest! {
    /// Severity serializes to a lowercase snake_case JSON string.
    #[test]
    fn severity_serializes_to_known_string(s in any_severity()) {
        let json = serde_json::to_string(&s).unwrap();
        let expected = match s {
            Severity::Info => "\"info\"",
            Severity::Warn => "\"warn\"",
            Severity::Error => "\"error\"",
        };
        prop_assert_eq!(json, expected);
    }

    /// VerdictStatus serializes to a lowercase snake_case JSON string.
    #[test]
    fn verdict_status_serializes_to_known_string(s in any_verdict_status()) {
        let json = serde_json::to_string(&s).unwrap();
        let expected = match s {
            VerdictStatus::Pass => "\"pass\"",
            VerdictStatus::Warn => "\"warn\"",
            VerdictStatus::Fail => "\"fail\"",
            VerdictStatus::Skip => "\"skip\"",
        };
        prop_assert_eq!(json, expected);
    }
}

// ============================================================================
// Serialization stability: serialize twice = identical bytes
// ============================================================================

proptest! {
    /// Double serialization produces identical JSON bytes.
    #[test]
    fn finding_serialization_stable(f in any_finding()) {
        let a = serde_json::to_string(&f).unwrap();
        let b = serde_json::to_string(&f).unwrap();
        prop_assert_eq!(a, b);
    }

    /// Verdict serialization is byte-stable.
    #[test]
    fn verdict_serialization_stable(v in any_verdict()) {
        let a = serde_json::to_string(&v).unwrap();
        let b = serde_json::to_string(&v).unwrap();
        prop_assert_eq!(a, b);
    }

    /// VerdictCounts with suppressed=0 omits the field via skip_serializing_if.
    #[test]
    fn verdict_counts_zero_suppressed_omitted(info in 0u64..10, warn in 0u64..10, error in 0u64..10) {
        let counts = VerdictCounts { info, warn, error, suppressed: 0 };
        let json = serde_json::to_string(&counts).unwrap();
        prop_assert!(!json.contains("suppressed"), "suppressed=0 should be omitted");

        // Roundtrip still works (default fills suppressed=0).
        let parsed: VerdictCounts = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(counts, parsed);
    }
}
