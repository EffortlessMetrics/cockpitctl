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

#[allow(clippy::too_many_arguments)]
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
}
