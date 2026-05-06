use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use cockpitctl_conform::ConformChecks;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use time::{Date, Month, OffsetDateTime};

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

    /// Check that crate-local cockpit.toml.example matches workspace root copy.
    ExampleSyncCheck,

    /// Validate Clippy policy ledgers, workspace lint metadata, and debt hygiene.
    CheckLintPolicy,

    /// Copy cockpit.toml.example → crates/cockpitctl-cli/cockpit.toml.example.
    ExampleSyncFix,

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
        Commands::ExampleSyncCheck => example_sync_check(),
        Commands::CheckLintPolicy => check_lint_policy(),
        Commands::ExampleSyncFix => example_sync_fix(),
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

fn example_sync_check() -> Result<()> {
    let src = PathBuf::from("cockpit.toml.example");
    let dst = PathBuf::from("crates/cockpitctl-cli/cockpit.toml.example");

    let src_bytes = fs::read(&src).with_context(|| format!("read {}", src.display()))?;
    let dst_bytes = fs::read(&dst).with_context(|| format!("read {}", dst.display()))?;

    if src_bytes != dst_bytes {
        anyhow::bail!(
            "example-sync-check: cockpit.toml.example out of sync — run `cargo run -p xtask -- example-sync-fix`"
        );
    }

    eprintln!("example-sync-check: cockpit.toml.example in sync");
    Ok(())
}

fn example_sync_fix() -> Result<()> {
    let src = PathBuf::from("cockpit.toml.example");
    let dst = PathBuf::from("crates/cockpitctl-cli/cockpit.toml.example");

    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }

    fs::copy(&src, &dst).with_context(|| format!("copy {} → {}", src.display(), dst.display()))?;
    eprintln!("example-sync-fix: copied cockpit.toml.example");
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

fn check_lint_policy() -> Result<()> {
    let cargo = read_toml_file(Path::new("Cargo.toml"))?;
    let policy = read_toml_file(Path::new("policy/clippy-lints.toml"))?;
    let debt = read_toml_file(Path::new("policy/clippy-debt.toml"))?;

    let workspace_msrv = string_at(&cargo, &["workspace", "package", "rust-version"])
        .context("Cargo.toml missing workspace.package.rust-version")?;
    let policy_msrv =
        string_at(&policy, &["msrv"]).context("policy/clippy-lints.toml missing msrv")?;
    if workspace_msrv != policy_msrv {
        anyhow::bail!(
            "lint policy MSRV mismatch: Cargo.toml has {}, policy has {}",
            workspace_msrv,
            policy_msrv
        );
    }

    check_policy_header(&policy)?;
    let workspace_lints = workspace_lint_levels(&cargo)?;
    let policy_lints = policy_lints(&policy)?;
    check_active_lints_match_workspace(&workspace_lints, &policy_lints)?;
    check_planned_lints(&workspace_lints, &policy_lints, policy_msrv)?;
    check_no_test_carveouts(Path::new("clippy.toml"))?;
    check_clippy_debt(&debt)?;
    check_panic_allowlist_shape(Path::new("policy/no-panic-allowlist.toml"))?;
    check_non_rust_allowlist_shape(Path::new("policy/non-rust-allowlist.toml"))?;

    eprintln!(
        "check-lint-policy: PASS ({} active lint(s), {} planned lint(s))",
        policy_lints
            .values()
            .filter(|lint| lint.status == "active")
            .count(),
        policy_lints
            .values()
            .filter(|lint| lint.status == "planned")
            .count()
    );
    Ok(())
}

#[derive(Debug)]
struct PolicyLint {
    level: String,
    status: String,
    activate_when_msrv: Option<String>,
}

fn read_toml_file(path: &Path) -> Result<toml::Value> {
    let text = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    toml::from_str(&text).with_context(|| format!("parse TOML {}", path.display()))
}

fn string_at<'a>(value: &'a toml::Value, path: &[&str]) -> Option<&'a str> {
    let mut current = value;
    for segment in path {
        current = current.get(*segment)?;
    }
    current.as_str()
}

fn array_at<'a>(value: &'a toml::Value, path: &[&str]) -> Option<&'a Vec<toml::Value>> {
    let mut current = value;
    for segment in path {
        current = current.get(*segment)?;
    }
    current.as_array()
}

