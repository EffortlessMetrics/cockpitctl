//! Snapshot tests for the feature grid layout.
//!
//! Captures a deterministic text representation of the feature grid for
//! different feature-flag configurations and asserts stability via `insta`.
//!
//! Because Cargo feature unification may enable feature flags when this crate
//! is tested as part of the workspace, each snapshot includes a suffix
//! reflecting the compile-time feature state, producing separate snapshot
//! files for features-on and features-off contexts.

use cockpitctl_feature_grid::{FEATURE_TOGGLE_GRID, feature_runtime_present};
use cockpitctl_feature_state::{Feature, RuntimeFeatureState};

/// Render a human-readable grid showing compile-time availability and
/// runtime presence for a given set of CLI args.
fn render_grid(cli_args: &[&str]) -> String {
    let args_owned: Vec<String> = cli_args.iter().map(|s| s.to_string()).collect();
    let mut lines = Vec::new();

    lines.push(format!("cli_args: {cli_args:?}"));
    lines.push(String::new());

    lines.push(format!(
        "{:<20} {:<12} {:<12} {:<10}",
        "feature", "compiled", "runtime", "grid_match"
    ));
    lines.push("-".repeat(56));

    for &feature in Feature::all() {
        let compiled = feature.is_available();
        let runtime = feature_runtime_present(feature, &args_owned);

        let grid_rows: Vec<_> = FEATURE_TOGGLE_GRID
            .iter()
            .filter(|c| c.feature == feature)
            .collect();
        let grid_match = grid_rows.iter().any(|c| c.matches_row(cli_args));

        lines.push(format!(
            "{:<20} {:<12} {:<12} {:<10}",
            feature.as_str(),
            compiled,
            runtime,
            grid_match,
        ));
    }

    lines.push(String::new());
    let state = RuntimeFeatureState::from_args(
        Feature::Hooks.is_available(),
        Feature::Buildfix.is_available(),
        Feature::PolicySigning.is_available(),
        &args_owned,
    );
    lines.push(format!(
        "RuntimeFeatureState: hooks={}, buildfix={}, policy_signing={}",
        state.hooks(),
        state.buildfix(),
        state.policy_signing(),
    ));

    lines.join("\n")
}

/// Returns a snapshot suffix reflecting compile-time feature state.
fn feature_suffix() -> &'static str {
    if Feature::Hooks.is_available() {
        "features_on"
    } else {
        "features_off"
    }
}

// ── Snapshot: no CLI args (default state) ───────────────────────────────────

#[test]
fn snapshot_grid_no_args() {
    let mut settings = insta::Settings::clone_current();
    settings.set_snapshot_suffix(feature_suffix());
    settings.bind(|| {
        insta::assert_snapshot!(render_grid(&[]));
    });
}

// ── Snapshot: all disable flags ─────────────────────────────────────────────

#[test]
fn snapshot_grid_all_disabled() {
    let mut settings = insta::Settings::clone_current();
    settings.set_snapshot_suffix(feature_suffix());
    settings.bind(|| {
        insta::assert_snapshot!(render_grid(&[
            "--disable-hooks",
            "--disable-buildfix",
            "--disable-policy-signing",
        ]));
    });
}

// ── Snapshot: only hooks disabled ───────────────────────────────────────────

#[test]
fn snapshot_grid_disable_hooks_only() {
    let mut settings = insta::Settings::clone_current();
    settings.set_snapshot_suffix(feature_suffix());
    settings.bind(|| {
        insta::assert_snapshot!(render_grid(&["--disable-hooks"]));
    });
}

// ── Snapshot: only buildfix disabled ────────────────────────────────────────

#[test]
fn snapshot_grid_disable_buildfix_only() {
    let mut settings = insta::Settings::clone_current();
    settings.set_snapshot_suffix(feature_suffix());
    settings.bind(|| {
        insta::assert_snapshot!(render_grid(&["--disable-buildfix"]));
    });
}

// ── Snapshot: only policy-signing disabled ──────────────────────────────────

#[test]
fn snapshot_grid_disable_policy_signing_only() {
    let mut settings = insta::Settings::clone_current();
    settings.set_snapshot_suffix(feature_suffix());
    settings.bind(|| {
        insta::assert_snapshot!(render_grid(&["--disable-policy-signing"]));
    });
}
