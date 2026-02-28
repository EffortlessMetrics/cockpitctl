//! Property-based tests for cockpitctl-ingest.
//!
//! Tests orchestration invariants: valid output, determinism, exit codes, sensor counts.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};

use cockpitctl_ingest::{
    CommentRead, DiscoveredSensors, IngestRequest, IngestUseCase, NoOpSchemaValidator, OutputSink,
    PolicySource, ReceiptSource, ReportRead,
};
use cockpitctl_types::{
    CockpitConfig, CockpitReport, Finding, Location, MissingPolicy, RunInfo, SensorPolicy,
    SensorReport, Severity, ToolInfo, Verdict, VerdictCounts, VerdictStatus,
};
use proptest::prelude::*;

// ============================================================================
// Strategies
// ============================================================================

fn any_severity() -> impl Strategy<Value = Severity> {
    prop_oneof![
        Just(Severity::Info),
        Just(Severity::Warn),
        Just(Severity::Error),
    ]
}

fn any_verdict_status() -> impl Strategy<Value = VerdictStatus> {
    prop_oneof![
        Just(VerdictStatus::Pass),
        Just(VerdictStatus::Warn),
        Just(VerdictStatus::Fail),
        Just(VerdictStatus::Skip),
    ]
}

fn any_location() -> impl Strategy<Value = Option<Location>> {
    prop::option::of((
        prop::option::of("[a-z/_.-]{1,30}"),
        prop::option::of(1u32..10000u32),
        prop::option::of(1u32..500u32),
    ))
    .prop_map(|opt| opt.map(|(path, line, col)| Location { path, line, col }))
}

fn any_finding() -> impl Strategy<Value = Finding> {
    (
        any_severity(),
        prop::option::of("[A-Z][A-Z0-9_]{0,10}"),
        "[A-Z][A-Z0-9_./-]{0,15}",
        ".{1,50}",
        any_location(),
    )
        .prop_map(|(severity, check_id, code, message, location)| Finding {
            severity,
            check_id,
            code,
            message,
            location,
            help: None,
            url: None,
            fingerprint: None,
            data: None,
        })
}

fn any_verdict_counts() -> impl Strategy<Value = VerdictCounts> {
    (0u64..100, 0u64..100, 0u64..100, 0u64..20).prop_map(|(info, warn, error, suppressed)| {
        VerdictCounts {
            info,
            warn,
            error,
            suppressed,
        }
    })
}

fn any_sensor_report() -> impl Strategy<Value = SensorReport> {
    (
        any_verdict_status(),
        any_verdict_counts(),
        prop::collection::vec(any_finding(), 0..10),
    )
        .prop_map(|(status, counts, findings)| SensorReport {
            schema: "sensor.report.v1".to_string(),
            tool: tool_info(),
            run: run_info(),
            verdict: Verdict {
                status,
                counts,
                reasons: vec![],
            },
            findings,
            artifacts: vec![],
            data: None,
        })
}

/// Generate 1..max_sensors valid sensor IDs with corresponding SensorReports.
fn any_receipt_set(max_sensors: usize) -> impl Strategy<Value = Vec<(String, SensorReport)>> {
    prop::collection::vec(("[a-z][a-z0-9]{0,8}", any_sensor_report()), 1..=max_sensors).prop_map(
        |pairs| {
            // Deduplicate sensor IDs, keeping the first occurrence.
            let mut seen = std::collections::HashSet::new();
            pairs
                .into_iter()
                .filter(|(id, _)| seen.insert(id.clone()))
                .collect()
        },
    )
}

// ============================================================================
// Test doubles
// ============================================================================

struct PropReceipts {
    sensors: Vec<String>,
    reports: HashMap<String, Vec<u8>>,
}