fn check_policy_header(policy: &toml::Value) -> Result<()> {
    let schema = policy
        .get("schema")
        .and_then(toml::Value::as_integer)
        .context("policy/clippy-lints.toml missing integer schema")?;
    if schema != 1 {
        anyhow::bail!("unsupported clippy policy schema: {}", schema);
    }

    let policy_table = policy
        .get("policy")
        .and_then(toml::Value::as_table)
        .context("policy/clippy-lints.toml missing [policy]")?;

    let required_bools = [
        ("panic_free_tests", true),
        ("allow_test_carveouts", false),
        ("blanket_categories", false),
    ];
    for (key, expected) in required_bools {
        let actual = policy_table
            .get(key)
            .and_then(toml::Value::as_bool)
            .with_context(|| format!("policy.clippy-lints missing boolean policy.{key}"))?;
        if actual != expected {
            anyhow::bail!("policy.{key} must be {expected}");
        }
    }

    let suppression_style = policy_table
        .get("suppression_style")
        .and_then(toml::Value::as_str)
        .context("policy.clippy-lints missing policy.suppression_style")?;
    if suppression_style != "expect-with-reason" {
        anyhow::bail!("policy.suppression_style must be expect-with-reason");
    }

    Ok(())
}

fn workspace_lint_levels(cargo: &toml::Value) -> Result<BTreeMap<String, String>> {
    let mut lints = BTreeMap::new();
    let workspace_lints = cargo
        .get("workspace")
        .and_then(|workspace| workspace.get("lints"))
        .and_then(toml::Value::as_table)
        .context("Cargo.toml missing [workspace.lints]")?;

    for (tool, table_value) in workspace_lints {
        let table = table_value
            .as_table()
            .with_context(|| format!("workspace.lints.{tool} must be a table"))?;
        for (lint_name, level_value) in table {
            let name = if tool == "clippy" {
                format!("clippy::{lint_name}")
            } else {
                lint_name.to_string()
            };
            let level = match level_value {
                toml::Value::String(level) => level.clone(),
                toml::Value::Table(table) => table
                    .get("level")
                    .and_then(toml::Value::as_str)
                    .with_context(|| format!("workspace lint {name} table missing level"))?
                    .to_string(),
                _ => anyhow::bail!("workspace lint {name} must be a string or level table"),
            };
            lints.insert(name, level);
        }
    }

    Ok(lints)
}

fn policy_lints(policy: &toml::Value) -> Result<BTreeMap<String, PolicyLint>> {
    let entries =
        array_at(policy, &["lint"]).context("policy/clippy-lints.toml missing [[lint]] entries")?;
    let mut lints = BTreeMap::new();
    for (index, entry) in entries.iter().enumerate() {
        let table = entry
            .as_table()
            .with_context(|| format!("lint entry {index} must be a table"))?;
        let name = required_str(table, "name", "lint", index)?.to_string();
        let level = required_str(table, "level", "lint", index)?.to_string();
        let status = required_str(table, "status", "lint", index)?.to_string();
        if status != "active" && status != "planned" {
            anyhow::bail!("lint {name} status must be active or planned");
        }
        let class = required_str(table, "class", "lint", index)?;
        let reason = required_str(table, "reason", "lint", index)?;
        if class.trim().is_empty() || reason.trim().is_empty() {
            anyhow::bail!("lint {name} class and reason must be non-empty");
        }
        let activate_when_msrv = table
            .get("activate_when_msrv")
            .and_then(toml::Value::as_str)
            .map(ToString::to_string);
        if status == "planned" && activate_when_msrv.is_none() {
            anyhow::bail!("planned lint {name} missing activate_when_msrv");
        }
        if lints
            .insert(
                name.clone(),
                PolicyLint {
                    level,
                    status,
                    activate_when_msrv,
                },
            )
            .is_some()
        {
            anyhow::bail!("duplicate lint policy entry: {name}");
        }
    }
    Ok(lints)
}

fn required_str<'a>(
    table: &'a toml::map::Map<String, toml::Value>,
    key: &str,
    kind: &str,
    index: usize,
) -> Result<&'a str> {
    table
        .get(key)
        .and_then(toml::Value::as_str)
        .with_context(|| format!("{kind} entry {index} missing string {key}"))
}

fn check_active_lints_match_workspace(
    workspace_lints: &BTreeMap<String, String>,
    policy_lints: &BTreeMap<String, PolicyLint>,
) -> Result<()> {
    let active: BTreeMap<_, _> = policy_lints
        .iter()
        .filter(|(_, lint)| lint.status == "active")
        .map(|(name, lint)| (name, &lint.level))
        .collect();

    for (name, level) in &active {
        match workspace_lints.get(*name) {
            Some(workspace_level) if workspace_level == *level => {}
            Some(workspace_level) => anyhow::bail!(
                "active lint {name} level mismatch: Cargo.toml has {workspace_level}, policy has {level}"
            ),
            None => anyhow::bail!("active lint {name} missing from workspace lint block"),
        }
    }

    let active_names: BTreeSet<_> = active.keys().copied().collect();
    for name in workspace_lints.keys() {
        if !active_names.contains(name) {
            anyhow::bail!("workspace lint {name} missing active policy ledger entry");
        }
    }

    Ok(())
}

