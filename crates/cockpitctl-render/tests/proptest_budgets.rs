//! Property-based invariant tests for render budget enforcement.
//!
//! Validates that budget caps, marker stability, determinism, ordering,
//! and severity mapping hold for arbitrary inputs.

use cockpitctl_render::{
    append_comment_sections, render_annotations, render_comment, render_github_annotations,
};
use cockpitctl_types::{
    CockpitConfig, CockpitReport, Finding, Highlight, Location, MissingPolicy, Policy,
    PolicySnapshot, Presence, RunInfo, SensorPolicy, SensorSummary, Severity, ToolInfo, Verdict,
    VerdictCounts, VerdictStatus, severity_rank,
};
use proptest::prelude::*;
use std::collections::BTreeMap;

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
    prop::option::of(
        (
            prop::option::of("[a-z/_.-]{1,50}"),
            prop::option::of(1u32..10000u32),
            prop::option::of(1u32..1000u32),
        )
            .prop_map(|(path, line, col)| Location { path, line, col }),
    )
}

fn any_finding() -> impl Strategy<Value = Finding> {
    (
        any_severity(),
        prop::option::of("[A-Z][A-Z0-9_]{0,20}"),
        "[A-Z][A-Z0-9_./-]{0,30}",
        ".{1,100}",
        any_location(),
        prop::option::of(".{0,50}"),
        prop::option::of("https?://[a-z.]+/[a-z/]*"),
        prop::option::of("[a-f0-9]{64}"),
    )
        .prop_map(
            |(severity, check_id, code, message, location, help, url, fingerprint)| Finding {
                severity,
                check_id,
                code,
                message,
                location,
                help,
                url,
                fingerprint,
                data: None,
            },
        )
}

fn any_highlight() -> impl Strategy<Value = Highlight> {
    ("[a-z_][a-z0-9_-]{0,20}", any_finding())
        .prop_map(|(sensor_id, finding)| Highlight { sensor_id, finding })
}

fn any_highlights(max_len: usize) -> impl Strategy<Value = Vec<Highlight>> {
    prop::collection::vec(any_highlight(), 0..=max_len)
}

fn any_verdict() -> impl Strategy<Value = Verdict> {
    (
        any_verdict_status(),
        (0u64..100, 0u64..100, 0u64..100, 0u64..10).prop_map(|(i, w, e, s)| VerdictCounts {
            info: i,
            warn: w,
            error: e,
            suppressed: s,
        }),
        prop::collection::vec(".{0,30}", 0..3),
    )
        .prop_map(|(status, counts, reasons)| Verdict {
            status,
            counts,
            reasons,
        })
}

fn any_sensor_summary() -> impl Strategy<Value = SensorSummary> {
    (
        "[a-z_][a-z0-9_-]{0,20}",
        any::<bool>(),
        prop_oneof![
            Just(MissingPolicy::Skip),
            Just(MissingPolicy::Warn),
            Just(MissingPolicy::Fail),
        ],
        prop_oneof![
            Just(Presence::Present),
            Just(Presence::Missing),
            Just(Presence::Invalid),
        ],
        any_verdict(),
    )
        .prop_map(|(id, blocking, missing, presence, verdict)| SensorSummary {
            id: id.clone(),
            blocking,
            missing,
            presence,
            report_path: format!("artifacts/{}/report.json", id),
            comment_path: None,
            verdict,
            truncated: false,
            errors: vec![],
            missing_policy_applied: None,
            policy_outcome: None,
        })
}

fn any_cockpit_config() -> impl Strategy<Value = CockpitConfig> {
    (
        any::<bool>(),
        1usize..15,
        1usize..30,
        1usize..30,
        prop::collection::vec("[A-Z][a-z]{0,15}", 0..5),
        prop::collection::btree_map(
            "[a-z_][a-z0-9_-]{0,15}",
            (
                any::<bool>(),
                prop_oneof![
                    Just(MissingPolicy::Skip),
                    Just(MissingPolicy::Warn),
                    Just(MissingPolicy::Fail),
                ],
            )
                .prop_map(|(blocking, missing)| SensorPolicy {
                    blocking,
                    missing,
                    section: None,
                    require_label: None,
                    repro: None,
                }),
            0..5,
        ),
    )
        .prop_map(
            |(
                warn_is_fail,
                max_highlights,
                max_per_sensor_findings,
                max_annotations,
                section_order,
                sensors,
            )| {
                CockpitConfig {
                    policy: Policy {
                        warn_is_fail,
                        max_highlights,
                        max_per_sensor_findings,
                        max_annotations,
                        section_order,
                        schema_validation: Default::default(),
                        max_receipt_size_bytes: 2 * 1024 * 1024,
                    },
                    buildfix: Default::default(),
                    policy_signing: Default::default(),
                    sensors,
                    hooks: vec![],
                }
            },
        )
}

