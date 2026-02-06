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

        /// Check finding location paths for hygiene violations.
        #[arg(long)]
        path_hygiene: bool,

        /// Check that findings are sorted in canonical order.
        #[arg(long)]
        ordering: bool,

        /// Check that reason tokens match ^[a-z0-9_]+$.
        #[arg(long)]
        reason_lint: bool,

        /// Run all conformance checks.
        #[arg(long)]
        all: bool,

        /// Sensor ID (required for --ordering).
        #[arg(long)]
        sensor_id: Option<String>,
    },

    /// Validate every sensor receipt in an artifacts/ directory at once.
    ConformDir {
        /// Artifacts directory to scan.
        #[arg(long)]
        dir: PathBuf,

        /// Also validate cockpit/report.json against cockpit.report.v1 schema.
        #[arg(long)]
        validate_cockpit: bool,

        /// Run all conformance checks per report.
        #[arg(long)]
        all: bool,

        /// Per-report path hygiene check.
        #[arg(long)]
        path_hygiene: bool,

        /// Per-report ordering check.
        #[arg(long)]
        ordering: bool,

        /// Per-report reason token lint.
        #[arg(long)]
        reason_lint: bool,

        /// Per-report survivability check.
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
            path_hygiene,
            ordering,
            reason_lint,
            all,
            sensor_id,
        } => conform(
            report,
            golden,
            survivability,
            path_hygiene,
            ordering,
            reason_lint,
            all,
            sensor_id,
        ),
        Commands::ConformDir {
            dir,
            validate_cockpit,
            all,
            path_hygiene,
            ordering,
            reason_lint,
            survivability,
        } => conform_dir(
            dir,
            validate_cockpit,
            &ConformChecks {
                path_hygiene: path_hygiene || all,
                ordering: ordering || all,
                reason_lint: reason_lint || all,
                survivability: survivability || all,
            },
        ),
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
    eprintln!(
        "  cargo run -p cockpitctl -- ingest --artifacts fixtures/happy_path/artifacts --config fixtures/happy_path/cockpit.toml"
    );
    eprintln!(
        "  cp fixtures/happy_path/artifacts/cockpit/report.json fixtures/happy_path/expected/report.json"
    );
    eprintln!(
        "  cp fixtures/happy_path/artifacts/cockpit/comment.md fixtures/happy_path/expected/comment.md"
    );
    eprintln!();
    Ok(())
}

