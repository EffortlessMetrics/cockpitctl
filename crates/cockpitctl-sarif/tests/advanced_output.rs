//! Advanced SARIF output tests: edge cases and SARIF 2.1.0 compliance.
//!
//! Covers: empty reports, single/multi sensor runs, full/partial locations,
//! severity mapping, schema/version fields, tool naming, message/rule mapping,
//! determinism, large reports, special characters, unicode paths, and empty codes.

use std::collections::BTreeMap;

use cockpitctl_sarif::{cockpit_report_to_sarif, cockpit_report_to_sarif_json};
use cockpitctl_types::*;
use pretty_assertions::assert_eq;

// ── Helpers ─────────────────────────────────────────────────────────────

fn base_report() -> CockpitReport {
    CockpitReport {
        schema: "cockpit.report.v1".to_string(),
        tool: ToolInfo {
            name: "cockpitctl".to_string(),
            version: "0.3.0".to_string(),
            commit: None,
        },
        run: RunInfo {
            started_at: "2026-02-01T00:00:00Z".to_string(),
            ended_at: None,
            duration_ms: None,
            host: None,
            git: None,
            ci: None,
            capabilities: BTreeMap::new(),
        },
        verdict: Verdict {
            status: VerdictStatus::Pass,
            counts: VerdictCounts {
                info: 0,
                warn: 0,
                error: 0,
                suppressed: 0,
            },
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

#[allow(clippy::too_many_arguments)]
fn make_highlight(
    sensor_id: &str,
    severity: Severity,
    code: &str,
    message: &str,
    path: Option<&str>,
    line: Option<u32>,
    col: Option<u32>,
    fingerprint: Option<&str>,
) -> Highlight {
    Highlight {
        sensor_id: sensor_id.to_string(),
        finding: Finding {
            severity,
            check_id: None,
            code: code.to_string(),
            message: message.to_string(),
            location: path.map(|p| Location {
                path: Some(p.to_string()),
                line,
                col,
            }),
            help: None,
            url: None,
            fingerprint: fingerprint.map(|f| f.to_string()),
            data: None,
        },
    }
}

// ── 1) Empty report → valid SARIF structure ─────────────────────────────

#[test]
fn adv_empty_report_valid_sarif_structure() {
    let report = base_report();
    let sarif = cockpit_report_to_sarif(&report);

    assert_eq!(sarif.version, "2.1.0");
    assert_eq!(sarif.runs.len(), 1, "even empty reports get one run");
    assert!(sarif.runs[0].results.is_empty());
    assert!(sarif.runs[0].tool.driver.rules.is_empty());

    // JSON must also be valid
    let json = cockpit_report_to_sarif_json(&report).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert!(parsed.is_object());
    assert_eq!(parsed["runs"].as_array().unwrap().len(), 1);
}

// ── 2) Single finding → one result in one run ───────────────────────────

#[test]
fn adv_single_finding_one_result_one_run() {
    let report = CockpitReport {
        highlights: vec![make_highlight(
            "clippy",
            Severity::Error,
            "clippy::unwrap_used",
            "used `unwrap()`",
            Some("src/main.rs"),
            Some(42),
            Some(10),
            None,
        )],
        ..base_report()
    };
    let sarif = cockpit_report_to_sarif(&report);

    assert_eq!(sarif.runs.len(), 1);
    assert_eq!(sarif.runs[0].results.len(), 1);
    assert_eq!(sarif.runs[0].tool.driver.rules.len(), 1);
    assert_eq!(sarif.runs[0].results[0].rule_id, "clippy::unwrap_used");
    assert_eq!(sarif.runs[0].results[0].level, "error");
}

// ── 3) Multiple sensors → all represented in results ────────────────────

#[test]
fn adv_multiple_sensors_all_results_present() {
    let report = CockpitReport {
        highlights: vec![
            make_highlight(
                "clippy",
                Severity::Error,
                "E001",
                "e1",
                Some("a.rs"),
                Some(1),
                None,
                None,
            ),
            make_highlight(
                "builddiag",
                Severity::Warn,
                "W001",
                "w1",
                Some("b.rs"),
                Some(2),
                None,
                None,
            ),
            make_highlight(
                "secaudit",
                Severity::Info,
                "I001",
                "i1",
                Some("c.rs"),
                Some(3),
                None,
                None,
            ),
        ],
        ..base_report()
    };
    let sarif = cockpit_report_to_sarif(&report);

    assert_eq!(sarif.runs[0].results.len(), 3);
    assert_eq!(sarif.runs[0].tool.driver.rules.len(), 3);

    let rule_ids: Vec<&str> = sarif.runs[0]
        .results
        .iter()
        .map(|r| r.rule_id.as_str())
        .collect();
    assert!(rule_ids.contains(&"E001"));
    assert!(rule_ids.contains(&"W001"));
    assert!(rule_ids.contains(&"I001"));
}

// ── 4) Finding with all fields → full result with location ──────────────

#[test]
fn adv_finding_all_fields_full_result() {
    let h = Highlight {
        sensor_id: "full-sensor".to_string(),
        finding: Finding {
            severity: Severity::Error,
            check_id: Some("CHK-99".to_string()),
            code: "FULL-001".to_string(),
            message: "fully populated finding".to_string(),
            location: Some(Location {
                path: Some("src/deep/nested/file.rs".to_string()),
                line: Some(999),
                col: Some(42),
            }),
            help: Some("Consider refactoring".to_string()),
            url: Some("https://example.com/rule/FULL-001".to_string()),
            fingerprint: Some("fp-all-fields".to_string()),
            data: Some(serde_json::json!({"extra": true})),
        },
    };
    let report = CockpitReport {
        highlights: vec![h],
        ..base_report()
    };
    let sarif = cockpit_report_to_sarif(&report);
    let r = &sarif.runs[0].results[0];

    assert_eq!(r.rule_id, "FULL-001");
    assert_eq!(r.level, "error");
    assert_eq!(r.message.text, "fully populated finding");
    assert_eq!(r.locations.len(), 1);
    assert_eq!(
        r.locations[0].physical_location.artifact_location.uri,
        "src/deep/nested/file.rs"
    );
    let region = r.locations[0].physical_location.region.as_ref().unwrap();
    assert_eq!(region.start_line, Some(999));
    assert_eq!(region.start_column, Some(42));
    assert_eq!(
        r.fingerprints.get("cockpitctl/v1"),
        Some(&"fp-all-fields".to_string())
    );
}

// ── 5) Finding without path → result without physical location ──────────

#[test]
fn adv_finding_without_path_no_physical_location() {
    let report = CockpitReport {
        highlights: vec![make_highlight(
            "sensor",
            Severity::Warn,
            "NO-PATH",
            "no path available",
            None,
            None,
            None,
            None,
        )],
        ..base_report()
    };
    let sarif = cockpit_report_to_sarif(&report);

    assert!(sarif.runs[0].results[0].locations.is_empty());

    // Verify JSON omits the locations key via skip_serializing_if
    let json = cockpit_report_to_sarif_json(&report).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert!(parsed["runs"][0]["results"][0].get("locations").is_none());
}

// ── 6) Finding without line → artifactLocation but no region ────────────

#[test]
fn adv_finding_without_line_has_artifact_but_no_region() {
    let report = CockpitReport {
        highlights: vec![make_highlight(
            "sensor",
            Severity::Info,
            "NO-LINE",
            "file-level finding",
            Some("src/lib.rs"),
            None,
            None,
            None,
        )],
        ..base_report()
    };
    let sarif = cockpit_report_to_sarif(&report);
    let loc = &sarif.runs[0].results[0].locations[0];

    assert_eq!(loc.physical_location.artifact_location.uri, "src/lib.rs");
    assert!(
        loc.physical_location.region.is_none(),
        "no line/col means no region"
    );
}

// ── 7) Severity mapping → error/warning/note ────────────────────────────

#[test]
fn adv_severity_mapping_all_levels() {
    let cases = [
        (Severity::Error, "error"),
        (Severity::Warn, "warning"),
        (Severity::Info, "note"),
    ];
    for (severity, expected_level) in &cases {
        let report = CockpitReport {
            highlights: vec![make_highlight(
                "s",
                severity.clone(),
                "CODE",
                "msg",
                Some("f.rs"),
                Some(1),
                None,
                None,
            )],
            ..base_report()
        };
        let sarif = cockpit_report_to_sarif(&report);
        assert_eq!(
            sarif.runs[0].results[0].level, *expected_level,
            "Severity::{severity:?} should map to {expected_level}"
        );
    }
}

// ── 8) SARIF version field is "2.1.0" ───────────────────────────────────

#[test]
fn adv_sarif_version_is_2_1_0() {
    let sarif = cockpit_report_to_sarif(&base_report());
    assert_eq!(sarif.version, "2.1.0");

    let json = cockpit_report_to_sarif_json(&base_report()).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["version"].as_str().unwrap(), "2.1.0");
}

// ── 9) Schema reference is present and correct ──────────────────────────

#[test]
fn adv_schema_reference_correct() {
    let sarif = cockpit_report_to_sarif(&base_report());
    assert_eq!(
        sarif.schema,
        "https://raw.githubusercontent.com/oasis-tcs/sarif-spec/main/sarif-2.1/schema/sarif-schema-2.1.0.json"
    );

    let json = cockpit_report_to_sarif_json(&base_report()).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    let schema = parsed["$schema"].as_str().unwrap();
    assert!(schema.contains("sarif-schema-2.1.0"));
    assert!(schema.starts_with("https://"));
}

// ── 10) Tool name matches report tool name ──────────────────────────────

#[test]
fn adv_tool_name_matches_report_tool() {
    let report = CockpitReport {
        tool: ToolInfo {
            name: "my-custom-director".to_string(),
            version: "4.5.6".to_string(),
            commit: None,
        },
        ..base_report()
    };
    let sarif = cockpit_report_to_sarif(&report);

    assert_eq!(sarif.runs[0].tool.driver.name, "my-custom-director");
    assert_eq!(sarif.runs[0].tool.driver.version, "4.5.6");
}

// ── 11) Message text matches finding message ────────────────────────────

#[test]
fn adv_message_text_matches_finding_message() {
    let msg = "expected `bool`, got `String`";
    let report = CockpitReport {
        highlights: vec![make_highlight(
            "sensor",
            Severity::Error,
            "E0308",
            msg,
            Some("src/main.rs"),
            Some(10),
            None,
            None,
        )],
        ..base_report()
    };
    let sarif = cockpit_report_to_sarif(&report);

    assert_eq!(sarif.runs[0].results[0].message.text, msg);
}

// ── 12) Rule ID matches finding code ────────────────────────────────────

#[test]
fn adv_rule_id_matches_finding_code() {
    let report = CockpitReport {
        highlights: vec![make_highlight(
            "sensor",
            Severity::Warn,
            "clippy::needless_return",
            "unneeded return",
            None,
            None,
            None,
            None,
        )],
        ..base_report()
    };
    let sarif = cockpit_report_to_sarif(&report);

    assert_eq!(sarif.runs[0].results[0].rule_id, "clippy::needless_return");
    assert_eq!(
        sarif.runs[0].tool.driver.rules[0].id,
        "clippy::needless_return"
    );
}

// ── 13) Deterministic output ────────────────────────────────────────────

#[test]
fn adv_deterministic_output_same_input_same_json() {
    let report = CockpitReport {
        highlights: vec![
            make_highlight(
                "z-sensor",
                Severity::Info,
                "Z001",
                "z",
                Some("z.rs"),
                Some(3),
                None,
                None,
            ),
            make_highlight(
                "a-sensor",
                Severity::Error,
                "A001",
                "a",
                Some("a.rs"),
                Some(1),
                None,
                None,
            ),
            make_highlight(
                "m-sensor",
                Severity::Warn,
                "M001",
                "m",
                Some("m.rs"),
                Some(2),
                None,
                None,
            ),
        ],
        ..base_report()
    };

    let json1 = cockpit_report_to_sarif_json(&report).unwrap();
    let json2 = cockpit_report_to_sarif_json(&report).unwrap();
    let json3 = cockpit_report_to_sarif_json(&report).unwrap();

    assert_eq!(
        json1, json2,
        "first and second call must produce identical JSON"
    );
    assert_eq!(
        json2, json3,
        "second and third call must produce identical JSON"
    );
}

// ── 14) Large report (100 findings) → all present ───────────────────────

#[test]
fn adv_large_report_100_findings_all_present() {
    let highlights: Vec<Highlight> = (0..100)
        .map(|i| {
            let sensor = format!("sensor-{}", i % 4);
            let code = format!("CODE-{i:04}");
            let msg = format!("Finding number {i}");
            let path = format!("src/file_{}.rs", i % 10);
            let fp = format!("fp-{i}");
            make_highlight(
                &sensor,
                match i % 3 {
                    0 => Severity::Error,
                    1 => Severity::Warn,
                    _ => Severity::Info,
                },
                &code,
                &msg,
                Some(&path),
                Some(i as u32 + 1),
                Some((i as u32 % 80) + 1),
                if i % 5 == 0 { Some(fp.as_str()) } else { None },
            )
        })
        .collect();

    let report = CockpitReport {
        highlights,
        ..base_report()
    };
    let sarif = cockpit_report_to_sarif(&report);

    assert_eq!(sarif.runs[0].results.len(), 100);
    assert_eq!(sarif.runs[0].tool.driver.rules.len(), 100);

    // Verify JSON is also valid with 100 results
    let json = cockpit_report_to_sarif_json(&report).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["runs"][0]["results"].as_array().unwrap().len(), 100);

    // Spot-check a few fingerprints
    let fp0 = parsed["runs"][0]["results"][0]["fingerprints"]["cockpitctl/v1"]
        .as_str()
        .unwrap();
    assert_eq!(fp0, "fp-0");
}

// ── 15) Special characters in messages → properly escaped in JSON ───────

#[test]
fn adv_special_characters_in_messages_escaped() {
    let msg = r#"expected `&str`, found `"hello <world> & 'friends'"` at "line" 42\n\ttab"#;
    let report = CockpitReport {
        highlights: vec![make_highlight(
            "sensor",
            Severity::Error,
            "SC-001",
            msg,
            Some("src/main.rs"),
            Some(42),
            None,
            None,
        )],
        ..base_report()
    };

    let json = cockpit_report_to_sarif_json(&report).unwrap();
    // Must parse as valid JSON despite quotes, angle brackets, ampersand, backslashes
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(
        parsed["runs"][0]["results"][0]["message"]["text"]
            .as_str()
            .unwrap(),
        msg
    );
}

// ── 16) Unicode in paths → properly encoded ─────────────────────────────

#[test]
fn adv_unicode_in_paths_encoded() {
    let report = CockpitReport {
        highlights: vec![make_highlight(
            "sensor-日本語",
            Severity::Warn,
            "règle-αβγ",
            "变量名 should be camelCase 🐫",
            Some("src/données/über_module.rs"),
            Some(1),
            None,
            None,
        )],
        ..base_report()
    };

    let json = cockpit_report_to_sarif_json(&report).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    assert_eq!(
        parsed["runs"][0]["results"][0]["ruleId"].as_str().unwrap(),
        "règle-αβγ"
    );
    assert!(
        parsed["runs"][0]["results"][0]["message"]["text"]
            .as_str()
            .unwrap()
            .contains('🐫')
    );
    assert_eq!(
        parsed["runs"][0]["results"][0]["locations"][0]["physicalLocation"]["artifactLocation"]["uri"]
            .as_str()
            .unwrap(),
        "src/données/über_module.rs"
    );
}

// ── 17) Finding with empty code → rule ID handled ───────────────────────

#[test]
fn adv_finding_empty_code_rule_id_handled() {
    let report = CockpitReport {
        highlights: vec![make_highlight(
            "sensor",
            Severity::Info,
            "",
            "finding with empty code",
            Some("src/lib.rs"),
            Some(5),
            None,
            None,
        )],
        ..base_report()
    };
    let sarif = cockpit_report_to_sarif(&report);

    assert_eq!(sarif.runs[0].results[0].rule_id, "");
    assert_eq!(sarif.runs[0].tool.driver.rules.len(), 1);
    assert_eq!(sarif.runs[0].tool.driver.rules[0].id, "");
}

// ── 18) Run structure: exactly one run per conversion ────────────────────

#[test]
fn adv_always_exactly_one_run() {
    // Empty report
    let sarif_empty = cockpit_report_to_sarif(&base_report());
    assert_eq!(sarif_empty.runs.len(), 1);

    // Report with findings from many sensors
    let report = CockpitReport {
        highlights: vec![
            make_highlight("s1", Severity::Error, "C1", "m1", None, None, None, None),
            make_highlight("s2", Severity::Warn, "C2", "m2", None, None, None, None),
            make_highlight("s3", Severity::Info, "C3", "m3", None, None, None, None),
            make_highlight("s4", Severity::Error, "C4", "m4", None, None, None, None),
            make_highlight("s5", Severity::Warn, "C5", "m5", None, None, None, None),
        ],
        ..base_report()
    };
    let sarif = cockpit_report_to_sarif(&report);
    assert_eq!(sarif.runs.len(), 1, "multi-sensor still produces one run");
}

// ── 19) Rules key omitted when no rules (empty highlights) ──────────────

#[test]
fn adv_rules_key_omitted_when_empty_in_json() {
    let json = cockpit_report_to_sarif_json(&base_report()).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    // rules should be absent due to skip_serializing_if = "Vec::is_empty"
    assert!(
        parsed["runs"][0]["tool"]["driver"].get("rules").is_none(),
        "empty rules should be omitted from JSON"
    );
}

// ── 20) Duplicate codes across different sensors → single rule ──────────

#[test]
fn adv_duplicate_codes_across_sensors_single_rule() {
    let report = CockpitReport {
        highlights: vec![
            make_highlight(
                "sensor-a",
                Severity::Error,
                "SHARED-001",
                "from a",
                Some("a.rs"),
                Some(1),
                None,
                None,
            ),
            make_highlight(
                "sensor-b",
                Severity::Warn,
                "SHARED-001",
                "from b",
                Some("b.rs"),
                Some(2),
                None,
                None,
            ),
            make_highlight(
                "sensor-c",
                Severity::Info,
                "SHARED-001",
                "from c",
                Some("c.rs"),
                Some(3),
                None,
                None,
            ),
        ],
        ..base_report()
    };
    let sarif = cockpit_report_to_sarif(&report);

    // Same code from different sensors deduplicates to one rule
    assert_eq!(sarif.runs[0].tool.driver.rules.len(), 1);
    assert_eq!(sarif.runs[0].tool.driver.rules[0].id, "SHARED-001");
    // But all three results are preserved
    assert_eq!(sarif.runs[0].results.len(), 3);
}

// ── 21) Rule short_description includes first sensor_id ─────────────────

#[test]
fn adv_rule_short_description_includes_sensor_id() {
    let report = CockpitReport {
        highlights: vec![make_highlight(
            "builddiag",
            Severity::Error,
            "E0308",
            "mismatched types",
            None,
            None,
            None,
            None,
        )],
        ..base_report()
    };
    let sarif = cockpit_report_to_sarif(&report);
    let rule = &sarif.runs[0].tool.driver.rules[0];

    assert_eq!(rule.id, "E0308");
    let desc = rule.short_description.as_ref().unwrap();
    assert_eq!(desc.text, "[builddiag] E0308");
}

// ── 22) JSON $schema key uses correct dollar-sign name ──────────────────

#[test]
fn adv_json_dollar_schema_key() {
    let json = cockpit_report_to_sarif_json(&base_report()).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    let obj = parsed.as_object().unwrap();

    assert!(obj.contains_key("$schema"), "must use $schema key");
    assert!(!obj.contains_key("schema"), "must not use plain schema key");
}

// ── 23) Results preserve highlight input order ──────────────────────────

#[test]
fn adv_results_preserve_highlight_input_order() {
    let report = CockpitReport {
        highlights: vec![
            make_highlight("c", Severity::Info, "THIRD", "3rd", None, None, None, None),
            make_highlight("a", Severity::Error, "FIRST", "1st", None, None, None, None),
            make_highlight("b", Severity::Warn, "SECOND", "2nd", None, None, None, None),
        ],
        ..base_report()
    };
    let sarif = cockpit_report_to_sarif(&report);

    assert_eq!(sarif.runs[0].results[0].rule_id, "THIRD");
    assert_eq!(sarif.runs[0].results[1].rule_id, "FIRST");
    assert_eq!(sarif.runs[0].results[2].rule_id, "SECOND");
}

// ── 24) Newlines and control characters in messages ─────────────────────

#[test]
fn adv_newlines_and_control_chars_in_messages() {
    let msg = "line 1\nline 2\ttab\rcarriage";
    let report = CockpitReport {
        highlights: vec![make_highlight(
            "s",
            Severity::Warn,
            "NL-001",
            msg,
            None,
            None,
            None,
            None,
        )],
        ..base_report()
    };
    let json = cockpit_report_to_sarif_json(&report).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(
        parsed["runs"][0]["results"][0]["message"]["text"]
            .as_str()
            .unwrap(),
        msg
    );
}

// ── 25) Location with path=None in Location struct → no location ────────

#[test]
fn adv_location_struct_with_path_none_no_location() {
    let h = Highlight {
        sensor_id: "sensor".to_string(),
        finding: Finding {
            severity: Severity::Warn,
            check_id: None,
            code: "LP-001".to_string(),
            message: "location present but path is None".to_string(),
            location: Some(Location {
                path: None,
                line: Some(42),
                col: Some(10),
            }),
            help: None,
            url: None,
            fingerprint: None,
            data: None,
        },
    };
    let report = CockpitReport {
        highlights: vec![h],
        ..base_report()
    };
    let sarif = cockpit_report_to_sarif(&report);
    assert!(
        sarif.runs[0].results[0].locations.is_empty(),
        "path=None means no SARIF location even with line/col"
    );
}
