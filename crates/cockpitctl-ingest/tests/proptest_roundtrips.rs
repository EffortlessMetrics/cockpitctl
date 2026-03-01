//! Property-based roundtrip and invariant tests for cockpitctl-ingest.
//!
//! Covers: output validity, schema version, verdict monotonicity, sensor
//! completeness, finding ordering, empty/all-pass/any-fail semantics.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};

use cockpitctl_ingest::{
    CommentRead, DiscoveredSensors, IngestRequest, IngestUseCase, NoOpSchemaValidator, OutputSink,
    PolicySource, ReceiptSource, ReportRead,
};
use cockpitctl_types::{
    CockpitConfig, CockpitReport, Finding, Location, MissingPolicy, RunInfo, SensorPolicy,
    SensorReport, Severity, ToolInfo, Verdict, VerdictCounts, VerdictStatus, severity_rank,
    verdict_status_rank,
};
use proptest::prelude::*;

// ============================================================================
// Strategies
// ============================================================================

fn arb_severity() -> impl Strategy<Value = Severity> {
    prop_oneof![
        Just(Severity::Info),
        Just(Severity::Warn),
        Just(Severity::Error),
    ]
}

fn arb_verdict_status() -> impl Strategy<Value = VerdictStatus> {
    prop_oneof![
        Just(VerdictStatus::Pass),
        Just(VerdictStatus::Warn),
        Just(VerdictStatus::Fail),
        Just(VerdictStatus::Skip),
    ]
}

fn arb_location() -> impl Strategy<Value = Option<Location>> {
    prop::option::of((
        prop::option::of("[a-z/_.-]{1,30}"),
        prop::option::of(1u32..10000u32),
        prop::option::of(1u32..500u32),
    ))
    .prop_map(|opt| opt.map(|(path, line, col)| Location { path, line, col }))
}

