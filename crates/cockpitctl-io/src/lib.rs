//! Filesystem adapters for cockpitctl (ports implementation).
//!
//! This crate is the boundary between IO and the ingest use case.

use anyhow::{Context, Result};
use cockpitctl_ingest::{
    CommentRead, DiscoveredSensors, OutputSink, PlanRead, PolicySource, ReceiptSource, ReportRead,
    SchemaValidationResult, SchemaValidator,
};
use cockpitctl_types::{CockpitConfig, is_valid_sensor_id};
use jsonschema::Validator;
use std::fs;
use std::path::{Path, PathBuf};

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

// ============================================================================
// Post-processor hook runner
// ============================================================================

/// Output from a post-processor hook.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct PostProcessOutput {
    #[serde(default)]
    pub comment_sections: Vec<CommentSection>,
    #[serde(default)]
    pub files: Vec<OutputFile>,
}

/// A comment section contributed by a hook.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct CommentSection {
    pub name: String,
    pub content: String,
    #[serde(default)]
    pub order: i32,
}

/// A file contributed by a hook.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct OutputFile {
    pub name: String,
    #[serde(with = "base64_bytes", default)]
    pub content: Vec<u8>,
}

mod base64_bytes {
    use base64::Engine;
    use serde::{Deserialize, Deserializer};

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        // Accept either base64-encoded string or raw string bytes.
        let s = String::deserialize(deserializer)?;
        // Try base64 decode first; fall back to raw UTF-8 bytes.
        match base64_decode(&s) {
            Some(bytes) => Ok(bytes),
            None => Ok(s.into_bytes()),
        }
    }

    fn base64_decode(s: &str) -> Option<Vec<u8>> {
        let s = s.trim();
        if s.is_empty() {
            return Some(Vec::new());
        }
        if s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'+' || b == b'/' || b == b'=')
        {
            base64::engine::general_purpose::STANDARD.decode(s).ok()
        } else {
            None
        }
    }
}

/// Run post-processor hooks and collect their outputs.
pub fn run_hooks(
    hooks: &[cockpitctl_types::HookConfig],
    report_json: &str,
    output_sink: &impl OutputSink,
) -> Result<Vec<CommentSection>> {
    let mut all_sections = Vec::new();

    for hook in hooks {
        match run_single_hook(hook, report_json) {
            Ok(output) => {
                for file in &output.files {
                    output_sink.write_extra_file(&file.name, &file.content)?;
                }
                all_sections.extend(output.comment_sections);
            }
            Err(e) => {
                eprintln!("cockpitctl: hook `{}` failed: {:#}", hook.name, e);
            }
        }
    }

    // Sort sections by (order, name) for determinism.
    all_sections.sort_by(|a, b| (a.order, &a.name).cmp(&(b.order, &b.name)));
    Ok(all_sections)
}

fn run_single_hook(
    hook: &cockpitctl_types::HookConfig,
    report_json: &str,
) -> Result<PostProcessOutput> {
    use std::io::{Read, Write};
    use std::process::{Command, Stdio};
    use std::time::Duration;
    use wait_timeout::ChildExt;

    let parts: Vec<&str> = hook.command.split_whitespace().collect();
    if parts.is_empty() {
        anyhow::bail!("hook `{}` has empty command", hook.name);
    }

    let mut child = Command::new(parts[0])
        .args(&parts[1..])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("spawn hook `{}`", hook.name))?;

    // Write report JSON to stdin.
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(report_json.as_bytes())
            .with_context(|| format!("write stdin for hook `{}`", hook.name))?;
    }

    // Take ownership of stdout/stderr handles and drain them in threads
    // to prevent pipe-buffer deadlock when the child writes >64KB.
    let mut stdout_handle = child.stdout.take();
    let mut stderr_handle = child.stderr.take();

    let stdout_thread = std::thread::spawn(move || -> Vec<u8> {
        let mut buf = Vec::new();
        if let Some(ref mut h) = stdout_handle {
            let _ = h.read_to_end(&mut buf);
        }
        buf
    });

    let stderr_thread = std::thread::spawn(move || -> Vec<u8> {
        let mut buf = Vec::new();
        if let Some(ref mut h) = stderr_handle {
            let _ = h.read_to_end(&mut buf);
        }
        buf
    });

    let timeout = Duration::from_millis(hook.timeout_ms);
    let status = match child
        .wait_timeout(timeout)
        .with_context(|| format!("wait for hook `{}`", hook.name))?
    {
        Some(status) => status,
        None => {
            // Timeout: kill the child and reap the zombie.
            let _ = child.kill();
            let _ = child.wait();
            anyhow::bail!("hook `{}` timed out after {}ms", hook.name, hook.timeout_ms);
        }
    };

    let stdout_bytes = stdout_thread.join().unwrap_or_default();
    let stderr_bytes = stderr_thread.join().unwrap_or_default();

    // Log stderr for diagnostics.
    let stderr = String::from_utf8_lossy(&stderr_bytes);
    if !stderr.is_empty() {
        eprintln!("cockpitctl: hook `{}` stderr: {}", hook.name, stderr.trim());
    }

    if !status.success() {
        anyhow::bail!("hook `{}` exited with status {}", hook.name, status);
    }

    let result: PostProcessOutput = serde_json::from_slice(&stdout_bytes)
        .with_context(|| format!("parse hook `{}` output", hook.name))?;

    Ok(result)
}

/// JSON Schema validator for sensor reports.
///
/// Validates receipts against the `sensor.report.v1` JSON schema.
pub struct JsonSchemaValidator {
    validator: Validator,
}

