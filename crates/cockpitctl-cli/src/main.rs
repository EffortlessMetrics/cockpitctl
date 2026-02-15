use std::collections::BTreeMap;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use cockpitctl_ingest::{IngestRequest, IngestUseCase, NoOpSchemaValidator, SchemaValidator};
use cockpitctl_io::{FsLayout, FsOutputSink, FsPolicySource, FsReceiptSource, JsonSchemaValidator};
use cockpitctl_render::{render_comment, render_github_annotations};
use cockpitctl_types::{RunInfo, SchemaValidation, ToolInfo};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

/// CLI schema validation mode for sensor receipts.
#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum SchemaValidationMode {
    /// Skip JSON schema validation; only parse receipts as JSON.
    Lax,
    /// Validate receipts against schemas/sensor.report.v1.json.
    Strict,
}

/// Output format for the cockpit report.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, ValueEnum)]
pub enum OutputFormat {
    /// Standard cockpit report (default).
    #[default]
    Cockpit,
    /// SARIF v2.1.0 (Static Analysis Results Interchange Format).
    Sarif,
}

impl From<SchemaValidationMode> for SchemaValidation {
    fn from(mode: SchemaValidationMode) -> Self {
        match mode {
            SchemaValidationMode::Lax => SchemaValidation::Lax,
            SchemaValidationMode::Strict => SchemaValidation::Strict,
        }
    }
}

#[derive(Parser, Debug)]
#[command(name = "cockpitctl")]
#[command(about = "Receipt-ingesting PR cockpit director", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Ingest sensor receipts under artifacts/ and render a cockpit report + comment.
    Ingest {
        /// Artifacts directory (default: artifacts)
        #[arg(long, default_value = "artifacts")]
        artifacts: String,

        /// Policy file (default: cockpit.toml)
        #[arg(long, default_value = "cockpit.toml")]
        config: String,

        /// Labels present on the PR (optional; used for label-gates)
        #[arg(long)]
        label: Vec<String>,

        /// Schema validation mode for sensor receipts.
        ///
        /// - lax: Skip JSON schema validation; only parse receipts as JSON.
        /// - strict: Validate receipts against schemas/sensor.report.v1.json; schema
        ///   violations are surfaced as findings rather than causing parse errors.
        #[arg(long, value_enum)]
        schema_validation: Option<SchemaValidationMode>,

        /// Emit GitHub Actions workflow command annotations (::error, ::warning, ::notice) to stdout.
        #[arg(long)]
        github_annotations: bool,

        /// Output format for the cockpit report.
        #[arg(long, value_enum, default_value = "cockpit")]
        format: OutputFormat,

        /// Path to a previous cockpit report for trend comparison.
        #[arg(long)]
        baseline: Option<String>,
    },

    /// Write a starter cockpit.toml (does not overwrite).
    Init {
        #[arg(long, default_value = "cockpit.toml")]
        path: String,
    },

    /// Validate a receipt file can be parsed (developer tool).
    Validate {
        /// Path to a JSON receipt or cockpit report.
        #[arg(long)]
        input: String,

        /// Perform strict JSON Schema validation (default).
        #[arg(long, conflicts_with = "lax")]
        strict: bool,

        /// Skip JSON Schema validation; only parse as JSON.
        #[arg(long, conflicts_with = "strict")]
        lax: bool,
    },

    /// Explain a cockpit finding code (e.g. cockpit.missing_receipt).
    Explain {
        /// The finding code to explain, or "all" to list every code.
        code: String,
    },
}

fn main_entry(cli: Cli) -> i32 {
    match run(cli) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("cockpitctl error: {:#}", e);
            1
        }
    }
}

#[cfg(not(test))]
fn main() -> std::process::ExitCode {
    std::process::ExitCode::from(main_entry(Cli::parse()) as u8)
}

#[cfg(test)]
fn main() {}

fn run(cli: Cli) -> Result<i32> {
    match cli.command {
        Commands::Ingest {
            artifacts,
            config,
            label,
            schema_validation,
            github_annotations,
            format,
            baseline,
        } => cmd_ingest(
            &artifacts,
            &config,
            label,
            schema_validation,
            github_annotations,
            format,
            baseline.as_deref(),
        ),
        Commands::Init { path } => cmd_init(&path),
        Commands::Validate { input, strict, lax } => cmd_validate(&input, strict, lax),
        Commands::Explain { code } => cmd_explain(&code),
    }
}