fn arb_finding() -> impl Strategy<Value = Finding> {
    (
        arb_severity(),
        prop::option::of("[A-Z][A-Z0-9_]{0,10}"),
        "[A-Z][A-Z0-9_./-]{0,15}",
        ".{1,50}",
        arb_location(),
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

fn arb_sensor_report() -> impl Strategy<Value = SensorReport> {
    (
        arb_verdict_status(),
        (0u64..50, 0u64..50, 0u64..50, 0u64..10).prop_map(|(i, w, e, s)| VerdictCounts {
            info: i,
            warn: w,
            error: e,
            suppressed: s,
        }),
        prop::collection::vec(arb_finding(), 0..8),
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

/// Generate 1..max_sensors unique sensor IDs with corresponding SensorReports.
fn arb_receipt_set(max_sensors: usize) -> impl Strategy<Value = Vec<(String, SensorReport)>> {
    prop::collection::vec(("[a-z][a-z0-9]{0,8}", arb_sensor_report()), 1..=max_sensors).prop_map(
        |pairs| {
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

struct RoundtripReceipts {
    sensors: Vec<String>,
    reports: HashMap<String, Vec<u8>>,
}

impl RoundtripReceipts {
    fn from_set(set: &[(String, SensorReport)]) -> Self {
        let sensors: Vec<String> = set.iter().map(|(id, _)| id.clone()).collect();
        let reports: HashMap<String, Vec<u8>> = set
            .iter()
            .map(|(id, report)| (id.clone(), serde_json::to_vec(report).unwrap()))
            .collect();
        Self { sensors, reports }
    }

    fn empty() -> Self {
        Self {
            sensors: vec![],
            reports: HashMap::new(),
        }
    }
}

impl ReceiptSource for RoundtripReceipts {
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

struct StaticPolicy {
    cfg: Option<CockpitConfig>,
}

impl PolicySource for StaticPolicy {
    fn load_config(&self) -> anyhow::Result<Option<CockpitConfig>> {
        Ok(self.cfg.clone())
    }
}

#[derive(Default)]
struct CaptureSink {
    report_json: RefCell<String>,
    comment_md: RefCell<String>,
}

impl OutputSink for CaptureSink {
    fn write_cockpit_report(&self, json: &str) -> anyhow::Result<()> {
        *self.report_json.borrow_mut() = json.to_string();
        Ok(())
    }

    fn write_cockpit_comment(&self, md: &str) -> anyhow::Result<()> {
        *self.comment_md.borrow_mut() = md.to_string();
        Ok(())
    }
}

// ============================================================================
// Helpers
// ============================================================================

fn tool_info() -> ToolInfo {
    ToolInfo {
        name: "proptest-roundtrip".to_string(),
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

/// Run the ingest pipeline and return the structured report, written JSON, and exit code.
fn run_ingest_full(
    receipts: RoundtripReceipts,
    cfg: Option<CockpitConfig>,
) -> (CockpitReport, String, i32) {
    let policy = StaticPolicy { cfg };
    let output = CaptureSink::default();
    let uc = IngestUseCase::new(
        receipts,
        policy,
        output,
        NoOpSchemaValidator,
        |_report, _cfg| "<!-- comment -->".to_string(),
    );
    let result = uc
        .execute(default_request())
        .expect("ingest must not panic");
    (result.report, result.comment_md, result.exit_code)
}

/// Build a CockpitConfig with all given sensors marked as blocking.
fn blocking_config(ids: &[String]) -> CockpitConfig {
    let mut cfg = CockpitConfig::default();
    for id in ids {
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
    cfg
}

// ============================================================================
// 1. Ingest always produces output (never panics)
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn ingest_never_panics(set in arb_receipt_set(10)) {
        let receipts = RoundtripReceipts::from_set(&set);
        // Must not panic — the execute call is inside run_ingest_full which unwraps.
        let (report, comment, _exit) = run_ingest_full(receipts, None);
        prop_assert!(!report.schema.is_empty());
        prop_assert!(!comment.is_empty());
    }
}

// ============================================================================
// 2. Output report is valid JSON (roundtrip)
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn output_report_is_valid_json(set in arb_receipt_set(8)) {
        let receipts = RoundtripReceipts::from_set(&set);
        let (report, _, _) = run_ingest_full(receipts, None);

        let json = serde_json::to_string_pretty(&report).unwrap();
        // Must parse back as generic JSON value.
        let _value: serde_json::Value = serde_json::from_str(&json).unwrap();
        // Must parse back as typed CockpitReport.
        let roundtripped: CockpitReport = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(report, roundtripped, "JSON roundtrip must preserve report");
    }
}

// ============================================================================
// 3. Output always has correct schema version
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn output_schema_is_cockpit_report_v1(set in arb_receipt_set(8)) {
        let receipts = RoundtripReceipts::from_set(&set);
        let (report, _, _) = run_ingest_full(receipts, None);
        prop_assert_eq!(&report.schema, "cockpit.report.v1");
    }
}

// ============================================================================
// 4. Sensor count in report matches input count
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn sensor_count_matches_input(set in arb_receipt_set(8)) {
        let n = set.len();
        let receipts = RoundtripReceipts::from_set(&set);
        let (report, _, _) = run_ingest_full(receipts, None);
        prop_assert_eq!(
            report.sensors.len(), n,
            "output sensor count must equal input receipt count"
        );
    }
}

// ============================================================================
// 5. Verdict monotonicity — overall verdict ≥ worst blocking sensor
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// When all sensors are blocking, the overall verdict must be at least as
    /// severe as the worst individual sensor verdict.
    #[test]
    fn verdict_monotonicity_blocking(set in arb_receipt_set(6)) {
        let ids: Vec<String> = set.iter().map(|(id, _)| id.clone()).collect();
        let cfg = blocking_config(&ids);
        let receipts = RoundtripReceipts::from_set(&set);
        let (report, _, _) = run_ingest_full(receipts, Some(cfg));

        // Find the worst (lowest rank) sensor verdict from input.
        let worst_input_rank = set
            .iter()
            .map(|(_, sr)| verdict_status_rank(&sr.verdict.status))
            .min()
            .unwrap_or(2); // default Pass rank if somehow empty

        let overall_rank = verdict_status_rank(&report.verdict.status);
        prop_assert!(
            overall_rank <= worst_input_rank,
            "overall rank {} must be <= worst sensor rank {} (lower = more severe)",
            overall_rank,
            worst_input_rank,
        );
    }
}

// ============================================================================
// 6. Empty input → pass verdict
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(16))]

    #[test]
    fn empty_input_yields_pass(_dummy in 0u8..1) {
        let receipts = RoundtripReceipts::empty();
        let (report, _, exit_code) = run_ingest_full(receipts, None);
        prop_assert_eq!(report.verdict.status, VerdictStatus::Pass);
        prop_assert_eq!(exit_code, 0);
        prop_assert!(report.sensors.is_empty());
    }
}

// ============================================================================
// 7. All-pass input → pass overall (when blocking)
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn all_pass_blocking_yields_pass(
        sensor_ids in prop::collection::vec("[a-z][a-z0-9]{0,6}", 1..6)
    ) {
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

        let cfg = blocking_config(&ids);
        let receipts = RoundtripReceipts::from_set(&set);
        let (report, _, exit_code) = run_ingest_full(receipts, Some(cfg));
        prop_assert_eq!(report.verdict.status, VerdictStatus::Pass);
        prop_assert_eq!(exit_code, 0);
    }
}

// ============================================================================
// 8. Any-fail input on a blocking sensor → fail overall
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// If any blocking sensor has Fail verdict, overall must be Fail with exit 2.
    #[test]
    fn any_blocking_fail_yields_fail(
        pass_ids in prop::collection::vec("[a-z][a-z0-9]{0,6}", 0..4),
        fail_id in "[a-z][a-z0-9]{0,6}",
    ) {
        let mut seen = std::collections::HashSet::new();
        seen.insert(fail_id.clone());
        let pass_ids: Vec<String> = pass_ids.into_iter().filter(|id| seen.insert(id.clone())).collect();

        let mut set: Vec<(String, SensorReport)> = pass_ids
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

        // Add the failing sensor.
        set.push((
            fail_id.clone(),
            SensorReport {
                schema: "sensor.report.v1".to_string(),
                tool: tool_info(),
                run: run_info(),
                verdict: Verdict {
                    status: VerdictStatus::Fail,
                    counts: VerdictCounts { error: 1, ..Default::default() },
                    reasons: vec!["test-fail".to_string()],
                },
                findings: vec![Finding {
                    severity: Severity::Error,
                    check_id: None,
                    code: "PROP_FAIL".to_string(),
                    message: "proptest injected failure".to_string(),
                    location: None,
                    help: None,
                    url: None,
                    fingerprint: None,
                    data: None,
                }],
                artifacts: vec![],
                data: None,
            },
        ));

        let all_ids: Vec<String> = set.iter().map(|(id, _)| id.clone()).collect();
        let cfg = blocking_config(&all_ids);
        let receipts = RoundtripReceipts::from_set(&set);
        let (report, _, exit_code) = run_ingest_full(receipts, Some(cfg));
        prop_assert_eq!(report.verdict.status, VerdictStatus::Fail);
        prop_assert_eq!(exit_code, 2);
    }
}

// ============================================================================
// 9. Highlights in output are sorted (severity desc)
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn highlights_are_severity_sorted(set in arb_receipt_set(8)) {
        let ids: Vec<String> = set.iter().map(|(id, _)| id.clone()).collect();
        let cfg = blocking_config(&ids);
        let receipts = RoundtripReceipts::from_set(&set);
        let (report, _, _) = run_ingest_full(receipts, Some(cfg));

        // Highlights must be in non-decreasing severity rank order (lower rank = more severe).
        for window in report.highlights.windows(2) {
            let rank_a = severity_rank(&window[0].finding.severity);
            let rank_b = severity_rank(&window[1].finding.severity);
            prop_assert!(
                rank_a <= rank_b,
                "highlights must be sorted by severity desc: {:?} (rank {}) should come before {:?} (rank {})",
                window[0].finding.severity, rank_a,
                window[1].finding.severity, rank_b,
            );
        }
    }
}