fn check_planned_lints(
    workspace_lints: &BTreeMap<String, String>,
    policy_lints: &BTreeMap<String, PolicyLint>,
    policy_msrv: &str,
) -> Result<()> {
    for (name, lint) in policy_lints
        .iter()
        .filter(|(_, lint)| lint.status == "planned")
    {
        if workspace_lints.contains_key(name)
            && version_less_than(
                policy_msrv,
                lint.activate_when_msrv.as_deref().unwrap_or(policy_msrv),
            )
        {
            anyhow::bail!(
                "planned lint {name} is active before MSRV {}",
                lint.activate_when_msrv.as_deref().unwrap_or("unknown")
            );
        }
    }
    Ok(())
}

fn version_less_than(left: &str, right: &str) -> bool {
    let parse = |version: &str| -> Vec<u64> {
        version
            .split('.')
            .filter_map(|segment| segment.parse::<u64>().ok())
            .collect()
    };
    parse(left) < parse(right)
}

fn check_no_test_carveouts(path: &Path) -> Result<()> {
    let text = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let forbidden = [
        "allow-unwrap-in-tests",
        "allow-expect-in-tests",
        "allow-panic-in-tests",
        "allow-indexing-slicing-in-tests",
        "allow-dbg-in-tests",
    ];
    for key in forbidden {
        if text.lines().any(|line| {
            let trimmed = line.trim_start();
            !trimmed.starts_with('#') && trimmed.starts_with(key)
        }) {
            anyhow::bail!("clippy.toml must not set test carveout {key}");
        }
    }
    Ok(())
}

