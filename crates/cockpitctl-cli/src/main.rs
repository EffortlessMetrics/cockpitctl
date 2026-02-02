use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use cockpitctl_ingest::{IngestRequest, IngestUseCase};
use cockpitctl_io::{FsLayout, FsOutputSink, FsPolicySource, FsReceiptSource};
use cockpitctl_render::render_comment;
use cockpitctl_types::{RunInfo, ToolInfo};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

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
    },
}

fn main() {
    let cli = Cli::parse();
    let code = match run(cli) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("cockpitctl error: {:#}", e);
            1
        }
    };
    std::process::exit(code);
}

fn run(cli: Cli) -> Result<i32> {
    match cli.command {
        Commands::Ingest { artifacts, config, label } => cmd_ingest(&artifacts, &config, label),
        Commands::Init { path } => cmd_init(&path),
        Commands::Validate { input } => cmd_validate(&input),
    }
}

fn now_rfc3339() -> String {
    if let Ok(v) = std::env::var("COCKPITCTL_STARTED_AT") {
        return v;
    }
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

fn cmd_ingest(artifacts: &str, config: &str, labels: Vec<String>) -> Result<i32> {
    let layout = FsLayout::new(artifacts, config);

    let receipts = FsReceiptSource::new(layout.clone());
    let policy = FsPolicySource::new(layout.clone());
    let output = FsOutputSink::new(layout);

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
    };

    let uc = IngestUseCase::new(receipts, policy, output, |r, cfg| render_comment(r, cfg));

    let req = IngestRequest { labels, tool, run };
    let result = uc.execute(req).context("ingest")?;
    Ok(result.exit_code)
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
    let example = include_str!("../../../cockpit.toml.example");
    fs::write(p, example).with_context(|| format!("write {}", p.display()))?;
    eprintln!("wrote {}", p.display());
    Ok(0)
}

fn cmd_validate(input: &str) -> Result<i32> {
    let bytes = std::fs::read(input).with_context(|| format!("read {}", input))?;

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
