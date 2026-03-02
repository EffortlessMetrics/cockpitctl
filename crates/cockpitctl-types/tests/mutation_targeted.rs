//! Mutation-targeted tests for cockpitctl-types.
//!
//! Each test catches a specific mutant that survived previous cargo-mutants analysis.

use cockpitctl_types::*;

// ===========================================================================
// severity_rank — all 3 variants must return distinct values with correct ordering
// ===========================================================================

#[test]
fn severity_rank_error_is_zero() {
    assert_eq!(severity_rank(&Severity::Error), 0);
}

#[test]
fn severity_rank_warn_is_one() {
    assert_eq!(severity_rank(&Severity::Warn), 1);
}

#[test]
fn severity_rank_info_is_two() {
    assert_eq!(severity_rank(&Severity::Info), 2);
}

#[test]
fn severity_rank_ordering_error_lt_warn_lt_info() {
    assert!(severity_rank(&Severity::Error) < severity_rank(&Severity::Warn));
    assert!(severity_rank(&Severity::Warn) < severity_rank(&Severity::Info));
    assert!(severity_rank(&Severity::Error) < severity_rank(&Severity::Info));
}

#[test]
fn severity_rank_all_distinct() {
    let ranks = [
        severity_rank(&Severity::Error),
        severity_rank(&Severity::Warn),
        severity_rank(&Severity::Info),
    ];
    assert_ne!(ranks[0], ranks[1]);
    assert_ne!(ranks[1], ranks[2]);
    assert_ne!(ranks[0], ranks[2]);
}

// ===========================================================================
// verdict_status_rank — all 4 variants must return distinct values
// ===========================================================================

#[test]
fn verdict_status_rank_fail_is_zero() {
    assert_eq!(verdict_status_rank(&VerdictStatus::Fail), 0);
}

#[test]
fn verdict_status_rank_warn_is_one() {
    assert_eq!(verdict_status_rank(&VerdictStatus::Warn), 1);
}

#[test]
fn verdict_status_rank_pass_is_two() {
    assert_eq!(verdict_status_rank(&VerdictStatus::Pass), 2);
}

#[test]
fn verdict_status_rank_skip_is_three() {
    assert_eq!(verdict_status_rank(&VerdictStatus::Skip), 3);
}

#[test]
fn verdict_status_rank_ordering() {
    assert!(verdict_status_rank(&VerdictStatus::Fail) < verdict_status_rank(&VerdictStatus::Warn));
    assert!(verdict_status_rank(&VerdictStatus::Warn) < verdict_status_rank(&VerdictStatus::Pass));
    assert!(verdict_status_rank(&VerdictStatus::Pass) < verdict_status_rank(&VerdictStatus::Skip));
}

#[test]
fn verdict_status_rank_all_distinct() {
    let ranks = [
        verdict_status_rank(&VerdictStatus::Fail),
        verdict_status_rank(&VerdictStatus::Warn),
        verdict_status_rank(&VerdictStatus::Pass),
        verdict_status_rank(&VerdictStatus::Skip),
    ];
    for i in 0..ranks.len() {
        for j in (i + 1)..ranks.len() {
            assert_ne!(ranks[i], ranks[j], "ranks[{i}] == ranks[{j}]");
        }
    }
}

// ===========================================================================
// is_valid_sensor_id — rejects dangerous inputs, accepts valid ones
// ===========================================================================

#[test]
fn sensor_id_rejects_empty() {
    assert!(!is_valid_sensor_id(""));
}

#[test]
fn sensor_id_rejects_dot_dot() {
    assert!(!is_valid_sensor_id(".."));
}

#[test]
fn sensor_id_rejects_slash() {
    assert!(!is_valid_sensor_id("bad/path"));
}

#[test]
fn sensor_id_rejects_backslash() {
    assert!(!is_valid_sensor_id("bad\\path"));
}

#[test]
fn sensor_id_rejects_single_dot() {
    assert!(!is_valid_sensor_id("."));
}

#[test]
fn sensor_id_rejects_triple_dot() {
    assert!(!is_valid_sensor_id("..."));
}

#[test]
fn sensor_id_accepts_builddiag() {
    assert!(is_valid_sensor_id("builddiag"));
}

#[test]
fn sensor_id_accepts_hyphen() {
    assert!(is_valid_sensor_id("-"));
}

#[test]
fn sensor_id_accepts_underscore() {
    assert!(is_valid_sensor_id("_"));
}

#[test]
fn sensor_id_accepts_my_sensor_v2() {
    assert!(is_valid_sensor_id("my-sensor_v2"));
}