impl PropReceipts {
    fn from_set(set: &[(String, SensorReport)]) -> Self {
        let sensors: Vec<String> = set.iter().map(|(id, _)| id.clone()).collect();
        let reports: HashMap<String, Vec<u8>> = set
            .iter()
            .map(|(id, report)| (id.clone(), serde_json::to_vec(report).unwrap()))
            .collect();
        Self { sensors, reports }
    }
}

impl ReceiptSource for PropReceipts {
    fn discovered_sensors(&self) -> anyhow::Result<DiscoveredSensors> {
        Ok(DiscoveredSensors {
            sensors: self.sensors.clone(),
            truncated: false,
            total_found: self.sensors.len(),
            invalid_sensor_ids: vec![],
        })
    }

    fn read_report_bytes(&self, sensor_id: &str) -> anyhow::Result<ReportRead> {
        match self.reports.get(sensor_id) {
            Some(bytes) => Ok(ReportRead::Bytes(bytes.clone())),
            None => Ok(ReportRead::Missing),
        }
    }

    fn report_path(&self, sensor_id: &str) -> String {
        format!("artifacts/{}/report.json", sensor_id)
    }

    fn comment_path_if_present(&self, _sensor_id: &str) -> anyhow::Result<CommentRead> {
        Ok(CommentRead::Missing)
    }
}

struct PropPolicy {
    cfg: Option<CockpitConfig>,
}

impl PolicySource for PropPolicy {
    fn load_config(&self) -> anyhow::Result<Option<CockpitConfig>> {
        Ok(self.cfg.clone())
    }
}

#[derive(Default)]
struct PropOutput {
    reports: RefCell<Vec<String>>,
    comments: RefCell<Vec<String>>,
}

impl OutputSink for PropOutput {
    fn write_cockpit_report(&self, json: &str) -> anyhow::Result<()> {
        self.reports.borrow_mut().push(json.to_string());
        Ok(())
    }

    fn write_cockpit_comment(&self, md: &str) -> anyhow::Result<()> {
        self.comments.borrow_mut().push(md.to_string());
        Ok(())
    }
}

// ============================================================================
// Helpers
// ============================================================================

fn tool_info() -> ToolInfo {
    ToolInfo {
        name: "proptest".to_string(),
        version: "0.0.0".to_string(),
        commit: None,
    }
}

fn run_info() -> RunInfo {
    RunInfo {
        started_at: "2026-01-01T00:00:00Z".to_string(),
        ended_at: None,
        duration_ms: None,
        host: None,
        git: None,
        ci: None,
        capabilities: BTreeMap::new(),
    }
}

fn default_request() -> IngestRequest {
    IngestRequest {
        labels: vec![],
        tool: tool_info(),
        run: run_info(),
        schema_validation_override: None,
    }
}

fn run_ingest(receipts: PropReceipts, cfg: Option<CockpitConfig>) -> (CockpitReport, String, i32) {
    let policy = PropPolicy { cfg };
    let output = PropOutput::default();
    let uc = IngestUseCase::new(
        receipts,
        policy,
        output,
        NoOpSchemaValidator,
        |_report, _cfg| "COMMENT".to_string(),
    );
    let result = uc.execute(default_request()).expect("ingest must not fail");
    (result.report, result.comment_md, result.exit_code)
}

// ============================================================================
// Property: valid output invariant
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// Any set of valid receipts produces a CockpitReport with correct schema,
    /// non-empty sensors list, and a valid verdict status.
    #[test]
    fn valid_receipts_produce_valid_report(set in any_receipt_set(8)) {
        let receipts = PropReceipts::from_set(&set);
        let (report, comment, _exit_code) = run_ingest(receipts, None);

        // Schema must be cockpit.report.v1.
        prop_assert_eq!(&report.schema, "cockpit.report.v1");

        // Sensor count matches input.
        prop_assert_eq!(report.sensors.len(), set.len());

        // Verdict status is one of the valid values.
        prop_assert!(
            matches!(
                report.verdict.status,
                VerdictStatus::Pass | VerdictStatus::Warn | VerdictStatus::Fail | VerdictStatus::Skip
            ),
            "verdict status must be valid"
        );

        // Comment was generated.
        prop_assert!(!comment.is_empty());

        // Report JSON round-trips.
        let json = serde_json::to_string(&report).unwrap();
        let parsed: CockpitReport = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(report, parsed);
    }
}