fn any_cockpit_report(max_sensors: usize) -> impl Strategy<Value = CockpitReport> {
    (
        any_verdict(),
        prop::collection::vec(any_sensor_summary(), 0..=max_sensors),
        any_highlights(15),
    )
        .prop_map(|(verdict, sensors, highlights)| CockpitReport {
            schema: "cockpit.report.v1".to_string(),
            tool: ToolInfo {
                name: "cockpitctl".to_string(),
                version: "0.0.0-test".to_string(),
                commit: None,
            },
            run: RunInfo {
                started_at: "2024-01-01T00:00:00Z".to_string(),
                ended_at: None,
                duration_ms: None,
                host: None,
                git: None,
                ci: None,
                capabilities: BTreeMap::new(),
            },
            verdict,
            sensors,
            highlights,
            policy: PolicySnapshot {
                warn_is_fail: false,
                max_highlights: 7,
                max_per_sensor_findings: 20,
                max_annotations: 25,
                section_order: vec![],
                sensors: vec![],
            },
            data: None,
        })
}

fn empty_report() -> CockpitReport {
    CockpitReport {
        schema: "cockpit.report.v1".to_string(),
        tool: ToolInfo {
            name: "cockpitctl".to_string(),
            version: "0.0.0-test".to_string(),
            commit: None,
        },
        run: RunInfo {
            started_at: "2024-01-01T00:00:00Z".to_string(),
            ended_at: None,
            duration_ms: None,
            host: None,
            git: None,
            ci: None,
            capabilities: BTreeMap::new(),
        },
        verdict: Verdict {
            status: VerdictStatus::Pass,
            counts: VerdictCounts::default(),
            reasons: vec![],
        },
        sensors: vec![],
        highlights: vec![],
        policy: PolicySnapshot {
            warn_is_fail: false,
            max_highlights: 7,
            max_per_sensor_findings: 20,
            max_annotations: 25,
            section_order: vec![],
            sensors: vec![],
        },
        data: None,
    }
}

// ============================================================================
// 1. Comment length respects annotation budget
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// Annotation rendered_count never exceeds max_annotations for any input.
    #[test]
    fn comment_annotation_count_respects_budget(
        report in any_cockpit_report(10),
        cfg in any_cockpit_config(),
    ) {
        let blocking: BTreeMap<String, bool> = report
            .sensors.iter().map(|s| (s.id.clone(), s.blocking)).collect();
        let result = render_annotations(&report.highlights, &cfg, &blocking);
        prop_assert!(
            result.rendered_count <= cfg.policy.max_annotations,
            "rendered {} exceeds budget {}",
            result.rendered_count,
            cfg.policy.max_annotations,
        );
        // Total count reflects actual input size.
        prop_assert_eq!(result.total_count, report.highlights.len());
    }
}

// ============================================================================
// 2. Markers always present regardless of budget or input
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// Begin/end markers are present for any report and any config,
    /// including extreme budget values.
    #[test]
    fn markers_always_present(
        report in any_cockpit_report(10),
        cfg in any_cockpit_config(),
    ) {
        let md = render_comment(&report, &cfg);
        prop_assert!(md.contains("<!-- cockpit:begin -->"), "missing begin marker");
        prop_assert!(md.contains("<!-- cockpit:end -->"), "missing end marker");
        // Begin must appear before end.
        let begin_pos = md.find("<!-- cockpit:begin -->").unwrap();
        let end_pos = md.find("<!-- cockpit:end -->").unwrap();
        prop_assert!(begin_pos < end_pos, "begin marker must precede end marker");
    }
}

// ============================================================================
// 3. Render is deterministic — same report → same comment
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// Rendering the same report + config twice yields byte-identical output.
    #[test]
    fn render_is_deterministic(
        report in any_cockpit_report(8),
        cfg in any_cockpit_config(),
    ) {
        let a = render_comment(&report, &cfg);
        let b = render_comment(&report, &cfg);
        prop_assert_eq!(&a, &b, "render_comment must be deterministic");

        let blocking: BTreeMap<String, bool> = report
            .sensors.iter().map(|s| (s.id.clone(), s.blocking)).collect();
        let ann_a = render_annotations(&report.highlights, &cfg, &blocking);
        let ann_b = render_annotations(&report.highlights, &cfg, &blocking);
        prop_assert_eq!(&ann_a.content, &ann_b.content, "render_annotations must be deterministic");
        prop_assert_eq!(ann_a.rendered_count, ann_b.rendered_count);

        let gh_a = render_github_annotations(&report.highlights, &cfg, &blocking);
        let gh_b = render_github_annotations(&report.highlights, &cfg, &blocking);
        prop_assert_eq!(&gh_a.lines, &gh_b.lines, "render_github_annotations must be deterministic");
    }
}