#[test]
fn sensor_id_rejects_path_traversal_prefix() {
    assert!(!is_valid_sensor_id("../escape"));
}

// ===========================================================================
// safety_level_rank — all 3 variants distinct
// ===========================================================================

#[test]
fn safety_level_rank_safe_is_zero() {
    assert_eq!(safety_level_rank(&SafetyLevel::Safe), 0);
}

#[test]
fn safety_level_rank_guarded_is_one() {
    assert_eq!(safety_level_rank(&SafetyLevel::Guarded), 1);
}

#[test]
fn safety_level_rank_unsafe_is_two() {
    assert_eq!(safety_level_rank(&SafetyLevel::Unsafe), 2);
}

#[test]
fn safety_level_rank_ordering() {
    assert!(safety_level_rank(&SafetyLevel::Safe) < safety_level_rank(&SafetyLevel::Guarded));
    assert!(safety_level_rank(&SafetyLevel::Guarded) < safety_level_rank(&SafetyLevel::Unsafe));
}

#[test]
fn safety_level_rank_all_distinct() {
    let ranks = [
        safety_level_rank(&SafetyLevel::Safe),
        safety_level_rank(&SafetyLevel::Guarded),
        safety_level_rank(&SafetyLevel::Unsafe),
    ];
    assert_ne!(ranks[0], ranks[1]);
    assert_ne!(ranks[1], ranks[2]);
    assert_ne!(ranks[0], ranks[2]);
}

// ===========================================================================
// Policy defaults
// ===========================================================================

#[test]
fn policy_default_max_highlights() {
    let cfg = CockpitConfig::default();
    assert_eq!(cfg.policy.max_highlights, 7);
}

#[test]
fn policy_default_max_per_sensor_findings() {
    let cfg = CockpitConfig::default();
    assert_eq!(cfg.policy.max_per_sensor_findings, 20);
}

#[test]
fn policy_default_max_annotations() {
    let cfg = CockpitConfig::default();
    assert_eq!(cfg.policy.max_annotations, 25);
}

#[test]
fn policy_default_max_receipt_size_bytes() {
    let cfg = CockpitConfig::default();
    assert_eq!(cfg.policy.max_receipt_size_bytes, 2 * 1024 * 1024);
}

#[test]
fn policy_default_warn_is_fail_false() {
    let cfg = CockpitConfig::default();
    assert!(!cfg.policy.warn_is_fail);
}

// ===========================================================================
// Default section order not empty, contains "Highlights" and "Other"
// ===========================================================================

#[test]
fn section_order_not_empty() {
    let cfg = CockpitConfig::default();
    assert!(!cfg.policy.section_order.is_empty());
}

#[test]
fn section_order_contains_highlights() {
    let cfg = CockpitConfig::default();
    assert!(
        cfg.policy.section_order.contains(&"Highlights".to_string()),
        "section_order must contain 'Highlights'"
    );
}

#[test]
fn section_order_contains_other() {
    let cfg = CockpitConfig::default();
    assert!(
        cfg.policy.section_order.contains(&"Other".to_string()),
        "section_order must contain 'Other'"
    );
}

// ===========================================================================
// is_zero serde behavior: suppressed=0 omitted, suppressed=1 included
// ===========================================================================

#[test]
fn verdict_counts_suppressed_zero_omitted_in_json() {
    let counts = VerdictCounts {
        info: 1,
        warn: 0,
        error: 0,
        suppressed: 0,
    };
    let json = serde_json::to_string(&counts).unwrap();
    assert!(
        !json.contains("suppressed"),
        "suppressed=0 should be omitted from JSON, got: {json}"
    );
}

#[test]
fn verdict_counts_suppressed_nonzero_included_in_json() {
    let counts = VerdictCounts {
        info: 0,
        warn: 0,
        error: 0,
        suppressed: 1,
    };
    let json = serde_json::to_string(&counts).unwrap();
    assert!(
        json.contains("suppressed"),
        "suppressed=1 should be included in JSON, got: {json}"
    );
}

#[test]
fn verdict_counts_roundtrip_with_suppressed() {
    let counts = VerdictCounts {
        info: 2,
        warn: 3,
        error: 4,
        suppressed: 5,
    };
    let json = serde_json::to_string(&counts).unwrap();
    let parsed: VerdictCounts = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, counts);
}

#[test]
fn verdict_counts_roundtrip_without_suppressed() {
    let counts = VerdictCounts {
        info: 1,
        warn: 2,
        error: 3,
        suppressed: 0,
    };
    let json = serde_json::to_string(&counts).unwrap();
    let parsed: VerdictCounts = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, counts);
}
