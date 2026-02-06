use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::fs;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "xtask")]
#[command(about = "Project automation tasks", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Basic schema sanity checks (JSON parse + required fields).
    SchemaCheck {
        #[arg(long, default_value = "contracts/schemas")]
        dir: PathBuf,
    },

    /// Validate that JSON Schema files are valid JSON and conform to JSON Schema spec.
    ValidateSchemas {
        /// Directory containing JSON schema files
        #[arg(long, default_value = "contracts/schemas")]
        dir: PathBuf,

        /// Reformat JSON files with consistent indentation
        #[arg(long)]
        fix: bool,
    },

    /// Print instructions for regenerating golden fixtures.
    FixturesHelp,

    /// Check that crate-local schema copies match contracts/schemas/.
    SchemaSyncCheck,

    /// Copy contracts/schemas/*.json → crates/cockpitctl-types/schemas/.
    SchemaSyncFix,

    /// Conformance harness: validate sensor receipts against the protocol.
    Conform {
        /// Path to the sensor report to validate.
        #[arg(long)]
        report: PathBuf,

        /// Optional golden file to check determinism against.
        #[arg(long)]
        golden: Option<PathBuf>,

        /// Check survivability: verify status=fail has explanatory findings.
        #[arg(long)]
        survivability: bool,
    },
}

const SCHEMA_FILES: &[&str] = &[
    "sensor.report.v1.json",
    "cockpit.report.v1.json",
    "buildfix.plan.v1.json",
];

fn main() {
    let cli = Cli::parse();
    if let Err(e) = run(cli) {
        eprintln!("xtask error: {:#}", e);
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Commands::SchemaCheck { dir } => schema_check(dir),
        Commands::ValidateSchemas { dir, fix } => validate_schemas(dir, fix),
        Commands::FixturesHelp => fixtures_help(),
        Commands::SchemaSyncCheck => schema_sync_check(),
        Commands::SchemaSyncFix => schema_sync_fix(),
        Commands::Conform {
            report,
            golden,
            survivability,
        } => conform(report, golden, survivability),
    }
}

fn schema_sync_check() -> Result<()> {
    let source = PathBuf::from("contracts/schemas");
    let dest = PathBuf::from("crates/cockpitctl-types/schemas");
    let mut mismatches = Vec::new();

    for &name in SCHEMA_FILES {
        let src = source.join(name);
        let dst = dest.join(name);

        let src_bytes = fs::read(&src).with_context(|| format!("read {}", src.display()))?;
        let dst_bytes = fs::read(&dst).with_context(|| format!("read {}", dst.display()))?;

        if src_bytes != dst_bytes {
            mismatches.push(name);
            eprintln!("  MISMATCH: {}", name);
        } else {
            eprintln!("  ok: {}", name);
        }
    }

    if mismatches.is_empty() {
        eprintln!(
            "schema-sync-check: all {} files in sync",
            SCHEMA_FILES.len()
        );
        Ok(())
    } else {
        anyhow::bail!(
            "schema-sync-check: {} file(s) out of sync — run `cargo run -p xtask -- schema-sync-fix`",
            mismatches.len()
        )
    }
}

fn schema_sync_fix() -> Result<()> {
    let source = PathBuf::from("contracts/schemas");
    let dest = PathBuf::from("crates/cockpitctl-types/schemas");

    fs::create_dir_all(&dest).with_context(|| format!("create {}", dest.display()))?;

    for &name in SCHEMA_FILES {
        let src = source.join(name);
        let dst = dest.join(name);
        fs::copy(&src, &dst)
            .with_context(|| format!("copy {} → {}", src.display(), dst.display()))?;
        eprintln!("  copied: {}", name);
    }

    eprintln!("schema-sync-fix: {} files synced", SCHEMA_FILES.len());
    Ok(())
}

fn schema_check(dir: PathBuf) -> Result<()> {
    let files = [
        dir.join("sensor.report.v1.json"),
        dir.join("cockpit.report.v1.json"),
    ];

    for f in files {
        let txt = fs::read_to_string(&f).with_context(|| format!("read {}", f.display()))?;
        let v: serde_json::Value =
            serde_json::from_str(&txt).with_context(|| format!("parse json {}", f.display()))?;

        let id = v.get("$id").and_then(|x| x.as_str()).unwrap_or("");
        let title = v.get("title").and_then(|x| x.as_str()).unwrap_or("");
        if id.is_empty() || title.is_empty() {
            anyhow::bail!("schema missing $id/title: {}", f.display());
        }

        eprintln!("ok: {} ({}, {})", f.display(), title, id);
    }

    Ok(())
}

fn fixtures_help() -> Result<()> {
    eprintln!("Golden fixtures live under ./fixtures/*.");
    eprintln!("To regenerate, run cockpitctl ingest on each fixture and copy outputs:");
    eprintln!();
    eprintln!("  cargo run -p cockpitctl -- ingest --artifacts fixtures/happy_path/artifacts --config fixtures/happy_path/cockpit.toml");
    eprintln!("  cp fixtures/happy_path/artifacts/cockpit/report.json fixtures/happy_path/expected/report.json");
    eprintln!("  cp fixtures/happy_path/artifacts/cockpit/comment.md fixtures/happy_path/expected/comment.md");
    eprintln!();
    Ok(())
}

