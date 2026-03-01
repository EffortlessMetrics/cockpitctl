use cockpitctl_io::FsLayout;

// ---------------------------------------------------------------------------
// FsLayout path computation snapshots
// ---------------------------------------------------------------------------

#[test]
fn snapshot_fs_layout_default_paths() {
    let layout = FsLayout::new("artifacts", "cockpit.toml");
    let paths = vec![
        ("artifacts_dir", layout.artifacts_dir.display().to_string()),
        ("out_dir", layout.out_dir.display().to_string()),
        ("config_path", layout.config_path.display().to_string()),
        ("max_receipt_bytes", layout.max_receipt_bytes.to_string()),
        ("max_receipts", layout.max_receipts.to_string()),
    ];
    insta::assert_debug_snapshot!("fs_layout_default_paths", paths);
}

#[test]
fn snapshot_fs_layout_sensor_paths() {
    let layout = FsLayout::new("artifacts", "cockpit.toml");
    let paths = vec![
        (
            "sensor_dir",
            layout.sensor_dir("clippy").display().to_string(),
        ),
        (
            "report_file",
            layout.report_file("clippy").display().to_string(),
        ),
        (
            "comment_file",
            layout.comment_file("clippy").display().to_string(),
        ),
        (
            "plan_file",
            layout.plan_file("clippy").display().to_string(),
        ),
    ];
    insta::assert_debug_snapshot!("fs_layout_sensor_paths", paths);
}

#[test]
fn snapshot_fs_layout_cockpit_output_paths() {
    let layout = FsLayout::new("artifacts", "cockpit.toml");
    let paths = vec![
        (
            "cockpit_report",
            layout.cockpit_report_file().display().to_string(),
        ),
        (
            "cockpit_comment",
            layout.cockpit_comment_file().display().to_string(),
        ),
        (
            "sarif_report",
            layout.sarif_report_file().display().to_string(),
        ),
    ];
    insta::assert_debug_snapshot!("fs_layout_cockpit_output_paths", paths);
}

#[test]
fn snapshot_fs_layout_custom_limits() {
    let layout = FsLayout::new("artifacts", "cockpit.toml")
        .with_max_receipts(50)
        .with_max_receipt_bytes(1024 * 1024);
    let limits = vec![
        ("max_receipts", layout.max_receipts.to_string()),
        ("max_receipt_bytes", layout.max_receipt_bytes.to_string()),
    ];
    insta::assert_debug_snapshot!("fs_layout_custom_limits", limits);
}
