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

        /// Check tool_error identity: require canonical check_id/code.
        #[arg(long)]
        tool_error_identity: bool,

        /// Validate sensor ID matches [a-zA-Z0-9_-]+.
        #[arg(long)]
        sensor_id_format: bool,

        /// Validate artifact pointer fields and path safety.
        #[arg(long)]
        artifact_pointers: bool,

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

        /// Per-report tool_error identity check.
        #[arg(long)]
        tool_error_identity: bool,

        /// Per-report sensor ID format check.
        #[arg(long)]
        sensor_id_format: bool,

        /// Per-report artifact pointer validation.
        #[arg(long)]
        artifact_pointers: bool,

        /// Validate presence semantics in cockpit report (requires --validate-cockpit).
        #[arg(long)]
        presence_semantics: bool,

        /// Allow missing report.json (skip instead of fail).
        #[arg(long)]
        allow_missing_report: bool,
    },
}

const SCHEMA_FILES: &[&str] = &[
    "sensor.report.v1.json",
    "cockpit.report.v1.json",
    "buildfix.plan.v1.json",
    "cockpit.promote.v1.json",
];

fn main_entry(cli: Cli) -> i32 {
    if let Err(e) = run(cli) {
        eprintln!("xtask error: {:#}", e);
        1
    } else {
        0
    }
}

#[cfg(not(coverage))]
fn main() {
    std::process::exit(main_entry(Cli::parse()));
}

