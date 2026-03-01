//! Property-based serde roundtrip tests for SARIF output types.
//!
//! Validates that all SARIF DTOs survive JSON serialization and
//! deserialization without data loss.

use cockpitctl_sarif::*;
use proptest::prelude::*;

// ============================================================================
// Strategies
// ============================================================================

fn any_sarif_message() -> impl Strategy<Value = SarifMessage> {
    "[a-zA-Z0-9 .,'!?()-]{1,60}".prop_map(|text| SarifMessage { text })
}

fn any_sarif_rule() -> impl Strategy<Value = SarifRule> {
    (
        "[a-zA-Z][a-zA-Z0-9_/-]{0,20}",
        prop::option::of(any_sarif_message()),
    )
        .prop_map(|(id, short_description)| SarifRule {
            id,
            short_description,
        })
}

fn any_sarif_region() -> impl Strategy<Value = SarifRegion> {
    (prop::option::of(1u32..10_000), prop::option::of(1u32..500)).prop_map(
        |(start_line, start_column)| SarifRegion {
            start_line,
            start_column,
        },
    )
}

fn any_sarif_artifact_location() -> impl Strategy<Value = SarifArtifactLocation> {
    "[a-z0-9/_.-]{1,30}".prop_map(|uri| SarifArtifactLocation { uri })
}

fn any_sarif_physical_location() -> impl Strategy<Value = SarifPhysicalLocation> {
    (
        any_sarif_artifact_location(),
        prop::option::of(any_sarif_region()),
    )
        .prop_map(|(artifact_location, region)| SarifPhysicalLocation {
            artifact_location,
            region,
        })
}

fn any_sarif_location() -> impl Strategy<Value = SarifLocation> {
    any_sarif_physical_location().prop_map(|physical_location| SarifLocation { physical_location })
}

fn any_sarif_result() -> impl Strategy<Value = SarifResult> {
    (
        "[a-zA-Z][a-zA-Z0-9_/-]{0,20}",
        prop_oneof![Just("error"), Just("warning"), Just("note"),],
        any_sarif_message(),
        prop::collection::vec(any_sarif_location(), 0..3),
        prop::collection::btree_map("[a-z/_]{1,10}", "[a-f0-9]{8,32}", 0..3),
    )
        .prop_map(
            |(rule_id, level, message, locations, fingerprints)| SarifResult {
                rule_id,
                level: level.to_string(),
                message,
                locations,
                fingerprints,
            },
        )
}

fn any_sarif_tool_component() -> impl Strategy<Value = SarifToolComponent> {
    (
        "[a-z][a-z0-9-]{0,15}",
        "[0-9]{1,2}\\.[0-9]{1,2}\\.[0-9]{1,2}",
        prop::collection::vec(any_sarif_rule(), 0..4),
    )
        .prop_map(|(name, version, rules)| SarifToolComponent {
            name,
            version,
            rules,
        })
}

fn any_sarif_tool() -> impl Strategy<Value = SarifTool> {
    any_sarif_tool_component().prop_map(|driver| SarifTool { driver })
}

fn any_sarif_run() -> impl Strategy<Value = SarifRun> {
    (
        any_sarif_tool(),
        prop::collection::vec(any_sarif_result(), 0..5),
    )
        .prop_map(|(tool, results)| SarifRun { tool, results })
}

fn any_sarif_log() -> impl Strategy<Value = SarifLog> {
    prop::collection::vec(any_sarif_run(), 1..3).prop_map(|runs| SarifLog {
        schema: "https://raw.githubusercontent.com/oasis-tcs/sarif-spec/main/sarif-2.1/schema/sarif-schema-2.1.0.json".to_string(),
        version: "2.1.0".to_string(),
        runs,
    })
}

// ============================================================================
// SARIF leaf type roundtrips
// ============================================================================

proptest! {
    #[test]
    fn sarif_message_serde_roundtrip(m in any_sarif_message()) {
        let json = serde_json::to_string(&m).unwrap();
        let parsed: SarifMessage = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(m, parsed);
    }

    #[test]
    fn sarif_rule_serde_roundtrip(r in any_sarif_rule()) {
        let json = serde_json::to_string(&r).unwrap();
        let parsed: SarifRule = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(r, parsed);
    }

    #[test]
    fn sarif_region_serde_roundtrip(r in any_sarif_region()) {
        let json = serde_json::to_string(&r).unwrap();
        let parsed: SarifRegion = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(r, parsed);
    }

    #[test]
    fn sarif_artifact_location_serde_roundtrip(a in any_sarif_artifact_location()) {
        let json = serde_json::to_string(&a).unwrap();
        let parsed: SarifArtifactLocation = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(a, parsed);
    }

    #[test]
    fn sarif_physical_location_serde_roundtrip(p in any_sarif_physical_location()) {
        let json = serde_json::to_string(&p).unwrap();
        let parsed: SarifPhysicalLocation = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(p, parsed);
    }
}

// ============================================================================
// SARIF compound type roundtrips
// ============================================================================

proptest! {
    #[test]
    fn sarif_location_serde_roundtrip(l in any_sarif_location()) {
        let json = serde_json::to_string(&l).unwrap();
        let parsed: SarifLocation = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(l, parsed);
    }

    #[test]
    fn sarif_result_serde_roundtrip(r in any_sarif_result()) {
        let json = serde_json::to_string(&r).unwrap();
        let parsed: SarifResult = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(r, parsed);
    }

    #[test]
    fn sarif_tool_component_serde_roundtrip(c in any_sarif_tool_component()) {
        let json = serde_json::to_string(&c).unwrap();
        let parsed: SarifToolComponent = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(c, parsed);
    }

    #[test]
    fn sarif_tool_serde_roundtrip(t in any_sarif_tool()) {
        let json = serde_json::to_string(&t).unwrap();
        let parsed: SarifTool = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(t, parsed);
    }

    #[test]
    fn sarif_run_serde_roundtrip(r in any_sarif_run()) {
        let json = serde_json::to_string(&r).unwrap();
        let parsed: SarifRun = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(r, parsed);
    }

    #[test]
    fn sarif_log_serde_roundtrip(log in any_sarif_log()) {
        let json = serde_json::to_string(&log).unwrap();
        let parsed: SarifLog = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(log, parsed);
    }
}

// ============================================================================
// String roundtrips (serialize → string → parse → equals)
// ============================================================================

proptest! {
    #[test]
    fn sarif_log_string_roundtrip(log in any_sarif_log()) {
        let json = serde_json::to_string(&log).unwrap();
        let reparsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        let json2 = serde_json::to_string(&reparsed).unwrap();
        let final_parsed: SarifLog = serde_json::from_str(&json2).unwrap();
        prop_assert_eq!(log, final_parsed);
    }

    #[test]
    fn sarif_result_string_roundtrip(r in any_sarif_result()) {
        let json = serde_json::to_string(&r).unwrap();
        let reparsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        let json2 = serde_json::to_string(&reparsed).unwrap();
        let final_parsed: SarifResult = serde_json::from_str(&json2).unwrap();
        prop_assert_eq!(r, final_parsed);
    }

    #[test]
    fn sarif_tool_component_string_roundtrip(c in any_sarif_tool_component()) {
        let json = serde_json::to_string(&c).unwrap();
        let reparsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        let json2 = serde_json::to_string(&reparsed).unwrap();
        let final_parsed: SarifToolComponent = serde_json::from_str(&json2).unwrap();
        prop_assert_eq!(c, final_parsed);
    }
}
