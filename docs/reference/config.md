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

[buildfix]
auto_apply = false
max_auto_apply_safety = "safe"
require_matched_finding = true

[buildfix.actuator]
command = "buildfix-actuator --apply"
timeout_ms = 30000

[policy_signing]
enabled = false
algorithm = "hmac_sha256"
key_env = "COCKPITCTL_POLICY_SIGNING_KEY"
key_id = "ci-key"

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
| `max_annotations` | int | `25` | Maximum annotations rendered in PR comment |
| `section_order` | array | see below | Order of sections in PR comment |
| `schema_validation` | string | `"lax"` | Receipt validation mode: `"lax"` or `"strict"` |

**Schema Validation:**

```toml
[policy]
schema_validation = "strict"  # Enable JSON Schema validation
```

- **`"lax"` (default):** Skip JSON Schema validation; only parse receipts as JSON
- **`"strict"`:** Validate receipts against `contracts/schemas/sensor.report.v1.json` before parsing; violations are surfaced as `cockpit.schema_violation` findings

> **Note:** The CLI `--schema-validation` flag overrides the config only when explicitly provided. If unset, the config mode applies.

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

### [buildfix]

Buildfix auto-apply settings.

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `auto_apply` | bool | `false` | Enable auto-apply execution after ingest |
| `max_auto_apply_safety` | string | `"safe"` | Maximum safety level allowed: `safe`, `guarded`, `unsafe` |
| `require_matched_finding` | bool | `true` | When true, only fixes matched to surfaced findings are eligible |

### [buildfix.actuator]

External actuator command configuration (required when `auto_apply = true` and you want fixes applied).

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `command` | string | (none) | Command to execute; receives `buildfix.apply.request.v1` JSON on stdin |
| `timeout_ms` | int | `30000` | Actuator timeout in milliseconds |

### [policy_signing]

Policy snapshot signing controls. When enabled, cockpitctl signs the canonical `report.policy` snapshot and emits:

- `data._policy_signature` in `cockpit.report.v1`
- `artifacts/cockpit/policy.signature.json`

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `enabled` | bool | `false` | Enable policy snapshot signing |
| `algorithm` | string | `"hmac_sha256"` | Signature algorithm (currently only `hmac_sha256`) |
| `key_path` | string | (none) | Path to signing key bytes |
| `key_env` | string | (none) | Environment variable name containing signing key bytes |
| `key_id` | string | (none) | Optional key identifier included in signature evidence |

`key_path` takes precedence over `key_env` when both are set.

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