// ============================================================================
// Property: determinism — same receipts in same order → same report
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn same_receipts_same_order_produce_identical_report(set in any_receipt_set(6)) {
        let receipts_a = PropReceipts::from_set(&set);
        let receipts_b = PropReceipts::from_set(&set);

        let (report_a, comment_a, exit_a) = run_ingest(receipts_a, None);
        let (report_b, comment_b, exit_b) = run_ingest(receipts_b, None);

        prop_assert_eq!(&report_a, &report_b, "reports must be identical");
        prop_assert_eq!(&comment_a, &comment_b, "comments must be identical");
        prop_assert_eq!(exit_a, exit_b, "exit codes must be identical");
    }
}

// ============================================================================
// Property: exit code contract — pass=0, policy_fail=2, error=1
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// Exit code is 0 for non-Fail verdicts and 2 for Fail.
    #[test]
    fn exit_code_matches_verdict(set in any_receipt_set(6)) {
        let receipts = PropReceipts::from_set(&set);
        let (report, _, exit_code) = run_ingest(receipts, None);

        match report.verdict.status {
            VerdictStatus::Fail => prop_assert_eq!(exit_code, 2, "Fail verdict must produce exit code 2"),
            _ => prop_assert_eq!(exit_code, 0, "non-Fail verdict must produce exit code 0"),
        }
    }

    /// With all blocking sensors passing, exit code is 0.
    #[test]
    fn all_pass_blocking_gives_exit_zero(
        sensor_ids in prop::collection::vec("[a-z][a-z0-9]{0,6}", 1..5)
    ) {
        // Deduplicate.
        let mut seen = std::collections::HashSet::new();
        let ids: Vec<String> = sensor_ids.into_iter().filter(|id| seen.insert(id.clone())).collect();
        if ids.is_empty() {
            return Ok(());
        }

        let set: Vec<(String, SensorReport)> = ids
            .iter()
            .map(|id| {
                (
                    id.clone(),
                    SensorReport {
                        schema: "sensor.report.v1".to_string(),
                        tool: tool_info(),
                        run: run_info(),
                        verdict: Verdict {
                            status: VerdictStatus::Pass,
                            counts: VerdictCounts::default(),
                            reasons: vec![],
                        },
                        findings: vec![],
                        artifacts: vec![],
                        data: None,
                    },
                )
            })
            .collect();

        let receipts = PropReceipts::from_set(&set);

        let mut cfg = CockpitConfig::default();
        for id in &ids {
            cfg.sensors.insert(
                id.clone(),
                SensorPolicy {
                    blocking: true,
                    missing: MissingPolicy::Fail,
                    section: None,
                    require_label: None,
                    repro: None,
                },
            );
        }

        let (_, _, exit_code) = run_ingest(receipts, Some(cfg));
        prop_assert_eq!(exit_code, 0, "all-pass blocking sensors must produce exit code 0");
    }

    /// A single blocking sensor with Fail verdict produces exit code 2.
    #[test]
    fn blocking_fail_gives_exit_two(sensor_id in "[a-z][a-z0-9]{0,6}") {
        let set = vec![(
            sensor_id.clone(),
            SensorReport {
                schema: "sensor.report.v1".to_string(),
                tool: tool_info(),
                run: run_info(),
                verdict: Verdict {
                    status: VerdictStatus::Fail,
                    counts: VerdictCounts { error: 1, ..Default::default() },
                    reasons: vec!["failure".to_string()],
                },
                findings: vec![Finding {
                    severity: Severity::Error,
                    check_id: None,
                    code: "TEST_FAIL".to_string(),
                    message: "test failure".to_string(),
                    location: None,
                    help: None,
                    url: None,
                    fingerprint: None,
                    data: None,
                }],
                artifacts: vec![],
                data: None,
            },
        )];

        let receipts = PropReceipts::from_set(&set);

        let mut cfg = CockpitConfig::default();
        cfg.sensors.insert(
            sensor_id,
            SensorPolicy {
                blocking: true,
                missing: MissingPolicy::Fail,
                section: None,
                require_label: None,
                repro: None,
            },
        );

        let (_, _, exit_code) = run_ingest(receipts, Some(cfg));
        prop_assert_eq!(exit_code, 2, "blocking Fail must produce exit code 2");
    }
}

