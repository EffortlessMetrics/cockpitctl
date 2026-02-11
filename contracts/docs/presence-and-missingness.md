# Presence and Missingness Contract

This document specifies how cockpitctl models sensor presence, handles missing or invalid receipts, and interacts with label gating and policy.

## Presence States

Every sensor in the cockpit report carries a `presence` field with one of three values:

| State | Meaning |
|-------|---------|
| `present` | Receipt file found and successfully parsed |
| `missing` | No receipt file at expected path |
| `invalid` | File exists but cannot be parsed, fails schema validation, exceeds size limit, or contains a path traversal |

## Missing Policy

Each sensor declares a `missing` policy in `cockpit.toml`:

| Policy | Verdict | Finding Emitted | Reason Token |
|--------|---------|-----------------|--------------|
| `skip` | `skip` | No | — |
| `warn` | `warn` | Yes (severity `warn`) | `missing_receipt` |
| `fail` | `fail` | Yes (severity `error`) | `missing_receipt` |

When missing policy is applied, the sensor summary sets:
- `missing_policy_applied` — the policy value that was applied (`skip`, `warn`, or `fail`)
- `policy_outcome` — the resulting outcome (`blocked`, `allowed`, or `informational`)

## No Green by Omission

Blocking sensors with `missing = "fail"` ensure that a missing receipt blocks the pipeline. This prevents passing a check simply by not running a sensor.

Configuration pattern:
```toml
[sensors.critical-sensor]
blocking = true
missing = "fail"
```

If `critical-sensor` produces no receipt, the aggregate verdict is `fail` with exit code 2.

## Label Gating

Sensors may declare `require_label` in policy. Label gating is evaluated before presence:

- If `require_label` is set and the label is **not** present in the PR labels, the sensor is **skipped entirely** (not treated as missing).
- The sensor summary uses `presence = "missing"` with `missing = "skip"` regardless of the sensor's configured missing policy.
- Label-gated sensors that are skipped do **not** emit findings or affect the aggregate verdict.

This distinction matters: a label-gated sensor that does not match is intentionally excluded, not accidentally absent.

## Error Surfacing

Parse errors, schema violations, and safety violations become cockpit-level findings:

| Condition | Code | Check ID | Severity |
|-----------|------|----------|----------|
| JSON parse failure | `cockpit.invalid_receipt` | `cockpit.invalid_receipt` | `error` |
| Schema validation failure (strict mode) | `cockpit.schema_violation` | `cockpit.schema_violation` | `error` |
| Path traversal in sensor ID | `cockpit.path_traversal` | `cockpit.path_traversal` | `error` |
| Receipt exceeds size limit | `cockpit.receipt_oversized` | `cockpit.receipt_oversized` | `error` |
| Receipt counts mismatch | `cockpit.receipt_inconsistent` | `cockpit.receipt_inconsistent` | `info` |

All error conditions set `presence = "invalid"` (except path traversal, which uses `presence = "missing"`).

## Policy Outcome

Each sensor summary includes a `policy_outcome` field indicating the effect on the pipeline:

| Outcome | Meaning |
|---------|---------|
| `blocked` | Sensor is blocking and verdict is `fail` |
| `allowed` | Sensor is blocking and verdict is not `fail` |
| `informational` | Sensor is non-blocking |

## Evaluation Order

1. Label gating check (skip if label not matched)
2. Path safety check (reject traversal)
3. File presence check (missing policy if absent)
4. Size limit check (reject if oversized)
5. Schema validation (strict mode only)
6. JSON parse and deserialization
7. Findings normalization and capping

## See Also

- [Token Registry](tokens.md) — reason tokens for each presence state
- [Determinism Contract](determinism.md) — ordering guarantees
- [Artifact Layout](artifact-layout.md) — expected file locations