#[cfg(coverage)]
fn main() {}

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
            tool_error_identity,
            sensor_id_format,
            artifact_pointers,
            all,
            sensor_id,
        } => conform(
            report,
            golden,
            survivability,
            path_hygiene,
            ordering,
            reason_lint,
            tool_error_identity,
            sensor_id_format,
            artifact_pointers,
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
            tool_error_identity,
            sensor_id_format,
            artifact_pointers,
            presence_semantics,
            allow_missing_report,
        } => conform_dir(
            dir,
            validate_cockpit,
            &ConformChecks {
                path_hygiene: path_hygiene || all,
                ordering: ordering || all,
                reason_lint: reason_lint || all,
                survivability: survivability || all,
                tool_error_identity: tool_error_identity || all,
                sensor_id_format: sensor_id_format || all,
                artifact_pointers: artifact_pointers || all,
            },
            allow_missing_report,
            presence_semantics || all,
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

fn check_tool_error_identity(report: &cockpitctl_types::SensorReport) -> Vec<String> {
    let mut violations = Vec::new();

    if !report.verdict.reasons.iter().any(|r| r == "tool_error") {
        return violations;
    }

    let has_canonical = report
        .findings
        .iter()
        .any(|f| f.check_id.as_deref() == Some("tool.runtime") && f.code == "runtime_error");

    if !has_canonical {
        violations.push(
            "verdict.reasons contains \"tool_error\" but no finding has check_id=\"tool.runtime\" + code=\"runtime_error\""
                .to_string(),
        );
    }

    violations
}

fn check_cockpit_reason_tokens(report: &cockpitctl_types::CockpitReport) -> Vec<String> {
    let mut violations = Vec::new();

    for (i, reason) in report.verdict.reasons.iter().enumerate() {
        if !is_valid_reason_token(reason) {
            violations.push(format!(
                "verdict.reasons[{}]: invalid token {:?}",
                i, reason
            ));
        }
    }

    for (si, sensor) in report.sensors.iter().enumerate() {
        for (ri, reason) in sensor.verdict.reasons.iter().enumerate() {
            if !is_valid_reason_token(reason) {
                violations.push(format!(
                    "sensors[{}].verdict.reasons[{}]: invalid token {:?}",
                    si, ri, reason
                ));
            }
        }
    }

    for (name, cap) in &report.run.capabilities {
        if let Some(reason) = &cap.reason
            && !is_valid_reason_token(reason)
        {
            violations.push(format!(
                "run.capabilities.{}.reason: invalid token {:?}",
                name, reason
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

fn check_sensor_id_format(sensor_id: &str) -> Vec<String> {
    let mut violations = Vec::new();
    let valid = !sensor_id.is_empty()
        && sensor_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
    if !valid {
        violations.push(format!(
            "sensor_id {:?} does not match [a-zA-Z0-9_-]+",
            sensor_id
        ));
    }
    violations
}

fn check_artifact_pointers(report: &cockpitctl_types::SensorReport) -> Vec<String> {
    let mut violations = Vec::new();
    for (i, artifact) in report.artifacts.iter().enumerate() {
        if artifact.id.is_empty() {
            violations.push(format!("artifacts[{}]: id is empty", i));
        }
        if artifact.path.is_empty() {
            violations.push(format!("artifacts[{}]: path is empty", i));
        } else {
            if artifact.path.contains("..") {
                violations.push(format!(
                    "artifacts[{}]: path contains \"..\": {}",
                    i, artifact.path
                ));
            }
            if artifact.path.starts_with('/') || artifact.path.starts_with('\\') {
                violations.push(format!(
                    "artifacts[{}]: path is absolute (starts with / or \\): {}",
                    i, artifact.path
                ));
            }
            if artifact.path.len() >= 2
                && artifact.path.as_bytes()[0].is_ascii_alphabetic()
                && artifact.path.as_bytes()[1] == b':'
            {
                violations.push(format!(
                    "artifacts[{}]: path is absolute (drive letter): {}",
                    i, artifact.path
                ));
            }
        }
        if artifact.mime.is_empty() {
            violations.push(format!("artifacts[{}]: mime is empty", i));
        }
    }
    violations
}

fn check_presence_semantics(report: &cockpitctl_types::CockpitReport) -> Vec<String> {
    use cockpitctl_types::Presence;

    let mut violations = Vec::new();
    for (i, sensor) in report.sensors.iter().enumerate() {
        if sensor.missing_policy_applied.is_some() && sensor.presence != Presence::Missing {
            violations.push(format!(
                "sensors[{}] ({}): missing_policy_applied is set but presence is {:?}, expected \"missing\"",
                i, sensor.id, sensor.presence
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
    tool_error_identity: bool,
    sensor_id_format: bool,
    artifact_pointers: bool,
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

    // Tool error identity check
    if checks.tool_error_identity {
        let violations = check_tool_error_identity(&parsed);
        if violations.is_empty() {
            eprintln!("  ok: tool-error-identity passed");
        } else {
            for v in &violations {
                eprintln!("  FAIL: tool-error-identity: {}", v);
            }
            all_violations.extend(violations);
        }
    }

    // Sensor ID format check
    if checks.sensor_id_format {
        let violations = check_sensor_id_format(sensor_id);
        if violations.is_empty() {
            eprintln!("  ok: sensor-id-format passed");
        } else {
            for v in &violations {
                eprintln!("  FAIL: sensor-id-format: {}", v);
            }
            all_violations.extend(violations);
        }
    }

    // Artifact pointers check
    if checks.artifact_pointers {
        let violations = check_artifact_pointers(&parsed);
        if violations.is_empty() {
            eprintln!("  ok: artifact-pointers passed");
        } else {
            for v in &violations {
                eprintln!("  FAIL: artifact-pointers: {}", v);
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
    tool_error_identity: bool,
    sensor_id_format: bool,
    artifact_pointers: bool,
    all: bool,
    sensor_id: Option<String>,
) -> Result<()> {
    if (ordering || all) && sensor_id.is_none() {
        anyhow::bail!("--ordering requires --sensor-id");
    }

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
        survivability: survivability || all,
        tool_error_identity: tool_error_identity || all,
        sensor_id_format: sensor_id_format || all,
        artifact_pointers: artifact_pointers || all,
    };

    conform_single(&content, sid, &checks)?;

    eprintln!("conformance: PASS");
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// conform-dir: validate all sensor receipts in an artifacts/ directory
// ─────────────────────────────────────────────────────────────────────────────

fn conform_dir(
    dir: PathBuf,
    validate_cockpit: bool,
    checks: &ConformChecks,
    allow_missing_report: bool,
    presence_semantics: bool,
) -> Result<()> {
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
            if allow_missing_report {
                eprintln!("  skip: no report.json found");
                results.push((name, "skip (no report.json)"));
            } else {
                eprintln!("  FAIL: no report.json found (use --allow-missing-report to skip)");
                results.push((name, "FAIL (no report.json)"));
                had_failure = true;
            }
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

                let mut cockpit_failed = false;
                let needs_parse = checks.reason_lint || presence_semantics;

                if needs_parse {
                    let parsed: cockpitctl_types::CockpitReport = serde_json::from_value(value)
                        .context("parse cockpit report for extended checks")?;

                    // Reason-lint on cockpit report
                    if checks.reason_lint {
                        let violations = check_cockpit_reason_tokens(&parsed);
                        if violations.is_empty() {
                            eprintln!("  ok: cockpit reason-lint passed");
                        } else {
                            for v in &violations {
                                eprintln!("  FAIL: cockpit reason-lint: {}", v);
                            }
                            cockpit_failed = true;
                            had_failure = true;
                        }
                    }

                    // Presence semantics on cockpit report
                    if presence_semantics {
                        let violations = check_presence_semantics(&parsed);
                        if violations.is_empty() {
                            eprintln!("  ok: cockpit presence-semantics passed");
                        } else {
                            for v in &violations {
                                eprintln!("  FAIL: cockpit presence-semantics: {}", v);
                            }
                            cockpit_failed = true;
                            had_failure = true;
                        }
                    }
                }

                results.push((
                    "cockpit".to_string(),
                    if cockpit_failed { "FAIL" } else { "PASS" },
                ));
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    static FS_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    struct CwdGuard(PathBuf);

    impl Drop for CwdGuard {
        fn drop(&mut self) {
            let _ = std::env::set_current_dir(&self.0);
        }
    }

    fn set_cwd(path: &std::path::Path) -> CwdGuard {
        let old = std::env::current_dir().expect("current_dir");
        std::env::set_current_dir(path).expect("set_current_dir");
        CwdGuard(old)
    }

    fn write_file(path: &std::path::Path, content: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create dirs");
        }
        fs::write(path, content).expect("write file");
    }

    fn minimal_schema_json(id: &str, title: &str) -> String {
        format!(
            r#"{{"$schema":"https://json-schema.org/draft/2020-12/schema","$id":"{}","title":"{}","type":"object"}}"#,
            id, title
        )
    }

    fn minimal_sensor_report() -> cockpitctl_types::SensorReport {
        cockpitctl_types::SensorReport {
            schema: "sensor.report.v1".to_string(),
            tool: cockpitctl_types::ToolInfo {
                name: "test".to_string(),
                version: "1.0.0".to_string(),
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
            findings: vec![],
            artifacts: vec![],
            data: None,
        }
    }

    fn minimal_sensor_report_json() -> String {
        serde_json::to_string(&minimal_sensor_report()).expect("serialize report")
    }

    fn minimal_cockpit_report() -> cockpitctl_types::CockpitReport {
        let cfg = cockpitctl_types::CockpitConfig::default();
        let policy = cockpitctl_types::PolicySnapshot {
            warn_is_fail: cfg.policy.warn_is_fail,
            max_highlights: cfg.policy.max_highlights,
            max_per_sensor_findings: cfg.policy.max_per_sensor_findings,
            max_annotations: cfg.policy.max_annotations,
            section_order: cfg.policy.section_order.clone(),
            sensors: vec![],
        };
        cockpitctl_types::CockpitReport {
            schema: "cockpit.report.v1".to_string(),
            tool: cockpitctl_types::ToolInfo {
                name: "cockpitctl".to_string(),
                version: "0.1.0".to_string(),
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
            policy,
            data: None,
        }
    }

    #[test]
    fn schema_check_ok_and_missing_fields() {
        let temp = TempDir::new().expect("tempdir");
        write_file(
            &temp.path().join("sensor.report.v1.json"),
            &minimal_schema_json("sensor.report.v1", "Sensor Report"),
        );
        write_file(
            &temp.path().join("cockpit.report.v1.json"),
            &minimal_schema_json("cockpit.report.v1", "Cockpit Report"),
        );
        schema_check(temp.path().to_path_buf()).expect("schema_check ok");

        let temp_bad = TempDir::new().expect("tempdir");
        write_file(
            &temp_bad.path().join("sensor.report.v1.json"),
            r#"{"title":"Missing id"}"#,
        );
        write_file(
            &temp_bad.path().join("cockpit.report.v1.json"),
            &minimal_schema_json("cockpit.report.v1", "Cockpit Report"),
        );
        let err = schema_check(temp_bad.path().to_path_buf()).expect_err("schema_check err");
        assert!(format!("{:#}", err).contains("schema missing"));
    }

    #[test]
    fn schema_sync_check_and_fix() {
        let _guard = FS_LOCK.lock().unwrap();
        let temp = TempDir::new().expect("tempdir");
        let _cwd = set_cwd(temp.path());

        for &name in SCHEMA_FILES {
            let content = format!("{{\"name\":\"{}\"}}", name);
            write_file(&temp.path().join("contracts/schemas").join(name), &content);
            write_file(
                &temp
                    .path()
                    .join("crates/cockpitctl-types/schemas")
                    .join(name),
                &content,
            );
        }
        schema_sync_check().expect("schema_sync_check ok");

        write_file(
            &temp
                .path()
                .join("crates/cockpitctl-types/schemas")
                .join(SCHEMA_FILES[0]),
            r#"{"mismatch":true}"#,
        );
        let err = schema_sync_check().expect_err("schema_sync_check mismatch");
        assert!(format!("{:#}", err).contains("out of sync"));

        schema_sync_fix().expect("schema_sync_fix");
        for &name in SCHEMA_FILES {
            let src = fs::read_to_string(temp.path().join("contracts/schemas").join(name))
                .expect("read src");
            let dst = fs::read_to_string(
                temp.path()
                    .join("crates/cockpitctl-types/schemas")
                    .join(name),
            )
            .expect("read dst");
            assert_eq!(src, dst);
        }
    }

    #[test]
    fn fixtures_help_runs() {
        fixtures_help().expect("fixtures_help");
    }

    #[test]
    fn validate_schemas_ok_error_and_fix() {
        let temp_ok = TempDir::new().expect("tempdir");
        write_file(
            &temp_ok.path().join("ok.json"),
            &minimal_schema_json("ok.schema", "OK"),
        );
        validate_schemas(temp_ok.path().to_path_buf(), false).expect("validate_schemas ok");

        let temp_err = TempDir::new().expect("tempdir");
        write_file(&temp_err.path().join("bad.json"), "{");
        let err = validate_schemas(temp_err.path().to_path_buf(), false)
            .expect_err("validate_schemas error");
        assert!(format!("{:#}", err).contains("schema validation failed"));

        let temp_fix = TempDir::new().expect("tempdir");
        write_file(
            &temp_fix.path().join("fix.json"),
            r#"{"$schema":"https://json-schema.org/draft/2020-12/schema","$id":"fix.schema","title":"Fix","type":"object"}"#,
        );
        let formatted_value: serde_json::Value =
            serde_json::from_str(&minimal_schema_json("already.schema", "Already"))
                .expect("parse schema");
        let formatted = serde_json::to_string_pretty(&formatted_value).expect("format schema");
        let formatted_with_newline = format!("{}\n", formatted);
        write_file(
            &temp_fix.path().join("already.json"),
            &formatted_with_newline,
        );
        validate_schemas(temp_fix.path().to_path_buf(), true).expect("validate_schemas fix");
        let fixed = fs::read_to_string(temp_fix.path().join("fix.json")).expect("read fixed");
        assert!(fixed.ends_with('\n'));
        assert!(fixed.contains('\n'));
    }

    #[test]
    fn reason_token_and_sensor_id_checks() {
        assert!(is_valid_reason_token("ok_token"));
        assert!(!is_valid_reason_token("Bad-Token"));

        assert!(check_sensor_id_format("good_id").is_empty());
        assert!(!check_sensor_id_format("bad.id").is_empty());
    }

    #[test]
    fn path_hygiene_and_ordering_checks() {
        let mut report = minimal_sensor_report();
        report.findings = vec![
            cockpitctl_types::Finding {
                severity: cockpitctl_types::Severity::Info,
                check_id: None,
                code: "I1".to_string(),
                message: "info".to_string(),
                location: Some(cockpitctl_types::Location {
                    path: Some("/abs/path".to_string()),
                    line: Some(1),
                    col: None,
                }),
                help: None,
                url: None,
                fingerprint: None,
                data: None,
            },
            cockpitctl_types::Finding {
                severity: cockpitctl_types::Severity::Warn,
                check_id: None,
                code: "W1".to_string(),
                message: "warn".to_string(),
                location: Some(cockpitctl_types::Location {
                    path: Some("C:\\temp\\file.rs".to_string()),
                    line: Some(2),
                    col: None,
                }),
                help: None,
                url: None,
                fingerprint: None,
                data: None,
            },
            cockpitctl_types::Finding {
                severity: cockpitctl_types::Severity::Error,
                check_id: None,
                code: "E1".to_string(),
                message: "err".to_string(),
                location: Some(cockpitctl_types::Location {
                    path: Some("src/../file.rs".to_string()),
                    line: Some(3),
                    col: None,
                }),
                help: None,
                url: None,
                fingerprint: None,
                data: None,
            },
            cockpitctl_types::Finding {
                severity: cockpitctl_types::Severity::Info,
                check_id: None,
                code: "I2".to_string(),
                message: "no path".to_string(),
                location: Some(cockpitctl_types::Location {
                    path: None,
                    line: None,
                    col: None,
                }),
                help: None,
                url: None,
                fingerprint: None,
                data: None,
            },
        ];

        let violations = check_path_hygiene(&report);
        assert!(violations.iter().any(|v| v.contains("absolute path")));
        assert!(violations.iter().any(|v| v.contains("drive letter")));
        assert!(violations.iter().any(|v| v.contains("path traversal")));
        assert!(violations.iter().any(|v| v.contains("backslash")));

        let mut ordering_report = minimal_sensor_report();
        ordering_report.findings = vec![
            cockpitctl_types::Finding {
                severity: cockpitctl_types::Severity::Info,
                check_id: None,
                code: "I1".to_string(),
                message: "info".to_string(),
                location: None,
                help: None,
                url: None,
                fingerprint: None,
                data: None,
            },
            cockpitctl_types::Finding {
                severity: cockpitctl_types::Severity::Error,
                check_id: None,
                code: "E1".to_string(),
                message: "err".to_string(),
                location: None,
                help: None,
                url: None,
                fingerprint: None,
                data: None,
            },
        ];
        let ordering = check_ordering(&ordering_report, "sensor");
        assert_eq!(ordering.len(), 1);
    }

    #[test]
    fn tool_error_identity_and_reason_lint_checks() {
        let mut report = minimal_sensor_report();
        assert!(check_tool_error_identity(&report).is_empty());

        report.verdict.reasons = vec!["tool_error".to_string()];
        let violations = check_tool_error_identity(&report);
        assert!(!violations.is_empty());

        report.findings.push(cockpitctl_types::Finding {
            severity: cockpitctl_types::Severity::Error,
            check_id: Some("tool.runtime".to_string()),
            code: "runtime_error".to_string(),
            message: "boom".to_string(),
            location: None,
            help: None,
            url: None,
            fingerprint: None,
            data: None,
        });
        assert!(check_tool_error_identity(&report).is_empty());

        report.verdict.reasons = vec!["Bad-Token".to_string()];
        report.run.capabilities.insert(
            "git".to_string(),
            cockpitctl_types::Capability {
                status: cockpitctl_types::CapabilityStatus::Available,
                reason: Some("Bad-Token".to_string()),
            },
        );
        let reasons = check_reason_tokens(&report);
        assert!(reasons.len() >= 2);
    }

    #[test]
    fn cockpit_reason_tokens_and_presence_semantics_checks() {
        let mut report = minimal_cockpit_report();
        report.verdict.reasons = vec!["Bad-Token".to_string()];
        report.run.capabilities.insert(
            "git".to_string(),
            cockpitctl_types::Capability {
                status: cockpitctl_types::CapabilityStatus::Available,
                reason: Some("Bad-Token".to_string()),
            },
        );
        report.sensors.push(cockpitctl_types::SensorSummary {
            id: "sensor".to_string(),
            blocking: true,
            missing: cockpitctl_types::MissingPolicy::Fail,
            presence: cockpitctl_types::Presence::Present,
            report_path: "artifacts/sensor/report.json".to_string(),
            comment_path: None,
            verdict: cockpitctl_types::Verdict {
                status: cockpitctl_types::VerdictStatus::Pass,
                counts: cockpitctl_types::VerdictCounts::default(),
                reasons: vec!["Bad-Token".to_string()],
            },
            truncated: false,
            errors: vec![],
            missing_policy_applied: Some(cockpitctl_types::MissingPolicy::Skip),
            policy_outcome: None,
        });

        let reason_violations = check_cockpit_reason_tokens(&report);
        assert!(reason_violations.len() >= 3);

        let presence_violations = check_presence_semantics(&report);
        assert_eq!(presence_violations.len(), 1);
    }

    #[test]
    fn artifact_pointer_checks() {
        let mut report = minimal_sensor_report();
        report.artifacts = vec![
            cockpitctl_types::ArtifactPointer {
                id: "".to_string(),
                path: "".to_string(),
                mime: "".to_string(),
                schema: None,
            },
            cockpitctl_types::ArtifactPointer {
                id: "ok".to_string(),
                path: "../bad".to_string(),
                mime: "text/plain".to_string(),
                schema: None,
            },
            cockpitctl_types::ArtifactPointer {
                id: "abs".to_string(),
                path: "/abs/path.txt".to_string(),
                mime: "text/plain".to_string(),
                schema: None,
            },
            cockpitctl_types::ArtifactPointer {
                id: "drive".to_string(),
                path: "C:\\abs\\path.txt".to_string(),
                mime: "text/plain".to_string(),
                schema: None,
            },
        ];
        let violations = check_artifact_pointers(&report);
        assert!(violations.len() >= 4);
    }

    #[test]
    fn conform_single_success_and_failure_paths() {
        let checks = ConformChecks {
            path_hygiene: false,
            ordering: false,
            reason_lint: false,
            survivability: false,
            tool_error_identity: false,
            sensor_id_format: false,
            artifact_pointers: false,
        };
        let ok = conform_single(&minimal_sensor_report_json(), "sensor", &checks);
        assert!(ok.is_ok());

        let err = conform_single("{}", "sensor", &checks).expect_err("schema error");
        assert!(format!("{:#}", err).contains("schema validation failed"));

        let mut fail_report = minimal_sensor_report();
        fail_report.verdict.status = cockpitctl_types::VerdictStatus::Fail;
        let fail_json = serde_json::to_string(&fail_report).expect("serialize");
        let checks_survivability = ConformChecks {
            path_hygiene: false,
            ordering: false,
            reason_lint: false,
            survivability: true,
            tool_error_identity: false,
            sensor_id_format: false,
            artifact_pointers: false,
        };
        let err = conform_single(&fail_json, "sensor", &checks_survivability)
            .expect_err("survivability error");
        assert!(format!("{:#}", err).contains("survivability"));

        let mut ordering_report = minimal_sensor_report();
        ordering_report.findings = vec![
            cockpitctl_types::Finding {
                severity: cockpitctl_types::Severity::Info,
                check_id: None,
                code: "I1".to_string(),
                message: "info".to_string(),
                location: None,
                help: None,
                url: None,
                fingerprint: None,
                data: None,
            },
            cockpitctl_types::Finding {
                severity: cockpitctl_types::Severity::Error,
                check_id: None,
                code: "E1".to_string(),
                message: "err".to_string(),
                location: None,
                help: None,
                url: None,
                fingerprint: None,
                data: None,
            },
        ];
        let ordering_json = serde_json::to_string(&ordering_report).expect("serialize");
        let checks_ordering = ConformChecks {
            path_hygiene: false,
            ordering: true,
            reason_lint: false,
            survivability: false,
            tool_error_identity: false,
            sensor_id_format: false,
            artifact_pointers: false,
        };
        let err =
            conform_single(&ordering_json, "sensor", &checks_ordering).expect_err("ordering error");
        assert!(format!("{:#}", err).contains("violation"));
    }

    #[test]
    fn conform_requires_sensor_id_for_ordering_and_handles_golden() {
        let temp = TempDir::new().expect("tempdir");
        let report_path = temp.path().join("report.json");
        write_file(&report_path, &minimal_sensor_report_json());

        let err = conform(
            report_path.clone(),
            None,
            false,
            false,
            true,
            false,
            false,
            false,
            false,
            false,
            None,
        )
        .expect_err("ordering requires sensor_id");
        assert!(format!("{:#}", err).contains("requires --sensor-id"));

        let golden_path = temp.path().join("golden.json");
        write_file(&golden_path, "not equal");
        let err = conform(
            report_path.clone(),
            Some(golden_path),
            false,
            false,
            false,
            false,
            false,
            false,
            false,
            false,
            Some("sensor".to_string()),
        )
        .expect_err("golden mismatch");
        assert!(format!("{:#}", err).contains("determinism check failed"));

        let golden_ok = temp.path().join("golden_ok.json");
        let content = minimal_sensor_report_json();
        write_file(&report_path, &content);
        write_file(&golden_ok, &content);
        conform(
            report_path,
            Some(golden_ok),
            false,
            false,
            false,
            false,
            false,
            false,
            false,
            false,
            Some("sensor".to_string()),
        )
        .expect("conform ok");
    }

    #[test]
    fn conform_dir_handles_missing_and_cockpit_validation() {
        let missing_dir = TempDir::new().expect("tempdir");
        let err = conform_dir(
            missing_dir.path().join("does_not_exist"),
            false,
            &ConformChecks {
                path_hygiene: false,
                ordering: false,
                reason_lint: false,
                survivability: false,
                tool_error_identity: false,
                sensor_id_format: false,
                artifact_pointers: false,
            },
            false,
            false,
        )
        .expect_err("missing dir");
        assert!(format!("{:#}", err).contains("does not exist"));

        let temp = TempDir::new().expect("tempdir");
        let artifacts = temp.path();
        let ok_sensor = artifacts.join("ok");
        write_file(
            &ok_sensor.join("report.json"),
            &minimal_sensor_report_json(),
        );
        fs::create_dir_all(artifacts.join("missing")).expect("create missing sensor dir");

        let checks = ConformChecks {
            path_hygiene: false,
            ordering: false,
            reason_lint: false,
            survivability: false,
            tool_error_identity: false,
            sensor_id_format: false,
            artifact_pointers: false,
        };
        conform_dir(artifacts.to_path_buf(), false, &checks, true, false)
            .expect("allow missing report");

        let err = conform_dir(artifacts.to_path_buf(), false, &checks, false, false)
            .expect_err("missing report should fail");
        assert!(format!("{:#}", err).contains("failed"));

        let temp2 = TempDir::new().expect("tempdir");
        let artifacts2 = temp2.path();
        let ok_sensor2 = artifacts2.join("ok");
        write_file(
            &ok_sensor2.join("report.json"),
            &minimal_sensor_report_json(),
        );
        conform_dir(artifacts2.to_path_buf(), true, &checks, true, false)
            .expect("validate cockpit skip when missing");

        let temp3 = TempDir::new().expect("tempdir");
        let artifacts3 = temp3.path();
        let ok_sensor3 = artifacts3.join("ok");
        write_file(
            &ok_sensor3.join("report.json"),
            &minimal_sensor_report_json(),
        );
        write_file(&artifacts3.join("cockpit").join("report.json"), "{}");
        let err = conform_dir(artifacts3.to_path_buf(), true, &checks, true, false)
            .expect_err("invalid cockpit report should fail");
        assert!(format!("{:#}", err).contains("conform-dir"));
    }

    #[test]
    fn conform_dir_cockpit_reason_lint_and_presence_semantics() {
        let temp = TempDir::new().expect("tempdir");
        let artifacts = temp.path();
        let ok_sensor = artifacts.join("ok");
        write_file(
            &ok_sensor.join("report.json"),
            &minimal_sensor_report_json(),
        );

        let mut cockpit_report = minimal_cockpit_report();
        cockpit_report.verdict.reasons = vec!["Bad-Token".to_string()];
        cockpit_report
            .sensors
            .push(cockpitctl_types::SensorSummary {
                id: "sensor".to_string(),
                blocking: true,
                missing: cockpitctl_types::MissingPolicy::Fail,
                presence: cockpitctl_types::Presence::Present,
                report_path: "artifacts/sensor/report.json".to_string(),
                comment_path: None,
                verdict: cockpitctl_types::Verdict {
                    status: cockpitctl_types::VerdictStatus::Pass,
                    counts: cockpitctl_types::VerdictCounts::default(),
                    reasons: vec!["Bad-Token".to_string()],
                },
                truncated: false,
                errors: vec![],
                missing_policy_applied: Some(cockpitctl_types::MissingPolicy::Skip),
                policy_outcome: None,
            });

        write_file(
            &artifacts.join("cockpit").join("report.json"),
            &serde_json::to_string(&cockpit_report).expect("serialize cockpit report"),
        );

        let checks = ConformChecks {
            path_hygiene: false,
            ordering: false,
            reason_lint: true,
            survivability: false,
            tool_error_identity: false,
            sensor_id_format: false,
            artifact_pointers: false,
        };
        let err = conform_dir(artifacts.to_path_buf(), true, &checks, true, true)
            .expect_err("cockpit reason/presence checks should fail");
        assert!(format!("{:#}", err).contains("failed"));
    }

    #[test]
    fn main_entry_dispatches_commands() {
        let _lock = FS_LOCK.lock().unwrap();
        let temp = TempDir::new().expect("tempdir");
        let root = temp.path();

        let schema_dir = root.join("schemas");
        write_file(
            &schema_dir.join("sensor.report.v1.json"),
            &minimal_schema_json("sensor.report.v1", "Sensor Report"),
        );
        write_file(
            &schema_dir.join("cockpit.report.v1.json"),
            &minimal_schema_json("cockpit.report.v1", "Cockpit Report"),
        );

        let contracts = root.join("contracts").join("schemas");
        let types = root.join("crates").join("cockpitctl-types").join("schemas");
        for name in SCHEMA_FILES {
            write_file(&contracts.join(name), &minimal_schema_json(name, "Schema"));
            write_file(&types.join(name), &minimal_schema_json(name, "Schema"));
        }

        let report_path = root.join("report.json");
        write_file(&report_path, &minimal_sensor_report_json());

        let artifacts = root.join("artifacts");
        write_file(
            &artifacts.join("sensor").join("report.json"),
            &minimal_sensor_report_json(),
        );

        let _cwd = set_cwd(root);

        assert_eq!(
            main_entry(Cli {
                command: Commands::FixturesHelp
            }),
            0
        );
        assert_eq!(
            main_entry(Cli {
                command: Commands::SchemaCheck {
                    dir: schema_dir.clone(),
                },
            }),
            0
        );
        assert_eq!(
            main_entry(Cli {
                command: Commands::ValidateSchemas {
                    dir: schema_dir.clone(),
                    fix: false,
                },
            }),
            0
        );
        assert_eq!(
            main_entry(Cli {
                command: Commands::SchemaSyncCheck
            }),
            0
        );
        assert_eq!(
            main_entry(Cli {
                command: Commands::SchemaSyncFix
            }),
            0
        );
        assert_eq!(
            main_entry(Cli {
                command: Commands::Conform {
                    report: report_path.clone(),
                    golden: None,
                    survivability: false,
                    path_hygiene: false,
                    ordering: false,
                    reason_lint: false,
                    tool_error_identity: false,
                    sensor_id_format: false,
                    artifact_pointers: false,
                    all: false,
                    sensor_id: None,
                },
            }),
            0
        );
        assert_eq!(
            main_entry(Cli {
                command: Commands::ConformDir {
                    dir: artifacts.clone(),
                    validate_cockpit: false,
                    all: false,
                    path_hygiene: false,
                    ordering: false,
                    reason_lint: false,
                    survivability: false,
                    tool_error_identity: false,
                    sensor_id_format: false,
                    artifact_pointers: false,
                    presence_semantics: false,
                    allow_missing_report: true,
                },
            }),
            0
        );
    }

    #[test]
    fn main_entry_returns_1_on_error() {
        let temp = TempDir::new().expect("tempdir");
        let missing = temp.path().join("missing");
        let code = main_entry(Cli {
            command: Commands::SchemaCheck { dir: missing },
        });
        assert_eq!(code, 1);
    }

    #[test]
    fn conform_single_survivability_branches() {
        let checks = ConformChecks {
            path_hygiene: false,
            ordering: false,
            reason_lint: false,
            survivability: true,
            tool_error_identity: false,
            sensor_id_format: false,
            artifact_pointers: false,
        };

        let mut fail_report = minimal_sensor_report();
        fail_report.verdict.status = cockpitctl_types::VerdictStatus::Fail;
        fail_report.findings.push(cockpitctl_types::Finding {
            severity: cockpitctl_types::Severity::Error,
            check_id: None,
            code: "E1".to_string(),
            message: "fail".to_string(),
            location: None,
            help: None,
            url: None,
            fingerprint: None,
            data: None,
        });
        let fail_json = serde_json::to_string(&fail_report).expect("serialize");
        conform_single(&fail_json, "sensor", &checks).expect("survivability ok");

        let pass_report = minimal_sensor_report_json();
        conform_single(&pass_report, "sensor", &checks).expect("survivability skipped");
    }

    #[test]
    fn conform_single_ordering_passes_with_no_findings() {
        let checks = ConformChecks {
            path_hygiene: false,
            ordering: true,
            reason_lint: false,
            survivability: false,
            tool_error_identity: false,
            sensor_id_format: false,
            artifact_pointers: false,
        };
        conform_single(&minimal_sensor_report_json(), "sensor", &checks)
            .expect("ordering should pass with empty findings");
    }

    #[test]
    fn conform_single_reports_multiple_violations() {
        let mut report = minimal_sensor_report();
        report.findings = vec![cockpitctl_types::Finding {
            severity: cockpitctl_types::Severity::Warn,
            check_id: None,
            code: "W1".to_string(),
            message: "warn".to_string(),
            location: Some(cockpitctl_types::Location {
                path: Some("../bad/path.rs".to_string()),
                line: Some(1),
                col: None,
            }),
            help: None,
            url: None,
            fingerprint: None,
            data: None,
        }];
        report.verdict.reasons = vec!["Bad-Token".to_string(), "tool_error".to_string()];
        report.artifacts = vec![cockpitctl_types::ArtifactPointer {
            id: "abs".to_string(),
            path: "/abs/path.txt".to_string(),
            mime: "text/plain".to_string(),
            schema: None,
        }];

        let checks = ConformChecks {
            path_hygiene: true,
            ordering: false,
            reason_lint: true,
            survivability: false,
            tool_error_identity: true,
            sensor_id_format: true,
            artifact_pointers: true,
        };

        let json = serde_json::to_string(&report).expect("serialize");
        let err = conform_single(&json, "bad.id", &checks).expect_err("violations expected");
        assert!(format!("{:#}", err).contains("conformance failed"));
    }

    #[test]
    fn conform_single_ok_branches_for_checks() {
        let mut report = minimal_sensor_report();
        report.findings = vec![cockpitctl_types::Finding {
            severity: cockpitctl_types::Severity::Info,
            check_id: None,
            code: "I1".to_string(),
            message: "info".to_string(),
            location: Some(cockpitctl_types::Location {
                path: Some("src/main.rs".to_string()),
                line: Some(1),
                col: None,
            }),
            help: None,
            url: None,
            fingerprint: None,
            data: None,
        }];
        report.artifacts = vec![cockpitctl_types::ArtifactPointer {
            id: "log".to_string(),
            path: "artifacts/log.txt".to_string(),
            mime: "text/plain".to_string(),
            schema: None,
        }];

        let checks = ConformChecks {
            path_hygiene: true,
            ordering: false,
            reason_lint: false,
            survivability: false,
            tool_error_identity: true,
            sensor_id_format: true,
            artifact_pointers: true,
        };

        let json = serde_json::to_string(&report).expect("serialize");
        conform_single(&json, "good_id", &checks).expect("checks should pass");
    }

    #[test]
    fn conform_golden_matches_and_mismatch() {
        let temp = TempDir::new().expect("tempdir");
        let report_path = temp.path().join("report.json");
        let golden_path = temp.path().join("golden.json");
        let content = minimal_sensor_report_json();
        write_file(&report_path, &content);
        write_file(&golden_path, &content);

        conform(
            report_path.clone(),
            Some(golden_path.clone()),
            false,
            false,
            false,
            false,
            false,
            false,
            false,
            false,
            None,
        )
        .expect("golden match");

        write_file(&golden_path, r#"{"schema":"sensor.report.v1"}"#);
        let err = conform(
            report_path,
            Some(golden_path),
            false,
            false,
            false,
            false,
            false,
            false,
            false,
            false,
            None,
        )
        .expect_err("golden mismatch");
        assert!(format!("{:#}", err).contains("determinism check failed"));
    }

    #[test]
    fn conform_dir_read_error_and_conform_failure() {
        let temp = TempDir::new().expect("tempdir");
        let artifacts = temp.path();

        // read_to_string error: report.json is a directory.
        fs::create_dir_all(artifacts.join("bad").join("report.json")).expect("create report dir");
        let checks = ConformChecks {
            path_hygiene: false,
            ordering: false,
            reason_lint: false,
            survivability: false,
            tool_error_identity: false,
            sensor_id_format: false,
            artifact_pointers: false,
        };
        let err = conform_dir(artifacts.to_path_buf(), false, &checks, true, false)
            .expect_err("read error");
        assert!(format!("{:#}", err).contains("conform-dir"));

        // conform_single error: invalid JSON
        let temp2 = TempDir::new().expect("tempdir");
        let artifacts2 = temp2.path();
        write_file(&artifacts2.join("bad").join("report.json"), "{");
        let err = conform_dir(artifacts2.to_path_buf(), false, &checks, true, false)
            .expect_err("conform_single error");
        assert!(format!("{:#}", err).contains("conform-dir"));
    }

    #[test]
    fn conform_dir_cockpit_checks_pass() {
        let temp = TempDir::new().expect("tempdir");
        let artifacts = temp.path();
        write_file(
            &artifacts.join("ok").join("report.json"),
            &minimal_sensor_report_json(),
        );

        let cockpit = minimal_cockpit_report();
        let cockpit_json = serde_json::to_string(&cockpit).expect("serialize cockpit");
        write_file(
            &artifacts.join("cockpit").join("report.json"),
            &cockpit_json,
        );

        let checks_reason_on = ConformChecks {
            path_hygiene: false,
            ordering: false,
            reason_lint: true,
            survivability: false,
            tool_error_identity: false,
            sensor_id_format: false,
            artifact_pointers: false,
        };

        conform_dir(artifacts.to_path_buf(), true, &checks_reason_on, true, true)
            .expect("cockpit checks pass");
        conform_dir(artifacts.to_path_buf(), true, &checks_reason_on, true, false)
            .expect("cockpit checks pass without presence semantics");

        let checks_reason_off = ConformChecks {
            path_hygiene: false,
            ordering: false,
            reason_lint: false,
            survivability: false,
            tool_error_identity: false,
            sensor_id_format: false,
            artifact_pointers: false,
        };
        conform_dir(artifacts.to_path_buf(), true, &checks_reason_off, true, true)
            .expect("cockpit checks pass without reason lint");
        conform_dir(artifacts.to_path_buf(), true, &checks_reason_off, true, false)
            .expect("cockpit checks pass with no extended checks");
    }

    #[test]
    fn validate_schemas_additional_error_paths() {
        let missing = TempDir::new().expect("tempdir");
        let err = validate_schemas(missing.path().join("nope"), false).expect_err("missing dir");
        assert!(format!("{:#}", err).contains("does not exist"));

        let temp = TempDir::new().expect("tempdir");
        let dir = temp.path();
        write_file(&dir.join("note.txt"), "skip me");
        fs::create_dir_all(dir.join("subdir.json")).expect("dir with .json extension");

        // File that will fail to read (locked on Windows, permissions on Unix).
        let locked = dir.join("locked.json");
        write_file(&locked, &minimal_schema_json("locked.schema", "Locked"));
        #[cfg(windows)]
        let _lock = {
            use std::fs::OpenOptions;
            use std::os::windows::fs::OpenOptionsExt;
            OpenOptions::new()
                .read(true)
                .share_mode(0)
                .open(&locked)
                .expect("lock file")
        };
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&locked).expect("meta").permissions();
            perms.set_mode(0o000);
            fs::set_permissions(&locked, perms).expect("chmod");
        }

        write_file(&dir.join("invalid.json"), "{");
        write_file(
            &dir.join("invalid_schema.json"),
            r#"{"$schema":"https://json-schema.org/draft/2020-12/schema","title":"Bad","type":123}"#,
        );
        write_file(
            &dir.join("no_schema.json"),
            r#"{"$id":"noschema","title":"NoSchema","type":"object"}"#,
        );

        let err = validate_schemas(dir.to_path_buf(), false).expect_err("validation errors");
        assert!(format!("{:#}", err).contains("schema validation failed"));
    }

    #[cfg(coverage)]
    #[test]
    fn main_noop_executes() {
        super::main();
    }
}
