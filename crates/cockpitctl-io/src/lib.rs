//! Filesystem adapters for cockpitctl (ports implementation).
//!
//! This crate is the boundary between IO and the ingest use case.

use anyhow::{Context, Result};
use cockpitctl_ingest::{OutputSink, PolicySource, ReceiptSource};
use cockpitctl_types::CockpitConfig;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

#[derive(Clone)]
pub struct FsLayout {
    pub artifacts_dir: PathBuf,
    pub out_dir: PathBuf,
    pub config_path: PathBuf,
    pub max_receipt_bytes: usize,
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
        }
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
}

impl FsReceiptSource {
    pub fn new(layout: FsLayout) -> Self { Self { layout } }

    fn is_valid_sensor_id(id: &str) -> bool {
        // Avoid path traversal and weirdness. Keep this conservative.
        !id.is_empty() && !id.contains("..") && !id.contains('/') && !id.contains('\\')
    }
}

impl ReceiptSource for FsReceiptSource {
    fn discovered_sensors(&self) -> Result<Vec<String>> {
        let mut out = Vec::new();
        if !self.layout.artifacts_dir.exists() {
            // No artifacts dir: valid for local runs. Treat as empty.
            return Ok(out);
        }

        // Each direct child directory of artifacts/ is a sensor candidate.
        for entry in fs::read_dir(&self.layout.artifacts_dir)
            .with_context(|| format!("read artifacts dir {}", self.layout.artifacts_dir.display()))?
        {
            let entry = entry?;
            let path = entry.path();
            if !path.is_dir() { continue; }
            let name = entry.file_name().to_string_lossy().to_string();
            if name == "cockpit" { continue; }
            if !Self::is_valid_sensor_id(&name) { continue; }
            if self.layout.report_file(&name).exists() {
                out.push(name);
            }
        }

        out.sort();
        Ok(out)
    }

    fn read_report_bytes(&self, sensor_id: &str) -> Result<Option<Vec<u8>>> {
        if !Self::is_valid_sensor_id(sensor_id) {
            return Ok(None);
        }
        let p = self.layout.report_file(sensor_id);
        if !p.exists() {
            return Ok(None);
        }
        let meta = fs::metadata(&p)?;
        if meta.len() as usize > self.layout.max_receipt_bytes {
            anyhow::bail!(
                "receipt too large: {} bytes at {} (cap {})",
                meta.len(),
                p.display(),
                self.layout.max_receipt_bytes
            );
        }
        let bytes = fs::read(&p).with_context(|| format!("read receipt {}", p.display()))?;
        Ok(Some(bytes))
    }

    fn report_path(&self, sensor_id: &str) -> String {
        format!("artifacts/{}/report.json", sensor_id)
    }

    fn comment_path_if_present(&self, sensor_id: &str) -> Result<Option<String>> {
        if !Self::is_valid_sensor_id(sensor_id) {
            return Ok(None);
        }
        let p = self.layout.comment_file(sensor_id);
        if p.exists() {
            Ok(Some(format!("artifacts/{}/comment.md", sensor_id)))
        } else {
            Ok(None)
        }
    }
}

#[derive(Clone)]
pub struct FsPolicySource {
    layout: FsLayout,
}

impl FsPolicySource {
    pub fn new(layout: FsLayout) -> Self { Self { layout } }
}

impl PolicySource for FsPolicySource {
    fn load_config(&self) -> Result<Option<CockpitConfig>> {
        let p = &self.layout.config_path;
        if !p.exists() {
            return Ok(None);
        }
        let txt = fs::read_to_string(p).with_context(|| format!("read config {}", p.display()))?;
        let cfg: CockpitConfig = toml::from_str(&txt).with_context(|| format!("parse TOML {}", p.display()))?;
        Ok(Some(cfg))
    }
}

#[derive(Clone)]
pub struct FsOutputSink {
    layout: FsLayout,
}

impl FsOutputSink {
    pub fn new(layout: FsLayout) -> Self { Self { layout } }
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
}
