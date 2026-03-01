//! Property-based tests for ingest pipeline determinism under input shuffling.
//!
//! Tests that the pipeline produces identical output regardless of the order
//! in which sensor receipts are discovered.

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

fn any_finding() -> impl Strategy<Value = Finding> {
    (
        any_severity(),
        prop::option::of("[A-Z][A-Z0-9_]{0,10}"),
        "[A-Z][A-Z0-9_./-]{0,15}",
        ".{1,50}",
        prop::option::of(
            (
                prop::option::of("[a-z/_.-]{1,30}"),
                prop::option::of(1u32..10000u32),
            )
                .prop_map(|(path, line)| Location {
                    path,
                    line,
                    col: None,
                }),
        ),
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

fn any_sensor_report() -> impl Strategy<Value = SensorReport> {
    (
        any_verdict_status(),
        (0u64..50, 0u64..50, 0u64..50, 0u64..10).prop_map(|(i, w, e, s)| VerdictCounts {
            info: i,
            warn: w,
            error: e,
            suppressed: s,
        }),
        prop::collection::vec(any_finding(), 0..8),
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

fn any_receipt_set(max_sensors: usize) -> impl Strategy<Value = Vec<(String, SensorReport)>> {
    prop::collection::vec(("[a-z][a-z0-9]{0,8}", any_sensor_report()), 1..=max_sensors).prop_map(
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

struct ShuffledReceipts {
    sensors: Vec<String>,
    reports: HashMap<String, Vec<u8>>,
}

impl ShuffledReceipts {
    fn from_set(set: &[(String, SensorReport)], order: Vec<String>) -> Self {
        let reports: HashMap<String, Vec<u8>> = set
            .iter()
            .map(|(id, report)| (id.clone(), serde_json::to_vec(report).unwrap()))
            .collect();
        Self {
            sensors: order,
            reports,
        }
    }
}

impl ReceiptSource for ShuffledReceipts {
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

struct SimplePolicy {
    cfg: Option<CockpitConfig>,
}

impl PolicySource for SimplePolicy {
    fn load_config(&self) -> anyhow::Result<Option<CockpitConfig>> {
        Ok(self.cfg.clone())
    }
}

#[derive(Default)]
struct CaptureOutput {
    reports: RefCell<Vec<String>>,
    comments: RefCell<Vec<String>>,
}

impl OutputSink for CaptureOutput {
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

fn run_ingest(
    receipts: ShuffledReceipts,
    cfg: Option<CockpitConfig>,
) -> (CockpitReport, String, i32) {
    let policy = SimplePolicy { cfg };
    let output = CaptureOutput::default();
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
// Property: shuffled input order produces identical output
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// Shuffling the sensor discovery order must not change the output report.
    #[test]
    fn shuffled_order_produces_identical_report(set in any_receipt_set(6)) {
        let ids: Vec<String> = set.iter().map(|(id, _)| id.clone()).collect();

        // Original order (as generated).
        let receipts_fwd = ShuffledReceipts::from_set(&set, ids.clone());
        let (report_fwd, comment_fwd, exit_fwd) = run_ingest(receipts_fwd, None);

        // Reversed order.
        let mut ids_rev = ids.clone();
        ids_rev.reverse();
        let receipts_rev = ShuffledReceipts::from_set(&set, ids_rev);
        let (report_rev, comment_rev, exit_rev) = run_ingest(receipts_rev, None);

        // JSON must be byte-identical.
        let json_fwd = serde_json::to_string_pretty(&report_fwd).unwrap();
        let json_rev = serde_json::to_string_pretty(&report_rev).unwrap();
        prop_assert_eq!(&json_fwd, &json_rev, "report JSON must be identical regardless of order");
        prop_assert_eq!(&comment_fwd, &comment_rev, "comment must be identical");
        prop_assert_eq!(exit_fwd, exit_rev, "exit code must be identical");
    }

    /// Rotating the sensor discovery order must not change the output report.
    #[test]
    fn rotated_order_produces_identical_report(
        set in any_receipt_set(6),
        rotate_by in 0usize..10,
    ) {
        let ids: Vec<String> = set.iter().map(|(id, _)| id.clone()).collect();
        if ids.is_empty() {
            return Ok(());
        }

        // Original order.
        let receipts_orig = ShuffledReceipts::from_set(&set, ids.clone());
        let (report_orig, _, exit_orig) = run_ingest(receipts_orig, None);

        // Rotated order.
        let mut ids_rotated = ids;
        let actual_rotate = rotate_by % ids_rotated.len();
        ids_rotated.rotate_left(actual_rotate);
        let receipts_rot = ShuffledReceipts::from_set(&set, ids_rotated);
        let (report_rot, _, exit_rot) = run_ingest(receipts_rot, None);

        let json_orig = serde_json::to_string_pretty(&report_orig).unwrap();
        let json_rot = serde_json::to_string_pretty(&report_rot).unwrap();
        prop_assert_eq!(&json_orig, &json_rot, "rotated order must produce same report");
        prop_assert_eq!(exit_orig, exit_rot, "exit codes must match");
    }
}

// ============================================================================
// Property: shuffled order with config produces identical output
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]

    /// With explicit config, shuffled sensor order still produces identical output.
    #[test]
    fn shuffled_order_with_config_deterministic(set in any_receipt_set(5)) {
        let ids: Vec<String> = set.iter().map(|(id, _)| id.clone()).collect();

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

        // Forward order.
        let receipts_fwd = ShuffledReceipts::from_set(&set, ids.clone());
        let (report_fwd, _, exit_fwd) = run_ingest(receipts_fwd, Some(cfg.clone()));

        // Reversed order.
        let mut ids_rev = ids;
        ids_rev.reverse();
        let receipts_rev = ShuffledReceipts::from_set(&set, ids_rev);
        let (report_rev, _, exit_rev) = run_ingest(receipts_rev, Some(cfg));

        let json_fwd = serde_json::to_string_pretty(&report_fwd).unwrap();
        let json_rev = serde_json::to_string_pretty(&report_rev).unwrap();
        prop_assert_eq!(&json_fwd, &json_rev, "shuffled with config must be identical");
        prop_assert_eq!(exit_fwd, exit_rev);
    }
}

// ============================================================================
// Property: sensor count is invariant under shuffling
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// Sensor count in the output report is invariant under discovery order.
    #[test]
    fn sensor_count_invariant_under_shuffle(set in any_receipt_set(8)) {
        let ids: Vec<String> = set.iter().map(|(id, _)| id.clone()).collect();
        let expected_count = ids.len();

        let receipts_fwd = ShuffledReceipts::from_set(&set, ids.clone());
        let (report_fwd, _, _) = run_ingest(receipts_fwd, None);

        let mut ids_rev = ids;
        ids_rev.reverse();
        let receipts_rev = ShuffledReceipts::from_set(&set, ids_rev);
        let (report_rev, _, _) = run_ingest(receipts_rev, None);

        prop_assert_eq!(report_fwd.sensors.len(), expected_count);
        prop_assert_eq!(report_rev.sensors.len(), expected_count);
    }
}