fn now_rfc3339() -> String {
    if let Ok(v) = std::env::var("COCKPITCTL_STARTED_AT") {
        return v;
    }
    if let Ok(epoch) = std::env::var("SOURCE_DATE_EPOCH")
        && let Ok(ts) = epoch.parse::<i64>()
        && let Ok(dt) = OffsetDateTime::from_unix_timestamp(ts)
        && let Ok(s) = dt.format(&Rfc3339)
    {
        return s;
    }
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

fn cmd_ingest(
    artifacts: &str,
    config: &str,
    labels: Vec<String>,
    schema_validation: Option<SchemaValidationMode>,
    github_annotations: bool,
    format: OutputFormat,
    baseline: Option<&str>,
) -> Result<i32> {
    // Two-phase config loading: read config first to extract operational limits,
    // then build adapters with those limits. Config is parsed again inside
    // IngestUseCase::execute() — this is idempotent and cheap.
    let pre_cfg: cockpitctl_types::CockpitConfig = if std::path::Path::new(config).exists() {
        let txt =
            std::fs::read_to_string(config).with_context(|| format!("read config {}", config))?;
        toml::from_str(&txt).with_context(|| format!("parse config {}", config))?
    } else {
        cockpitctl_types::CockpitConfig::default()
    };

    let layout = FsLayout::new(artifacts, config)
        .with_max_receipt_bytes(pre_cfg.policy.max_receipt_size_bytes);

    let receipts = FsReceiptSource::new(layout.clone());
    let policy = FsPolicySource::new(layout.clone());
    let output = FsOutputSink::new(layout.clone());

    let tool = ToolInfo {
        name: "cockpitctl".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        commit: option_env!("GIT_SHA").map(|s| s.to_string()),
    };

    let run = RunInfo {
        started_at: now_rfc3339(),
        ended_at: None,
        duration_ms: None,
        host: None,
        git: None,
        ci: None,
        capabilities: BTreeMap::new(),
    };

    let schema_validation_override = schema_validation.map(SchemaValidation::from);
    let req = IngestRequest {
        labels,
        tool,
        run,
        schema_validation_override,
    };

    // Execute with the appropriate schema validator based on CLI flag.
    let result = match schema_validation_override {
        Some(SchemaValidation::Lax) => {
            let uc = IngestUseCase::new(receipts, policy, output, NoOpSchemaValidator, |r, cfg| {
                render_comment(r, cfg)
            });
            uc.execute(req).context("ingest")?
        }
        _ => {
            let validator = JsonSchemaValidator::sensor_report_v1()
                .context("load sensor.report.v1 JSON schema")?;
            let uc = IngestUseCase::new(receipts, policy, output, validator, |r, cfg| {
                render_comment(r, cfg)
            });
            uc.execute(req).context("ingest")?
        }
    };

    // Emit GitHub Actions annotations if requested.
    if github_annotations {
        let sensor_blocking: BTreeMap<String, bool> = result
            .report
            .sensors
            .iter()
            .map(|s| (s.id.clone(), s.blocking))
            .collect();
        let gh = render_github_annotations(&result.report.highlights, &pre_cfg, &sensor_blocking);
        for line in &gh.lines {
            println!("{}", line);
        }
    }

    // Write SARIF output if requested.
    if matches!(format, OutputFormat::Sarif) {
        let sarif_json = cockpitctl_sarif::cockpit_report_to_sarif_json(&result.report)
            .context("render SARIF")?;
        let sarif_path = std::path::Path::new(artifacts)
            .join("cockpit")
            .join("sarif.json");
        std::fs::write(&sarif_path, &sarif_json)
            .with_context(|| format!("write {}", sarif_path.display()))?;
    }

    // Compute and render trend if baseline provided.
    if let Some(baseline_path) = baseline {
        let baseline_bytes = std::fs::read(baseline_path)
            .with_context(|| format!("read baseline {}", baseline_path))?;
        let baseline_report: cockpitctl_types::CockpitReport =
            serde_json::from_slice(&baseline_bytes)
                .with_context(|| format!("parse baseline {}", baseline_path))?;
        let trend = cockpitctl_domain::compute_trend(&baseline_report, &result.report);
        let trend_md = cockpitctl_render::render_trend_section(&trend);
        eprint!("{}", trend_md);
    }

    // Run post-processor hooks if configured.
    if !pre_cfg.hooks.is_empty() {
        let report_json =
            serde_json::to_string_pretty(&result.report).context("serialize report for hooks")?;
        let hook_output = FsOutputSink::new(layout);
        let sections = cockpitctl_io::run_hooks(&pre_cfg.hooks, &report_json, &hook_output)
            .context("run hooks")?;
        if !sections.is_empty() {
            eprintln!("cockpitctl: {} hook section(s) collected", sections.len());
        }
    }

    Ok(result.exit_code)
}

fn cmd_explain(code: &str) -> Result<i32> {
    if code == "all" {
        let codes = cockpitctl_domain::all_codes();
        for e in &codes {
            println!("{:<35} {}", e.code, e.title);
        }
        return Ok(0);
    }

    match cockpitctl_domain::explain_code(code) {
        Some(e) => {
            println!("{}", e.code);
            println!("  Title:       {}", e.title);
            println!("  Description: {}", e.description);
            println!("  Cause:       {}", e.cause);
            println!("  Fix:         {}", e.fix);
            Ok(0)
        }
        None => {
            eprintln!("unknown code: {}", code);
            eprintln!("run `cockpitctl explain all` to list all codes");
            Ok(1)
        }
    }
}

fn cmd_init(path: &str) -> Result<i32> {
    use std::fs;
    use std::path::Path;

    let p = Path::new(path);
    if p.exists() {
        eprintln!("refusing to overwrite existing {}", p.display());
        return Ok(2);
    }

    // Keep init output simple: copy from repository example if present.
    let example = include_str!("../cockpit.toml.example");
    fs::write(p, example).with_context(|| format!("write {}", p.display()))?;
    eprintln!("wrote {}", p.display());
    Ok(0)
}

fn cmd_validate(input: &str, _strict: bool, lax: bool) -> Result<i32> {
    let bytes = std::fs::read(input).with_context(|| format!("read {}", input))?;
    let mode = if lax {
        SchemaValidation::Lax
    } else {
        SchemaValidation::Strict
    };

    match mode {
        SchemaValidation::Lax => {
            // Try sensor report first, then cockpit report.
            if serde_json::from_slice::<cockpitctl_types::SensorReport>(&bytes).is_ok() {
                eprintln!("ok: parsed as sensor.report.v1 shape");
                return Ok(0);
            }
            if serde_json::from_slice::<cockpitctl_types::CockpitReport>(&bytes).is_ok() {
                eprintln!("ok: parsed as cockpit.report.v1 shape");
                return Ok(0);
            }

            anyhow::bail!("input did not parse as SensorReport or CockpitReport")
        }
        SchemaValidation::Strict => {
            let value: serde_json::Value =
                serde_json::from_slice(&bytes).context("parse JSON input")?;
            let schema_hint = value.get("schema").and_then(|s| s.as_str());

            let mut candidates = Vec::new();
            if schema_hint == Some("cockpit.report.v1") {
                candidates.push((
                    "cockpit.report.v1",
                    JsonSchemaValidator::cockpit_report_v1()
                        .context("load cockpit.report.v1 JSON schema")?,
                ));
            } else if schema_hint.is_some() {
                candidates.push((
                    "sensor.report.v1",
                    JsonSchemaValidator::sensor_report_v1()
                        .context("load sensor.report.v1 JSON schema")?,
                ));
            } else {
                candidates.push((
                    "sensor.report.v1",
                    JsonSchemaValidator::sensor_report_v1()
                        .context("load sensor.report.v1 JSON schema")?,
                ));
                candidates.push((
                    "cockpit.report.v1",
                    JsonSchemaValidator::cockpit_report_v1()
                        .context("load cockpit.report.v1 JSON schema")?,
                ));
            }

            let mut errors = Vec::new();
            for (label, validator) in candidates {
                match validator.validate_receipt(&bytes)? {
                    cockpitctl_ingest::SchemaValidationResult::Valid => {
                        eprintln!("ok: validated as {}", label);
                        return Ok(0);
                    }
                    cockpitctl_ingest::SchemaValidationResult::Invalid(errs) => {
                        errors.push(format_schema_errors(label, &errs));
                    }
                }
            }

            anyhow::bail!("strict validation failed:\n{}", errors.join("\n"))
        }
    }
}

fn format_schema_errors(label: &str, errs: &[String]) -> String {
    let detail = if errs.is_empty() {
        "schema validation failed".to_string()
    } else {
        errs.join("; ")
    };
    format!("{}: {}", label, detail)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn write_temp_json(temp: &TempDir, name: &str, content: &str) -> std::path::PathBuf {
        let path = temp.path().join(name);
        std::fs::write(&path, content).expect("write temp file");
        path
    }

    fn minimal_sensor_report_json() -> String {
        let report = cockpitctl_types::SensorReport {
            schema: "sensor.report.v1".to_string(),
            tool: ToolInfo {
                name: "test".to_string(),
                version: "1.0.0".to_string(),
                commit: None,
            },
            run: RunInfo {
                started_at: "2026-02-01T00:00:00Z".to_string(),
                ended_at: None,
                duration_ms: None,
                host: None,
                git: None,
                ci: None,
                capabilities: BTreeMap::new(),
            },
            verdict: cockpitctl_types::Verdict {
                status: cockpitctl_types::VerdictStatus::Pass,
                counts: cockpitctl_types::VerdictCounts::default(),
                reasons: vec![],
            },
            findings: vec![],
            artifacts: vec![],
            data: None,
        };
        serde_json::to_string(&report).expect("serialize sensor report")
    }

    fn minimal_cockpit_report_json() -> String {
        let cfg = cockpitctl_types::CockpitConfig::default();
        let report = cockpitctl_types::CockpitReport {
            schema: "cockpit.report.v1".to_string(),
            tool: ToolInfo {
                name: "cockpitctl".to_string(),
                version: "0.2.0".to_string(),
                commit: None,
            },
            run: RunInfo {
                started_at: "2026-02-01T00:00:00Z".to_string(),
                ended_at: None,
                duration_ms: None,
                host: None,
                git: None,
                ci: None,
                capabilities: BTreeMap::new(),
            },
            verdict: cockpitctl_types::Verdict {
                status: cockpitctl_types::VerdictStatus::Pass,
                counts: cockpitctl_types::VerdictCounts::default(),
                reasons: vec![],
            },
            sensors: vec![],
            highlights: vec![],
            policy: cockpitctl_domain::snapshot_policy(&cfg),
            data: None,
        };
        serde_json::to_string(&report).expect("serialize cockpit report")
    }

    fn setup_minimal_ingest(
        temp: &TempDir,
        sensor_id: &str,
    ) -> (std::path::PathBuf, std::path::PathBuf) {
        let artifacts = temp.path().join("artifacts");
        let sensor_dir = artifacts.join(sensor_id);
        std::fs::create_dir_all(&sensor_dir).expect("create sensor dir");
        std::fs::write(sensor_dir.join("report.json"), minimal_sensor_report_json())
            .expect("write report");
        let config_path = temp.path().join("cockpit.toml");
        (artifacts, config_path)
    }

    #[test]
    fn schema_validation_mode_maps_to_policy_enum() {
        assert_eq!(
            SchemaValidation::from(SchemaValidationMode::Lax),
            SchemaValidation::Lax
        );
        assert_eq!(
            SchemaValidation::from(SchemaValidationMode::Strict),
            SchemaValidation::Strict
        );
    }

    #[test]
    fn format_schema_errors_handles_empty_and_non_empty() {
        let empty = format_schema_errors("sensor.report.v1", &[]);
        assert!(empty.contains("schema validation failed"));

        let filled = format_schema_errors(
            "sensor.report.v1",
            &[String::from("missing schema"), String::from("bad status")],
        );
        assert!(filled.contains("missing schema"));
        assert!(filled.contains("bad status"));
    }

    #[test]
    fn now_rfc3339_prefers_env_var() {
        let _guard = ENV_LOCK.lock().unwrap();
        unsafe {
            std::env::set_var("COCKPITCTL_STARTED_AT", "2026-02-01T00:00:00Z");
            std::env::remove_var("SOURCE_DATE_EPOCH");
        }
        let got = now_rfc3339();
        unsafe {
            std::env::remove_var("COCKPITCTL_STARTED_AT");
        }
        assert_eq!(got, "2026-02-01T00:00:00Z");
    }

    #[test]
    fn now_rfc3339_uses_source_date_epoch() {
        let _guard = ENV_LOCK.lock().unwrap();
        unsafe {
            std::env::remove_var("COCKPITCTL_STARTED_AT");
            std::env::set_var("SOURCE_DATE_EPOCH", "0");
        }
        let got = now_rfc3339();
        unsafe {
            std::env::remove_var("SOURCE_DATE_EPOCH");
        }
        assert_eq!(got, "1970-01-01T00:00:00Z");
    }

    #[test]
    fn now_rfc3339_falls_back_on_invalid_epoch() {
        let _guard = ENV_LOCK.lock().unwrap();
        unsafe {
            std::env::remove_var("COCKPITCTL_STARTED_AT");
            std::env::set_var("SOURCE_DATE_EPOCH", "not-a-number");
        }
        let got = now_rfc3339();
        unsafe {
            std::env::remove_var("SOURCE_DATE_EPOCH");
        }
        assert!(got.contains('T'));
        assert!(got.ends_with('Z'));
    }

    #[test]
    fn cmd_init_creates_file_and_respects_existing() {
        let _guard = ENV_LOCK.lock().unwrap();
        let temp = TempDir::new().expect("tempdir");
        let path = temp.path().join("cockpit.toml");

        let code = cmd_init(path.to_string_lossy().as_ref()).expect("cmd_init");
        assert_eq!(code, 0);
        assert!(path.exists());

        let code_existing = cmd_init(path.to_string_lossy().as_ref()).expect("cmd_init");
        assert_eq!(code_existing, 2);
    }

    #[test]
    fn cmd_init_returns_error_on_write_failure() {
        let temp = TempDir::new().expect("tempdir");
        let bad_path = temp.path().join("missing_dir").join("cockpit.toml");
        let err = cmd_init(bad_path.to_string_lossy().as_ref()).expect_err("expected error");
        assert!(format!("{:#}", err).contains("write"));
    }

    #[test]
    fn cmd_validate_lax_accepts_sensor_report() {
        let temp = TempDir::new().expect("tempdir");
        let path = write_temp_json(&temp, "sensor.json", &minimal_sensor_report_json());
        let code =
            cmd_validate(path.to_string_lossy().as_ref(), false, true).expect("cmd_validate");
        assert_eq!(code, 0);
    }

    #[test]
    fn cmd_validate_lax_accepts_cockpit_report() {
        let temp = TempDir::new().expect("tempdir");
        let mut value: serde_json::Value =
            serde_json::from_str(&minimal_cockpit_report_json()).expect("parse cockpit json");
        value["findings"] = serde_json::Value::String("not-an-array".to_string());
        let json = serde_json::to_string(&value).expect("serialize cockpit json");
        let path = write_temp_json(&temp, "cockpit.json", &json);
        let code =
            cmd_validate(path.to_string_lossy().as_ref(), false, true).expect("cmd_validate");
        assert_eq!(code, 0);
    }

    #[test]
    fn cmd_validate_lax_rejects_invalid_json() {
        let temp = TempDir::new().expect("tempdir");
        let path = write_temp_json(&temp, "bad.json", "{ not json }");
        let err =
            cmd_validate(path.to_string_lossy().as_ref(), false, true).expect_err("expected error");
        let msg = format!("{:#}", err);
        assert!(msg.contains("did not parse"));
    }

    #[test]
    fn cmd_validate_strict_cockpit_schema_passes() {
        let temp = TempDir::new().expect("tempdir");
        let path = write_temp_json(&temp, "cockpit.json", &minimal_cockpit_report_json());
        let code =
            cmd_validate(path.to_string_lossy().as_ref(), true, false).expect("cmd_validate");
        assert_eq!(code, 0);
    }

    #[test]
    fn cmd_validate_strict_sensor_schema_passes() {
        let temp = TempDir::new().expect("tempdir");
        let path = write_temp_json(&temp, "sensor.json", &minimal_sensor_report_json());
        let code =
            cmd_validate(path.to_string_lossy().as_ref(), true, false).expect("cmd_validate");
        assert_eq!(code, 0);
    }

    #[test]
    fn cmd_validate_strict_no_schema_fails_all_candidates() {
        let temp = TempDir::new().expect("tempdir");
        let bad = r#"{ "tool": { "name": "x", "version": "1.0" } }"#;
        let path = write_temp_json(&temp, "bad.json", bad);
        let err =
            cmd_validate(path.to_string_lossy().as_ref(), true, false).expect_err("expected error");
        let msg = format!("{:#}", err);
        assert!(msg.contains("strict validation failed"));
    }

    #[test]
    fn run_dispatches_init_validate_and_ingest() {
        let _guard = ENV_LOCK.lock().unwrap();
        unsafe {
            std::env::set_var("COCKPITCTL_STARTED_AT", "2026-02-01T00:00:00Z");
        }

        let temp = TempDir::new().expect("tempdir");
        let init_path = temp.path().join("init.toml");
        let code = run(Cli {
            command: Commands::Init {
                path: init_path.to_string_lossy().to_string(),
            },
        })
        .expect("run init");
        assert_eq!(code, 0);
        assert!(init_path.exists());

        let validate_path = write_temp_json(&temp, "sensor.json", &minimal_sensor_report_json());
        let code = run(Cli {
            command: Commands::Validate {
                input: validate_path.to_string_lossy().to_string(),
                strict: false,
                lax: true,
            },
        })
        .expect("run validate");
        assert_eq!(code, 0);

        let (artifacts, config_path) = setup_minimal_ingest(&temp, "sensor");
        let code = run(Cli {
            command: Commands::Ingest {
                artifacts: artifacts.to_string_lossy().to_string(),
                config: config_path.to_string_lossy().to_string(),
                label: vec![],
                schema_validation: None,
                github_annotations: false,
                format: OutputFormat::Cockpit,
                baseline: None,
            },
        })
        .expect("run ingest");
        assert_eq!(code, 0);

        unsafe {
            std::env::remove_var("COCKPITCTL_STARTED_AT");
        }
    }

    #[test]
    fn cmd_ingest_lax_writes_outputs() {
        let _guard = ENV_LOCK.lock().unwrap();
        unsafe {
            std::env::set_var("COCKPITCTL_STARTED_AT", "2026-02-01T00:00:00Z");
        }

        let temp = TempDir::new().expect("tempdir");
        let (artifacts, config_path) = setup_minimal_ingest(&temp, "sensor");
        let code = cmd_ingest(
            artifacts.to_string_lossy().as_ref(),
            config_path.to_string_lossy().as_ref(),
            vec![],
            Some(SchemaValidationMode::Lax),
            false,
            OutputFormat::Cockpit,
            None,
        )
        .expect("cmd_ingest");
        assert_eq!(code, 0);

        let out_dir = artifacts.join("cockpit");
        assert!(out_dir.join("report.json").exists());
        assert!(out_dir.join("comment.md").exists());

        unsafe {
            std::env::remove_var("COCKPITCTL_STARTED_AT");
        }
    }

    #[test]
    fn cmd_ingest_lax_propagates_ingest_error() {
        let _guard = ENV_LOCK.lock().unwrap();
        unsafe {
            std::env::set_var("COCKPITCTL_STARTED_AT", "2026-02-01T00:00:00Z");
        }

        let temp = TempDir::new().expect("tempdir");
        let artifacts_file = temp.path().join("artifacts");
        std::fs::write(&artifacts_file, "not a dir").expect("write artifacts file");
        let config_path = temp.path().join("cockpit.toml");

        let err = cmd_ingest(
            artifacts_file.to_string_lossy().as_ref(),
            config_path.to_string_lossy().as_ref(),
            vec![],
            Some(SchemaValidationMode::Lax),
            false,
            OutputFormat::Cockpit,
            None,
        )
        .expect_err("expected ingest error");
        assert!(format!("{:#}", err).contains("ingest"));

        unsafe {
            std::env::remove_var("COCKPITCTL_STARTED_AT");
        }
    }

    #[test]
    fn cmd_ingest_strict_propagates_ingest_error() {
        let _guard = ENV_LOCK.lock().unwrap();
        unsafe {
            std::env::set_var("COCKPITCTL_STARTED_AT", "2026-02-01T00:00:00Z");
        }

        let temp = TempDir::new().expect("tempdir");
        let artifacts_file = temp.path().join("artifacts");
        std::fs::write(&artifacts_file, "not a dir").expect("write artifacts file");
        let config_path = temp.path().join("cockpit.toml");

        let err = cmd_ingest(
            artifacts_file.to_string_lossy().as_ref(),
            config_path.to_string_lossy().as_ref(),
            vec![],
            None,
            false,
            OutputFormat::Cockpit,
            None,
        )
        .expect_err("expected ingest error");
        assert!(format!("{:#}", err).contains("ingest"));

        unsafe {
            std::env::remove_var("COCKPITCTL_STARTED_AT");
        }
    }

    #[test]
    fn main_entry_returns_1_on_error() {
        let code = main_entry(Cli {
            command: Commands::Validate {
                input: "does-not-exist.json".to_string(),
                strict: true,
                lax: false,
            },
        });
        assert_eq!(code, 1);
    }

    #[test]
    fn main_entry_returns_ok_on_success() {
        let temp = TempDir::new().expect("tempdir");
        let path = write_temp_json(&temp, "sensor.json", &minimal_sensor_report_json());
        let code = main_entry(Cli {
            command: Commands::Validate {
                input: path.to_string_lossy().to_string(),
                strict: false,
                lax: true,
            },
        });
        assert_eq!(code, 0);
    }

    #[test]
    fn main_noop_executes_in_tests() {
        super::main();
    }
}
