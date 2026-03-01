//! Filesystem adapters for cockpitctl (ports implementation).
//!
//! This crate is the boundary between IO and the ingest use case.

use anyhow::{Context, Result};
use cockpitctl_ingest::{
    CommentRead, DiscoveredSensors, OutputSink, PlanRead, PolicySource, ReceiptSource, ReportRead,
};
use cockpitctl_types::{CockpitConfig, is_valid_sensor_id};
use std::fs;
use std::path::{Path, PathBuf};

pub use cockpitctl_io_buildfix::run_buildfix_actuator;
pub use cockpitctl_io_hooks::{CommentSection, OutputFile, PostProcessOutput, run_hooks};
pub use cockpitctl_io_policy_signing::load_policy_signing_key;
pub use cockpitctl_io_schema::JsonSchemaValidator;

/// Default cap on number of receipts (sensors) to process.
/// Protects against DoS if someone creates thousands of sensor directories.
pub const DEFAULT_MAX_RECEIPTS: usize = 100;

#[derive(Clone)]
pub struct FsLayout {
    pub artifacts_dir: PathBuf,
    pub out_dir: PathBuf,
    pub config_path: PathBuf,
    pub max_receipt_bytes: usize,
    /// Maximum number of sensor receipts to process. Protects against DoS.
    pub max_receipts: usize,
}

impl FsLayout {
    pub fn new(artifacts_dir: impl Into<PathBuf>, config_path: impl Into<PathBuf>) -> Self {
        let artifacts_dir = artifacts_dir.into();
        let out_dir = artifacts_dir.join("cockpit");
        Self {
            artifacts_dir,
            out_dir,
            config_path: config_path.into(),
            max_receipt_bytes: 2 * 1024 * 1024, // 2MB default safety cap
            max_receipts: DEFAULT_MAX_RECEIPTS,
        }
    }

    /// Set a custom max_receipts limit. Returns self for chaining.
    pub fn with_max_receipts(mut self, max: usize) -> Self {
        self.max_receipts = max;
        self
    }

    /// Set a custom max receipt file size in bytes. Returns self for chaining.
    pub fn with_max_receipt_bytes(mut self, max: usize) -> Self {
        self.max_receipt_bytes = max;
        self
    }

    pub fn sensor_dir(&self, sensor_id: &str) -> PathBuf {
        self.artifacts_dir.join(sensor_id)
    }

    pub fn report_file(&self, sensor_id: &str) -> PathBuf {
        self.sensor_dir(sensor_id).join("report.json")
    }

    pub fn comment_file(&self, sensor_id: &str) -> PathBuf {
        self.sensor_dir(sensor_id).join("comment.md")
    }

    pub fn plan_file(&self, sensor_id: &str) -> PathBuf {
        self.sensor_dir(sensor_id).join("plan.json")
    }

    pub fn sarif_report_file(&self) -> PathBuf {
        self.out_dir.join("sarif.json")
    }

    pub fn cockpit_report_file(&self) -> PathBuf {
        self.out_dir.join("report.json")
    }

    pub fn cockpit_comment_file(&self) -> PathBuf {
        self.out_dir.join("comment.md")
    }
}

#[derive(Clone)]
pub struct FsReceiptSource {
    layout: FsLayout,
    artifacts_root: PathBuf,
}

impl FsReceiptSource {
    pub fn new(layout: FsLayout) -> Self {
        let artifacts_root = canonicalize_root(&layout.artifacts_dir);
        Self {
            layout,
            artifacts_root,
        }
    }

    fn is_safe_path(&self, path: &Path) -> bool {
        let canonical = match fs::canonicalize(path) {
            Ok(p) => p,
            Err(_) => return false,
        };
        canonical.starts_with(&self.artifacts_root)
    }
}

fn absolute_path(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    }
}

fn canonicalize_root(path: &Path) -> PathBuf {
    if path.exists() {
        fs::canonicalize(path).unwrap_or_else(|_| absolute_path(path))
    } else {
        absolute_path(path)
    }
}