// ============================================================================
// 4. Annotations count matches findings count (up to budget)
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    /// GitHub annotation line count equals min(highlights, max_annotations).
    #[test]
    fn gh_annotation_count_matches_budget(
        highlights in any_highlights(30),
        cfg in any_cockpit_config(),
    ) {
        let blocking = BTreeMap::new();
        let result = render_github_annotations(&highlights, &cfg, &blocking);
        let expected = highlights.len().min(cfg.policy.max_annotations);
        prop_assert_eq!(
            result.rendered_count, expected,
            "rendered_count must be min(highlights.len(), max_annotations)"
        );
        prop_assert_eq!(
            result.lines.len(), result.rendered_count,
            "lines.len() must equal rendered_count"
        );
    }
}

// ============================================================================
// 5. Finding truncation preserves severity ordering
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// When annotations are truncated, the kept items follow deterministic
    /// severity-descending order: errors before warnings before info.
    #[test]
    fn truncation_preserves_severity_order(
        highlights in prop::collection::vec(any_highlight(), 2..40),
        max_annotations in 1usize..10,
    ) {
        prop_assume!(highlights.len() > max_annotations);

        let mut cfg = CockpitConfig::default();
        cfg.policy.max_annotations = max_annotations;

        let blocking = BTreeMap::new();
        let result = render_github_annotations(&highlights, &cfg, &blocking);

        // Verify rendered lines follow severity order (error < warn < info in rank).
        let mut prev_rank = 0u8;
        for line in &result.lines {
            let rank = if line.starts_with("::error") {
                severity_rank(&Severity::Error)
            } else if line.starts_with("::warning") {
                severity_rank(&Severity::Warn)
            } else if line.starts_with("::notice") {
                severity_rank(&Severity::Info)
            } else {
                panic!("unexpected annotation line prefix: {}", line);
            };
            // severity_rank: Error=0, Warn=1, Info=2 — must be non-decreasing.
            prop_assert!(
                rank >= prev_rank,
                "severity order violated: rank {} after {}",
                rank,
                prev_rank,
            );
            prev_rank = rank;
        }
    }
}

// ============================================================================
// 6. Empty report renders to valid comment with markers
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// An empty report (no sensors, no highlights) still produces a
    /// non-empty comment with required markers and sections.
    #[test]
    fn empty_report_renders_valid_comment(cfg in any_cockpit_config()) {
        let report = empty_report();
        let md = render_comment(&report, &cfg);

        prop_assert!(!md.is_empty(), "comment must not be empty");
        prop_assert!(md.contains("<!-- cockpit:begin -->"), "missing begin marker");
        prop_assert!(md.contains("<!-- cockpit:end -->"), "missing end marker");
        prop_assert!(md.contains("## Cockpit"), "missing heading");
        prop_assert!(md.contains("### Summary"), "missing Summary section");
        prop_assert!(md.contains("_No highlights._"), "should say no highlights");
        prop_assert!(md.contains("_No annotations._"), "should say no annotations");
    }
}

// ============================================================================
// 7. Appended comment sections preserve markers
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// Appending arbitrary sections always preserves begin/end markers
    /// and inserts sections before the end marker.
    #[test]
    fn appended_sections_preserve_markers(
        report in any_cockpit_report(5),
        cfg in any_cockpit_config(),
        section_names in prop::collection::vec("[A-Z][a-z]{1,15}", 1..5),
        section_bodies in prop::collection::vec(".{1,100}", 1..5),
    ) {
        let base = render_comment(&report, &cfg);
        let sections: Vec<(String, String)> = section_names.into_iter()
            .zip(section_bodies)
            .collect();

        let result = append_comment_sections(&base, &sections);
        prop_assert!(result.contains("<!-- cockpit:begin -->"), "missing begin marker");
        prop_assert!(result.contains("<!-- cockpit:end -->"), "missing end marker");

        // All appended section headers must appear before the end marker.
        let end_pos = result.rfind("<!-- cockpit:end -->").unwrap();
        for (name, _) in &sections {
            let header = format!("### {}", name.trim());
            let header_pos = result.find(&header);
            prop_assert!(header_pos.is_some(), "section '{}' not found", name);
            prop_assert!(
                header_pos.unwrap() < end_pos,
                "section '{}' must appear before end marker",
                name,
            );
        }
    }
}