fn conform(report: PathBuf, golden: Option<PathBuf>, survivability: bool) -> Result<()> {
    use cockpitctl_types::{SensorReport, VerdictStatus, SENSOR_REPORT_V1_SCHEMA_JSON};

    eprintln!("conformance check: {}", report.display());

    // Step 1: Read the report
    let content =
        fs::read_to_string(&report).with_context(|| format!("read {}", report.display()))?;

    // Step 2: Parse as JSON
    let value: serde_json::Value = serde_json::from_str(&content)
        .with_context(|| format!("parse json {}", report.display()))?;

    // Step 3: Schema validation
    let schema: serde_json::Value = serde_json::from_str(SENSOR_REPORT_V1_SCHEMA_JSON)
        .context("parse embedded sensor.report.v1 schema")?;

    let validator =
        jsonschema::validator_for(&schema).context("compile sensor.report.v1 schema")?;

    let errors: Vec<_> = validator.iter_errors(&value).collect();
    if !errors.is_empty() {
        eprintln!("  FAIL: schema validation errors:");
        for e in &errors {
            eprintln!("    - {}", e);
        }
        anyhow::bail!("schema validation failed with {} error(s)", errors.len());
    }
    eprintln!("  ok: schema validation passed");

    // Step 4: Determinism check (if golden provided)
    if let Some(golden_path) = golden {
        let golden_content = fs::read_to_string(&golden_path)
            .with_context(|| format!("read golden {}", golden_path.display()))?;

        if content != golden_content {
            eprintln!("  FAIL: report does not match golden file");
            eprintln!("    golden: {}", golden_path.display());
            eprintln!("    actual: {}", report.display());
            anyhow::bail!("determinism check failed: report differs from golden");
        }
        eprintln!("  ok: determinism check passed (matches golden)");
    }

    // Step 5: Survivability check (if requested)
    if survivability {
        let parsed: SensorReport =
            serde_json::from_value(value).context("parse as SensorReport")?;

        if parsed.verdict.status == VerdictStatus::Fail {
            let has_explanatory = !parsed.findings.is_empty() || !parsed.verdict.reasons.is_empty();

            if !has_explanatory {
                eprintln!("  FAIL: status=fail but no findings or reasons");
                anyhow::bail!(
                    "survivability check failed: status=fail requires explanatory findings or reasons"
                );
            }
            eprintln!("  ok: survivability check passed (fail has explanations)");
        } else {
            eprintln!("  ok: survivability check skipped (status != fail)");
        }
    }

    eprintln!("conformance: PASS");
    Ok(())
}

fn validate_schemas(dir: PathBuf, fix: bool) -> Result<()> {
    use walkdir::WalkDir;

    if !dir.exists() {
        anyhow::bail!("schema directory does not exist: {}", dir.display());
    }

    let mut errors: Vec<String> = Vec::new();
    let mut files_checked = 0;
    let mut files_fixed = 0;

    for entry in WalkDir::new(&dir)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();

        // Skip non-JSON files
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }

        // Skip directories
        if !path.is_file() {
            continue;
        }

        files_checked += 1;
        eprintln!("checking: {}", path.display());

        // Step 1: Read the file
        let content = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => {
                errors.push(format!("{}: failed to read file: {}", path.display(), e));
                continue;
            }
        };

        // Step 2: Validate it's valid JSON
        let value: serde_json::Value = match serde_json::from_str(&content) {
            Ok(v) => v,
            Err(e) => {
                errors.push(format!("{}: invalid JSON: {}", path.display(), e));
                continue;
            }
        };

        // Step 3: Validate it's a valid JSON Schema (meta-validation)
        if let Err(meta_err) = jsonschema::meta::validate(&value) {
            errors.push(format!(
                "{}: not a valid JSON Schema: {}",
                path.display(),
                meta_err
            ));
            continue;
        }

        // Step 4: Check for recommended fields
        if value.get("$schema").is_none() {
            eprintln!("  warning: {} missing $schema field", path.display());
        }

        // Step 5: Optionally fix formatting
        if fix {
            let formatted = serde_json::to_string_pretty(&value)
                .with_context(|| format!("failed to format {}", path.display()))?;
            let formatted_with_newline = format!("{}\n", formatted);

            if formatted_with_newline != content {
                fs::write(path, &formatted_with_newline)
                    .with_context(|| format!("failed to write {}", path.display()))?;
                eprintln!("  fixed: {}", path.display());
                files_fixed += 1;
            }
        }

        eprintln!("  ok: valid JSON Schema");
    }

    // Summary
    eprintln!();
    eprintln!(
        "checked {} file(s), {} error(s)",
        files_checked,
        errors.len()
    );
    if fix {
        eprintln!("fixed {} file(s)", files_fixed);
    }

    if !errors.is_empty() {
        eprintln!();
        eprintln!("errors:");
        for e in &errors {
            eprintln!("  {}", e);
        }
        anyhow::bail!("schema validation failed with {} error(s)", errors.len());
    }

    Ok(())
}
