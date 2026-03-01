//! Comprehensive determinism regression harness.
//!
//! cockpitctl MUST produce byte-identical output for identical inputs.
//! These tests run 50–100 iterations to catch any nondeterminism in:
//! - Full pipeline output (report JSON + comment markdown)
//! - JSON serialization, SARIF conversion, annotation rendering
//! - Fingerprinting, sorting, hashing, and config parsing

#![allow(dead_code)]

use std::cell::RefCell;
use std::collections::BTreeMap;

use cockpitctl_core::domain::{
    derive_fingerprint, finding_sort_key, snapshot_policy, sort_findings,
};
use cockpitctl_core::ingest::{CommentRead, DiscoveredSensors, PlanRead, ReportRead};
use cockpitctl_core::render::{render_comment, render_github_annotations};
use cockpitctl_core::sarif::cockpit_report_to_sarif_json;
use cockpitctl_core::types::{
    CockpitConfig, Finding, Location, MissingPolicy, RunInfo, SensorPolicy, SensorReport, Severity,
    ToolInfo, Verdict, VerdictCounts, VerdictStatus,
};
use cockpitctl_core::{
    IngestRequest, IngestUseCase, NoOpSchemaValidator, OutputSink, PolicySource, ReceiptSource,
    policy_snapshot_sha256_hex,
};

// ---------------------------------------------------------------------------
// In-memory test doubles (mirrored from pipeline_inmemory.rs)
// ---------------------------------------------------------------------------

struct MemReceiptSource {
    sensors: Vec<String>,
    reports: BTreeMap<String, ReportRead>,
}

impl MemReceiptSource {
    fn new(sensors: Vec<&str>, reports: BTreeMap<String, ReportRead>) -> Self {
        Self {
            sensors: sensors.into_iter().map(String::from).collect(),
            reports,
        }
    }
}

impl ReceiptSource for MemReceiptSource {
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
            Some(ReportRead::Bytes(b)) => Ok(ReportRead::Bytes(b.clone())),
            Some(ReportRead::Oversized { size, cap }) => Ok(ReportRead::Oversized {
                size: *size,
                cap: *cap,
            }),
            Some(ReportRead::UnsafePath) => Ok(ReportRead::UnsafePath),
            Some(ReportRead::Missing) | None => Ok(ReportRead::Missing),
        }
    }

    fn report_path(&self, sensor_id: &str) -> String {
        format!("artifacts/{sensor_id}/report.json")
    }

    fn comment_path_if_present(&self, _sensor_id: &str) -> anyhow::Result<CommentRead> {
        Ok(CommentRead::Missing)
    }

    fn read_plan_bytes(&self, _sensor_id: &str) -> anyhow::Result<PlanRead> {
        Ok(PlanRead::Missing)
    }
}

struct MemPolicySource {
    config: Option<CockpitConfig>,
}

impl PolicySource for MemPolicySource {
    fn load_config(&self) -> anyhow::Result<Option<CockpitConfig>> {
        Ok(self.config.clone())
    }
}

struct MemOutputSink {
    report_json: RefCell<String>,
    comment_md: RefCell<String>,
}

impl MemOutputSink {
    fn new() -> Self {
        Self {
            report_json: RefCell::new(String::new()),
            comment_md: RefCell::new(String::new()),
        }
    }
}

impl OutputSink for MemOutputSink {
    fn write_cockpit_report(&self, json: &str) -> anyhow::Result<()> {
        *self.report_json.borrow_mut() = json.to_string();
        Ok(())
    }

