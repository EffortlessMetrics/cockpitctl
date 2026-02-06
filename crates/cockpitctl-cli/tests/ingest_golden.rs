use std::fs;
use std::path::{Path, PathBuf};

fn read_to_string(p: &Path) -> String {
    fs::read_to_string(p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()))
}

fn workspace_root() -> PathBuf {
    // CARGO_MANIFEST_DIR points at crates/cockpitctl-cli
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn ingest_happy_path_fixture_matches_golden() {
    let fixture = workspace_root().join("fixtures/happy_path");
    let artifacts = fixture.join("artifacts");
    let config = fixture.join("cockpit.toml");

    // Clean any previous outputs.
    let out_dir = artifacts.join("cockpit");
    let _ = fs::remove_dir_all(&out_dir);

    let mut cmd = assert_cmd::cargo_bin_cmd!("cockpitctl");
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

    let mut cmd = assert_cmd::cargo_bin_cmd!("cockpitctl");
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

    let mut cmd = assert_cmd::cargo_bin_cmd!("cockpitctl");
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

    let mut cmd = assert_cmd::cargo_bin_cmd!("cockpitctl");
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

    let mut cmd = assert_cmd::cargo_bin_cmd!("cockpitctl");
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