fn check_clippy_debt(debt: &toml::Value) -> Result<()> {
    let schema = debt
        .get("schema")
        .and_then(toml::Value::as_integer)
        .context("policy/clippy-debt.toml missing integer schema")?;
    if schema != 1 {
        anyhow::bail!("unsupported clippy debt schema: {}", schema);
    }

    let today = OffsetDateTime::now_utc().date();
    for (index, entry) in debt
        .get("debt")
        .and_then(toml::Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
    {
        let table = entry
            .as_table()
            .with_context(|| format!("debt entry {index} must be a table"))?;
        for key in ["lint", "path", "owner", "reason", "expires"] {
            let value = required_str(table, key, "debt", index)?;
            if value.trim().is_empty() {
                anyhow::bail!("debt entry {index} field {key} must be non-empty");
            }
        }
        let expires = parse_policy_date(required_str(table, "expires", "debt", index)?)?;
        if expires < today {
            anyhow::bail!("debt entry {index} expired on {expires}");
        }
    }
    Ok(())
}

fn parse_policy_date(value: &str) -> Result<Date> {
    let mut parts = value.split('-');
    let year = parts
        .next()
        .context("missing year")?
        .parse::<i32>()
        .with_context(|| format!("parse year in policy date {value}"))?;
    let month = parts
        .next()
        .context("missing month")?
        .parse::<u8>()
        .with_context(|| format!("parse month in policy date {value}"))?;
    let day = parts
        .next()
        .context("missing day")?
        .parse::<u8>()
        .with_context(|| format!("parse day in policy date {value}"))?;
    if parts.next().is_some() {
        anyhow::bail!("policy date must use YYYY-MM-DD: {value}");
    }
    let month =
        Month::try_from(month).with_context(|| format!("parse month in policy date {value}"))?;
    Date::from_calendar_date(year, month, day).with_context(|| format!("parse policy date {value}"))
}

fn check_panic_allowlist_shape(path: &Path) -> Result<()> {
    let allowlist = read_toml_file(path)?;
    let schema = string_at(&allowlist, &["schema_version"])
        .with_context(|| format!("{} missing schema_version", path.display()))?;
    if schema != "0.3" {
        anyhow::bail!("{} schema_version must be 0.3", path.display());
    }
    for (index, entry) in allowlist
        .get("allow")
        .and_then(toml::Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
    {
        let table = entry
            .as_table()
            .with_context(|| format!("panic allow entry {index} must be a table"))?;
        for key in ["path", "family", "classification", "owner", "explanation"] {
            let value = required_str(table, key, "panic allow", index)?;
            if value.trim().is_empty() {
                anyhow::bail!("panic allow entry {index} field {key} must be non-empty");
            }
        }
        if let Some(expires) = table.get("expires").and_then(toml::Value::as_str) {
            let expires = parse_policy_date(expires)?;
            if expires < OffsetDateTime::now_utc().date() {
                anyhow::bail!("panic allow entry {index} expired on {expires}");
            }
        }
        let selector = table
            .get("selector")
            .and_then(toml::Value::as_table)
            .with_context(|| format!("panic allow entry {index} missing selector"))?;
        for key in ["kind", "container"] {
            let value = required_str(selector, key, "panic selector", index)?;
            if value.trim().is_empty() {
                anyhow::bail!("panic allow entry {index} selector.{key} must be non-empty");
            }
        }
    }
    Ok(())
}

fn check_non_rust_allowlist_shape(path: &Path) -> Result<()> {
    let allowlist = read_toml_file(path)?;
    let schema = string_at(&allowlist, &["schema_version"])
        .with_context(|| format!("{} missing schema_version", path.display()))?;
    if schema != "1.0" {
        anyhow::bail!("{} schema_version must be 1.0", path.display());
    }
    let entries = allowlist
        .get("allow")
        .and_then(toml::Value::as_array)
        .with_context(|| format!("{} missing [[allow]] entries", path.display()))?;
    for (index, entry) in entries.iter().enumerate() {
        let table = entry
            .as_table()
            .with_context(|| format!("non-Rust allow entry {index} must be a table"))?;
        let has_path = table.get("path").and_then(toml::Value::as_str).is_some();
        let has_glob = table.get("glob").and_then(toml::Value::as_str).is_some();
        if has_path == has_glob {
            anyhow::bail!("non-Rust allow entry {index} must set exactly one of path or glob");
        }
        for key in ["kind", "owner", "reason", "surface", "classification"] {
            let value = required_str(table, key, "non-Rust allow", index)?;
            if value.trim().is_empty() {
                anyhow::bail!("non-Rust allow entry {index} field {key} must be non-empty");
            }
        }
        let covered_by = table
            .get("covered_by")
            .and_then(toml::Value::as_array)
            .with_context(|| format!("non-Rust allow entry {index} missing covered_by"))?;
        if covered_by.is_empty() {
            anyhow::bail!("non-Rust allow entry {index} covered_by must not be empty");
        }
        if let Some(expires) = table.get("expires").and_then(toml::Value::as_str) {
            let expires = parse_policy_date(expires)?;
            if expires < OffsetDateTime::now_utc().date() {
                anyhow::bail!("non-Rust allow entry {index} expired on {expires}");
            }
        }
    }
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Conformance: thin wrappers delegating to cockpitctl-conform
// ─────────────────────────────────────────────────────────────────────────────

/// Print violations for a single conform result, matching the original output format.
fn print_conform_result(result: &cockpitctl_conform::ConformResult, checks: &ConformChecks) {
    if result.violations.iter().any(|v| v.check == "schema") {
        eprintln!("  FAIL: schema validation errors:");
        for v in result.violations.iter().filter(|v| v.check == "schema") {
            eprintln!("    - {}", v.message);
        }
        return;
    }
    eprintln!("  ok: schema validation passed");

    let check_names = [
        ("survivability", checks.survivability),
        ("path_hygiene", checks.path_hygiene),
        ("ordering", checks.ordering),
        ("reason_lint", checks.reason_lint),
        ("tool_error_identity", checks.tool_error_identity),
        ("sensor_id_format", checks.sensor_id_format),
        ("artifact_pointers", checks.artifact_pointers),
    ];

    for (check_name, enabled) in &check_names {
        if !enabled {
            continue;
        }

        let check_violations: Vec<_> = result
            .violations
            .iter()
            .filter(|v| v.check == *check_name)
            .collect();

        let display_name = check_name.replace('_', "-");

        if check_violations.is_empty() {
            if *check_name == "survivability" {
                eprintln!("  ok: survivability check passed");
            } else {
                eprintln!("  ok: {} passed", display_name);
            }
        } else {
            for v in &check_violations {
                eprintln!("  FAIL: {}: {}", display_name, v.message);
            }
        }
    }
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

    // Determinism check (if golden provided)
    if let Some(golden_path) = golden {
        let golden_content = fs::read_to_string(&golden_path)
            .with_context(|| format!("read golden {}", golden_path.display()))?;

        if let Some(msg) = cockpitctl_conform::check_determinism(&content, &golden_content) {
            eprintln!("  FAIL: {}", msg);
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

    let result = cockpitctl_conform::conform_single(&content, sid, &checks)?;
    print_conform_result(&result, &checks);

    if !result.is_pass() {
        anyhow::bail!(
            "conformance failed with {} violation(s)",
            result.violations.len()
        );
    }

    eprintln!("conformance: PASS");
    Ok(())
}

fn conform_dir(
    dir: PathBuf,
    validate_cockpit: bool,
    checks: &ConformChecks,
    allow_missing_report: bool,
    presence_semantics: bool,
) -> Result<()> {
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

        match cockpitctl_conform::conform_single(&content, &name, checks) {
            Ok(result) => {
                print_conform_result(&result, checks);
                if result.is_pass() {
                    results.push((name, "PASS"));
                } else {
                    results.push((name, "FAIL"));
                    had_failure = true;
                }
            }
            Err(e) => {
                eprintln!("  FAIL: {:#}", e);
                results.push((name, "FAIL"));
                had_failure = true;
            }
        }
    }

    // Optionally validate cockpit/report.json
    if validate_cockpit {
        let cockpit_report = dir.join("cockpit").join("report.json");
        eprintln!();
        eprintln!("--- cockpit report ---");

        if cockpit_report.exists() {
            let content = fs::read_to_string(&cockpit_report)
                .with_context(|| format!("read {}", cockpit_report.display()))?;

            let schema_violations = cockpitctl_conform::validate_cockpit_schema(&content)?;
            if !schema_violations.is_empty() {
                eprintln!("  FAIL: cockpit report schema validation errors:");
                for v in &schema_violations {
                    eprintln!("    - {}", v.message);
                }
                results.push(("cockpit".to_string(), "FAIL"));
                had_failure = true;
            } else {
                eprintln!("  ok: cockpit report schema validation passed");

                let mut cockpit_failed = false;
                let needs_extended = checks.reason_lint || presence_semantics;

                if needs_extended {
                    let violations = cockpitctl_conform::check_cockpit_extended(
                        &content,
                        checks.reason_lint,
                        presence_semantics,
                    )?;

                    for v in &violations {
                        let display_check = v.check.replace('_', "-");
                        eprintln!("  FAIL: cockpit {}: {}", display_check, v.message);
                        cockpit_failed = true;
                        had_failure = true;
                    }

                    if !cockpit_failed {
                        if checks.reason_lint {
                            eprintln!("  ok: cockpit reason-lint passed");
                        }
                        if presence_semantics {
                            eprintln!("  ok: cockpit presence-semantics passed");
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
    fn example_sync_check_and_fix() {
        let _guard = FS_LOCK.lock().unwrap();
        let temp = TempDir::new().expect("tempdir");
        let _cwd = set_cwd(temp.path());

        let content = "# example toml\n[policy]\nwarn_is_fail = false\n";
        write_file(&temp.path().join("cockpit.toml.example"), content);
        write_file(
            &temp
                .path()
                .join("crates/cockpitctl-cli/cockpit.toml.example"),
            content,
        );
        example_sync_check().expect("example_sync_check ok");

        write_file(
            &temp
                .path()
                .join("crates/cockpitctl-cli/cockpit.toml.example"),
            "mismatch",
        );
        let err = example_sync_check().expect_err("example_sync_check mismatch");
        assert!(format!("{:#}", err).contains("out of sync"));

        example_sync_fix().expect("example_sync_fix");
        example_sync_check().expect("example_sync_check after fix");
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

        // Add example sync files for testing
        write_file(&root.join("cockpit.toml.example"), "example");
        write_file(
            &root.join("crates/cockpitctl-cli/cockpit.toml.example"),
            "example",
        );

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
                command: Commands::ExampleSyncCheck
            }),
            0
        );
        assert_eq!(
            main_entry(Cli {
                command: Commands::ExampleSyncFix
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
        conform_dir(
            artifacts.to_path_buf(),
            true,
            &checks_reason_on,
            true,
            false,
        )
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
        conform_dir(
            artifacts.to_path_buf(),
            true,
            &checks_reason_off,
            true,
            true,
        )
        .expect("cockpit checks pass without reason lint");
        conform_dir(
            artifacts.to_path_buf(),
            true,
            &checks_reason_off,
            true,
            false,
        )
        .expect("cockpit checks pass with no extended checks");
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

    #[cfg(coverage)]
    #[test]
    fn main_noop_executes() {
        super::main();
    }
}