// ============================================================================
// 8. Budget of 0 produces minimal annotations — no annotation content
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// With max_annotations=0, annotation output contains no numbered items
    /// and GitHub annotations produce zero lines.
    #[test]
    fn zero_annotation_budget_produces_minimal_output(
        highlights in any_highlights(20),
    ) {
        let mut cfg = CockpitConfig::default();
        cfg.policy.max_annotations = 0;

        let blocking = BTreeMap::new();
        let md_result = render_annotations(&highlights, &cfg, &blocking);
        prop_assert_eq!(md_result.rendered_count, 0, "rendered_count must be 0");

        let gh_result = render_github_annotations(&highlights, &cfg, &blocking);
        prop_assert!(gh_result.lines.is_empty(), "GH lines must be empty");
        prop_assert_eq!(gh_result.rendered_count, 0);
    }
}

// ============================================================================
// 9. Large input doesn't panic — 10000 findings
// ============================================================================

#[test]
fn large_input_does_not_panic() {
    let highlights: Vec<Highlight> = (0..10_000)
        .map(|i| Highlight {
            sensor_id: format!("sensor_{}", i % 50),
            finding: Finding {
                severity: match i % 3 {
                    0 => Severity::Error,
                    1 => Severity::Warn,
                    _ => Severity::Info,
                },
                check_id: Some(format!("CHK{:04}", i)),
                code: format!("CODE{:04}", i),
                message: format!(
                    "Finding number {} with some extra text to make it realistic",
                    i
                ),
                location: Some(Location {
                    path: Some(format!("src/module_{}/file_{}.rs", i % 20, i % 100)),
                    line: Some((i % 5000) as u32 + 1),
                    col: None,
                }),
                help: None,
                url: None,
                fingerprint: None,
                data: None,
            },
        })
        .collect();

    let mut cfg = CockpitConfig::default();
    cfg.policy.max_annotations = 50;

    let blocking = BTreeMap::new();

    // Must not panic.
    let md_result = render_annotations(&highlights, &cfg, &blocking);
    assert_eq!(md_result.total_count, 10_000);
    assert_eq!(md_result.rendered_count, 50);
    assert!(md_result.truncated);

    let gh_result = render_github_annotations(&highlights, &cfg, &blocking);
    assert_eq!(gh_result.lines.len(), 50);
    assert!(gh_result.truncated);

    // Also test full comment rendering with a large report.
    let report = CockpitReport {
        schema: "cockpit.report.v1".to_string(),
        tool: ToolInfo {
            name: "cockpitctl".to_string(),
            version: "0.0.0-test".to_string(),
            commit: None,
        },
        run: RunInfo {
            started_at: "2024-01-01T00:00:00Z".to_string(),
            ended_at: None,
            duration_ms: None,
            host: None,
            git: None,
            ci: None,
            capabilities: BTreeMap::new(),
        },
        verdict: Verdict {
            status: VerdictStatus::Fail,
            counts: VerdictCounts {
                info: 3334,
                warn: 3333,
                error: 3333,
                suppressed: 0,
            },
            reasons: vec!["too many findings".to_string()],
        },
        sensors: vec![],
        highlights,
        policy: PolicySnapshot {
            warn_is_fail: false,
            max_highlights: 7,
            max_per_sensor_findings: 20,
            max_annotations: 50,
            section_order: vec![],
            sensors: vec![],
        },
        data: None,
    };
    let md = render_comment(&report, &cfg);
    assert!(md.contains("<!-- cockpit:begin -->"));
    assert!(md.contains("<!-- cockpit:end -->"));
}

// ============================================================================
// 10. Annotation severity mapping is consistent
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    /// Each severity always maps to the same GitHub annotation level:
    /// Error → ::error, Warn → ::warning, Info → ::notice.
    #[test]
    fn severity_mapping_is_consistent(severity in any_severity()) {
        let highlight = Highlight {
            sensor_id: "test_sensor".to_string(),
            finding: Finding {
                severity: severity.clone(),
                check_id: None,
                code: "TEST001".to_string(),
                message: "test message".to_string(),
                location: Some(Location {
                    path: Some("src/lib.rs".to_string()),
                    line: Some(1),
                    col: None,
                }),
                help: None,
                url: None,
                fingerprint: None,
                data: None,
            },
        };

        let mut cfg = CockpitConfig::default();
        cfg.policy.max_annotations = 10;

        let blocking = BTreeMap::new();
        let result = render_github_annotations(&[highlight], &cfg, &blocking);
        prop_assert_eq!(result.lines.len(), 1);

        let line = &result.lines[0];
        match severity {
            Severity::Error => prop_assert!(
                line.starts_with("::error "),
                "Error must map to ::error, got: {}",
                line,
            ),
            Severity::Warn => prop_assert!(
                line.starts_with("::warning "),
                "Warn must map to ::warning, got: {}",
                line,
            ),
            Severity::Info => prop_assert!(
                line.starts_with("::notice "),
                "Info must map to ::notice, got: {}",
                line,
            ),
        }
    }
}
