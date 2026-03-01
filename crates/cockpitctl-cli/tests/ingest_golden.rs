use std::fs;
use std::path::{Path, PathBuf};

fn read_to_string(p: &Path) -> String {
    fs::read_to_string(p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()))
}

fn workspace_root() -> PathBuf {
    // CARGO_MANIFEST_DIR points at crates/cockpitctl-cli
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn cockpitctl_cmd() -> assert_cmd::Command {
    let mut cmd = assert_cmd::cargo_bin_cmd!("cockpitctl");
    if let Ok(profile) = std::env::var("LLVM_PROFILE_FILE") {
        cmd.env("LLVM_PROFILE_FILE", profile);
    }
    cmd
}

/// Run `cockpitctl ingest` and assert the expected exit code.
fn run_ingest_expecting(fixture: &Path, code: i32) -> (String, String) {
    let artifacts = fixture.join("artifacts");
    let config = fixture.join("cockpit.toml");

    let out_dir = artifacts.join("cockpit");
    let _ = fs::remove_dir_all(&out_dir);

    let mut cmd = cockpitctl_cmd();
    cmd.env("COCKPITCTL_STARTED_AT", "2026-02-02T12:00:00Z");
    cmd.args([
        "ingest",
        "--artifacts",
        artifacts.to_string_lossy().as_ref(),
        "--config",
        config.to_string_lossy().as_ref(),
    ]);

    cmd.assert().code(code);

    let report = read_to_string(&out_dir.join("report.json"));
    let comment = read_to_string(&out_dir.join("comment.md"));
    (report, comment)
}

#[test]
fn ingest_happy_path_fixture_matches_golden() {
    let fixture = workspace_root().join("fixtures/happy_path");
    let artifacts = fixture.join("artifacts");
    let config = fixture.join("cockpit.toml");

    // Clean any previous outputs.
    let out_dir = artifacts.join("cockpit");
    let _ = fs::remove_dir_all(&out_dir);

    let mut cmd = cockpitctl_cmd();
    cmd.env("COCKPITCTL_STARTED_AT", "2026-02-02T12:00:00Z");
    cmd.args([
        "ingest",
        "--artifacts",
        artifacts.to_string_lossy().as_ref(),
        "--config",
        config.to_string_lossy().as_ref(),
    ]);

    cmd.assert().success();

    let got_report = read_to_string(&out_dir.join("report.json"));
    let got_comment = read_to_string(&out_dir.join("comment.md"));

    let exp_report = read_to_string(&fixture.join("expected").join("report.json"));
    let exp_comment = read_to_string(&fixture.join("expected").join("comment.md"));

    pretty_assertions::assert_eq!(got_report, exp_report, "report.json changed from golden");
    pretty_assertions::assert_eq!(got_comment, exp_comment, "comment.md changed from golden");
}

#[test]
fn ingest_missing_receipt_fixture_matches_golden_and_exits_2() {
    let fixture = workspace_root().join("fixtures/missing_receipt");
    let artifacts = fixture.join("artifacts");
    let config = fixture.join("cockpit.toml");

    // Clean any previous outputs.
    let out_dir = artifacts.join("cockpit");
    let _ = fs::remove_dir_all(&out_dir);

    let mut cmd = cockpitctl_cmd();
    cmd.env("COCKPITCTL_STARTED_AT", "2026-02-02T12:00:00Z");
    cmd.args([
        "ingest",
        "--artifacts",
        artifacts.to_string_lossy().as_ref(),
        "--config",
        config.to_string_lossy().as_ref(),
    ]);

    cmd.assert().code(2);

    let got_report = read_to_string(&out_dir.join("report.json"));
    let got_comment = read_to_string(&out_dir.join("comment.md"));

    let exp_report = read_to_string(&fixture.join("expected").join("report.json"));
    let exp_comment = read_to_string(&fixture.join("expected").join("comment.md"));

    pretty_assertions::assert_eq!(got_report, exp_report, "report.json changed from golden");
    pretty_assertions::assert_eq!(got_comment, exp_comment, "comment.md changed from golden");
}

#[test]
fn ingest_skip_receipt_fixture_matches_golden() {
    let fixture = workspace_root().join("fixtures/skip_receipt");
    let artifacts = fixture.join("artifacts");
    let config = fixture.join("cockpit.toml");

    let out_dir = artifacts.join("cockpit");
    let _ = fs::remove_dir_all(&out_dir);

    let mut cmd = cockpitctl_cmd();
    cmd.env("COCKPITCTL_STARTED_AT", "2026-02-02T12:00:00Z");
    cmd.args([
        "ingest",
        "--artifacts",
        artifacts.to_string_lossy().as_ref(),
        "--config",
        config.to_string_lossy().as_ref(),
    ]);

    cmd.assert().success();

    let got_report = read_to_string(&out_dir.join("report.json"));
    let got_comment = read_to_string(&out_dir.join("comment.md"));

    let exp_report = read_to_string(&fixture.join("expected").join("report.json"));
    let exp_comment = read_to_string(&fixture.join("expected").join("comment.md"));

    pretty_assertions::assert_eq!(got_report, exp_report, "report.json changed from golden");
    pretty_assertions::assert_eq!(got_comment, exp_comment, "comment.md changed from golden");
}

#[test]
fn ingest_tool_error_fixture_matches_golden_and_exits_2() {
    let fixture = workspace_root().join("fixtures/tool_error");
    let artifacts = fixture.join("artifacts");
    let config = fixture.join("cockpit.toml");

    let out_dir = artifacts.join("cockpit");
    let _ = fs::remove_dir_all(&out_dir);

    let mut cmd = cockpitctl_cmd();
    cmd.env("COCKPITCTL_STARTED_AT", "2026-02-02T12:00:00Z");
    cmd.args([
        "ingest",
        "--artifacts",
        artifacts.to_string_lossy().as_ref(),
        "--config",
        config.to_string_lossy().as_ref(),
    ]);

    cmd.assert().code(2);

    let got_report = read_to_string(&out_dir.join("report.json"));
    let got_comment = read_to_string(&out_dir.join("comment.md"));

    let exp_report = read_to_string(&fixture.join("expected").join("report.json"));
    let exp_comment = read_to_string(&fixture.join("expected").join("comment.md"));

    pretty_assertions::assert_eq!(got_report, exp_report, "report.json changed from golden");
    pretty_assertions::assert_eq!(got_comment, exp_comment, "comment.md changed from golden");
}

#[test]
fn ingest_mixed_verdicts_fixture_matches_golden_and_exits_2() {
    let fixture = workspace_root().join("fixtures/mixed_verdicts");
    let artifacts = fixture.join("artifacts");
    let config = fixture.join("cockpit.toml");

    let out_dir = artifacts.join("cockpit");
    let _ = fs::remove_dir_all(&out_dir);

    let mut cmd = cockpitctl_cmd();
    cmd.env("COCKPITCTL_STARTED_AT", "2026-02-02T12:00:00Z");
    cmd.args([
        "ingest",
        "--artifacts",
        artifacts.to_string_lossy().as_ref(),
        "--config",
        config.to_string_lossy().as_ref(),
    ]);

    cmd.assert().code(2);

    let got_report = read_to_string(&out_dir.join("report.json"));
    let got_comment = read_to_string(&out_dir.join("comment.md"));

    let exp_report = read_to_string(&fixture.join("expected").join("report.json"));
    let exp_comment = read_to_string(&fixture.join("expected").join("comment.md"));

    pretty_assertions::assert_eq!(got_report, exp_report, "report.json changed from golden");
    pretty_assertions::assert_eq!(got_comment, exp_comment, "comment.md changed from golden");
}

// ═══════════════════════════════════════════════════════════════════════════════
// Expanded golden tests (insta snapshots)
// ═══════════════════════════════════════════════════════════════════════════════

// 1. Single sensor pass — one sensor, pass verdict, zero findings.
#[test]
fn golden_single_sensor_pass() {
    let fixture = workspace_root().join("fixtures/empty_findings");
    let (report, comment) = run_ingest_expecting(&fixture, 0);
    insta::assert_snapshot!("single_sensor_pass_report", report);
    insta::assert_snapshot!("single_sensor_pass_comment", comment);
}

// 2. Single sensor fail — one blocking sensor with error → exit 2.
#[test]
fn golden_single_sensor_fail() {
    let fixture = workspace_root().join("fixtures/single_fail");
    let (report, comment) = run_ingest_expecting(&fixture, 2);
    insta::assert_snapshot!("single_sensor_fail_report", report);
    insta::assert_snapshot!("single_sensor_fail_comment", comment);
}

// 3. Three sensor mixed — pass + warn + fail → exit 2, aggregation golden.
#[test]
fn golden_three_sensor_mixed() {
    let fixture = workspace_root().join("fixtures/three_sensor_mixed");
    let (report, comment) = run_ingest_expecting(&fixture, 2);
    insta::assert_snapshot!("three_sensor_mixed_report", report);
    insta::assert_snapshot!("three_sensor_mixed_comment", comment);
}

// 4. Schema validation lax — receipt with extra field accepted in lax mode.
#[test]
fn golden_schema_validation_lax() {
    let fixture = workspace_root().join("fixtures/schema_lax");
    let (report, comment) = run_ingest_expecting(&fixture, 0);
    insta::assert_snapshot!("schema_lax_report", report);
    insta::assert_snapshot!("schema_lax_comment", comment);
}

// 5. Schema validation strict — receipt with extra field rejected → exit 2.
#[test]
#[cfg(feature = "feature-schema")]
fn golden_schema_validation_strict() {
    let fixture = workspace_root().join("fixtures/schema_violation");
    let (report, comment) = run_ingest_expecting(&fixture, 2);
    insta::assert_snapshot!("schema_strict_report", report);
    insta::assert_snapshot!("schema_strict_comment", comment);
}

// 6. Max highlights budget — 6 findings, max_highlights=3 → truncated golden.
#[test]
fn golden_max_highlights_budget() {
    let fixture = workspace_root().join("fixtures/highlight_cap");
    let (report, comment) = run_ingest_expecting(&fixture, 2);
    insta::assert_snapshot!("max_highlights_report", report);
    insta::assert_snapshot!("max_highlights_comment", comment);
}

// 7. No findings at all — multiple sensors, zero findings → clean pass.
#[test]
fn golden_no_findings_multi() {
    let fixture = workspace_root().join("fixtures/no_findings_multi");
    let (report, comment) = run_ingest_expecting(&fixture, 0);
    insta::assert_snapshot!("no_findings_multi_report", report);
    insta::assert_snapshot!("no_findings_multi_comment", comment);
}

// 8. Warn-is-fail policy — warn_is_fail=true converts warnings to failure → exit 2.
#[test]
fn golden_warn_is_fail() {
    let fixture = workspace_root().join("fixtures/warn_as_fail");
    let (report, comment) = run_ingest_expecting(&fixture, 2);
    insta::assert_snapshot!("warn_is_fail_report", report);
    insta::assert_snapshot!("warn_is_fail_comment", comment);
}

// 9. All sensors skip — every sensor reports skip → golden with skip handling.
#[test]
fn golden_all_sensors_skip() {
    let fixture = workspace_root().join("fixtures/all_skip");
    let (report, comment) = run_ingest_expecting(&fixture, 0);
    insta::assert_snapshot!("all_sensors_skip_report", report);
    insta::assert_snapshot!("all_sensors_skip_comment", comment);
}

// 10. Tokmd sensor — token counting sensor fixture → golden report.
#[test]
fn golden_tokmd_sensor() {
    let fixture = workspace_root().join("fixtures/tokmd_receipt");
    let (report, comment) = run_ingest_expecting(&fixture, 0);
    insta::assert_snapshot!("tokmd_sensor_report", report);
    insta::assert_snapshot!("tokmd_sensor_comment", comment);
}