    fn write_cockpit_comment(&self, md: &str) -> anyhow::Result<()> {
        *self.comment_md.borrow_mut() = md.to_string();
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Fixture builders
// ---------------------------------------------------------------------------

fn tool_info() -> ToolInfo {
    ToolInfo {
        name: "determinism-harness".to_string(),
        version: "0.0.1".to_string(),
        commit: None,
    }
}

fn run_info() -> RunInfo {
    RunInfo {
        started_at: "2026-06-01T00:00:00Z".to_string(),
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

fn make_finding(severity: Severity, code: &str, message: &str, path: &str, line: u32) -> Finding {
    Finding {
        severity,
        check_id: None,
        code: code.to_string(),
        message: message.to_string(),
        location: Some(Location {
            path: Some(path.to_string()),
            line: Some(line),
            col: None,
        }),
        help: None,
        url: None,
        fingerprint: None,
        data: None,
    }
}

fn make_sensor_report(status: VerdictStatus, findings: Vec<Finding>) -> Vec<u8> {
    let warn_count = findings
        .iter()
        .filter(|f| f.severity == Severity::Warn)
        .count() as u64;
    let error_count = findings
        .iter()
        .filter(|f| f.severity == Severity::Error)
        .count() as u64;
    let info_count = findings
        .iter()
        .filter(|f| f.severity == Severity::Info)
        .count() as u64;
    let report = SensorReport {
        schema: "sensor.report.v1".to_string(),
        tool: ToolInfo {
            name: "test-sensor".to_string(),
            version: "1.0.0".to_string(),
            commit: None,
        },
        run: RunInfo {
            started_at: "2026-06-01T00:00:00Z".to_string(),
            ended_at: None,
            duration_ms: None,
            host: None,
            git: None,
            ci: None,
            capabilities: BTreeMap::new(),
        },
        verdict: Verdict {
            status,
            counts: VerdictCounts {
                info: info_count,
                warn: warn_count,
                error: error_count,
                suppressed: 0,
            },
            reasons: vec![],
        },
        findings,
        artifacts: vec![],
        data: None,
    };
    serde_json::to_vec(&report).unwrap()
}

fn blocking_sensor() -> SensorPolicy {
    SensorPolicy {
        blocking: true,
        missing: MissingPolicy::Fail,
        ..Default::default()
    }
}

/// Build a comprehensive 5-sensor fixture with 20 findings across sensors,
/// mixed severities, and highlights at budget boundary.
fn five_sensor_fixture() -> (Vec<(&'static str, Vec<u8>)>, CockpitConfig) {
    let sensor_a_findings = vec![
        make_finding(
            Severity::Error,
            "a.err1",
            "Build failed in module A",
            "src/a.rs",
            10,
        ),
        make_finding(
            Severity::Warn,
            "a.warn1",
            "Unused import in A",
            "src/a.rs",
            20,
        ),
        make_finding(
            Severity::Warn,
            "a.warn2",
            "Dead code in A",
            "src/a_util.rs",
            5,
        ),
        make_finding(
            Severity::Info,
            "a.info1",
            "Consider refactoring",
            "src/a.rs",
            30,
        ),
    ];
    let sensor_b_findings = vec![
        make_finding(
            Severity::Error,
            "b.err1",
            "Type mismatch in B",
            "src/b.rs",
            15,
        ),
        make_finding(
            Severity::Error,
            "b.err2",
            "Missing return in B",
            "src/b.rs",
            42,
        ),
        make_finding(
            Severity::Warn,
            "b.warn1",
            "Deprecated API in B",
            "src/b_api.rs",
            8,
        ),
        make_finding(
            Severity::Info,
            "b.info1",
            "Documentation missing",
            "src/b.rs",
            1,
        ),
    ];
    let sensor_c_findings = vec![
        make_finding(
            Severity::Warn,
            "c.warn1",
            "Potential null deref",
            "src/c.rs",
            100,
        ),
        make_finding(
            Severity::Warn,
            "c.warn2",
            "Unchecked error",
            "src/c.rs",
            200,
        ),
        make_finding(
            Severity::Warn,
            "c.warn3",
            "Magic number",
            "src/c_const.rs",
            7,
        ),
        make_finding(
            Severity::Info,
            "c.info1",
            "Complexity too high",
            "src/c.rs",
            50,
        ),
    ];
    let sensor_d_findings = vec![
        make_finding(
            Severity::Error,
            "d.err1",
            "Security vulnerability",
            "src/d.rs",
            3,
        ),
        make_finding(
            Severity::Warn,
            "d.warn1",
            "Weak hash algorithm",
            "src/d_crypto.rs",
            22,
        ),
        make_finding(
            Severity::Warn,
            "d.warn2",
            "Hardcoded config",
            "src/d.rs",
            88,
        ),
        make_finding(
            Severity::Info,
            "d.info1",
            "Consider encryption",
            "src/d.rs",
            90,
        ),
    ];
    let sensor_e_findings = vec![
        make_finding(
            Severity::Error,
            "e.err1",
            "Test failure in E",
            "tests/e_test.rs",
            11,
        ),
        make_finding(
            Severity::Warn,
            "e.warn1",
            "Flaky test detected",
            "tests/e_test.rs",
            55,
        ),
        make_finding(
            Severity::Warn,
            "e.warn2",
            "Slow test",
            "tests/e_perf.rs",
            33,
        ),
        make_finding(
            Severity::Info,
            "e.info1",
            "Coverage below threshold",
            "tests/e_test.rs",
            1,
        ),
    ];

    let sensors = vec![
        (
            "sensor-alpha",
            make_sensor_report(VerdictStatus::Fail, sensor_a_findings),
        ),
        (
            "sensor-bravo",
            make_sensor_report(VerdictStatus::Fail, sensor_b_findings),
        ),
        (
            "sensor-charlie",
            make_sensor_report(VerdictStatus::Warn, sensor_c_findings),
        ),
        (
            "sensor-delta",
            make_sensor_report(VerdictStatus::Fail, sensor_d_findings),
        ),
        (
            "sensor-echo",
            make_sensor_report(VerdictStatus::Fail, sensor_e_findings),
        ),
    ];

    let mut cfg = CockpitConfig::default();
    // Set max_highlights to 7 (default) to test budget boundary with 20 findings.
    for (id, _) in &sensors {
        cfg.sensors.insert(id.to_string(), blocking_sensor());
    }

    (sensors, cfg)
}

/// Run the in-memory pipeline and return (report_json, comment_md).
fn run_inmemory_pipeline(
    sensor_list: &[&str],
    reports: BTreeMap<String, ReportRead>,
    cfg: CockpitConfig,
) -> (String, String) {
    let receipts = MemReceiptSource::new(sensor_list.to_vec(), reports);
    let policy = MemPolicySource { config: Some(cfg) };
    let output = MemOutputSink::new();
    let uc = IngestUseCase::new(
        receipts,
        policy,
        output,
        NoOpSchemaValidator,
        render_comment,
    );
    let _result = uc
        .execute(default_request())
        .expect("pipeline should succeed");
    // The result's comment_md and report are also written to the MemOutputSink,
    // but we can access them from the result directly.
    let report_json = serde_json::to_string_pretty(&_result.report).unwrap();
    (_result.comment_md.clone(), report_json)
}

/// Build a BTreeMap of reports from the fixture data.
fn reports_from_fixture(sensors: &[(&str, Vec<u8>)]) -> BTreeMap<String, ReportRead> {
    sensors
        .iter()
        .map(|(id, bytes)| (id.to_string(), ReportRead::Bytes(bytes.clone())))
        .collect()
}

/// Build a BTreeMap of reports from the given sensor IDs and a shared fixture.
fn reports_from_fixture_with_order(
    order: &[&str],
    fixture: &[(&str, Vec<u8>)],
) -> BTreeMap<String, ReportRead> {
    let lookup: BTreeMap<&str, &Vec<u8>> = fixture.iter().map(|(id, b)| (*id, b)).collect();
    order
        .iter()
        .map(|id| (id.to_string(), ReportRead::Bytes(lookup[id].clone())))
        .collect()
}

// ---------------------------------------------------------------------------
// 1. Full pipeline N-times: run 100 times with same inputs → identical output
// ---------------------------------------------------------------------------

#[test]
fn full_pipeline_100_runs_identical_output() {
    let (sensors, cfg) = five_sensor_fixture();
    let sensor_ids: Vec<&str> = sensors.iter().map(|(id, _)| *id).collect();

    let (first_comment, first_report) =
        run_inmemory_pipeline(&sensor_ids, reports_from_fixture(&sensors), cfg.clone());

    for i in 1..100 {
        let (comment, report) =
            run_inmemory_pipeline(&sensor_ids, reports_from_fixture(&sensors), cfg.clone());
        assert_eq!(first_report, report, "report.json differs at iteration {i}");
        assert_eq!(
            first_comment, comment,
            "comment.md differs at iteration {i}"
        );
    }
}

// ---------------------------------------------------------------------------
// 2. Shuffled input order: different sensor orderings → same output
// ---------------------------------------------------------------------------

#[test]
fn shuffled_sensor_order_produces_identical_output() {
    let (sensors, cfg) = five_sensor_fixture();

    // Canonical ordering (lexical).
    let canonical: Vec<&str> = sensors.iter().map(|(id, _)| *id).collect();
    let (first_comment, first_report) =
        run_inmemory_pipeline(&canonical, reports_from_fixture(&sensors), cfg.clone());

    // Various permutations of sensor discovery order.
    let permutations: Vec<Vec<&str>> = vec![
        vec![
            "sensor-echo",
            "sensor-delta",
            "sensor-charlie",
            "sensor-bravo",
            "sensor-alpha",
        ],
        vec![
            "sensor-charlie",
            "sensor-alpha",
            "sensor-echo",
            "sensor-bravo",
            "sensor-delta",
        ],
        vec![
            "sensor-delta",
            "sensor-echo",
            "sensor-alpha",
            "sensor-charlie",
            "sensor-bravo",
        ],
        vec![
            "sensor-bravo",
            "sensor-charlie",
            "sensor-delta",
            "sensor-alpha",
            "sensor-echo",
        ],
        vec![
            "sensor-echo",
            "sensor-alpha",
            "sensor-bravo",
            "sensor-delta",
            "sensor-charlie",
        ],
    ];

    for (i, perm) in permutations.iter().enumerate() {
        let (comment, report) = run_inmemory_pipeline(
            perm,
            reports_from_fixture_with_order(perm, &sensors),
            cfg.clone(),
        );
        assert_eq!(
            first_report, report,
            "report.json differs with permutation {i}: {perm:?}"
        );
        assert_eq!(
            first_comment, comment,
            "comment.md differs with permutation {i}: {perm:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// 3. Report JSON determinism: serialize 50 times → identical bytes
// ---------------------------------------------------------------------------

#[test]
fn report_json_serialization_50_times_identical() {
    let (sensors, cfg) = five_sensor_fixture();
    let sensor_ids: Vec<&str> = sensors.iter().map(|(id, _)| *id).collect();
    let reports = reports_from_fixture(&sensors);

    let receipts = MemReceiptSource::new(sensor_ids, reports);
    let policy = MemPolicySource { config: Some(cfg) };
    let output = MemOutputSink::new();
    let uc = IngestUseCase::new(
        receipts,
        policy,
        output,
        NoOpSchemaValidator,
        render_comment,
    );
    let result = uc.execute(default_request()).unwrap();

    let first_json = serde_json::to_string_pretty(&result.report).unwrap();
    for i in 1..50 {
        let json = serde_json::to_string_pretty(&result.report).unwrap();
        assert_eq!(
            first_json, json,
            "JSON serialization differs at iteration {i}"
        );
    }
}

// ---------------------------------------------------------------------------
// 4. Comment markdown determinism: render 50 times → identical bytes
// ---------------------------------------------------------------------------

#[test]
fn comment_markdown_50_times_identical() {
    let (sensors, cfg) = five_sensor_fixture();
    let sensor_ids: Vec<&str> = sensors.iter().map(|(id, _)| *id).collect();
    let reports = reports_from_fixture(&sensors);

    let receipts = MemReceiptSource::new(sensor_ids, reports);
    let policy = MemPolicySource {
        config: Some(cfg.clone()),
    };
    let output = MemOutputSink::new();
    let uc = IngestUseCase::new(
        receipts,
        policy,
        output,
        NoOpSchemaValidator,
        render_comment,
    );
    let result = uc.execute(default_request()).unwrap();

    let first_md = render_comment(&result.report, &cfg);
    for i in 1..50 {
        let md = render_comment(&result.report, &cfg);
        assert_eq!(first_md, md, "comment.md differs at iteration {i}");
    }
}

// ---------------------------------------------------------------------------
// 5. SARIF determinism: convert 50 times → identical output
// ---------------------------------------------------------------------------

#[test]
fn sarif_conversion_50_times_identical() {
    let (sensors, cfg) = five_sensor_fixture();
    let sensor_ids: Vec<&str> = sensors.iter().map(|(id, _)| *id).collect();
    let reports = reports_from_fixture(&sensors);

    let receipts = MemReceiptSource::new(sensor_ids, reports);
    let policy = MemPolicySource { config: Some(cfg) };
    let output = MemOutputSink::new();
    let uc = IngestUseCase::new(
        receipts,
        policy,
        output,
        NoOpSchemaValidator,
        render_comment,
    );
    let result = uc.execute(default_request()).unwrap();

    let first_sarif = cockpit_report_to_sarif_json(&result.report).unwrap();
    for i in 1..50 {
        let sarif = cockpit_report_to_sarif_json(&result.report).unwrap();
        assert_eq!(first_sarif, sarif, "SARIF output differs at iteration {i}");
    }
}

// ---------------------------------------------------------------------------
// 6. Annotation determinism: generate 50 times → identical output
// ---------------------------------------------------------------------------

#[test]
fn annotation_rendering_50_times_identical() {
    let (sensors, cfg) = five_sensor_fixture();
    let sensor_ids: Vec<&str> = sensors.iter().map(|(id, _)| *id).collect();
    let reports = reports_from_fixture(&sensors);

    let receipts = MemReceiptSource::new(sensor_ids, reports);
    let policy = MemPolicySource {
        config: Some(cfg.clone()),
    };
    let output = MemOutputSink::new();
    let uc = IngestUseCase::new(
        receipts,
        policy,
        output,
        NoOpSchemaValidator,
        render_comment,
    );
    let result = uc.execute(default_request()).unwrap();

    let sensor_blocking: BTreeMap<String, bool> = cfg
        .sensors
        .iter()
        .map(|(id, sp)| (id.clone(), sp.blocking))
        .collect();

    let first = render_github_annotations(&result.report.highlights, &cfg, &sensor_blocking);
    for i in 1..50 {
        let ann = render_github_annotations(&result.report.highlights, &cfg, &sensor_blocking);
        assert_eq!(
            first.lines, ann.lines,
            "annotation lines differ at iteration {i}"
        );
        assert_eq!(
            first.truncated, ann.truncated,
            "annotation truncated differs at iteration {i}"
        );
        assert_eq!(
            first.rendered_count, ann.rendered_count,
            "annotation count differs at iteration {i}"
        );
    }
}

// ---------------------------------------------------------------------------
// 7. Fingerprint determinism: same finding → same fingerprint always
// ---------------------------------------------------------------------------

#[test]
fn fingerprint_determinism_100_iterations() {
    let finding = make_finding(
        Severity::Error,
        "sec.vuln",
        "SQL injection detected",
        "src/db.rs",
        42,
    );

    let first_fp = derive_fingerprint("security-scanner", &finding);
    assert_eq!(
        first_fp.len(),
        64,
        "fingerprint should be 64 hex chars (SHA-256)"
    );

    for i in 1..100 {
        let fp = derive_fingerprint("security-scanner", &finding);
        assert_eq!(first_fp, fp, "fingerprint differs at iteration {i}");
    }

    // Different sensor_id → different fingerprint.
    let other_fp = derive_fingerprint("other-scanner", &finding);
    assert_ne!(
        first_fp, other_fp,
        "different sensor should yield different fingerprint"
    );
}

// ---------------------------------------------------------------------------
// 8. Sort determinism with ties: 100 findings with same severity → stable key
// ---------------------------------------------------------------------------

#[test]
fn sort_determinism_with_ties_100_findings() {
    // Build 100 findings all with Severity::Warn spread across 10 files, 10 codes.
    let mut findings: Vec<Finding> = Vec::new();
    for file_idx in 0..10 {
        for code_idx in 0..10 {
            findings.push(make_finding(
                Severity::Warn,
                &format!("lint.{code_idx:03}"),
                &format!("Warning number {code_idx} in file {file_idx}"),
                &format!("src/mod_{file_idx:02}.rs"),
                (code_idx * 10 + file_idx) as u32,
            ));
        }
    }

    // Sort multiple times and verify identical ordering.
    let mut first_sorted = findings.clone();
    sort_findings("tied-sensor", &mut first_sorted);

    let first_keys: Vec<_> = first_sorted
        .iter()
        .map(|f| finding_sort_key("tied-sensor", f))
        .collect();

    for i in 1..50 {
        let mut sorted = findings.clone();
        sort_findings("tied-sensor", &mut sorted);
        let keys: Vec<_> = sorted
            .iter()
            .map(|f| finding_sort_key("tied-sensor", f))
            .collect();
        assert_eq!(first_keys, keys, "sort keys differ at iteration {i}");
    }

    // Verify ordering: sorted by severity_rank → sensor_id → path → line → code → message.
    for window in first_keys.windows(2) {
        assert!(
            window[0] <= window[1],
            "sort order violated: {:?} > {:?}",
            window[0],
            window[1]
        );
    }
}

// ---------------------------------------------------------------------------
// 9. Config parsing determinism: parse same config 50 times → identical result
// ---------------------------------------------------------------------------

#[test]
fn config_parsing_50_times_identical() {
    let config_toml = r#"
[policy]
warn_is_fail = false
max_highlights = 10
max_per_sensor_findings = 15
schema_validation = "strict"

[sensors.alpha]
blocking = true
missing = "fail"
section = "Tests"

[sensors.bravo]
blocking = true
missing = "warn"

[sensors.charlie]
blocking = false
missing = "skip"
section = "Diagnostics"

[sensors.delta]
blocking = true
missing = "fail"

[sensors.echo]
blocking = false
missing = "warn"
"#;

    let first: CockpitConfig = toml::from_str(config_toml).unwrap();
    for i in 1..50 {
        let parsed: CockpitConfig = toml::from_str(config_toml).unwrap();
        assert_eq!(first, parsed, "config parse differs at iteration {i}");
    }

    // Re-serialize and compare.
    let first_json = serde_json::to_string_pretty(&first).unwrap();
    for i in 1..50 {
        let parsed: CockpitConfig = toml::from_str(config_toml).unwrap();
        let json = serde_json::to_string_pretty(&parsed).unwrap();
        assert_eq!(
            first_json, json,
            "config JSON serialization differs at iteration {i}"
        );
    }
}

// ---------------------------------------------------------------------------
// 10. Multi-sensor discovery order: randomized list → lexical output
// ---------------------------------------------------------------------------

#[test]
fn multi_sensor_discovery_produces_lexical_output() {
    // Provide sensors in various non-lexical orders; the pipeline should
    // always produce sensors sorted lexically in the report.
    let findings = vec![make_finding(
        Severity::Warn,
        "x.warn",
        "warning",
        "src/x.rs",
        1,
    )];

    let sensor_ids = ["zebra", "alpha", "mango", "bravo", "yankee"];
    let bytes: Vec<(&str, Vec<u8>)> = sensor_ids
        .iter()
        .map(|id| {
            (
                *id,
                make_sensor_report(VerdictStatus::Warn, findings.clone()),
            )
        })
        .collect();

    let mut cfg = CockpitConfig::default();
    for id in &sensor_ids {
        cfg.sensors.insert(id.to_string(), blocking_sensor());
    }

    // Run with multiple orderings.
    let orderings: Vec<Vec<&str>> = vec![
        vec!["zebra", "alpha", "mango", "bravo", "yankee"],
        vec!["yankee", "bravo", "mango", "alpha", "zebra"],
        vec!["mango", "zebra", "bravo", "yankee", "alpha"],
    ];

    let mut all_results = Vec::new();
    for order in &orderings {
        let (comment, report) = run_inmemory_pipeline(
            order,
            reports_from_fixture_with_order(order, &bytes),
            cfg.clone(),
        );
        all_results.push((comment, report));
    }

    // All must be identical.
    for (i, (comment, report)) in all_results.iter().enumerate().skip(1) {
        assert_eq!(
            all_results[0].1, *report,
            "report.json differs with ordering {i}"
        );
        assert_eq!(
            all_results[0].0, *comment,
            "comment.md differs with ordering {i}"
        );
    }

    // Verify sensors appear in lexical order in the report JSON.
    let report_val: serde_json::Value = serde_json::from_str(&all_results[0].1).unwrap();
    let sensor_order: Vec<&str> = report_val["sensors"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["id"].as_str().unwrap())
        .collect();
    let mut expected = sensor_order.clone();
    expected.sort();
    assert_eq!(
        sensor_order, expected,
        "sensors should be in lexical order in report"
    );
}

// ---------------------------------------------------------------------------
// 11. Hash stability: SHA-256 hashes are consistent across runs
// ---------------------------------------------------------------------------

#[test]
fn policy_snapshot_hash_stability() {
    let (_, cfg) = five_sensor_fixture();
    let snapshot = snapshot_policy(&cfg);

    let first_hash = policy_snapshot_sha256_hex(&snapshot).unwrap();
    assert_eq!(first_hash.len(), 64, "hash should be 64 hex chars");

    for i in 1..100 {
        let hash = policy_snapshot_sha256_hex(&snapshot).unwrap();
        assert_eq!(first_hash, hash, "policy hash differs at iteration {i}");
    }

    // Modifying the config should change the hash.
    let mut cfg2 = cfg.clone();
    cfg2.policy.warn_is_fail = true;
    let snapshot2 = snapshot_policy(&cfg2);
    let hash2 = policy_snapshot_sha256_hex(&snapshot2).unwrap();
    assert_ne!(
        first_hash, hash2,
        "different config should produce different hash"
    );
}

// ---------------------------------------------------------------------------
// 12. Timestamp handling: reports with same timestamp → identical output
// ---------------------------------------------------------------------------

#[test]
fn same_timestamp_produces_identical_output() {
    // Run the full pipeline 50 times. The timestamp is baked into the fixture,
    // so all runs use the same "2026-06-01T00:00:00Z". Output must be identical.
    let (sensors, cfg) = five_sensor_fixture();
    let sensor_ids: Vec<&str> = sensors.iter().map(|(id, _)| *id).collect();

    let (first_comment, first_report) =
        run_inmemory_pipeline(&sensor_ids, reports_from_fixture(&sensors), cfg.clone());

    for i in 1..50 {
        let (comment, report) =
            run_inmemory_pipeline(&sensor_ids, reports_from_fixture(&sensors), cfg.clone());
        assert_eq!(
            first_report, report,
            "report differs at iteration {i} (timestamp)"
        );
        assert_eq!(
            first_comment, comment,
            "comment differs at iteration {i} (timestamp)"
        );
    }
}
