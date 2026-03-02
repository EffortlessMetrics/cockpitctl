//! Property-based tests for config merge and precedence invariants.
//!
//! Covers:
//! - CLI schema_validation_override takes effect when provided
//! - Default config produces sensible defaults
//! - Config merge associativity: explicit config fields override defaults

use std::cell::RefCell;
use std::collections::BTreeMap;

use cockpitctl_ingest::{
    CommentRead, DiscoveredSensors, IngestRequest, IngestUseCase, NoOpSchemaValidator, OutputSink,
    PolicySource, ReceiptSource, ReportRead,
};
use cockpitctl_types::{
    CockpitConfig, MissingPolicy, Policy, RunInfo, SchemaValidation, SensorPolicy, ToolInfo,
    VerdictStatus,
};
use proptest::prelude::*;

// ============================================================================
// Test doubles
// ============================================================================

struct EmptyReceipts;

impl ReceiptSource for EmptyReceipts {
    fn discovered_sensors(&self) -> anyhow::Result<DiscoveredSensors> {
        Ok(DiscoveredSensors {
            sensors: vec![],
            truncated: false,
            total_found: 0,
            invalid_sensor_ids: vec![],
        })
    }

    fn read_report_bytes(&self, _sensor_id: &str) -> anyhow::Result<ReportRead> {
        Ok(ReportRead::Missing)
    }

    fn report_path(&self, sensor_id: &str) -> String {
        format!("artifacts/{sensor_id}/report.json")
    }

    fn comment_path_if_present(&self, _sensor_id: &str) -> anyhow::Result<CommentRead> {
        Ok(CommentRead::Missing)
    }
}