fn is_valid_reason_token(s: &str) -> bool {
    !s.is_empty()
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

fn check_path_hygiene(report: &cockpitctl_types::SensorReport) -> Vec<String> {
    let mut violations = Vec::new();
    for (i, f) in report.findings.iter().enumerate() {
        if let Some(loc) = &f.location
            && let Some(path) = &loc.path
        {
            if path.starts_with('/') || path.starts_with('\\') {
                violations.push(format!(
                    "finding[{}]: absolute path (starts with / or \\): {}",
                    i, path
                ));
            } else if path.len() >= 2
                && path.as_bytes()[0].is_ascii_alphabetic()
                && path.as_bytes()[1] == b':'
            {
                violations.push(format!(
                    "finding[{}]: absolute path (drive letter): {}",
                    i, path
                ));
            }
            if path.contains("..") {
                violations.push(format!(
                    "finding[{}]: path traversal (contains ..): {}",
                    i, path
                ));
            }
            if path.contains('\\') {
                violations.push(format!("finding[{}]: backslash in path: {}", i, path));
            }
        }
    }
    violations
}

fn check_ordering(report: &cockpitctl_types::SensorReport, sensor_id: &str) -> Vec<String> {
    use cockpitctl_types::{FindingSortKey, severity_rank};

    let keys: Vec<FindingSortKey> = report
        .findings
        .iter()
        .map(|f| FindingSortKey {
            severity_rank: severity_rank(&f.severity),
            sensor_id: sensor_id.to_string(),
            path: f
                .location
                .as_ref()
                .and_then(|l| l.path.as_deref())
                .unwrap_or("")
                .to_string(),
            line: f.location.as_ref().and_then(|l| l.line).unwrap_or(0),
            code: f.code.clone(),
            message: f.message.clone(),
        })
        .collect();

    let mut violations = Vec::new();
    for i in 1..keys.len() {
        if keys[i] < keys[i - 1] {
            violations.push(format!(
                "finding[{}] is out of order (severity_rank={}, code={}) < finding[{}] (severity_rank={}, code={})",
                i, keys[i].severity_rank, keys[i].code,
                i - 1, keys[i - 1].severity_rank, keys[i - 1].code,
            ));
        }
    }
    violations
}

fn check_reason_tokens(report: &cockpitctl_types::SensorReport) -> Vec<String> {
    let mut violations = Vec::new();

    for (i, reason) in report.verdict.reasons.iter().enumerate() {
        if !is_valid_reason_token(reason) {
            violations.push(format!(
                "verdict.reasons[{}]: invalid token {:?}",
                i, reason
            ));
        }
    }

    for (name, cap) in &report.run.capabilities {
        if let Some(reason) = &cap.reason
            && !is_valid_reason_token(reason)
        {
            violations.push(format!(
                "capabilities.{}.reason: invalid token {:?}",
                name, reason
            ));
        }
    }

    violations
}

// ─────────────────────────────────────────────────────────────────────────────
// Conformance: reusable single-report check
// ─────────────────────────────────────────────────────────────────────────────

struct ConformChecks {
    path_hygiene: bool,
    ordering: bool,
    reason_lint: bool,
    survivability: bool,
}

/// Validate a single sensor report from its already-read content.
/// Returns Ok(()) on success, Err on any check failure.
fn conform_single(content: &str, sensor_id: &str, checks: &ConformChecks) -> Result<()> {
    use cockpitctl_types::{SENSOR_REPORT_V1_SCHEMA_JSON, SensorReport, VerdictStatus};

    // Parse as JSON
    let value: serde_json::Value = serde_json::from_str(content).context("parse JSON")?;

    // Schema validation
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

    // Parse for extended checks
    let parsed: SensorReport = serde_json::from_value(value).context("parse as SensorReport")?;

    // Survivability check
    if checks.survivability {
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

    let mut all_violations = Vec::new();

    // Path hygiene check
    if checks.path_hygiene {
        let violations = check_path_hygiene(&parsed);
        if violations.is_empty() {
            eprintln!("  ok: path hygiene passed");
        } else {
            for v in &violations {
                eprintln!("  FAIL: path-hygiene: {}", v);
            }
            all_violations.extend(violations);
        }
    }

    // Ordering check
    if checks.ordering {
        let violations = check_ordering(&parsed, sensor_id);
        if violations.is_empty() {
            eprintln!("  ok: ordering passed");
        } else {
            for v in &violations {
                eprintln!("  FAIL: ordering: {}", v);
            }
            all_violations.extend(violations);
        }
    }

    // Reason token lint
    if checks.reason_lint {
        let violations = check_reason_tokens(&parsed);
        if violations.is_empty() {
            eprintln!("  ok: reason-lint passed");
        } else {
            for v in &violations {
                eprintln!("  FAIL: reason-lint: {}", v);
            }
            all_violations.extend(violations);
        }
    }

    if !all_violations.is_empty() {
        anyhow::bail!(
            "conformance failed with {} violation(s)",
            all_violations.len()
        );
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn conform(
    report: PathBuf,
    golden: Option<PathBuf>,
    survivability: bool,
    path_hygiene: bool,
    ordering: bool,
    reason_lint: bool,
    all: bool,
    sensor_id: Option<String>,
) -> Result<()> {
    eprintln!("conformance check: {}", report.display());

    let content =
        fs::read_to_string(&report).with_context(|| format!("read {}", report.display()))?;

    // Determinism check (if golden provided) — before delegating to conform_single.
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

    let sid = sensor_id.as_deref().unwrap_or("unknown");
    let checks = ConformChecks {
        path_hygiene: path_hygiene || all,
        ordering: ordering || all,
        reason_lint: reason_lint || all,
        survivability,
    };

    conform_single(&content, sid, &checks)?;

    eprintln!("conformance: PASS");
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// conform-dir: validate all sensor receipts in an artifacts/ directory
// ─────────────────────────────────────────────────────────────────────────────

fn conform_dir(dir: PathBuf, validate_cockpit: bool, checks: &ConformChecks) -> Result<()> {
    use cockpitctl_types::COCKPIT_REPORT_V1_SCHEMA_JSON;

    if !dir.exists() {
        anyhow::bail!("artifacts directory does not exist: {}", dir.display());
    }

    eprintln!("conform-dir: scanning {}", dir.display());

    let mut entries: Vec<_> = fs::read_dir(&dir)
        .with_context(|| format!("read dir {}", dir.display()))?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .collect();

    entries.sort_by_key(|e| e.file_name());

    let mut results: Vec<(String, &str)> = Vec::new();
    let mut had_failure = false;

    for entry in &entries {
        let name = entry.file_name().to_string_lossy().to_string();

        // Skip cockpit subdirectory during sensor enumeration.
        if name == "cockpit" {
            continue;
        }

        let report_path = entry.path().join("report.json");
        eprintln!();
        eprintln!("--- sensor: {} ---", name);

        if !report_path.exists() {
            eprintln!("  skip: no report.json found");
            results.push((name, "skip (no report.json)"));
            continue;
        }

        let content = match fs::read_to_string(&report_path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("  FAIL: could not read report: {}", e);
                results.push((name, "FAIL (read error)"));
                had_failure = true;
                continue;
            }
        };

        match conform_single(&content, &name, checks) {
            Ok(()) => {
                results.push((name, "PASS"));
            }
            Err(e) => {
                eprintln!("  FAIL: {:#}", e);
                results.push((name, "FAIL"));
                had_failure = true;
            }
        }
    }

    // Optionally validate cockpit/report.json against cockpit.report.v1 schema.
    if validate_cockpit {
        let cockpit_report = dir.join("cockpit").join("report.json");
        eprintln!();
        eprintln!("--- cockpit report ---");

        if cockpit_report.exists() {
            let content = fs::read_to_string(&cockpit_report)
                .with_context(|| format!("read {}", cockpit_report.display()))?;

            let value: serde_json::Value = serde_json::from_str(&content)
                .with_context(|| format!("parse json {}", cockpit_report.display()))?;

            let schema: serde_json::Value = serde_json::from_str(COCKPIT_REPORT_V1_SCHEMA_JSON)
                .context("parse embedded cockpit.report.v1 schema")?;

            let validator =
                jsonschema::validator_for(&schema).context("compile cockpit.report.v1 schema")?;

            let errors: Vec<_> = validator.iter_errors(&value).collect();
            if !errors.is_empty() {
                eprintln!("  FAIL: cockpit report schema validation errors:");
                for e in &errors {
                    eprintln!("    - {}", e);
                }
                results.push(("cockpit".to_string(), "FAIL"));
                had_failure = true;
            } else {
                eprintln!("  ok: cockpit report schema validation passed");
                results.push(("cockpit".to_string(), "PASS"));
            }
        } else {
            eprintln!("  skip: no cockpit/report.json found");
            results.push(("cockpit".to_string(), "skip (not found)"));
        }
    }

    // Print summary table.
    eprintln!();
    eprintln!("Summary:");
    eprintln!("  {:<20} Status", "Sensor");
    eprintln!("  {:<20} ------", "------");
    for (name, status) in &results {
        eprintln!("  {:<20} {}", name, status);
    }

    if had_failure {
        anyhow::bail!("conform-dir: one or more sensors failed conformance checks");
    }

    eprintln!();
    eprintln!("conform-dir: PASS ({} sensor(s) checked)", results.len());
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
