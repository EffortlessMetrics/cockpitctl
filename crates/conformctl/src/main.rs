use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use cockpitctl_conform::{ConformChecks, check_determinism};
use std::fs;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "conformctl")]
#[command(version)]
#[command(about = "Standalone conformance checker for cockpitctl sensor receipts")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Validate a single sensor receipt against the protocol.
    Check {
        /// Path to the sensor report to validate.
        #[arg(long)]
        report: PathBuf,

        /// Optional golden file to check determinism against.
        #[arg(long)]
        golden: Option<PathBuf>,

        /// Sensor ID (required for --ordering).
        #[arg(long)]
        sensor_id: Option<String>,

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
    },

    /// Validate every sensor receipt in an artifacts/ directory at once.
    CheckDir {
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

fn main_entry(cli: Cli) -> i32 {
    if let Err(e) = run(cli) {
        eprintln!("conformctl error: {:#}", e);
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
        Commands::Check {
            report,
            golden,
            sensor_id,
            survivability,
            path_hygiene,
            ordering,
            reason_lint,
            all,
            tool_error_identity,
            sensor_id_format,
            artifact_pointers,
        } => check(
            report,
            golden,
            sensor_id,
            survivability,
            path_hygiene,
            ordering,
            reason_lint,
            tool_error_identity,
            sensor_id_format,
            artifact_pointers,
            all,
        ),
        Commands::CheckDir {
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
        } => check_dir(
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

#[expect(
    clippy::too_many_arguments,
    reason = "CLI and test helpers mirror stable input surfaces."
)]
fn check(
    report: PathBuf,
    golden: Option<PathBuf>,
    sensor_id: Option<String>,
    survivability: bool,
    path_hygiene: bool,
    ordering: bool,
    reason_lint: bool,
    tool_error_identity: bool,
    sensor_id_format: bool,
    artifact_pointers: bool,
    all: bool,
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

        if let Some(msg) = check_determinism(&content, &golden_content) {
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
    print_result(&result, &checks);

    if !result.is_pass() {
        anyhow::bail!(
            "conformance failed with {} violation(s)",
            result.violations.len()
        );
    }

    eprintln!("conformance: PASS");
    Ok(())
}

fn print_result(result: &cockpitctl_conform::ConformResult, checks: &ConformChecks) {
    if result.violations.iter().any(|v| v.check == "schema") {
        eprintln!("  FAIL: schema validation errors:");
        for v in result.violations.iter().filter(|v| v.check == "schema") {
            eprintln!("    - {}", v.message);
        }
        return;
    }
    eprintln!("  ok: schema validation passed");

    // Print per-check results
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
                // Match xtask output: survivability prints a contextual message
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

fn check_dir(
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
                print_result(&result, checks);
                if result.is_pass() {
                    results.push((name, "PASS"));
                } else {
                    for v in &result.violations {
                        let display_check = v.check.replace('_', "-");
                        eprintln!("  FAIL: {}: {}", display_check, v.message);
                    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn write_file(path: &std::path::Path, content: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create dirs");
        }
        fs::write(path, content).expect("write file");
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
    fn check_requires_sensor_id_for_ordering() {
        let temp = TempDir::new().expect("tempdir");
        let report_path = temp.path().join("report.json");
        write_file(&report_path, &minimal_sensor_report_json());

        let err = check(
            report_path,
            None,
            None,
            false,
            false,
            true,
            false,
            false,
            false,
            false,
            false,
        )
        .expect_err("ordering requires sensor_id");
        assert!(format!("{:#}", err).contains("requires --sensor-id"));
    }

    #[test]
    fn check_all_requires_sensor_id() {
        let temp = TempDir::new().expect("tempdir");
        let report_path = temp.path().join("report.json");
        write_file(&report_path, &minimal_sensor_report_json());

        let err = check(
            report_path,
            None,
            None,
            false,
            false,
            false,
            false,
            false,
            false,
            false,
            true,
        )
        .expect_err("all requires sensor_id");
        assert!(format!("{:#}", err).contains("requires --sensor-id"));
    }

    #[test]
    fn check_golden_mismatch() {
        let temp = TempDir::new().expect("tempdir");
        let report_path = temp.path().join("report.json");
        let golden_path = temp.path().join("golden.json");
        write_file(&report_path, &minimal_sensor_report_json());
        write_file(&golden_path, "not equal");

        let err = check(
            report_path,
            Some(golden_path),
            Some("sensor".to_string()),
            false,
            false,
            false,
            false,
            false,
            false,
            false,
            false,
        )
        .expect_err("golden mismatch");
        assert!(format!("{:#}", err).contains("determinism check failed"));
    }

    #[test]
    fn check_golden_match_and_pass() {
        let temp = TempDir::new().expect("tempdir");
        let report_path = temp.path().join("report.json");
        let golden_path = temp.path().join("golden.json");
        let content = minimal_sensor_report_json();
        write_file(&report_path, &content);
        write_file(&golden_path, &content);

        check(
            report_path,
            Some(golden_path),
            Some("sensor".to_string()),
            false,
            false,
            false,
            false,
            false,
            false,
            false,
            false,
        )
        .expect("should pass");
    }

    #[test]
    fn check_basic_pass() {
        let temp = TempDir::new().expect("tempdir");
        let report_path = temp.path().join("report.json");
        write_file(&report_path, &minimal_sensor_report_json());

        check(
            report_path,
            None,
            Some("sensor".to_string()),
            false,
            false,
            false,
            false,
            false,
            false,
            false,
            false,
        )
        .expect("should pass");
    }

    #[test]
    fn check_schema_failure() {
        let temp = TempDir::new().expect("tempdir");
        let report_path = temp.path().join("report.json");
        write_file(&report_path, "{}");

        let err = check(
            report_path,
            None,
            None,
            false,
            false,
            false,
            false,
            false,
            false,
            false,
            false,
        )
        .expect_err("schema fail");
        assert!(format!("{:#}", err).contains("conformance failed"));
    }

    #[test]
    fn check_dir_missing_dir() {
        let temp = TempDir::new().expect("tempdir");
        let err = check_dir(
            temp.path().join("does_not_exist"),
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
    }

    #[test]
    fn check_dir_allow_missing_report() {
        let temp = TempDir::new().expect("tempdir");
        let artifacts = temp.path();
        write_file(
            &artifacts.join("ok").join("report.json"),
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

        check_dir(artifacts.to_path_buf(), false, &checks, true, false)
            .expect("allow missing report");

        let err = check_dir(artifacts.to_path_buf(), false, &checks, false, false)
            .expect_err("missing report should fail");
        assert!(format!("{:#}", err).contains("failed"));
    }

    #[test]
    fn check_dir_cockpit_validation() {
        let temp = TempDir::new().expect("tempdir");
        let artifacts = temp.path();
        write_file(
            &artifacts.join("ok").join("report.json"),
            &minimal_sensor_report_json(),
        );

        let checks = ConformChecks {
            path_hygiene: false,
            ordering: false,
            reason_lint: false,
            survivability: false,
            tool_error_identity: false,
            sensor_id_format: false,
            artifact_pointers: false,
        };

        // cockpit missing → skip
        check_dir(artifacts.to_path_buf(), true, &checks, true, false)
            .expect("cockpit skip when missing");

        // cockpit invalid → fail
        write_file(&artifacts.join("cockpit").join("report.json"), "{}");
        let err = check_dir(artifacts.to_path_buf(), true, &checks, true, false)
            .expect_err("invalid cockpit");
        assert!(format!("{:#}", err).contains("conform-dir"));
    }

    #[test]
    fn check_dir_cockpit_extended_pass() {
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

        let checks = ConformChecks {
            path_hygiene: false,
            ordering: false,
            reason_lint: true,
            survivability: false,
            tool_error_identity: false,
            sensor_id_format: false,
            artifact_pointers: false,
        };

        check_dir(artifacts.to_path_buf(), true, &checks, true, true).expect("cockpit checks pass");
    }

    #[test]
    fn check_dir_cockpit_extended_fail() {
        let temp = TempDir::new().expect("tempdir");
        let artifacts = temp.path();
        write_file(
            &artifacts.join("ok").join("report.json"),
            &minimal_sensor_report_json(),
        );

        let mut cockpit = minimal_cockpit_report();
        cockpit.verdict.reasons = vec!["Bad-Token".to_string()];
        cockpit.sensors.push(cockpitctl_types::SensorSummary {
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
        let cockpit_json = serde_json::to_string(&cockpit).expect("serialize cockpit");
        write_file(
            &artifacts.join("cockpit").join("report.json"),
            &cockpit_json,
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

        let err = check_dir(artifacts.to_path_buf(), true, &checks, true, true)
            .expect_err("cockpit extended checks should fail");
        assert!(format!("{:#}", err).contains("failed"));
    }

    #[test]
    fn check_dir_read_error() {
        let temp = TempDir::new().expect("tempdir");
        let artifacts = temp.path();
        // report.json is a directory → read error
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
        let err = check_dir(artifacts.to_path_buf(), false, &checks, true, false)
            .expect_err("read error");
        assert!(format!("{:#}", err).contains("conform-dir"));
    }

    #[test]
    fn check_dir_invalid_json() {
        let temp = TempDir::new().expect("tempdir");
        let artifacts = temp.path();
        write_file(&artifacts.join("bad").join("report.json"), "{");

        let checks = ConformChecks {
            path_hygiene: false,
            ordering: false,
            reason_lint: false,
            survivability: false,
            tool_error_identity: false,
            sensor_id_format: false,
            artifact_pointers: false,
        };
        let err = check_dir(artifacts.to_path_buf(), false, &checks, true, false)
            .expect_err("invalid json");
        assert!(format!("{:#}", err).contains("conform-dir"));
    }

    #[test]
    fn main_entry_returns_0_on_success() {
        let temp = TempDir::new().expect("tempdir");
        let report_path = temp.path().join("report.json");
        write_file(&report_path, &minimal_sensor_report_json());

        let code = main_entry(Cli {
            command: Commands::Check {
                report: report_path,
                golden: None,
                sensor_id: None,
                survivability: false,
                path_hygiene: false,
                ordering: false,
                reason_lint: false,
                all: false,
                tool_error_identity: false,
                sensor_id_format: false,
                artifact_pointers: false,
            },
        });
        assert_eq!(code, 0);
    }

    #[test]
    fn main_entry_returns_1_on_error() {
        let temp = TempDir::new().expect("tempdir");
        let code = main_entry(Cli {
            command: Commands::Check {
                report: temp.path().join("missing.json"),
                golden: None,
                sensor_id: None,
                survivability: false,
                path_hygiene: false,
                ordering: false,
                reason_lint: false,
                all: false,
                tool_error_identity: false,
                sensor_id_format: false,
                artifact_pointers: false,
            },
        });
        assert_eq!(code, 1);
    }

    #[test]
    fn clap_supports_version_flag() {
        let err = Cli::try_parse_from(["conformctl", "--version"])
            .expect_err("version flag should short-circuit parse");
        assert_eq!(err.kind(), clap::error::ErrorKind::DisplayVersion);
    }

    #[cfg(coverage)]
    #[test]
    fn main_noop_executes() {
        super::main();
    }

    // ── CLI argument parsing tests ───────────────────────────────────────

    #[test]
    fn clap_parse_check_with_all_flags() {
        let cli = Cli::try_parse_from([
            "conformctl",
            "check",
            "--report",
            "r.json",
            "--golden",
            "g.json",
            "--sensor-id",
            "my-sensor",
            "--survivability",
            "--path-hygiene",
            "--ordering",
            "--reason-lint",
            "--tool-error-identity",
            "--sensor-id-format",
            "--artifact-pointers",
            "--all",
        ])
        .expect("parse should succeed");
        match cli.command {
            Commands::Check {
                report,
                golden,
                sensor_id,
                survivability,
                path_hygiene,
                ordering,
                reason_lint,
                all,
                tool_error_identity,
                sensor_id_format,
                artifact_pointers,
            } => {
                assert_eq!(report, PathBuf::from("r.json"));
                assert_eq!(golden, Some(PathBuf::from("g.json")));
                assert_eq!(sensor_id, Some("my-sensor".to_string()));
                assert!(survivability);
                assert!(path_hygiene);
                assert!(ordering);
                assert!(reason_lint);
                assert!(all);
                assert!(tool_error_identity);
                assert!(sensor_id_format);
                assert!(artifact_pointers);
            }
            _ => panic!("expected Check"),
        }
    }

    #[test]
    fn clap_parse_check_dir_with_all_flags() {
        let cli = Cli::try_parse_from([
            "conformctl",
            "check-dir",
            "--dir",
            "artifacts",
            "--validate-cockpit",
            "--all",
            "--path-hygiene",
            "--ordering",
            "--reason-lint",
            "--survivability",
            "--tool-error-identity",
            "--sensor-id-format",
            "--artifact-pointers",
            "--presence-semantics",
            "--allow-missing-report",
        ])
        .expect("parse should succeed");
        match cli.command {
            Commands::CheckDir {
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
            } => {
                assert_eq!(dir, PathBuf::from("artifacts"));
                assert!(validate_cockpit);
                assert!(all);
                assert!(path_hygiene);
                assert!(ordering);
                assert!(reason_lint);
                assert!(survivability);
                assert!(tool_error_identity);
                assert!(sensor_id_format);
                assert!(artifact_pointers);
                assert!(presence_semantics);
                assert!(allow_missing_report);
            }
            _ => panic!("expected CheckDir"),
        }
    }

    #[test]
    fn clap_parse_check_requires_report_arg() {
        let err =
            Cli::try_parse_from(["conformctl", "check"]).expect_err("check requires --report");
        assert_eq!(err.kind(), clap::error::ErrorKind::MissingRequiredArgument);
    }

    #[test]
    fn clap_parse_check_dir_requires_dir_arg() {
        let err =
            Cli::try_parse_from(["conformctl", "check-dir"]).expect_err("check-dir requires --dir");
        assert_eq!(err.kind(), clap::error::ErrorKind::MissingRequiredArgument);
    }

    #[test]
    fn clap_parse_check_minimal() {
        let cli =
            Cli::try_parse_from(["conformctl", "check", "--report", "r.json"]).expect("parse");
        match cli.command {
            Commands::Check {
                golden,
                sensor_id,
                survivability,
                path_hygiene,
                ordering,
                reason_lint,
                all,
                tool_error_identity,
                sensor_id_format,
                artifact_pointers,
                ..
            } => {
                assert!(golden.is_none());
                assert!(sensor_id.is_none());
                assert!(!survivability);
                assert!(!path_hygiene);
                assert!(!ordering);
                assert!(!reason_lint);
                assert!(!all);
                assert!(!tool_error_identity);
                assert!(!sensor_id_format);
                assert!(!artifact_pointers);
            }
            _ => panic!("expected Check"),
        }
    }

    #[test]
    fn clap_parse_check_dir_minimal() {
        let cli =
            Cli::try_parse_from(["conformctl", "check-dir", "--dir", "artifacts"]).expect("parse");
        match cli.command {
            Commands::CheckDir {
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
                ..
            } => {
                assert!(!validate_cockpit);
                assert!(!all);
                assert!(!path_hygiene);
                assert!(!ordering);
                assert!(!reason_lint);
                assert!(!survivability);
                assert!(!tool_error_identity);
                assert!(!sensor_id_format);
                assert!(!artifact_pointers);
                assert!(!presence_semantics);
                assert!(!allow_missing_report);
            }
            _ => panic!("expected CheckDir"),
        }
    }

    #[test]
    fn clap_supports_help_flag() {
        let err = Cli::try_parse_from(["conformctl", "--help"])
            .expect_err("help should short-circuit parse");
        assert_eq!(err.kind(), clap::error::ErrorKind::DisplayHelp);
    }

    #[test]
    fn clap_rejects_unknown_subcommand() {
        let err = Cli::try_parse_from(["conformctl", "unknown"])
            .expect_err("unknown subcommand should fail");
        assert_eq!(err.kind(), clap::error::ErrorKind::InvalidSubcommand);
    }

    #[test]
    fn clap_check_help() {
        let err = Cli::try_parse_from(["conformctl", "check", "--help"])
            .expect_err("help should short-circuit");
        assert_eq!(err.kind(), clap::error::ErrorKind::DisplayHelp);
    }

    #[test]
    fn clap_check_dir_help() {
        let err = Cli::try_parse_from(["conformctl", "check-dir", "--help"])
            .expect_err("help should short-circuit");
        assert_eq!(err.kind(), clap::error::ErrorKind::DisplayHelp);
    }

    // ── Report validation flows ──────────────────────────────────────────

    #[test]
    fn check_all_with_sensor_id_passes() {
        let temp = TempDir::new().expect("tempdir");
        let report_path = temp.path().join("report.json");
        write_file(&report_path, &minimal_sensor_report_json());

        check(
            report_path,
            None,
            Some("my-sensor".to_string()),
            false,
            false,
            false,
            false,
            false,
            false,
            false,
            true, // all
        )
        .expect("all checks should pass on minimal valid report");
    }

    #[test]
    fn check_each_individual_check_passes_on_valid_report() {
        let temp = TempDir::new().expect("tempdir");
        let report_path = temp.path().join("report.json");
        write_file(&report_path, &minimal_sensor_report_json());

        // path_hygiene
        check(
            report_path.clone(),
            None,
            Some("sensor".to_string()),
            false,
            true,
            false,
            false,
            false,
            false,
            false,
            false,
        )
        .expect("path_hygiene pass");

        // reason_lint
        check(
            report_path.clone(),
            None,
            Some("sensor".to_string()),
            false,
            false,
            false,
            true,
            false,
            false,
            false,
            false,
        )
        .expect("reason_lint pass");

        // survivability
        check(
            report_path.clone(),
            None,
            Some("sensor".to_string()),
            true,
            false,
            false,
            false,
            false,
            false,
            false,
            false,
        )
        .expect("survivability pass");

        // tool_error_identity
        check(
            report_path.clone(),
            None,
            Some("sensor".to_string()),
            false,
            false,
            false,
            false,
            true,
            false,
            false,
            false,
        )
        .expect("tool_error_identity pass");

        // sensor_id_format
        check(
            report_path.clone(),
            None,
            Some("sensor".to_string()),
            false,
            false,
            false,
            false,
            false,
            true,
            false,
            false,
        )
        .expect("sensor_id_format pass");

        // artifact_pointers
        check(
            report_path.clone(),
            None,
            Some("sensor".to_string()),
            false,
            false,
            false,
            false,
            false,
            false,
            true,
            false,
        )
        .expect("artifact_pointers pass");

        // ordering
        check(
            report_path,
            None,
            Some("sensor".to_string()),
            false,
            false,
            true,
            false,
            false,
            false,
            false,
            false,
        )
        .expect("ordering pass");
    }

    #[test]
    fn check_survivability_fails_on_fail_verdict_no_findings_no_reasons() {
        let temp = TempDir::new().expect("tempdir");
        let report_path = temp.path().join("report.json");

        let mut report = minimal_sensor_report();
        report.verdict.status = cockpitctl_types::VerdictStatus::Fail;
        report.findings.clear();
        report.verdict.reasons.clear();
        write_file(
            &report_path,
            &serde_json::to_string(&report).expect("serialize"),
        );

        let err = check(
            report_path,
            None,
            Some("sensor".to_string()),
            true, // survivability
            false,
            false,
            false,
            false,
            false,
            false,
            false,
        )
        .expect_err("survivability should fail");
        assert!(format!("{:#}", err).contains("conformance failed"));
    }

    #[test]
    fn check_path_hygiene_violation_on_traversal() {
        let temp = TempDir::new().expect("tempdir");
        let report_path = temp.path().join("report.json");

        let mut report = minimal_sensor_report();
        report.findings.push(cockpitctl_types::Finding {
            severity: cockpitctl_types::Severity::Warn,
            check_id: None,
            code: "test".to_string(),
            message: "test finding".to_string(),
            location: Some(cockpitctl_types::Location {
                path: Some("../../etc/passwd".to_string()),
                line: Some(1),
                col: None,
            }),
            help: None,
            url: None,
            fingerprint: None,
            data: None,
        });
        report.verdict.counts.warn = 1;
        write_file(
            &report_path,
            &serde_json::to_string(&report).expect("serialize"),
        );

        let err = check(
            report_path,
            None,
            Some("sensor".to_string()),
            false,
            true, // path_hygiene
            false,
            false,
            false,
            false,
            false,
            false,
        )
        .expect_err("path hygiene should fail");
        assert!(format!("{:#}", err).contains("conformance failed"));
    }

    #[test]
    fn check_reason_lint_violation_on_bad_token() {
        let temp = TempDir::new().expect("tempdir");
        let report_path = temp.path().join("report.json");

        let mut report = minimal_sensor_report();
        report.verdict.reasons = vec!["Bad-Token".to_string()];
        write_file(
            &report_path,
            &serde_json::to_string(&report).expect("serialize"),
        );

        let err = check(
            report_path,
            None,
            Some("sensor".to_string()),
            false,
            false,
            false,
            true, // reason_lint
            false,
            false,
            false,
            false,
        )
        .expect_err("reason_lint should fail");
        assert!(format!("{:#}", err).contains("conformance failed"));
    }

    #[test]
    fn check_sensor_id_format_violation_on_invalid_id() {
        let temp = TempDir::new().expect("tempdir");
        let report_path = temp.path().join("report.json");
        write_file(&report_path, &minimal_sensor_report_json());

        let err = check(
            report_path,
            None,
            Some("bad sensor!@#".to_string()),
            false,
            false,
            false,
            false,
            false,
            true, // sensor_id_format
            false,
            false,
        )
        .expect_err("sensor_id_format should fail");
        assert!(format!("{:#}", err).contains("conformance failed"));
    }

    #[test]
    fn check_artifact_pointers_violation_on_traversal() {
        let temp = TempDir::new().expect("tempdir");
        let report_path = temp.path().join("report.json");

        let mut report = minimal_sensor_report();
        report.artifacts.push(cockpitctl_types::ArtifactPointer {
            id: "bad".to_string(),
            path: "../../../etc/passwd".to_string(),
            mime: "text/plain".to_string(),
            schema: None,
        });
        write_file(
            &report_path,
            &serde_json::to_string(&report).expect("serialize"),
        );

        let err = check(
            report_path,
            None,
            Some("sensor".to_string()),
            false,
            false,
            false,
            false,
            false,
            false,
            true, // artifact_pointers
            false,
        )
        .expect_err("artifact_pointers should fail on traversal");
        assert!(format!("{:#}", err).contains("conformance failed"));
    }

    #[test]
    fn check_ordering_violation_on_unsorted_findings() {
        let temp = TempDir::new().expect("tempdir");
        let report_path = temp.path().join("report.json");

        let mut report = minimal_sensor_report();
        // Add findings in wrong order: info before error (should be error first)
        report.findings.push(cockpitctl_types::Finding {
            severity: cockpitctl_types::Severity::Info,
            check_id: None,
            code: "a".to_string(),
            message: "info finding".to_string(),
            location: None,
            help: None,
            url: None,
            fingerprint: None,
            data: None,
        });
        report.findings.push(cockpitctl_types::Finding {
            severity: cockpitctl_types::Severity::Error,
            check_id: None,
            code: "b".to_string(),
            message: "error finding".to_string(),
            location: None,
            help: None,
            url: None,
            fingerprint: None,
            data: None,
        });
        report.verdict.counts.info = 1;
        report.verdict.counts.error = 1;
        write_file(
            &report_path,
            &serde_json::to_string(&report).expect("serialize"),
        );

        let err = check(
            report_path,
            None,
            Some("sensor".to_string()),
            false,
            false,
            true, // ordering
            false,
            false,
            false,
            false,
            false,
        )
        .expect_err("ordering should fail");
        assert!(format!("{:#}", err).contains("conformance failed"));
    }

    #[test]
    fn check_tool_error_identity_violation() {
        let temp = TempDir::new().expect("tempdir");
        let report_path = temp.path().join("report.json");

        let mut report = minimal_sensor_report();
        report.verdict.status = cockpitctl_types::VerdictStatus::Fail;
        report.verdict.reasons = vec!["tool_error".to_string()];
        // No findings with check_id=tool.runtime
        write_file(
            &report_path,
            &serde_json::to_string(&report).expect("serialize"),
        );

        let err = check(
            report_path,
            None,
            Some("sensor".to_string()),
            false,
            false,
            false,
            false,
            true, // tool_error_identity
            false,
            false,
            false,
        )
        .expect_err("tool_error_identity should fail");
        assert!(format!("{:#}", err).contains("conformance failed"));
    }

    // ── Error paths ──────────────────────────────────────────────────────

    #[test]
    fn check_missing_report_file() {
        let temp = TempDir::new().expect("tempdir");
        let err = check(
            temp.path().join("nonexistent.json"),
            None,
            None,
            false,
            false,
            false,
            false,
            false,
            false,
            false,
            false,
        )
        .expect_err("missing file");
        assert!(format!("{:#}", err).contains("read"));
    }

    #[test]
    fn check_missing_golden_file() {
        let temp = TempDir::new().expect("tempdir");
        let report_path = temp.path().join("report.json");
        write_file(&report_path, &minimal_sensor_report_json());

        let err = check(
            report_path,
            Some(temp.path().join("nonexistent_golden.json")),
            Some("sensor".to_string()),
            false,
            false,
            false,
            false,
            false,
            false,
            false,
            false,
        )
        .expect_err("missing golden");
        assert!(format!("{:#}", err).contains("read golden"));
    }

    #[test]
    fn check_invalid_json_in_report() {
        let temp = TempDir::new().expect("tempdir");
        let report_path = temp.path().join("report.json");
        write_file(&report_path, "not json at all");

        check(
            report_path,
            None,
            None,
            false,
            false,
            false,
            false,
            false,
            false,
            false,
            false,
        )
        .expect_err("invalid json should error");
    }

    // ── Directory scanning logic ─────────────────────────────────────────

    #[test]
    fn check_dir_empty_directory() {
        let temp = TempDir::new().expect("tempdir");

        let checks = ConformChecks {
            path_hygiene: false,
            ordering: false,
            reason_lint: false,
            survivability: false,
            tool_error_identity: false,
            sensor_id_format: false,
            artifact_pointers: false,
        };

        // Empty dir with no sensor subdirs → passes with 0 sensors checked
        check_dir(temp.path().to_path_buf(), false, &checks, false, false)
            .expect("empty dir should pass");
    }

    #[test]
    fn check_dir_skips_cockpit_subdir_during_sensor_enumeration() {
        let temp = TempDir::new().expect("tempdir");
        let artifacts = temp.path();

        // Only a cockpit dir, no sensors
        write_file(
            &artifacts.join("cockpit").join("report.json"),
            &minimal_sensor_report_json(), // not valid cockpit, but won't be checked
        );

        let checks = ConformChecks {
            path_hygiene: false,
            ordering: false,
            reason_lint: false,
            survivability: false,
            tool_error_identity: false,
            sensor_id_format: false,
            artifact_pointers: false,
        };

        // Should pass because cockpit dir is skipped (no sensors to check)
        check_dir(artifacts.to_path_buf(), false, &checks, false, false)
            .expect("cockpit dir should be skipped in sensor enumeration");
    }

    #[test]
    fn check_dir_multiple_sensors_all_pass() {
        let temp = TempDir::new().expect("tempdir");
        let artifacts = temp.path();

        let json = minimal_sensor_report_json();
        write_file(&artifacts.join("alpha").join("report.json"), &json);
        write_file(&artifacts.join("beta").join("report.json"), &json);
        write_file(&artifacts.join("gamma").join("report.json"), &json);

        let checks = ConformChecks {
            path_hygiene: false,
            ordering: false,
            reason_lint: false,
            survivability: false,
            tool_error_identity: false,
            sensor_id_format: false,
            artifact_pointers: false,
        };

        check_dir(artifacts.to_path_buf(), false, &checks, false, false)
            .expect("all sensors should pass");
    }

    #[test]
    fn check_dir_one_sensor_fails_schema() {
        let temp = TempDir::new().expect("tempdir");
        let artifacts = temp.path();

        let json = minimal_sensor_report_json();
        write_file(&artifacts.join("alpha").join("report.json"), &json);
        write_file(&artifacts.join("beta").join("report.json"), "{}");

        let checks = ConformChecks {
            path_hygiene: false,
            ordering: false,
            reason_lint: false,
            survivability: false,
            tool_error_identity: false,
            sensor_id_format: false,
            artifact_pointers: false,
        };

        let err = check_dir(artifacts.to_path_buf(), false, &checks, false, false)
            .expect_err("one sensor fails");
        assert!(format!("{:#}", err).contains("conform-dir"));
    }

    #[test]
    fn check_dir_with_all_checks_passes_on_valid_sensors() {
        let temp = TempDir::new().expect("tempdir");
        let artifacts = temp.path();

        let json = minimal_sensor_report_json();
        write_file(&artifacts.join("sensor-a").join("report.json"), &json);
        write_file(&artifacts.join("sensor-b").join("report.json"), &json);

        let checks = ConformChecks {
            path_hygiene: true,
            ordering: true,
            reason_lint: true,
            survivability: true,
            tool_error_identity: true,
            sensor_id_format: true,
            artifact_pointers: true,
        };

        check_dir(artifacts.to_path_buf(), false, &checks, false, false)
            .expect("all checks pass on valid sensors");
    }

    #[test]
    fn check_dir_files_only_no_subdirectories() {
        let temp = TempDir::new().expect("tempdir");
        let artifacts = temp.path();

        // Write a file at root level, no subdirectories
        write_file(&artifacts.join("stray_file.json"), "{}");

        let checks = ConformChecks {
            path_hygiene: false,
            ordering: false,
            reason_lint: false,
            survivability: false,
            tool_error_identity: false,
            sensor_id_format: false,
            artifact_pointers: false,
        };

        check_dir(artifacts.to_path_buf(), false, &checks, false, false)
            .expect("files only dir should pass with 0 sensors");
    }

    #[test]
    fn check_dir_cockpit_valid_without_extended_checks() {
        let temp = TempDir::new().expect("tempdir");
        let artifacts = temp.path();

        write_file(
            &artifacts.join("sensor").join("report.json"),
            &minimal_sensor_report_json(),
        );

        let cockpit = minimal_cockpit_report();
        write_file(
            &artifacts.join("cockpit").join("report.json"),
            &serde_json::to_string(&cockpit).expect("serialize"),
        );

        let checks = ConformChecks {
            path_hygiene: false,
            ordering: false,
            reason_lint: false,
            survivability: false,
            tool_error_identity: false,
            sensor_id_format: false,
            artifact_pointers: false,
        };

        // validate_cockpit=true but no extended checks (reason_lint=false, presence_semantics=false)
        check_dir(artifacts.to_path_buf(), true, &checks, false, false)
            .expect("cockpit valid without extended checks");
    }

    #[test]
    fn check_dir_cockpit_invalid_json() {
        let temp = TempDir::new().expect("tempdir");
        let artifacts = temp.path();

        write_file(
            &artifacts.join("sensor").join("report.json"),
            &minimal_sensor_report_json(),
        );
        write_file(
            &artifacts.join("cockpit").join("report.json"),
            "not valid json",
        );

        let checks = ConformChecks {
            path_hygiene: false,
            ordering: false,
            reason_lint: false,
            survivability: false,
            tool_error_identity: false,
            sensor_id_format: false,
            artifact_pointers: false,
        };

        check_dir(artifacts.to_path_buf(), true, &checks, false, false)
            .expect_err("invalid cockpit json should error");
    }

    #[test]
    fn check_dir_sensor_with_survivability_violation() {
        let temp = TempDir::new().expect("tempdir");
        let artifacts = temp.path();

        let mut report = minimal_sensor_report();
        report.verdict.status = cockpitctl_types::VerdictStatus::Fail;
        report.findings.clear();
        report.verdict.reasons.clear();
        write_file(
            &artifacts.join("bad-sensor").join("report.json"),
            &serde_json::to_string(&report).expect("serialize"),
        );

        let checks = ConformChecks {
            path_hygiene: false,
            ordering: false,
            reason_lint: false,
            survivability: true,
            tool_error_identity: false,
            sensor_id_format: false,
            artifact_pointers: false,
        };

        let err = check_dir(artifacts.to_path_buf(), false, &checks, false, false)
            .expect_err("survivability violation in dir scan");
        assert!(format!("{:#}", err).contains("conform-dir"));
    }

    // ── main_entry integration ───────────────────────────────────────────

    #[test]
    fn main_entry_check_dir_returns_0() {
        let temp = TempDir::new().expect("tempdir");
        let artifacts = temp.path();
        write_file(
            &artifacts.join("sensor").join("report.json"),
            &minimal_sensor_report_json(),
        );

        let code = main_entry(Cli {
            command: Commands::CheckDir {
                dir: artifacts.to_path_buf(),
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
                allow_missing_report: false,
            },
        });
        assert_eq!(code, 0);
    }

    #[test]
    fn main_entry_check_dir_returns_1_on_missing_dir() {
        let temp = TempDir::new().expect("tempdir");

        let code = main_entry(Cli {
            command: Commands::CheckDir {
                dir: temp.path().join("nonexistent"),
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
                allow_missing_report: false,
            },
        });
        assert_eq!(code, 1);
    }

    #[test]
    fn main_entry_check_all_passes() {
        let temp = TempDir::new().expect("tempdir");
        let report_path = temp.path().join("report.json");
        write_file(&report_path, &minimal_sensor_report_json());

        let code = main_entry(Cli {
            command: Commands::Check {
                report: report_path,
                golden: None,
                sensor_id: Some("my-sensor".to_string()),
                survivability: false,
                path_hygiene: false,
                ordering: false,
                reason_lint: false,
                all: true,
                tool_error_identity: false,
                sensor_id_format: false,
                artifact_pointers: false,
            },
        });
        assert_eq!(code, 0);
    }

    #[test]
    fn main_entry_returns_1_on_conformance_failure() {
        let temp = TempDir::new().expect("tempdir");
        let report_path = temp.path().join("report.json");
        write_file(&report_path, "{}");

        let code = main_entry(Cli {
            command: Commands::Check {
                report: report_path,
                golden: None,
                sensor_id: None,
                survivability: false,
                path_hygiene: false,
                ordering: false,
                reason_lint: false,
                all: false,
                tool_error_identity: false,
                sensor_id_format: false,
                artifact_pointers: false,
            },
        });
        assert_eq!(code, 1);
    }

    // ── run() dispatches correctly ───────────────────────────────────────

    #[test]
    fn run_check_dir_all_flag_enables_all_checks() {
        let temp = TempDir::new().expect("tempdir");
        let artifacts = temp.path();
        write_file(
            &artifacts.join("sensor-a").join("report.json"),
            &minimal_sensor_report_json(),
        );

        let cli = Cli {
            command: Commands::CheckDir {
                dir: artifacts.to_path_buf(),
                validate_cockpit: false,
                all: true,
                path_hygiene: false,
                ordering: false,
                reason_lint: false,
                survivability: false,
                tool_error_identity: false,
                sensor_id_format: false,
                artifact_pointers: false,
                presence_semantics: false,
                allow_missing_report: false,
            },
        };

        run(cli).expect("--all with valid reports should pass");
    }

    #[test]
    fn run_check_dir_presence_semantics_via_all() {
        let temp = TempDir::new().expect("tempdir");
        let artifacts = temp.path();
        write_file(
            &artifacts.join("sensor-a").join("report.json"),
            &minimal_sensor_report_json(),
        );
        let cockpit = minimal_cockpit_report();
        write_file(
            &artifacts.join("cockpit").join("report.json"),
            &serde_json::to_string(&cockpit).expect("serialize"),
        );

        let cli = Cli {
            command: Commands::CheckDir {
                dir: artifacts.to_path_buf(),
                validate_cockpit: true,
                all: true,
                path_hygiene: false,
                ordering: false,
                reason_lint: false,
                survivability: false,
                tool_error_identity: false,
                sensor_id_format: false,
                artifact_pointers: false,
                presence_semantics: false,
                allow_missing_report: false,
            },
        };

        run(cli).expect("presence_semantics via --all should pass on valid cockpit");
    }

    // ── Edge cases ───────────────────────────────────────────────────────

    #[test]
    fn check_with_findings_and_all_checks_passes_when_sorted() {
        let temp = TempDir::new().expect("tempdir");
        let report_path = temp.path().join("report.json");

        let mut report = minimal_sensor_report();
        // Add findings in correct sorted order: error first, then warn, then info
        report.findings.push(cockpitctl_types::Finding {
            severity: cockpitctl_types::Severity::Error,
            check_id: None,
            code: "a_code".to_string(),
            message: "error finding".to_string(),
            location: Some(cockpitctl_types::Location {
                path: Some("src/lib.rs".to_string()),
                line: Some(1),
                col: None,
            }),
            help: None,
            url: None,
            fingerprint: None,
            data: None,
        });
        report.findings.push(cockpitctl_types::Finding {
            severity: cockpitctl_types::Severity::Warn,
            check_id: None,
            code: "b_code".to_string(),
            message: "warn finding".to_string(),
            location: Some(cockpitctl_types::Location {
                path: Some("src/lib.rs".to_string()),
                line: Some(10),
                col: None,
            }),
            help: None,
            url: None,
            fingerprint: None,
            data: None,
        });
        report.findings.push(cockpitctl_types::Finding {
            severity: cockpitctl_types::Severity::Info,
            check_id: None,
            code: "c_code".to_string(),
            message: "info finding".to_string(),
            location: None,
            help: None,
            url: None,
            fingerprint: None,
            data: None,
        });
        report.verdict.counts.error = 1;
        report.verdict.counts.warn = 1;
        report.verdict.counts.info = 1;
        report.verdict.status = cockpitctl_types::VerdictStatus::Fail;
        write_file(
            &report_path,
            &serde_json::to_string(&report).expect("serialize"),
        );

        check(
            report_path,
            None,
            Some("sensor".to_string()),
            true,
            true,
            true,
            true,
            true,
            true,
            true,
            false, // not all, but each individually
        )
        .expect("sorted findings should pass all checks");
    }

    #[test]
    fn check_dir_cockpit_with_presence_semantics_only() {
        let temp = TempDir::new().expect("tempdir");
        let artifacts = temp.path();

        write_file(
            &artifacts.join("sensor").join("report.json"),
            &minimal_sensor_report_json(),
        );

        let cockpit = minimal_cockpit_report();
        write_file(
            &artifacts.join("cockpit").join("report.json"),
            &serde_json::to_string(&cockpit).expect("serialize"),
        );

        let checks = ConformChecks {
            path_hygiene: false,
            ordering: false,
            reason_lint: false,
            survivability: false,
            tool_error_identity: false,
            sensor_id_format: false,
            artifact_pointers: false,
        };

        check_dir(
            artifacts.to_path_buf(),
            true,
            &checks,
            false,
            true, // presence_semantics
        )
        .expect("presence_semantics on empty cockpit should pass");
    }

    #[test]
    fn check_dir_allow_missing_report_with_all_missing() {
        let temp = TempDir::new().expect("tempdir");
        let artifacts = temp.path();

        // Sensor dirs without report.json
        fs::create_dir_all(artifacts.join("sensor-a")).expect("mkdir");
        fs::create_dir_all(artifacts.join("sensor-b")).expect("mkdir");

        let checks = ConformChecks {
            path_hygiene: false,
            ordering: false,
            reason_lint: false,
            survivability: false,
            tool_error_identity: false,
            sensor_id_format: false,
            artifact_pointers: false,
        };

        // With allow_missing_report, should pass
        check_dir(artifacts.to_path_buf(), false, &checks, true, false)
            .expect("allow missing report should pass");

        // Without allow_missing_report, should fail
        let err = check_dir(artifacts.to_path_buf(), false, &checks, false, false)
            .expect_err("missing reports should fail");
        assert!(format!("{:#}", err).contains("conform-dir"));
    }

    #[test]
    fn check_path_hygiene_violation_on_absolute_path() {
        let temp = TempDir::new().expect("tempdir");
        let report_path = temp.path().join("report.json");

        let mut report = minimal_sensor_report();
        report.findings.push(cockpitctl_types::Finding {
            severity: cockpitctl_types::Severity::Warn,
            check_id: None,
            code: "test".to_string(),
            message: "test".to_string(),
            location: Some(cockpitctl_types::Location {
                path: Some("/etc/passwd".to_string()),
                line: None,
                col: None,
            }),
            help: None,
            url: None,
            fingerprint: None,
            data: None,
        });
        report.verdict.counts.warn = 1;
        write_file(
            &report_path,
            &serde_json::to_string(&report).expect("serialize"),
        );

        let err = check(
            report_path,
            None,
            Some("sensor".to_string()),
            false,
            true, // path_hygiene
            false,
            false,
            false,
            false,
            false,
            false,
        )
        .expect_err("absolute path should fail hygiene");
        assert!(format!("{:#}", err).contains("conformance failed"));
    }

    #[test]
    fn check_reason_lint_passes_on_valid_tokens() {
        let temp = TempDir::new().expect("tempdir");
        let report_path = temp.path().join("report.json");

        let mut report = minimal_sensor_report();
        report.verdict.reasons = vec!["valid_token".to_string(), "another_one".to_string()];
        write_file(
            &report_path,
            &serde_json::to_string(&report).expect("serialize"),
        );

        check(
            report_path,
            None,
            Some("sensor".to_string()),
            false,
            false,
            false,
            true, // reason_lint
            false,
            false,
            false,
            false,
        )
        .expect("valid tokens should pass reason_lint");
    }

    #[test]
    fn check_multiple_violations_reports_count() {
        let temp = TempDir::new().expect("tempdir");
        let report_path = temp.path().join("report.json");

        let mut report = minimal_sensor_report();
        // Multiple path hygiene violations
        for i in 0..3 {
            report.findings.push(cockpitctl_types::Finding {
                severity: cockpitctl_types::Severity::Warn,
                check_id: None,
                code: format!("code_{}", i),
                message: format!("finding {}", i),
                location: Some(cockpitctl_types::Location {
                    path: Some(format!("../../bad/path_{}", i)),
                    line: Some(i + 1),
                    col: None,
                }),
                help: None,
                url: None,
                fingerprint: None,
                data: None,
            });
        }
        report.verdict.counts.warn = 3;
        write_file(
            &report_path,
            &serde_json::to_string(&report).expect("serialize"),
        );

        let err = check(
            report_path,
            None,
            Some("sensor".to_string()),
            false,
            true, // path_hygiene
            false,
            false,
            false,
            false,
            false,
            false,
        )
        .expect_err("multiple violations");
        let msg = format!("{:#}", err);
        assert!(msg.contains("violation(s)"));
    }

    #[test]
    fn check_sensor_id_format_passes_on_valid_ids() {
        let temp = TempDir::new().expect("tempdir");
        let report_path = temp.path().join("report.json");
        write_file(&report_path, &minimal_sensor_report_json());

        // Valid IDs: alphanumeric, hyphens, underscores
        for id in &["my-sensor", "sensor_v2", "SensorA", "abc123", "a-b_c"] {
            check(
                report_path.clone(),
                None,
                Some(id.to_string()),
                false,
                false,
                false,
                false,
                false,
                true, // sensor_id_format
                false,
                false,
            )
            .unwrap_or_else(|_| panic!("sensor id '{}' should be valid", id));
        }
    }

    #[test]
    fn check_dir_validate_cockpit_not_requested() {
        let temp = TempDir::new().expect("tempdir");
        let artifacts = temp.path();
        write_file(
            &artifacts.join("sensor").join("report.json"),
            &minimal_sensor_report_json(),
        );
        // Invalid cockpit report exists, but we're not asking to validate it
        write_file(&artifacts.join("cockpit").join("report.json"), "{}");

        let checks = ConformChecks {
            path_hygiene: false,
            ordering: false,
            reason_lint: false,
            survivability: false,
            tool_error_identity: false,
            sensor_id_format: false,
            artifact_pointers: false,
        };

        // Should pass because validate_cockpit=false
        check_dir(artifacts.to_path_buf(), false, &checks, false, false)
            .expect("cockpit should not be validated when not requested");
    }

    #[test]
    fn check_golden_determinism_check_happens_before_other_checks() {
        let temp = TempDir::new().expect("tempdir");
        let report_path = temp.path().join("report.json");
        let golden_path = temp.path().join("golden.json");
        write_file(&report_path, &minimal_sensor_report_json());
        write_file(&golden_path, "different content");

        // Even with --all, determinism check fails first
        let err = check(
            report_path,
            Some(golden_path),
            Some("sensor".to_string()),
            false,
            false,
            false,
            false,
            false,
            false,
            false,
            true,
        )
        .expect_err("golden mismatch should fail before other checks");
        assert!(format!("{:#}", err).contains("determinism check failed"));
    }

    #[test]
    fn check_report_with_valid_artifact_pointers() {
        let temp = TempDir::new().expect("tempdir");
        let report_path = temp.path().join("report.json");

        let mut report = minimal_sensor_report();
        report.artifacts.push(cockpitctl_types::ArtifactPointer {
            id: "coverage".to_string(),
            path: "coverage/lcov.info".to_string(),
            mime: "text/plain".to_string(),
            schema: None,
        });
        write_file(
            &report_path,
            &serde_json::to_string(&report).expect("serialize"),
        );

        check(
            report_path,
            None,
            Some("sensor".to_string()),
            false,
            false,
            false,
            false,
            false,
            false,
            true, // artifact_pointers
            false,
        )
        .expect("valid artifact pointers should pass");
    }
}