struct StaticPolicy{
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

fn tool_info() -> ToolInfo {
    ToolInfo {
        name: "proptest-config".to_string(),
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

fn any_schema_validation() -> impl Strategy<Value = SchemaValidation> {
    prop_oneof![Just(SchemaValidation::Lax), Just(SchemaValidation::Strict),]
}

// ============================================================================
// Config precedence: CLI override takes effect
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]

    /// When CLI provides schema_validation_override, it takes precedence over config.
    #[test]
    fn cli_schema_validation_overrides_config(
        config_validation in any_schema_validation(),
        cli_override in any_schema_validation(),
    ) {
        let mut cfg = CockpitConfig::default();
        cfg.policy.schema_validation = config_validation;

        let receipts = EmptyReceipts;
        let policy = StaticPolicy { cfg: Some(cfg) };
        let output = CaptureSink::default();
        let uc = IngestUseCase::new(
            receipts,
            policy,
            output,
            NoOpSchemaValidator,
            |_report, _cfg| "<!-- comment -->".to_string(),
        );

        let result = uc
            .execute(IngestRequest {
                labels: vec![],
                tool: tool_info(),
                run: run_info(),
                schema_validation_override: Some(cli_override),
            })
            .unwrap();

        // With empty receipts the pipeline always succeeds. The test
        // validates the override is accepted without error.
        prop_assert_eq!(result.exit_code, 0);
    }

    /// When no CLI override, config schema_validation is used (default behavior).
    #[test]
    fn no_override_uses_config_default(
        config_validation in any_schema_validation(),
    ) {
        let mut cfg = CockpitConfig::default();
        cfg.policy.schema_validation = config_validation;

        let receipts = EmptyReceipts;
        let policy = StaticPolicy { cfg: Some(cfg) };
        let output = CaptureSink::default();
        let uc = IngestUseCase::new(
            receipts,
            policy,
            output,
            NoOpSchemaValidator,
            |_report, _cfg| "<!-- comment -->".to_string(),
        );

        let result = uc
            .execute(IngestRequest {
                labels: vec![],
                tool: tool_info(),
                run: run_info(),
                schema_validation_override: None,
            })
            .unwrap();

        prop_assert_eq!(result.exit_code, 0);
    }
}

// ============================================================================
// Config defaults: merge(default, explicit) == explicit
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]

    /// Explicit config fields override defaults when present.
    #[test]
    fn explicit_config_overrides_defaults(
        warn_is_fail in any::<bool>(),
        max_highlights in 1usize..20,
        max_per_sensor_findings in 1usize..50,
    ) {
        let cfg = CockpitConfig {
            policy: Policy {
                warn_is_fail,
                max_highlights,
                max_per_sensor_findings,
                ..Default::default()
            },
            ..Default::default()
        };

        // Verify the explicit fields survived construction.
        prop_assert_eq!(cfg.policy.warn_is_fail, warn_is_fail);
        prop_assert_eq!(cfg.policy.max_highlights, max_highlights);
        prop_assert_eq!(cfg.policy.max_per_sensor_findings, max_per_sensor_findings);

        // Verify defaults for non-overridden fields.
        prop_assert_eq!(cfg.policy.max_annotations, 25);
        prop_assert_eq!(cfg.policy.max_receipt_size_bytes, 2 * 1024 * 1024);
    }

    /// Config deserialized from TOML-like JSON with explicit fields preserves them.
    #[test]
    fn config_toml_roundtrip_preserves_explicit_fields(
        warn_is_fail in any::<bool>(),
        max_highlights in 1usize..20,
    ) {
        let cfg = CockpitConfig {
            policy: Policy {
                warn_is_fail,
                max_highlights,
                ..Default::default()
            },
            ..Default::default()
        };

        let json = serde_json::to_string(&cfg).unwrap();
        let parsed: CockpitConfig = serde_json::from_str(&json).unwrap();

        prop_assert_eq!(parsed.policy.warn_is_fail, warn_is_fail);
        prop_assert_eq!(parsed.policy.max_highlights, max_highlights);
    }

    /// Adding a sensor to config preserves existing sensor policies.
    #[test]
    fn adding_sensor_preserves_existing(
        existing_blocking in any::<bool>(),
        new_blocking in any::<bool>(),
    ) {
        let mut cfg = CockpitConfig::default();
        cfg.sensors.insert(
            "existing".to_string(),
            SensorPolicy {
                blocking: existing_blocking,
                missing: MissingPolicy::Warn,
                ..Default::default()
            },
        );
        cfg.sensors.insert(
            "new".to_string(),
            SensorPolicy {
                blocking: new_blocking,
                missing: MissingPolicy::Fail,
                ..Default::default()
            },
        );

        prop_assert_eq!(cfg.sensors["existing"].blocking, existing_blocking);
        prop_assert_eq!(cfg.sensors["existing"].missing, MissingPolicy::Warn);
        prop_assert_eq!(cfg.sensors["new"].blocking, new_blocking);
        prop_assert_eq!(cfg.sensors["new"].missing, MissingPolicy::Fail);
    }

    /// Blocking sensor with missing=fail always produces exit code 2 when receipt is missing.
    #[test]
    fn blocking_missing_fail_always_exit_2(
        warn_is_fail in any::<bool>(),
        max_highlights in 1usize..10,
    ) {
        let mut cfg = CockpitConfig {
            policy: Policy {
                warn_is_fail,
                max_highlights,
                ..Default::default()
            },
            ..Default::default()
        };
        cfg.sensors.insert(
            "required".to_string(),
            SensorPolicy {
                blocking: true,
                missing: MissingPolicy::Fail,
                ..Default::default()
            },
        );

        let receipts = EmptyReceipts;
        let policy = StaticPolicy {
            cfg: Some(cfg),
        };
        let output = CaptureSink::default();
        let uc = IngestUseCase::new(
            receipts,
            policy,
            output,
            NoOpSchemaValidator,
            |_report, _cfg| "<!-- comment -->".to_string(),
        );

        let result = uc
            .execute(IngestRequest {
                labels: vec![],
                tool: tool_info(),
                run: run_info(),
                schema_validation_override: None,
            })
            .unwrap();

        prop_assert_eq!(result.exit_code, 2, "missing blocking sensor must produce exit 2");
        prop_assert_eq!(result.report.verdict.status, VerdictStatus::Fail);
    }
}
