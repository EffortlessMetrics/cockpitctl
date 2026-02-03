# cockpit.toml Reference

The `cockpit.toml` file defines merge governance policy. Sensors emit observations; cockpitctl applies policy.

## File Location

By default, cockpitctl looks for `cockpit.toml` in the current directory. Override with `--config`.

## Complete Example

```toml
[policy]
warn_is_fail = false
max_highlights = 7
max_per_sensor_findings = 20
max_annotations = 25
section_order = ["Highlights", "Repo contract", "Dependencies", "Policy", "Tests", "Diagnostics", "Performance", "Environment", "Other"]

[sensors.builddiag]
blocking = true
missing = "fail"
section = "Repo contract"
repro = "builddiag check --profile team"

[sensors.diffguard]
blocking = true
missing = "warn"
section = "Policy"
repro = "diffguard diff HEAD~1"

[sensors.covguard]
blocking = false
missing = "skip"
section = "Tests"

[sensors.perf-bench]
blocking = false
missing = "skip"
section = "Performance"
require_label = "run-perf"
```

## Sections

### [policy]

Global policy settings.

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `warn_is_fail` | bool | `false` | When true, warnings from blocking sensors cause policy failure |
| `max_highlights` | int | `7` | Maximum findings shown in highlights section |
| `max_per_sensor_findings` | int | `20` | Maximum findings surfaced per sensor |
| `max_annotations` | int | `25` | Maximum annotations for annotation emitters (future) |
| `section_order` | array | see below | Order of sections in PR comment |

**Default section order:**

```toml
section_order = [
  "Highlights",
  "Repo contract",
  "Dependencies",
  "Policy",
  "Tests",
  "Diagnostics",
  "Performance",
  "Environment",
  "Other"
]
```

### [sensors.<id>]

Per-sensor configuration. The `<id>` must match the sensor's directory name under `artifacts/`.

| Key | Type | Required | Description |
|-----|------|----------|-------------|
| `blocking` | bool | yes | Whether this sensor participates in overall verdict |
| `missing` | string | yes | What to do when receipt is missing: `skip`, `warn`, or `fail` |
| `section` | string | no | Which section the sensor appears in |
| `repro` | string | no | One-line command to reproduce locally |
| `require_label` | string | no | Only enforce when this PR label is present |

## Policy Behavior

### Blocking Sensors

When `blocking = true`:
- Sensor's verdict contributes to overall cockpit verdict
- A `fail` verdict causes exit code `2`
- A `warn` verdict causes exit code `2` if `warn_is_fail = true`

When `blocking = false`:
- Sensor is informational only
- Its verdict does not affect exit code

### Missing Receipt Handling

| Value | Behavior |
|-------|----------|
| `skip` | Sensor is silently skipped; no finding generated |
| `warn` | Generates `cockpit.missing_receipt` warning; sensor shown as skipped |
| `fail` | Generates `cockpit.missing_receipt` error; if blocking, causes failure |

### Label Gates

When `require_label` is set:
- If the label is absent, the sensor is treated as "effectively skipped"
- Useful for expensive checks that shouldn't run on every PR

Note: cockpitctl does not fetch labels from GitHub. The label context must be supplied externally (e.g., via CI workflow).

## Implicit Sensors

If no `cockpit.toml` exists or no sensors are defined:
- cockpitctl discovers receipts from `artifacts/*/report.json`
- All discovered sensors are treated as non-blocking
- Missing receipts are not flagged (no expectations)

## See Also

- [CLI Reference](cli.md) - How to specify config path
- [Handle Missing Receipts](../how-to/handle-missing-receipts.md) - Practical guidance