// ============================================================================
// 10. Report includes all sensor IDs from input (completeness)
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn report_includes_all_input_sensor_ids(set in arb_receipt_set(8)) {
        let input_ids: std::collections::HashSet<String> =
            set.iter().map(|(id, _)| id.clone()).collect();
        let receipts = RoundtripReceipts::from_set(&set);
        let (report, _, _) = run_ingest_full(receipts, None);

        let output_ids: std::collections::HashSet<String> =
            report.sensors.iter().map(|s| s.id.clone()).collect();

        for id in &input_ids {
            prop_assert!(
                output_ids.contains(id),
                "sensor '{}' from input must appear in output report",
                id
            );
        }
    }
}

// ============================================================================
// 11. Non-blocking fail does NOT cause overall fail
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn non_blocking_fail_does_not_cause_overall_fail(
        sensor_id in "[a-z][a-z0-9]{0,6}"
    ) {
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
                    code: "NB_FAIL".to_string(),
                    message: "non-blocking failure".to_string(),
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

        let mut cfg = CockpitConfig::default();
        cfg.sensors.insert(
            sensor_id,
            SensorPolicy {
                blocking: false,
                missing: MissingPolicy::Skip,
                section: None,
                require_label: None,
                repro: None,
            },
        );

        let receipts = RoundtripReceipts::from_set(&set);
        let (report, _, exit_code) = run_ingest_full(receipts, Some(cfg));

        // Non-blocking sensor fails should NOT escalate to overall Fail.
        prop_assert_ne!(
            report.verdict.status, VerdictStatus::Fail,
            "non-blocking sensor failure must not cause overall fail"
        );
        prop_assert_eq!(exit_code, 0);
    }
}

// ============================================================================
// 12. Output sensors are sorted (deterministic ordering)
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// Sensor summaries in the output must be in a deterministic (sorted) order.
    #[test]
    fn output_sensors_are_deterministically_ordered(set in arb_receipt_set(8)) {
        let receipts = RoundtripReceipts::from_set(&set);
        let (report, _, _) = run_ingest_full(receipts, None);

        let sensor_ids: Vec<String> = report.sensors.iter().map(|s| s.id.clone()).collect();
        let mut sorted_ids = sensor_ids.clone();
        sorted_ids.sort();
        prop_assert_eq!(
            sensor_ids, sorted_ids,
            "output sensor IDs must be in lexical order"
        );
    }
}
