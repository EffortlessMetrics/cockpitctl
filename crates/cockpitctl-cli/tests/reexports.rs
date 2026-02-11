use cockpitctl::types::{MissingPolicy, RunInfo, SensorSummary, Verdict, VerdictCounts};
use cockpitctl::{CockpitConfig, CockpitReport, Presence, ToolInfo, VerdictStatus, render_comment};

#[test]
fn cli_lib_reexports_core_api() {
    let cfg = CockpitConfig::default();
    let policy = cockpitctl::domain::snapshot_policy(&cfg);
    let report = CockpitReport {
        schema: "cockpit.report.v1".to_string(),
        tool: ToolInfo {
            name: "cockpitctl".to_string(),
            version: "0.2.0".to_string(),
            commit: None,
        },
        run: RunInfo {
            started_at: "2026-02-01T00:00:00Z".to_string(),
            ended_at: None,
            duration_ms: None,
            host: None,
            git: None,
            ci: None,
            capabilities: std::collections::BTreeMap::new(),
        },
        verdict: Verdict {
            status: VerdictStatus::Pass,
            counts: VerdictCounts::default(),
            reasons: vec![],
        },
        sensors: vec![SensorSummary {
            id: "alpha".to_string(),
            blocking: true,
            missing: MissingPolicy::Fail,
            presence: Presence::Present,
            report_path: "artifacts/alpha/report.json".to_string(),
            comment_path: None,
            verdict: Verdict {
                status: VerdictStatus::Pass,
                counts: VerdictCounts::default(),
                reasons: vec![],
            },
            truncated: false,
            errors: vec![],
            missing_policy_applied: None,
            policy_outcome: None,
        }],
        highlights: vec![],
        policy,
        data: None,
    };

    let md = render_comment(&report, &cfg);
    assert!(md.contains("## Cockpit"));
}
