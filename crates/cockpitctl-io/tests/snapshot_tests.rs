use cockpitctl_io::FsLayout;

/// Normalize path separators to forward slashes for cross-platform snapshots.
fn norm(p: impl std::fmt::Display) -> String {
    p.to_string().replace('\\', "/")
}

// ---------------------------------------------------------------------------
// FsLayout path computation snapshots
// ---------------------------------------------------------------------------

#[test]
fn snapshot_fs_layout_default_paths() {
    let layout = FsLayout::new("artifacts", "cockpit.toml");
    let paths = vec![
        ("artifacts_dir", norm(layout.artifacts_dir.display())),
        ("out_dir", norm(layout.out_dir.display())),
        ("config_path", norm(layout.config_path.display())),
        ("max_receipt_bytes", layout.max_receipt_bytes.to_string()),
        ("max_receipts", layout.max_receipts.to_string()),
    ];
    insta::assert_debug_snapshot!("fs_layout_default_paths", paths);
}

#[test]
fn snapshot_fs_layout_sensor_paths() {
    let layout = FsLayout::new("artifacts", "cockpit.toml");
    let paths = vec![
        ("sensor_dir", norm(layout.sensor_dir("clippy").display())),
        ("report_file", norm(layout.report_file("clippy").display())),
        (
            "comment_file",
            norm(layout.comment_file("clippy").display()),
        ),
        ("plan_file", norm(layout.plan_file("clippy").display())),
    ];
    insta::assert_debug_snapshot!("fs_layout_sensor_paths", paths);
}

#[test]
fn snapshot_fs_layout_cockpit_output_paths() {
    let layout = FsLayout::new("artifacts", "cockpit.toml");
    let paths = vec![
        (
            "cockpit_report",
            norm(layout.cockpit_report_file().display()),
        ),
        (
            "cockpit_comment",
            norm(layout.cockpit_comment_file().display()),
        ),
        ("sarif_report", norm(layout.sarif_report_file().display())),
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
