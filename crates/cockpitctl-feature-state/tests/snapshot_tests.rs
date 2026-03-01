use cockpitctl_feature_state::{Feature, RuntimeFeatureState};

// ---------------------------------------------------------------------------
// Feature contract snapshots
// ---------------------------------------------------------------------------

#[test]
fn snapshot_all_feature_contracts() {
    let contracts: Vec<String> = Feature::all()
        .iter()
        .map(|f| format!("{:#?}", f.contract()))
        .collect();
    insta::assert_debug_snapshot!("all_feature_contracts", contracts);
}

#[test]
fn snapshot_feature_catalog() {
    let catalog: Vec<(&str, &str, bool)> = Feature::all()
        .iter()
        .map(|f| (f.as_str(), f.disable_flag(), f.is_available()))
        .collect();
    insta::assert_debug_snapshot!("feature_catalog", catalog);
}

// ---------------------------------------------------------------------------
// RuntimeFeatureState snapshots
// ---------------------------------------------------------------------------

#[test]
fn snapshot_default_state_all_disabled() {
    let state = RuntimeFeatureState::new(false, false, false);
    let view: Vec<(&str, bool)> = Feature::all()
        .iter()
        .map(|f| (f.as_str(), state.is_enabled(*f)))
        .collect();
    insta::assert_debug_snapshot!("runtime_state_all_disabled", view);
}

#[test]
fn snapshot_all_features_enabled() {
    let state = RuntimeFeatureState::new(true, true, true);
    let view: Vec<(&str, bool)> = Feature::all()
        .iter()
        .map(|f| (f.as_str(), state.is_enabled(*f)))
        .collect();
    insta::assert_debug_snapshot!("runtime_state_all_enabled", view);
}

#[test]
fn snapshot_hooks_only_enabled() {
    let state = RuntimeFeatureState::new(true, false, false);
    let view: Vec<(&str, bool)> = Feature::all()
        .iter()
        .map(|f| (f.as_str(), state.is_enabled(*f)))
        .collect();
    insta::assert_debug_snapshot!("runtime_state_hooks_only", view);
}

#[test]
fn snapshot_from_disable_flags() {
    let state = RuntimeFeatureState::from_disable_flags(true, false, true, true, true, false);
    let view: Vec<(&str, bool)> = Feature::all()
        .iter()
        .map(|f| (f.as_str(), state.is_enabled(*f)))
        .collect();
    insta::assert_debug_snapshot!("runtime_state_from_disable_flags", view);
}

#[test]
fn snapshot_from_cli_args() {
    let args: Vec<String> = vec![
        "--disable-buildfix".into(),
        "--disable-policy-signing".into(),
    ];
    let state = RuntimeFeatureState::from_args(true, true, true, &args);
    let view: Vec<(&str, bool)> = Feature::all()
        .iter()
        .map(|f| (f.as_str(), state.is_enabled(*f)))
        .collect();
    insta::assert_debug_snapshot!("runtime_state_from_cli_args", view);
}
