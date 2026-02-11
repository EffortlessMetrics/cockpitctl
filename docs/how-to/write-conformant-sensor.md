# Write a Conformant Sensor

This guide shows how to build a sensor that emits valid receipts for cockpitctl.

## What a Sensor Must Do

A sensor:
1. Analyzes some aspect of the code/build
2. Emits a `sensor.report.v1` receipt
3. Writes to `artifacts/<sensor_id>/report.json`

A sensor must **not**:
- Write outside its artifacts directory
- Assume cockpitctl will parse tool-specific data
- Change finding codes between versions

## Minimal Receipt

```json
{
  "schema": "sensor.report.v1",
  "tool": {
    "name": "my-sensor",
    "version": "1.0.0"
  },
  "run": {
    "started_at": "2024-01-15T10:30:00Z"
  },
  "verdict": {
    "status": "pass",
    "counts": {
      "info": 0,
      "warn": 0,
      "error": 0
    }
  },
  "findings": []
}
```

## Adding Findings

```json
{
  "findings": [
    {
      "severity": "warn",
      "code": "my-sensor.missing-test",
      "message": "Function 'calculate' has no test coverage",
      "location": {
        "path": "src/math.rs",
        "line": 42,
        "col": 1
      }
    }
  ],
  "verdict": {
    "status": "warn",
    "counts": {
      "info": 0,
      "warn": 1,
      "error": 0
    }
  }
}
```

### Finding Fields

| Field | Required | Description |
|-------|----------|-------------|
| `severity` | yes | `info`, `warn`, or `error` |
| `code` | yes | Stable identifier like `sensor.rule-name` |
| `message` | yes | Human-readable description |
| `location` | no | File path, line, column |
| `fingerprint` | no | Stable ID for deduplication |
| `check_id` | no | Rule/check identifier |
| `help` | no | Additional guidance |
| `url` | no | Link to documentation |
| `data` | no | Tool-specific payload |

### Finding Codes

Use a consistent naming scheme:

```
<sensor-name>.<rule-category>.<specific-rule>
```

Examples:
- `covguard.coverage.below-threshold`
- `builddiag.build.missing-dependency`
- `diffguard.policy.forbidden-api`

Codes must be stable. Never rename codes; deprecate and alias instead.

## Verdict Status

| Status | When to Use |
|--------|-------------|
| `pass` | No issues found |
| `warn` | Advisory issues (non-blocking by default) |
| `fail` | Blocking issues found |
| `skip` | Sensor didn't run (not applicable, precondition failed) |

Match the verdict to your findings:
- `error` findings → `fail` verdict
- `warn` findings only → `warn` verdict
- No findings → `pass` verdict

### Counts Must Match Findings

The `verdict.counts` must reflect the findings array:

- Count `info`, `warn`, and `error` severities accurately
- `cockpitctl` recomputes counts and emits `cockpit.receipt_inconsistent` if they differ

Keep counts consistent to avoid noisy highlights and confusion.

## Tool-Specific Data

Put custom data in `data` fields:

```json
{
  "data": {
    "coverage_percent": 85.5,
    "files_analyzed": 42,
    "internal_metrics": { ... }
  },
  "findings": [
    {
      "severity": "info",
      "code": "covguard.coverage.report",
      "message": "Coverage is 85.5%",
      "data": {
        "threshold": 80,
        "actual": 85.5
      }
    }
  ]
}
```

cockpitctl passes `data` through without interpretation.

## Fingerprint Stability

If you provide `fingerprint`, it must be **stable across runs** for the same finding:

- Use a deterministic hash (e.g., SHA-256 hex, 64 chars)
- Include stable inputs like `sensor_id`, `code`, `message`, and location (`path`, `line`)
- Avoid timestamps, random IDs, or run-specific data

Stable fingerprints enable deduplication and deterministic highlights.

## Run Metadata

Include execution context:

```json
{
  "run": {
    "started_at": "2024-01-15T10:30:00Z",
    "ended_at": "2024-01-15T10:30:05Z",
    "duration_ms": 5000,
    "git": {
      "repo": "owner/repo",
      "head_sha": "abc1234",
      "base_sha": "def5678"
    },
    "ci": {
      "provider": "github",
      "run_id": "12345"
    }
  }
}
```

## Output Path

Write to:
```
artifacts/<sensor-id>/report.json
```

The sensor ID must:
- Match your sensor's identifier
- Not contain path traversal (`..`)
- Be URL-safe (alphanumeric, hyphens, underscores)

## Optional Comment

If your sensor produces detailed output, write:
```
artifacts/<sensor-id>/comment.md
```

cockpitctl will link to it but won't inline it.

## Reason Tokens

The `verdict.reasons` array carries machine-readable tokens explaining the verdict. Tokens must match `^[a-z0-9_]+$` (lowercase ASCII, digits, underscores only).

Common sensor-emitted tokens:

| Token | When to use |
|-------|-------------|
| `tool_error` | The tool crashed or hit a runtime error |
| `no_baseline` | A baseline-dependent check had no baseline |

Sensors may define additional tokens. See [Token Registry](../../contracts/docs/tokens.md) for the full list.

## Error Handling

When your sensor fails internally, use the `tool_error` reason token and the canonical identity tuple (`check_id: "tool.runtime"`, `code: "runtime_error"`):

```json
{
  "schema": "sensor.report.v1",
  "tool": { "name": "my-sensor", "version": "1.0.0" },
  "run": { "started_at": "..." },
  "verdict": {
    "status": "fail",
    "counts": { "info": 0, "warn": 0, "error": 1 },
    "reasons": ["tool_error"]
  },
  "findings": [
    {
      "severity": "error",
      "check_id": "tool.runtime",
      "code": "runtime_error",
      "message": "Failed to parse config: invalid TOML at line 5"
    }
  ]
}
```

The `tool_error` identity is enforced by `xtask conform --tool-error-identity`. Using the canonical `check_id`/`code` pair ensures cockpitctl and downstream consumers can detect tool failures consistently.

Always emit a receipt if possible, even on failure. This surfaces errors in cockpitctl rather than silently missing.

## Determinism

Sensors should be deterministic:
- Same inputs → same receipt
- Stable finding order
- No random IDs without purpose

This enables:
- Reproducible builds
- Meaningful diff of receipts
- Caching

## Example: Rust Sensor

```rust
use serde::Serialize;
use chrono::Utc;

#[derive(Serialize)]
struct SensorReport {
    schema: &'static str,
    tool: Tool,
    run: Run,
    verdict: Verdict,
    findings: Vec<Finding>,
}

fn main() {
    let started_at = Utc::now();
    let findings = analyze();

    let report = SensorReport {
        schema: "sensor.report.v1",
        tool: Tool {
            name: "my-sensor".into(),
            version: env!("CARGO_PKG_VERSION").into(),
        },
        run: Run {
            started_at: started_at.to_rfc3339(),
        },
        verdict: compute_verdict(&findings),
        findings,
    };

    std::fs::create_dir_all("artifacts/my-sensor")?;
    let json = serde_json::to_string_pretty(&report)?;
    std::fs::write("artifacts/my-sensor/report.json", json)?;
}
```

## See Also

- [Sensor Report Schema](../reference/sensor-report-schema.md) - Full schema reference
- [Test Sensor Conformance](test-sensor-conformance.md) - Validating sensors
- [Composition Model](../explanation/composition-model.md) - How sensors work together