impl JsonSchemaValidator {
    /// Create a new validator by loading the schema from a file path.
    pub fn from_file(schema_path: impl AsRef<std::path::Path>) -> Result<Self> {
        let schema_str = fs::read_to_string(schema_path.as_ref())
            .with_context(|| format!("read schema file {}", schema_path.as_ref().display()))?;
        let schema: serde_json::Value = serde_json::from_str(&schema_str).with_context(|| {
            format!("parse schema JSON from {}", schema_path.as_ref().display())
        })?;
        let validator =
            Validator::new(&schema).map_err(|e| anyhow::anyhow!("invalid JSON schema: {}", e))?;
        Ok(Self { validator })
    }

    /// Create a new validator from a JSON schema value.
    pub fn from_schema(schema: &serde_json::Value) -> Result<Self> {
        let validator =
            Validator::new(schema).map_err(|e| anyhow::anyhow!("invalid JSON schema: {}", e))?;
        Ok(Self { validator })
    }

    /// Create a new validator using the embedded sensor.report.v1 schema.
    pub fn sensor_report_v1() -> Result<Self> {
        const SCHEMA: &str = cockpitctl_types::SENSOR_REPORT_V1_SCHEMA_JSON;
        let schema: serde_json::Value =
            serde_json::from_str(SCHEMA).context("parse embedded sensor.report.v1 schema")?;
        Self::from_schema(&schema)
    }

    /// Create a new validator using the embedded cockpit.report.v1 schema.
    pub fn cockpit_report_v1() -> Result<Self> {
        const SCHEMA: &str = cockpitctl_types::COCKPIT_REPORT_V1_SCHEMA_JSON;
        let schema: serde_json::Value =
            serde_json::from_str(SCHEMA).context("parse embedded cockpit.report.v1 schema")?;
        Self::from_schema(&schema)
    }
}

impl SchemaValidator for JsonSchemaValidator {
    fn validate_receipt(&self, bytes: &[u8]) -> Result<SchemaValidationResult> {
        // First, parse the JSON. If parsing fails, return Invalid with the parse error.
        // This ensures ingest survivability: malformed JSON produces a finding, not a runtime abort.
        let value: serde_json::Value = match serde_json::from_slice(bytes) {
            Ok(v) => v,
            Err(e) => {
                // Malformed JSON: return Invalid with the parse error message
                return Ok(SchemaValidationResult::Invalid(vec![format!(
                    "malformed JSON: {}",
                    e
                )]));
            }
        };

        // Validate against the schema.
        let result = self.validator.validate(&value);

        if result.is_ok() {
            Ok(SchemaValidationResult::Valid)
        } else {
            let errors: Vec<String> = self
                .validator
                .iter_errors(&value)
                .map(|e| {
                    let path = e.instance_path().to_string();
                    if path.is_empty() {
                        e.to_string()
                    } else {
                        format!("{}: {}", path, e)
                    }
                })
                .collect();
            Ok(SchemaValidationResult::Invalid(errors))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::TempDir;

    /// Minimal valid sensor report for testing.
    fn minimal_report() -> &'static str {
        r#"{
            "schema": "sensor.report.v1",
            "tool": { "name": "test", "version": "1.0" },
            "run": { "started_at": "2026-01-01T00:00:00Z" },
            "verdict": { "status": "pass", "counts": { "info": 0, "warn": 0, "error": 0 } }
        }"#
    }

    #[test]
    fn discovered_sensors_respects_max_receipts_cap() {
        let tmp = TempDir::new().unwrap();
        let artifacts = tmp.path().join("artifacts");

        // Create 5 sensor directories with reports.
        for i in 0..5 {
            let sensor_dir = artifacts.join(format!("sensor_{:02}", i));
            fs::create_dir_all(&sensor_dir).unwrap();
            fs::write(sensor_dir.join("report.json"), minimal_report()).unwrap();
        }

        // Use a cap of 3.
        let layout =
            FsLayout::new(&artifacts, tmp.path().join("cockpit.toml")).with_max_receipts(3);
        let source = FsReceiptSource::new(layout);
        let result = source.discovered_sensors().unwrap();

        // Should return exactly 3 sensors, truncated flag true, total 5.
        assert_eq!(result.sensors.len(), 3);
        assert!(result.truncated);
        assert_eq!(result.total_found, 5);
        assert!(result.invalid_sensor_ids.is_empty());

        // Should be the first 3 in lexical order.
        assert_eq!(result.sensors, vec!["sensor_00", "sensor_01", "sensor_02"]);
    }

    #[test]
    fn discovered_sensors_no_truncation_when_under_cap() {
        let tmp = TempDir::new().unwrap();
        let artifacts = tmp.path().join("artifacts");

        // Create 2 sensor directories.
        for i in 0..2 {
            let sensor_dir = artifacts.join(format!("sensor_{}", i));
            fs::create_dir_all(&sensor_dir).unwrap();
            fs::write(sensor_dir.join("report.json"), minimal_report()).unwrap();
        }

        // Use default cap (100).
        let layout = FsLayout::new(&artifacts, tmp.path().join("cockpit.toml"));
        let source = FsReceiptSource::new(layout);
        let result = source.discovered_sensors().unwrap();

        assert_eq!(result.sensors.len(), 2);
        assert!(!result.truncated);
        assert_eq!(result.total_found, 2);
        assert!(result.invalid_sensor_ids.is_empty());
    }