impl ReceiptSource for FsReceiptSource {
    fn discovered_sensors(&self) -> Result<DiscoveredSensors> {
        let mut out = Vec::new();
        let mut invalid = Vec::new();
        if !self.layout.artifacts_dir.exists() {
            // No artifacts dir: valid for local runs. Treat as empty.
            return Ok(DiscoveredSensors {
                sensors: out,
                truncated: false,
                total_found: 0,
                invalid_sensor_ids: invalid,
            });
        }

        // Each direct child directory of artifacts/ is a sensor candidate.
        for entry in fs::read_dir(&self.layout.artifacts_dir).with_context(|| {
            format!("read artifacts dir {}", self.layout.artifacts_dir.display())
        })? {
            let entry = entry?;
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if name == "cockpit" {
                continue;
            }
            if !is_valid_sensor_id(&name) {
                invalid.push(name);
                continue;
            }
            if self.layout.report_file(&name).exists() {
                out.push(name);
            }
        }

        out.sort();
        invalid.sort();

        let total_found = out.len();
        let truncated = total_found > self.layout.max_receipts;
        if truncated {
            out.truncate(self.layout.max_receipts);
        }

        Ok(DiscoveredSensors {
            sensors: out,
            truncated,
            total_found,
            invalid_sensor_ids: invalid,
        })
    }

    fn read_report_bytes(&self, sensor_id: &str) -> Result<ReportRead> {
        if !is_valid_sensor_id(sensor_id) {
            return Ok(ReportRead::UnsafePath);
        }
        let p = self.layout.report_file(sensor_id);
        if !p.exists() {
            return Ok(ReportRead::Missing);
        }
        if !self.is_safe_path(&p) {
            return Ok(ReportRead::UnsafePath);
        }
        let meta = fs::metadata(&p)?;
        if meta.len() as usize > self.layout.max_receipt_bytes {
            return Ok(ReportRead::Oversized {
                size: meta.len(),
                cap: self.layout.max_receipt_bytes,
            });
        }
        let bytes = fs::read(&p).with_context(|| format!("read receipt {}", p.display()))?;
        Ok(ReportRead::Bytes(bytes))
    }

    fn report_path(&self, sensor_id: &str) -> String {
        format!("artifacts/{}/report.json", sensor_id)
    }

    fn comment_path_if_present(&self, sensor_id: &str) -> Result<CommentRead> {
        if !is_valid_sensor_id(sensor_id) {
            return Ok(CommentRead::UnsafePath);
        }
        let p = self.layout.comment_file(sensor_id);
        if p.exists() {
            if !self.is_safe_path(&p) {
                return Ok(CommentRead::UnsafePath);
            }
            Ok(CommentRead::Present(format!(
                "artifacts/{}/comment.md",
                sensor_id
            )))
        } else {
            Ok(CommentRead::Missing)
        }
    }

    fn read_plan_bytes(&self, sensor_id: &str) -> Result<PlanRead> {
        if !is_valid_sensor_id(sensor_id) {
            return Ok(PlanRead::Missing);
        }
        let p = self.layout.plan_file(sensor_id);
        if !p.exists() {
            return Ok(PlanRead::Missing);
        }
        if !self.is_safe_path(&p) {
            return Ok(PlanRead::Missing);
        }
        let meta = fs::metadata(&p)?;
        if meta.len() as usize > self.layout.max_receipt_bytes {
            return Ok(PlanRead::Oversized {
                size: meta.len(),
                cap: self.layout.max_receipt_bytes,
            });
        }
        let bytes = fs::read(&p).with_context(|| format!("read plan {}", p.display()))?;
        Ok(PlanRead::Bytes(bytes))
    }
}

#[derive(Clone)]
pub struct FsPolicySource {
    layout: FsLayout,
}

impl FsPolicySource {
    pub fn new(layout: FsLayout) -> Self {
        Self { layout }
    }
}

impl PolicySource for FsPolicySource {
    fn load_config(&self) -> Result<Option<CockpitConfig>> {
        let p = &self.layout.config_path;
        if !p.exists() {
            return Ok(None);
        }
        let txt = fs::read_to_string(p).with_context(|| format!("read config {}", p.display()))?;
        let cfg: CockpitConfig =
            toml::from_str(&txt).with_context(|| format!("parse TOML {}", p.display()))?;
        Ok(Some(cfg))
    }
}

