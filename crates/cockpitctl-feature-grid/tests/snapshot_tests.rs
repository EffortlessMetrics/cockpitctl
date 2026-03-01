use cockpitctl_feature_grid::{FEATURE_TOGGLE_GRID, feature_runtime_present, parse_feature_state};
use cockpitctl_feature_state::Feature;

// ---------------------------------------------------------------------------
// Feature toggle grid snapshot
// ---------------------------------------------------------------------------

#[test]
fn snapshot_feature_toggle_grid() {
    let grid: Vec<String> = FEATURE_TOGGLE_GRID
        .iter()
        .map(|c| {
            format!(
                "feature={} args={:?} expected={:?}",
                c.feature.as_str(),
                c.args,
                c.expected
            )
        })
        .collect();
    insta::assert_debug_snapshot!("feature_toggle_grid", grid);
}

// ---------------------------------------------------------------------------
// Runtime presence matrix snapshot
// ---------------------------------------------------------------------------

#[test]
fn snapshot_runtime_presence_matrix() {
    let scenarios: Vec<(&str, &[&str], bool)> = vec![
        (
            "hooks",
            &[] as &[&str],
            feature_runtime_present(Feature::Hooks, &[] as &[&str]),
        ),
        (
            "hooks",
            &["--disable-hooks"],
            feature_runtime_present(Feature::Hooks, &["--disable-hooks"]),
        ),
        (
            "buildfix",
            &[],
            feature_runtime_present(Feature::Buildfix, &[] as &[&str]),
        ),
        (
            "buildfix",
            &["--disable-buildfix"],
            feature_runtime_present(Feature::Buildfix, &["--disable-buildfix"]),
        ),
        (
            "policy-signing",
            &[],
            feature_runtime_present(Feature::PolicySigning, &[] as &[&str]),
        ),
        (
            "policy-signing",
            &["--disable-policy-signing"],
            feature_runtime_present(Feature::PolicySigning, &["--disable-policy-signing"]),
        ),
    ];
    insta::assert_debug_snapshot!("runtime_presence_matrix", scenarios);
}

// ---------------------------------------------------------------------------
// parse_feature_state snapshot
// ---------------------------------------------------------------------------

#[test]
fn snapshot_parse_feature_state_tokens() {
    let tokens = [
        "present", "absent", "enabled", "disabled", "on", "off", "PRESENT", "ABSENT", "unknown",
        "", "yes", "true",
    ];
    let results: Vec<(&str, Option<bool>)> = tokens
        .iter()
        .map(|t| (*t, parse_feature_state(t)))
        .collect();
    insta::assert_debug_snapshot!("parse_feature_state_tokens", results);
}

// ---------------------------------------------------------------------------
// Grid case evaluation snapshot
// ---------------------------------------------------------------------------

#[test]
fn snapshot_grid_case_evaluation() {
    let results: Vec<(String, bool)> = FEATURE_TOGGLE_GRID
        .iter()
        .map(|c| {
            let desc = format!("{}+{:?}=>{:?}", c.feature.as_str(), c.args, c.expected);
            (desc, c.matches_row(c.args))
        })
        .collect();
    insta::assert_debug_snapshot!("grid_case_evaluation", results);
}