    #[test]
    fn discovered_sensors_cap_at_exact_limit_is_not_truncated() {
        let tmp = TempDir::new().unwrap();
        let artifacts = tmp.path().join("artifacts");

        // Create exactly 3 sensors with cap of 3.
        for i in 0..3 {
            let sensor_dir = artifacts.join(format!("sensor_{}", i));
            fs::create_dir_all(&sensor_dir).unwrap();
            fs::write(sensor_dir.join("report.json"), minimal_report()).unwrap();
        }

        let layout =
            FsLayout::new(&artifacts, tmp.path().join("cockpit.toml")).with_max_receipts(3);
        let source = FsReceiptSource::new(layout);
        let result = source.discovered_sensors().unwrap();

        assert_eq!(result.sensors.len(), 3);
        assert!(!result.truncated);
        assert_eq!(result.total_found, 3);
        assert!(result.invalid_sensor_ids.is_empty());
    }

    #[test]
    fn default_max_receipts_is_100() {
        let layout = FsLayout::new("/tmp/artifacts", "/tmp/cockpit.toml");
        assert_eq!(layout.max_receipts, 100);
        assert_eq!(DEFAULT_MAX_RECEIPTS, 100);
    }

    // -------------------------------------------------------------------------
    // JsonSchemaValidator tests
    // -------------------------------------------------------------------------