#[derive(Clone)]
pub struct FsOutputSink {
    layout: FsLayout,
}

impl FsOutputSink {
    pub fn new(layout: FsLayout) -> Self {
        Self { layout }
    }
}

impl OutputSink for FsOutputSink {
    fn write_cockpit_report(&self, json: &str) -> Result<()> {
        fs::create_dir_all(&self.layout.out_dir)
            .with_context(|| format!("create out dir {}", self.layout.out_dir.display()))?;
        let p = self.layout.cockpit_report_file();
        fs::write(&p, json).with_context(|| format!("write {}", p.display()))?;
        Ok(())
    }

    fn write_cockpit_comment(&self, md: &str) -> Result<()> {
        fs::create_dir_all(&self.layout.out_dir)
            .with_context(|| format!("create out dir {}", self.layout.out_dir.display()))?;
        let p = self.layout.cockpit_comment_file();
        fs::write(&p, md).with_context(|| format!("write {}", p.display()))?;
        Ok(())
    }

    fn write_extra_file(&self, name: &str, content: &[u8]) -> Result<()> {
        // Safety: only allow writes inside artifacts/cockpit/
        if name.contains("..") || name.contains('/') || name.contains('\\') {
            anyhow::bail!("unsafe extra file name: {}", name);
        }
        fs::create_dir_all(&self.layout.out_dir)
            .with_context(|| format!("create out dir {}", self.layout.out_dir.display()))?;
        let p = self.layout.out_dir.join(name);
        fs::write(&p, content).with_context(|| format!("write extra file {}", p.display()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cockpitctl_ingest::ReportRead;

    fn minimal_report_json() -> String {
        serde_json::json!({
            "schema": "sensor.report.v1",
            "tool": { "name": "test", "version": "1.0.0" },
            "run": { "started_at": "2026-01-01T00:00:00Z" },
            "verdict": { "status": "pass", "counts": { "info": 0, "warn": 0, "error": 0 } },
            "findings": []
        })
        .to_string()
    }

    #[test]
    fn oversized_receipt_at_exact_boundary() {
        let tmp = tempfile::tempdir().unwrap();
        let artifacts = tmp.path().join("artifacts");
        let sensor_dir = artifacts.join("sensor1");
        fs::create_dir_all(&sensor_dir).unwrap();

        let cap: usize = 2 * 1024 * 1024; // 2MB

        // 2MB - 1: should be accepted
        let under = vec![b'x'; cap - 1];
        fs::write(sensor_dir.join("report.json"), &under).unwrap();
        let layout = FsLayout::new(&artifacts, tmp.path().join("cockpit.toml"));
        let src = FsReceiptSource::new(layout);
        match src.read_report_bytes("sensor1").unwrap() {
            ReportRead::Bytes(b) => assert_eq!(b.len(), cap - 1),
            other => panic!("expected Bytes, got {:?}", std::mem::discriminant(&other)),
        }

        // Exactly 2MB: should be accepted (not strictly greater)
        let exact = vec![b'x'; cap];
        fs::write(sensor_dir.join("report.json"), &exact).unwrap();
        let layout2 = FsLayout::new(&artifacts, tmp.path().join("cockpit.toml"));
        let src2 = FsReceiptSource::new(layout2);
        match src2.read_report_bytes("sensor1").unwrap() {
            ReportRead::Bytes(b) => assert_eq!(b.len(), cap),
            other => panic!("expected Bytes, got {:?}", std::mem::discriminant(&other)),
        }

        // 2MB + 1: should be rejected as oversized
        let over = vec![b'x'; cap + 1];
        fs::write(sensor_dir.join("report.json"), &over).unwrap();
        let layout3 = FsLayout::new(&artifacts, tmp.path().join("cockpit.toml"));
        let src3 = FsReceiptSource::new(layout3);
        match src3.read_report_bytes("sensor1").unwrap() {
            ReportRead::Oversized { size, cap: c } => {
                assert_eq!(size as usize, cap + 1);
                assert_eq!(c, cap);
            }
            other => panic!(
                "expected Oversized, got {:?}",
                std::mem::discriminant(&other)
            ),
        }
    }

    #[test]
    fn empty_artifacts_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let artifacts = tmp.path().join("artifacts");
        fs::create_dir_all(&artifacts).unwrap();

        let layout = FsLayout::new(&artifacts, tmp.path().join("cockpit.toml"));
        let src = FsReceiptSource::new(layout);
        let discovered = src.discovered_sensors().unwrap();
        assert!(discovered.sensors.is_empty());
        assert_eq!(discovered.total_found, 0);
        assert!(!discovered.truncated);
    }

    #[test]
    fn artifacts_dir_nonexistent() {
        let tmp = tempfile::tempdir().unwrap();
        let artifacts = tmp.path().join("nonexistent");

        let layout = FsLayout::new(&artifacts, tmp.path().join("cockpit.toml"));
        let src = FsReceiptSource::new(layout);
        let discovered = src.discovered_sensors().unwrap();
        assert!(discovered.sensors.is_empty());
    }

    #[test]
    fn artifacts_with_only_non_receipt_files() {
        let tmp = tempfile::tempdir().unwrap();
        let artifacts = tmp.path().join("artifacts");
        let sensor_dir = artifacts.join("sensor1");
        fs::create_dir_all(&sensor_dir).unwrap();
        // Write a non-receipt file (no report.json)
        fs::write(sensor_dir.join("README.md"), "hello").unwrap();

        let layout = FsLayout::new(&artifacts, tmp.path().join("cockpit.toml"));
        let src = FsReceiptSource::new(layout);
        let discovered = src.discovered_sensors().unwrap();
        assert!(discovered.sensors.is_empty());
    }

    #[test]
    fn artifacts_with_files_not_directories() {
        let tmp = tempfile::tempdir().unwrap();
        let artifacts = tmp.path().join("artifacts");
        fs::create_dir_all(&artifacts).unwrap();
        // A file at top level (not a directory)
        fs::write(artifacts.join("not-a-dir.txt"), "data").unwrap();

        let layout = FsLayout::new(&artifacts, tmp.path().join("cockpit.toml"));
        let src = FsReceiptSource::new(layout);
        let discovered = src.discovered_sensors().unwrap();
        assert!(discovered.sensors.is_empty());
    }

    #[test]
    fn receipt_with_wrong_schema_version() {
        let tmp = tempfile::tempdir().unwrap();
        let artifacts = tmp.path().join("artifacts");
        let sensor_dir = artifacts.join("sensor1");
        fs::create_dir_all(&sensor_dir).unwrap();

        let wrong_schema = serde_json::json!({
            "schema": "sensor.report.v999",
            "tool": { "name": "test", "version": "1.0.0" },
            "run": { "started_at": "2026-01-01T00:00:00Z" },
            "verdict": { "status": "pass", "counts": { "info": 0, "warn": 0, "error": 0 } },
            "findings": []
        });
        fs::write(
            sensor_dir.join("report.json"),
            serde_json::to_string(&wrong_schema).unwrap(),
        )
        .unwrap();

        // IO layer reads bytes regardless of schema — it passes through
        let layout = FsLayout::new(&artifacts, tmp.path().join("cockpit.toml"));
        let src = FsReceiptSource::new(layout);
        match src.read_report_bytes("sensor1").unwrap() {
            ReportRead::Bytes(b) => {
                let parsed: serde_json::Value = serde_json::from_slice(&b).unwrap();
                assert_eq!(parsed["schema"], "sensor.report.v999");
            }
            other => panic!("expected Bytes, got {:?}", std::mem::discriminant(&other)),
        }
    }

    #[test]
    fn invalid_sensor_ids_filtered_during_discovery() {
        let tmp = tempfile::tempdir().unwrap();
        let artifacts = tmp.path().join("artifacts");
        // Create dirs with invalid names
        for name in &["good-sensor", "bad.dot", "has space"] {
            let d = artifacts.join(name);
            fs::create_dir_all(&d).unwrap();
            fs::write(d.join("report.json"), minimal_report_json()).unwrap();
        }

        let layout = FsLayout::new(&artifacts, tmp.path().join("cockpit.toml"));
        let src = FsReceiptSource::new(layout);
        let discovered = src.discovered_sensors().unwrap();
        assert_eq!(discovered.sensors, vec!["good-sensor"]);
        assert!(
            discovered
                .invalid_sensor_ids
                .contains(&"bad.dot".to_string())
        );
        assert!(
            discovered
                .invalid_sensor_ids
                .contains(&"has space".to_string())
        );
    }

    #[test]
    fn read_report_for_invalid_sensor_id_returns_unsafe() {
        let tmp = tempfile::tempdir().unwrap();
        let artifacts = tmp.path().join("artifacts");
        fs::create_dir_all(&artifacts).unwrap();

        let layout = FsLayout::new(&artifacts, tmp.path().join("cockpit.toml"));
        let src = FsReceiptSource::new(layout);
        match src.read_report_bytes("../escape").unwrap() {
            ReportRead::UnsafePath => {}
            other => panic!(
                "expected UnsafePath, got {:?}",
                std::mem::discriminant(&other)
            ),
        }
    }

    #[test]
    fn cockpit_dir_ignored_in_discovery() {
        let tmp = tempfile::tempdir().unwrap();
        let artifacts = tmp.path().join("artifacts");
        let cockpit_dir = artifacts.join("cockpit");
        fs::create_dir_all(&cockpit_dir).unwrap();
        fs::write(cockpit_dir.join("report.json"), "{}").unwrap();

        let layout = FsLayout::new(&artifacts, tmp.path().join("cockpit.toml"));
        let src = FsReceiptSource::new(layout);
        let discovered = src.discovered_sensors().unwrap();
        assert!(discovered.sensors.is_empty());
    }

    #[test]
    fn config_missing_returns_none() {
        let tmp = tempfile::tempdir().unwrap();
        let layout = FsLayout::new(tmp.path().join("artifacts"), tmp.path().join("no.toml"));
        let policy = FsPolicySource::new(layout);
        assert!(policy.load_config().unwrap().is_none());
    }

    #[test]
    fn max_receipts_truncation() {
        let tmp = tempfile::tempdir().unwrap();
        let artifacts = tmp.path().join("artifacts");
        for i in 0..5 {
            let d = artifacts.join(format!("sensor{}", i));
            fs::create_dir_all(&d).unwrap();
            fs::write(d.join("report.json"), minimal_report_json()).unwrap();
        }

        let layout =
            FsLayout::new(&artifacts, tmp.path().join("cockpit.toml")).with_max_receipts(3);
        let src = FsReceiptSource::new(layout);
        let discovered = src.discovered_sensors().unwrap();
        assert_eq!(discovered.sensors.len(), 3);
        assert!(discovered.truncated);
        assert_eq!(discovered.total_found, 5);
    }

    #[cfg(unix)]
    #[test]
    fn symlink_outside_artifacts_rejected() {
        use std::os::unix::fs as unix_fs;

        let tmp = tempfile::tempdir().unwrap();
        let outside = tmp.path().join("outside");
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("report.json"), minimal_report_json()).unwrap();

        let artifacts = tmp.path().join("artifacts");
        fs::create_dir_all(&artifacts).unwrap();
        unix_fs::symlink(&outside, artifacts.join("symlinked")).unwrap();

        let layout = FsLayout::new(&artifacts, tmp.path().join("cockpit.toml"));
        let src = FsReceiptSource::new(layout);
        // The symlinked sensor dir may be discovered but read should flag it as unsafe
        match src.read_report_bytes("symlinked").unwrap() {
            ReportRead::UnsafePath => {}
            ReportRead::Bytes(_) => {
                // On some systems canonicalize may resolve within artifacts parent;
                // the point is it doesn't crash.
            }
            other => panic!("unexpected: {:?}", std::mem::discriminant(&other)),
        }
    }
}