// ============================================================================
// Property: sensor count — output sensor count ≤ input receipt count
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// Output sensor count equals input receipt count when no config overrides.
    #[test]
    fn sensor_count_matches_input_without_config(set in any_receipt_set(8)) {
        let input_count = set.len();
        let receipts = PropReceipts::from_set(&set);
        let (report, _, _) = run_ingest(receipts, None);

        prop_assert_eq!(
            report.sensors.len(),
            input_count,
            "sensor count must match input receipt count when using discovered sensors"
        );
    }

    /// Output sensor count equals configured sensor count (which may differ from discovered).
    #[test]
    fn sensor_count_matches_config(set in any_receipt_set(6)) {
        let ids: Vec<String> = set.iter().map(|(id, _)| id.clone()).collect();
        let receipts = PropReceipts::from_set(&set);

        // Use only a subset of sensor IDs in the config.
        let cfg_ids: Vec<String> = if ids.len() > 1 {
            ids[..ids.len() / 2].to_vec()
        } else {
            ids.clone()
        };

        let mut cfg = CockpitConfig::default();
        for id in &cfg_ids {
            cfg.sensors.insert(
                id.clone(),
                SensorPolicy {
                    blocking: false,
                    missing: MissingPolicy::Skip,
                    section: None,
                    require_label: None,
                    repro: None,
                },
            );
        }

        let (report, _, _) = run_ingest(receipts, Some(cfg));
        prop_assert_eq!(
            report.sensors.len(),
            cfg_ids.len(),
            "sensor count must match configured sensor count"
        );
    }
}

// ============================================================================
// Property: highlights are bounded by max_highlights
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn highlights_bounded_by_max(set in any_receipt_set(8)) {
        let receipts = PropReceipts::from_set(&set);
        let (report, _, _) = run_ingest(receipts, None);

        // Default max_highlights is 10.
        prop_assert!(
            report.highlights.len() <= 10,
            "highlights count must be <= max_highlights (default 10)"
        );
    }
}

// ============================================================================
// Property: determinism with explicit config
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]

    #[test]
    fn deterministic_with_config(set in any_receipt_set(5)) {
        let ids: Vec<String> = set.iter().map(|(id, _)| id.clone()).collect();

        // Build a deterministic config for these sensors.
        let mut cfg = CockpitConfig::default();
        for id in &ids {
            cfg.sensors.insert(
                id.clone(),
                SensorPolicy {
                    blocking: true,
                    missing: MissingPolicy::Warn,
                    section: None,
                    require_label: None,
                    repro: None,
                },
            );
        }

        let receipts_a = PropReceipts::from_set(&set);
        let receipts_b = PropReceipts::from_set(&set);

        let (report_a, _, exit_a) = run_ingest(receipts_a, Some(cfg.clone()));
        let (report_b, _, exit_b) = run_ingest(receipts_b, Some(cfg));

        // Serialize to JSON for byte-level comparison.
        let json_a = serde_json::to_string_pretty(&report_a).unwrap();
        let json_b = serde_json::to_string_pretty(&report_b).unwrap();

        prop_assert_eq!(&json_a, &json_b, "JSON output must be byte-identical");
        prop_assert_eq!(exit_a, exit_b, "exit codes must match");
    }
}