    /// Minimal valid sensor report with all required fields and the findings array.
    fn valid_sensor_report() -> &'static str {
        r#"{
            "schema": "sensor.report.v1",
            "tool": { "name": "test-tool", "version": "1.0.0" },
            "run": { "started_at": "2026-01-15T10:30:00Z" },
            "verdict": { "status": "pass", "counts": { "info": 0, "warn": 0, "error": 0 } },
            "findings": []
        }"#
    }

    #[test]
    fn json_schema_validator_accepts_valid_receipt() {
        let validator = JsonSchemaValidator::sensor_report_v1().unwrap();
        let result = validator
            .validate_receipt(valid_sensor_report().as_bytes())
            .unwrap();
        assert!(matches!(result, SchemaValidationResult::Valid));
    }

    #[test]
    fn json_schema_validator_accepts_valid_receipt_with_findings() {
        let report = r#"{
            "schema": "sensor.report.v1",
            "tool": { "name": "clippy", "version": "0.1.0" },
            "run": { "started_at": "2026-01-15T10:30:00Z" },
            "verdict": { "status": "warn", "counts": { "info": 0, "warn": 1, "error": 0 } },
            "findings": [
                {
                    "severity": "warn",
                    "code": "clippy::unwrap_used",
                    "message": "used `unwrap()` on a `Result` value"
                }
            ]
        }"#;
        let validator = JsonSchemaValidator::sensor_report_v1().unwrap();
        let result = validator.validate_receipt(report.as_bytes()).unwrap();
        assert!(matches!(result, SchemaValidationResult::Valid));
    }

    #[test]
    fn json_schema_validator_accepts_valid_receipt_with_full_finding() {
        let report = r#"{
            "schema": "sensor.report.v1",
            "tool": { "name": "clippy", "version": "0.1.0", "commit": "abc123" },
            "run": {
                "started_at": "2026-01-15T10:30:00Z",
                "ended_at": "2026-01-15T10:31:00Z",
                "duration_ms": 60000
            },
            "verdict": { "status": "warn", "counts": { "info": 0, "warn": 1, "error": 0 } },
            "findings": [
                {
                    "severity": "warn",
                    "code": "clippy::unwrap_used",
                    "message": "used `unwrap()` on a `Result` value",
                    "location": { "path": "src/main.rs", "line": 42, "col": 10 },
                    "help": "Consider using `expect()` or `?` instead",
                    "url": "https://rust-lang.github.io/rust-clippy/",
                    "fingerprint": "abc123def456",
                    "check_id": "unwrap-check"
                }
            ],
            "data": { "custom": "payload" }
        }"#;
        let validator = JsonSchemaValidator::sensor_report_v1().unwrap();
        let result = validator.validate_receipt(report.as_bytes()).unwrap();
        assert!(matches!(result, SchemaValidationResult::Valid));
    }

    #[test]
    fn json_schema_validator_rejects_missing_required_fields() {
        // Missing "schema" field
        let report = r#"{
            "tool": { "name": "test", "version": "1.0" },
            "run": { "started_at": "2026-01-15T10:30:00Z" },
            "verdict": { "status": "pass", "counts": { "info": 0, "warn": 0, "error": 0 } },
            "findings": []
        }"#;
        let validator = JsonSchemaValidator::sensor_report_v1().unwrap();
        let invalid_result = validator.validate_receipt(report.as_bytes()).unwrap();
        let valid_result = validator
            .validate_receipt(valid_sensor_report().as_bytes())
            .unwrap();

        let mut saw_invalid = false;
        let mut saw_valid = false;
        for result in [invalid_result, valid_result] {
            match result {
                SchemaValidationResult::Invalid(errors) => {
                    saw_invalid = true;
                    assert!(!errors.is_empty(), "should have at least one error");
                    let joined = errors.join(" ");
                    let has_schema = joined.contains("schema");
                    let has_required = joined.contains("required");
                    assert!(has_schema | has_required);
                }
                SchemaValidationResult::Valid => {
                    saw_valid = true;
                }
            }
        }
        assert!(saw_invalid);
        assert!(saw_valid);
    }

    #[test]
    fn json_schema_validator_rejects_invalid_verdict_status() {
        let report = r#"{
            "schema": "sensor.report.v1",
            "tool": { "name": "test", "version": "1.0" },
            "run": { "started_at": "2026-01-15T10:30:00Z" },
            "verdict": { "status": "invalid_status", "counts": { "info": 0, "warn": 0, "error": 0 } },
            "findings": []
        }"#;
        let validator = JsonSchemaValidator::sensor_report_v1().unwrap();
        let invalid_result = validator.validate_receipt(report.as_bytes()).unwrap();
        let valid_result = validator
            .validate_receipt(valid_sensor_report().as_bytes())
            .unwrap();

        let mut saw_invalid = false;
        let mut saw_valid = false;
        for result in [invalid_result, valid_result] {
            match result {
                SchemaValidationResult::Invalid(errors) => {
                    saw_invalid = true;
                    assert!(!errors.is_empty(), "should have at least one error");
                    // Should mention the invalid enum value
                    let joined = errors.join(" ");
                    let has_invalid = joined.contains("invalid_status");
                    let has_status = joined.contains("status");
                    let has_enum = joined.contains("enum");
                    assert!(has_invalid | has_status | has_enum);
                }
                SchemaValidationResult::Valid => {
                    saw_valid = true;
                }
            }
        }
        assert!(saw_invalid);
        assert!(saw_valid);
    }

    #[test]
    fn json_schema_validator_rejects_invalid_finding_severity() {
        let report = r#"{
            "schema": "sensor.report.v1",
            "tool": { "name": "test", "version": "1.0" },
            "run": { "started_at": "2026-01-15T10:30:00Z" },
            "verdict": { "status": "pass", "counts": { "info": 0, "warn": 0, "error": 0 } },
            "findings": [
                {
                    "severity": "critical",
                    "code": "test-001",
                    "message": "test message"
                }
            ]
        }"#;
        let validator = JsonSchemaValidator::sensor_report_v1().unwrap();
        let invalid_result = validator.validate_receipt(report.as_bytes()).unwrap();
        let valid_result = validator
            .validate_receipt(valid_sensor_report().as_bytes())
            .unwrap();

        let mut saw_invalid = false;
        let mut saw_valid = false;
        for result in [invalid_result, valid_result] {
            match result {
                SchemaValidationResult::Invalid(errors) => {
                    saw_invalid = true;
                    assert!(!errors.is_empty());
                }
                SchemaValidationResult::Valid => {
                    saw_valid = true;
                }
            }
        }
        assert!(saw_invalid);
        assert!(saw_valid);
    }

    #[test]
    fn json_schema_validator_rejects_finding_missing_required_fields() {
        // Finding missing "code" field
        let report = r#"{
            "schema": "sensor.report.v1",
            "tool": { "name": "test", "version": "1.0" },
            "run": { "started_at": "2026-01-15T10:30:00Z" },
            "verdict": { "status": "pass", "counts": { "info": 0, "warn": 0, "error": 0 } },
            "findings": [
                {
                    "severity": "warn",
                    "message": "test message"
                }
            ]
        }"#;
        let validator = JsonSchemaValidator::sensor_report_v1().unwrap();
        let invalid_result = validator.validate_receipt(report.as_bytes()).unwrap();
        let valid_result = validator
            .validate_receipt(valid_sensor_report().as_bytes())
            .unwrap();

        let mut saw_invalid = false;
        let mut saw_valid = false;
        for result in [invalid_result, valid_result] {
            match result {
                SchemaValidationResult::Invalid(errors) => {
                    saw_invalid = true;
                    assert!(!errors.is_empty(), "should have at least one error");
                    let joined = errors.join(" ");
                    let has_code = joined.contains("code");
                    let has_required = joined.contains("required");
                    assert!(has_code | has_required);
                }
                SchemaValidationResult::Valid => {
                    saw_valid = true;
                }
            }
        }
        assert!(saw_invalid);
        assert!(saw_valid);
    }

    #[test]
    fn json_schema_validator_rejects_additional_properties_at_root() {
        let report = r#"{
            "schema": "sensor.report.v1",
            "tool": { "name": "test", "version": "1.0" },
            "run": { "started_at": "2026-01-15T10:30:00Z" },
            "verdict": { "status": "pass", "counts": { "info": 0, "warn": 0, "error": 0 } },
            "findings": [],
            "extra_field": "not allowed"
        }"#;
        let validator = JsonSchemaValidator::sensor_report_v1().unwrap();
        let invalid_result = validator.validate_receipt(report.as_bytes()).unwrap();
        let valid_result = validator
            .validate_receipt(valid_sensor_report().as_bytes())
            .unwrap();

        let mut saw_invalid = false;
        let mut saw_valid = false;
        for result in [invalid_result, valid_result] {
            match result {
                SchemaValidationResult::Invalid(errors) => {
                    saw_invalid = true;
                    assert!(!errors.is_empty());
                }
                SchemaValidationResult::Valid => {
                    saw_valid = true;
                }
            }
        }
        assert!(saw_invalid);
        assert!(saw_valid);
    }

    #[test]
    fn json_schema_validator_returns_invalid_for_malformed_json() {
        let invalid_json = b"{ not valid json }";
        let validator = JsonSchemaValidator::sensor_report_v1().unwrap();
        let result = validator.validate_receipt(invalid_json).unwrap();
        match result {
            SchemaValidationResult::Invalid(errors) => {
                assert!(!errors.is_empty(), "should have validation errors");
                let error_msg = errors.join(" ");
                assert!(
                    error_msg.contains("malformed JSON") || error_msg.contains("JSON"),
                    "error should mention malformed JSON or JSON, got: {}",
                    error_msg
                );
            }
            SchemaValidationResult::Valid => {
                panic!("should return Invalid for malformed JSON");
            }
        }
    }

    #[test]
    fn json_schema_validator_rejects_empty_code_in_finding() {
        // code has minLength: 1
        let report = r#"{
            "schema": "sensor.report.v1",
            "tool": { "name": "test", "version": "1.0" },
            "run": { "started_at": "2026-01-15T10:30:00Z" },
            "verdict": { "status": "pass", "counts": { "info": 0, "warn": 0, "error": 0 } },
            "findings": [
                {
                    "severity": "warn",
                    "code": "",
                    "message": "test message"
                }
            ]
        }"#;
        let validator = JsonSchemaValidator::sensor_report_v1().unwrap();
        let invalid_result = validator.validate_receipt(report.as_bytes()).unwrap();
        let valid_result = validator
            .validate_receipt(valid_sensor_report().as_bytes())
            .unwrap();

        let mut saw_invalid = false;
        let mut saw_valid = false;
        for result in [invalid_result, valid_result] {
            match result {
                SchemaValidationResult::Invalid(errors) => {
                    saw_invalid = true;
                    assert!(!errors.is_empty());
                }
                SchemaValidationResult::Valid => {
                    saw_valid = true;
                }
            }
        }
        assert!(saw_invalid);
        assert!(saw_valid);
    }

    #[test]
    fn json_schema_validator_rejects_negative_line_number() {
        // line has minimum: 1
        let report = r#"{
            "schema": "sensor.report.v1",
            "tool": { "name": "test", "version": "1.0" },
            "run": { "started_at": "2026-01-15T10:30:00Z" },
            "verdict": { "status": "pass", "counts": { "info": 0, "warn": 0, "error": 0 } },
            "findings": [
                {
                    "severity": "warn",
                    "code": "test",
                    "message": "test",
                    "location": { "path": "src/main.rs", "line": 0 }
                }
            ]
        }"#;
        let validator = JsonSchemaValidator::sensor_report_v1().unwrap();
        let invalid_result = validator.validate_receipt(report.as_bytes()).unwrap();
        let valid_result = validator
            .validate_receipt(valid_sensor_report().as_bytes())
            .unwrap();

        let mut saw_invalid = false;
        let mut saw_valid = false;
        for result in [invalid_result, valid_result] {
            match result {
                SchemaValidationResult::Invalid(errors) => {
                    saw_invalid = true;
                    assert!(!errors.is_empty());
                }
                SchemaValidationResult::Valid => {
                    saw_valid = true;
                }
            }
        }
        assert!(saw_invalid);
        assert!(saw_valid);
    }

    // -------------------------------------------------------------------------
    // FsReceiptSource / FsPolicySource / FsOutputSink tests
    // -------------------------------------------------------------------------

    #[test]
    fn discovered_sensors_missing_artifacts_dir_returns_empty() {
        let tmp = TempDir::new().unwrap();
        let artifacts = tmp.path().join("missing_artifacts");
        let layout = FsLayout::new(&artifacts, tmp.path().join("cockpit.toml"));
        let source = FsReceiptSource::new(layout);

        let result = source.discovered_sensors().unwrap();
        assert!(result.sensors.is_empty());
        assert!(!result.truncated);
        assert_eq!(result.total_found, 0);
    }

    #[test]
    fn discovered_sensors_read_dir_error_includes_context() {
        let tmp = TempDir::new().unwrap();
        let artifacts = tmp.path().join("artifacts");
        fs::write(&artifacts, "not a dir").unwrap();

        let layout = FsLayout::new(&artifacts, tmp.path().join("cockpit.toml"));
        let source = FsReceiptSource::new(layout);

        let err = source
            .discovered_sensors()
            .err()
            .expect("expected read_dir error");
        let msg = format!("{:#}", err);
        assert!(msg.contains("read artifacts dir"));
    }

    #[test]
    fn report_path_formats_relative_location() {
        let layout = FsLayout::new("artifacts", "cockpit.toml");
        let source = FsReceiptSource::new(layout);
        assert_eq!(source.report_path("sensor"), "artifacts/sensor/report.json");
    }

    #[test]
    fn discovered_sensors_records_invalid_ids() {
        let tmp = TempDir::new().unwrap();
        let artifacts = tmp.path().join("artifacts");
        fs::create_dir_all(&artifacts).unwrap();

        let good = artifacts.join("good_sensor");
        fs::create_dir_all(&good).unwrap();
        fs::write(good.join("report.json"), minimal_report()).unwrap();

        let bad = artifacts.join("bad.id");
        fs::create_dir_all(&bad).unwrap();
        fs::write(bad.join("report.json"), minimal_report()).unwrap();

        let layout = FsLayout::new(&artifacts, tmp.path().join("cockpit.toml"));
        let source = FsReceiptSource::new(layout);

        let result = source.discovered_sensors().unwrap();
        assert_eq!(result.sensors, vec!["good_sensor"]);
        assert_eq!(result.invalid_sensor_ids, vec!["bad.id"]);
    }

    #[test]
    fn read_report_bytes_missing_and_unsafe() {
        let tmp = TempDir::new().unwrap();
        let artifacts = tmp.path().join("artifacts");
        fs::create_dir_all(&artifacts).unwrap();

        let layout = FsLayout::new(&artifacts, tmp.path().join("cockpit.toml"));
        let source = FsReceiptSource::new(layout);

        let missing = source.read_report_bytes("missing").unwrap();
        assert!(matches!(missing, ReportRead::Missing));

        let unsafe_path = source.read_report_bytes("bad..id").unwrap();
        assert!(matches!(unsafe_path, ReportRead::UnsafePath));
    }

    #[test]
    fn read_report_bytes_oversized() {
        let tmp = TempDir::new().unwrap();
        let artifacts = tmp.path().join("artifacts");
        let sensor_dir = artifacts.join("big");
        fs::create_dir_all(&sensor_dir).unwrap();
        fs::write(sensor_dir.join("report.json"), b"0123456789ABCDEF").unwrap();

        let mut layout = FsLayout::new(&artifacts, tmp.path().join("cockpit.toml"));
        layout.max_receipt_bytes = 4;
        let source = FsReceiptSource::new(layout);

        let result = source.read_report_bytes("big").unwrap();
        assert!(matches!(result, ReportRead::Oversized { .. }));
    }

    #[test]
    fn unsafe_paths_are_rejected_even_when_files_exist() {
        let tmp = TempDir::new().unwrap();
        let artifacts = tmp.path().join("artifacts");
        let sensor_dir = artifacts.join("sensor");
        fs::create_dir_all(&sensor_dir).unwrap();
        fs::write(sensor_dir.join("report.json"), minimal_report()).unwrap();
        fs::write(sensor_dir.join("comment.md"), "hello").unwrap();

        let layout = FsLayout::new(&artifacts, tmp.path().join("cockpit.toml"));
        let mut source = FsReceiptSource::new(layout);
        source.artifacts_root = tmp.path().join("somewhere_else");

        let report = source.read_report_bytes("sensor").unwrap();
        assert!(matches!(report, ReportRead::UnsafePath));

        let comment = source.comment_path_if_present("sensor").unwrap();
        assert!(matches!(comment, CommentRead::UnsafePath));
    }

    #[test]
    fn comment_path_if_present_respects_unsafe_and_present() {
        let tmp = TempDir::new().unwrap();
        let artifacts = tmp.path().join("artifacts");
        let sensor_dir = artifacts.join("sensor");
        fs::create_dir_all(&sensor_dir).unwrap();
        fs::write(sensor_dir.join("comment.md"), "hello").unwrap();

        let layout = FsLayout::new(&artifacts, tmp.path().join("cockpit.toml"));
        let source = FsReceiptSource::new(layout);

        let present = source.comment_path_if_present("sensor").unwrap();
        assert!(matches!(present, CommentRead::Present(p) if p == "artifacts/sensor/comment.md"));

        let unsafe_path = source.comment_path_if_present("bad..id").unwrap();
        assert!(matches!(unsafe_path, CommentRead::UnsafePath));
    }

    #[test]
    fn policy_source_loads_config_and_missing_returns_none() {
        let tmp = TempDir::new().unwrap();
        let config_path = tmp.path().join("cockpit.toml");
        let layout = FsLayout::new(tmp.path().join("artifacts"), &config_path);
        let policy = FsPolicySource::new(layout.clone());

        // Missing file => None
        let missing = policy.load_config().unwrap();
        assert!(missing.is_none());

        // Write a minimal config
        fs::write(
            &config_path,
            r#"[policy]
schema_validation = "lax"

[sensors.alpha]
blocking = true
missing = "warn"
"#,
        )
        .unwrap();

        let loaded = policy.load_config().unwrap().expect("config");
        assert!(loaded.sensors.contains_key("alpha"));
        assert!(matches!(
            loaded.policy.schema_validation,
            cockpitctl_types::SchemaValidation::Lax
        ));
    }

    #[test]
    fn output_sink_writes_report_and_comment() {
        let tmp = TempDir::new().unwrap();
        let artifacts = tmp.path().join("artifacts");
        let layout = FsLayout::new(&artifacts, tmp.path().join("cockpit.toml"));
        let sink = FsOutputSink::new(layout);

        sink.write_cockpit_report("{\"ok\":true}\n").unwrap();
        sink.write_cockpit_comment("# comment\n").unwrap();

        let report_path = artifacts.join("cockpit").join("report.json");
        let comment_path = artifacts.join("cockpit").join("comment.md");
        assert_eq!(fs::read_to_string(report_path).unwrap(), "{\"ok\":true}\n");
        assert_eq!(fs::read_to_string(comment_path).unwrap(), "# comment\n");
    }

    #[test]
    fn json_schema_validator_from_file_validates() {
        let tmp = TempDir::new().unwrap();
        let schema_path = tmp.path().join("sensor.schema.json");
        fs::write(
            schema_path.clone(),
            cockpitctl_types::SENSOR_REPORT_V1_SCHEMA_JSON,
        )
        .unwrap();

        let validator = JsonSchemaValidator::from_file(&schema_path).unwrap();
        let result = validator
            .validate_receipt(valid_sensor_report().as_bytes())
            .unwrap();
        assert!(matches!(result, SchemaValidationResult::Valid));
    }

    #[test]
    fn json_schema_validator_from_file_missing_path_errors() {
        let tmp = TempDir::new().unwrap();
        let missing = tmp.path().join("missing.schema.json");
        let err = JsonSchemaValidator::from_file(&missing)
            .err()
            .expect("expected error");
        let msg = format!("{:#}", err);
        assert!(msg.contains("read schema file"));
    }

    #[test]
    fn json_schema_validator_from_file_invalid_json_errors() {
        let tmp = TempDir::new().unwrap();
        let schema_path = tmp.path().join("bad.schema.json");
        fs::write(&schema_path, "{").unwrap();
        let err = JsonSchemaValidator::from_file(&schema_path)
            .err()
            .expect("expected error");
        let msg = format!("{:#}", err);
        assert!(msg.contains("parse schema JSON"));
    }

    #[test]
    fn json_schema_validator_cockpit_report_accepts_minimal_report() {
        let cfg = cockpitctl_types::CockpitConfig::default();
        let report = cockpitctl_types::CockpitReport {
            schema: "cockpit.report.v1".to_string(),
            tool: cockpitctl_types::ToolInfo {
                name: "cockpitctl".to_string(),
                version: "0.2.0".to_string(),
                commit: None,
            },
            run: cockpitctl_types::RunInfo {
                started_at: "2026-02-01T00:00:00Z".to_string(),
                ended_at: None,
                duration_ms: None,
                host: None,
                git: None,
                ci: None,
                capabilities: std::collections::BTreeMap::new(),
            },
            verdict: cockpitctl_types::Verdict {
                status: cockpitctl_types::VerdictStatus::Pass,
                counts: cockpitctl_types::VerdictCounts::default(),
                reasons: vec![],
            },
            sensors: vec![],
            highlights: vec![],
            policy: cockpitctl_types::PolicySnapshot {
                warn_is_fail: cfg.policy.warn_is_fail,
                max_highlights: cfg.policy.max_highlights,
                max_per_sensor_findings: cfg.policy.max_per_sensor_findings,
                max_annotations: cfg.policy.max_annotations,
                section_order: cfg.policy.section_order.clone(),
                sensors: vec![],
            },
            data: None,
        };

        let json = serde_json::to_vec(&report).unwrap();
        let validator = JsonSchemaValidator::cockpit_report_v1().unwrap();
        let result = validator.validate_receipt(&json).unwrap();
        assert!(matches!(result, SchemaValidationResult::Valid));
    }

    #[test]
    fn canonicalize_root_with_relative_path_returns_absolute() {
        let rel = PathBuf::from("relative_artifacts_dir");
        let abs = super::canonicalize_root(&rel);
        assert!(abs.is_absolute());
        assert!(abs.ends_with(&rel));
    }

    #[test]
    fn canonicalize_root_with_existing_path_uses_canonicalize() {
        let tmp = TempDir::new().unwrap();
        let existing = tmp.path().join("artifacts");
        fs::create_dir_all(&existing).unwrap();

        let canonical = super::canonicalize_root(&existing);
        assert!(canonical.is_absolute());
        let expected = fs::canonicalize(&existing).unwrap();
        assert_eq!(canonical, expected);
    }

    #[test]
    fn is_safe_path_returns_false_when_canonicalize_fails() {
        let tmp = TempDir::new().unwrap();
        let artifacts = tmp.path().join("artifacts");
        fs::create_dir_all(&artifacts).unwrap();

        let layout = FsLayout::new(&artifacts, tmp.path().join("cockpit.toml"));
        let source = FsReceiptSource::new(layout);

        let missing = tmp.path().join("does_not_exist").join("report.json");
        assert!(!source.is_safe_path(&missing));
    }

    #[test]
    fn discovered_sensors_skips_non_dirs_cockpit_and_missing_report() {
        let tmp = TempDir::new().unwrap();
        let artifacts = tmp.path().join("artifacts");
        fs::create_dir_all(&artifacts).unwrap();

        // Non-directory entry should be ignored.
        fs::write(artifacts.join("not_a_dir"), "noise").unwrap();

        // cockpit dir should be ignored.
        let cockpit_dir = artifacts.join("cockpit");
        fs::create_dir_all(&cockpit_dir).unwrap();
        fs::write(cockpit_dir.join("report.json"), minimal_report()).unwrap();

        // Directory without report.json should be ignored.
        fs::create_dir_all(artifacts.join("no_report")).unwrap();

        // Invalid sensor id should be recorded even without report.json.
        fs::create_dir_all(artifacts.join("bad.id")).unwrap();

        // Valid sensor with report.json should be discovered.
        let ok = artifacts.join("sensor_ok");
        fs::create_dir_all(&ok).unwrap();
        fs::write(ok.join("report.json"), minimal_report()).unwrap();

        let layout = FsLayout::new(&artifacts, tmp.path().join("cockpit.toml"));
        let source = FsReceiptSource::new(layout);
        let result = source.discovered_sensors().unwrap();

        assert_eq!(result.sensors, vec!["sensor_ok"]);
        assert_eq!(result.invalid_sensor_ids, vec!["bad.id"]);
        assert!(!result.truncated);
        assert_eq!(result.total_found, 1);
    }

    #[test]
    fn read_report_bytes_returns_bytes_for_existing_report() {
        let tmp = TempDir::new().unwrap();
        let artifacts = tmp.path().join("artifacts");
        let sensor_dir = artifacts.join("sensor");
        fs::create_dir_all(&sensor_dir).unwrap();
        fs::write(sensor_dir.join("report.json"), minimal_report()).unwrap();

        let layout = FsLayout::new(&artifacts, tmp.path().join("cockpit.toml"));
        let source = FsReceiptSource::new(layout);

        let result = source.read_report_bytes("sensor").unwrap();
        assert!(matches!(
            result,
            ReportRead::Bytes(ref bytes) if bytes == minimal_report().as_bytes()
        ));
    }

    #[test]
    fn comment_path_if_present_missing_returns_missing() {
        let tmp = TempDir::new().unwrap();
        let artifacts = tmp.path().join("artifacts");
        let sensor_dir = artifacts.join("sensor");
        fs::create_dir_all(&sensor_dir).unwrap();

        let layout = FsLayout::new(&artifacts, tmp.path().join("cockpit.toml"));
        let source = FsReceiptSource::new(layout);

        let missing = source.comment_path_if_present("sensor").unwrap();
        assert!(matches!(missing, CommentRead::Missing));
    }

    #[test]
    fn policy_source_invalid_toml_errors() {
        let tmp = TempDir::new().unwrap();
        let config_path = tmp.path().join("cockpit.toml");
        fs::write(&config_path, "not = toml =").unwrap();

        let layout = FsLayout::new(tmp.path().join("artifacts"), &config_path);
        let policy = FsPolicySource::new(layout);

        let err = policy.load_config().expect_err("expected TOML parse error");
        assert!(format!("{:#}", err).contains("parse TOML"));
    }

    #[test]
    fn policy_source_read_error_is_reported() {
        let tmp = TempDir::new().unwrap();
        let config_path = tmp.path().join("cockpit.toml");
        fs::write(&config_path, "[policy]\n").unwrap();

        #[cfg(windows)]
        let _lock = {
            use std::fs::OpenOptions;
            use std::os::windows::fs::OpenOptionsExt;
            OpenOptions::new()
                .read(true)
                .share_mode(0)
                .open(&config_path)
                .expect("lock file")
        };
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&config_path).unwrap().permissions();
            perms.set_mode(0o000);
            fs::set_permissions(&config_path, perms).unwrap();
        }

        let layout = FsLayout::new(tmp.path().join("artifacts"), &config_path);
        let policy = FsPolicySource::new(layout);

        let err = policy.load_config().expect_err("expected read error");
        assert!(format!("{:#}", err).contains("read config"));
    }

    #[test]
    fn output_sink_errors_when_out_dir_uncreatable() {
        let tmp = TempDir::new().unwrap();
        let artifacts = tmp.path().join("artifacts");
        fs::write(&artifacts, "not a dir").unwrap();

        let layout = FsLayout::new(&artifacts, tmp.path().join("cockpit.toml"));
        let sink = FsOutputSink::new(layout);

        let err = sink
            .write_cockpit_report("{\"ok\":true}\n")
            .expect_err("expected report write error");
        assert!(format!("{:#}", err).contains("create out dir"));

        let err = sink
            .write_cockpit_comment("# comment\n")
            .expect_err("expected comment write error");
        assert!(format!("{:#}", err).contains("create out dir"));
    }

    #[test]
    fn json_schema_validator_from_schema_invalid_schema_errors() {
        let bad_schema = serde_json::json!({ "type": 123 });
        let err = JsonSchemaValidator::from_schema(&bad_schema)
            .err()
            .expect("expected schema error");
        assert!(format!("{:#}", err).contains("invalid JSON schema"));
    }

    #[test]
    fn json_schema_validator_from_file_invalid_schema_errors() {
        let tmp = TempDir::new().unwrap();
        let schema_path = tmp.path().join("bad.schema.json");
        fs::write(&schema_path, r#"{ "type": 123 }"#).unwrap();

        let err = JsonSchemaValidator::from_file(&schema_path)
            .err()
            .expect("expected schema error");
        assert!(format!("{:#}", err).contains("invalid JSON schema"));
    }

    #[test]
    fn read_report_bytes_read_error_when_locked() {
        let tmp = TempDir::new().unwrap();
        let artifacts = tmp.path().join("artifacts");
        let sensor_dir = artifacts.join("sensor");
        fs::create_dir_all(&sensor_dir).unwrap();
        let report_path = sensor_dir.join("report.json");
        fs::write(&report_path, minimal_report()).unwrap();

        #[cfg(windows)]
        let _lock = {
            use std::fs::OpenOptions;
            use std::os::windows::fs::OpenOptionsExt;
            OpenOptions::new()
                .read(true)
                .share_mode(0)
                .open(&report_path)
                .expect("lock report file")
        };
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&report_path).unwrap().permissions();
            perms.set_mode(0o000);
            fs::set_permissions(&report_path, perms).unwrap();
        }

        let layout = FsLayout::new(&artifacts, tmp.path().join("cockpit.toml"));
        let source = FsReceiptSource::new(layout);

        let err = source
            .read_report_bytes("sensor")
            .err()
            .expect("expected read error");
        assert!(format!("{:#}", err).contains("read receipt"));
    }

    // -------------------------------------------------------------------------
    // base64 decoder tests
    // -------------------------------------------------------------------------

    fn decode_output_file_content(content: &str) -> Vec<u8> {
        let json = format!(r#"{{"name":"test.txt","content":"{}"}}"#, content);
        let file: OutputFile = serde_json::from_str(&json).unwrap();
        file.content
    }

    #[test]
    fn output_file_base64_content_is_decoded() {
        let bytes = decode_output_file_content("SGVsbG8gV29ybGQ=");
        assert_eq!(bytes, b"Hello World");
    }

    #[test]
    fn output_file_plain_text_content_is_preserved() {
        // Contains a space, which is not a valid base64 char → falls back to raw bytes.
        let bytes = decode_output_file_content("Hello World");
        assert_eq!(bytes, b"Hello World");
    }

    #[test]
    fn output_file_empty_content_is_empty_vec() {
        let bytes = decode_output_file_content("");
        assert!(bytes.is_empty());
    }

    #[test]
    fn output_file_invalid_base64_falls_back_to_raw() {
        // All chars are base64-valid but the padding is wrong → decode fails → raw bytes.
        let bytes = decode_output_file_content("NOT===VALID");
        assert_eq!(bytes, b"NOT===VALID");
    }

    #[test]
    fn output_file_binary_base64_roundtrip() {
        // [0x00, 0xFF, 0x80, 0x7F]
        let bytes = decode_output_file_content("AP+Afw==");
        assert_eq!(bytes, vec![0x00, 0xFF, 0x80, 0x7F]);
    }
}
