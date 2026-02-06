# Token Registry

This document is the canonical registry for all reason tokens in the cockpitctl protocol.

## Format Rule

Reason tokens must match `^[a-z0-9_]+$` (lowercase ASCII letters, digits, and underscores only).

**Enforcement points:**
- `xtask conform --reason-lint` validates tokens in fixture files
- `is_valid_reason_token()` in xtask checks `verdict.reasons[]` and `capabilities.*.reason`

**Exception:** `warn_is_fail:<sensor_id>` is parameterized and uses a colon separator (see Policy-Derived Reason Tokens below).

## Sensor-Emitted Reason Tokens

These tokens are set by sensors in their `verdict.reasons[]` array. This is an **open set** — sensors may define additional tokens.

| Token | Meaning |
|-------|---------|
| `tool_error` | The tool crashed or encountered a runtime error |
| `no_baseline` | A baseline-dependent check had no baseline available |

## Cockpit-Synthesized Reason Tokens

These tokens are injected by cockpitctl into per-sensor `verdict.reasons[]` during ingestion. This is a **closed set** — only cockpitctl may add new entries.

| Token | Meaning | Source |
|-------|---------|--------|
| `missing_receipt` | Expected sensor did not produce a receipt | `synthesize_missing_sensor` |
| `invalid_receipt` | Receipt exists but failed JSON/serde parsing | `synthesize_invalid_sensor` |
| `schema_violation` | Receipt failed JSON Schema validation (strict mode) | `synthesize_schema_violation_sensor` |
| `path_traversal` | Sensor ID contains `..` path traversal | `synthesize_path_traversal_sensor` |
| `receipt_oversized` | Receipt file exceeds size limit (default 2 MB) | `synthesize_receipt_oversized_sensor` |

**Note:** `cockpit.receipt_inconsistent` appears as a finding code but is **not** pushed to `verdict.reasons[]` — it is surfaced only as a finding with `code = "cockpit.receipt_inconsistent"`. This inconsistency with the other synthesized tokens is documented here for future resolution.

## Policy-Derived Reason Tokens

These tokens are injected into the **aggregate** verdict's `reasons[]` by policy evaluation.

| Pattern | Example | Meaning |
|---------|---------|---------|
| `warn_is_fail:<sensor_id>` | `warn_is_fail:linter` | The `warn_is_fail` policy escalated warnings to failures for this sensor |

The colon-separated format is an **exception** to the `^[a-z0-9_]+$` format rule. The conformance linter validates the prefix but permits the parameterized suffix.

## Capability Reason Tokens

Capabilities in `run.capabilities` may carry an optional `reason` field explaining their status.

| Token | Capability Status | Meaning |
|-------|-------------------|---------|
| `no_baseline` | `unavailable` | The capability requires a baseline that was not provided |

Capability reasons follow the same format rule as verdict reasons.

## Canonical Identity Tuples

When a reason token implies a specific finding, the finding must use the canonical `check_id` and `code` pair.

### Sensor-Emitted

| Reason Token | `check_id` | `code` | Notes |
|--------------|------------|--------|-------|
| `tool_error` | `tool.runtime` | `runtime_error` | Conformance-enforced by `check_tool_error_identity` |

### Cockpit-Synthesized

| Reason Token | `check_id` | `code` |
|--------------|------------|--------|
| `missing_receipt` | `cockpit.missing_receipt` | `cockpit.missing_receipt` |
| `invalid_receipt` | `cockpit.invalid_receipt` | `cockpit.invalid_receipt` |
| `schema_violation` | `cockpit.schema_violation` | `cockpit.schema_violation` |
| `path_traversal` | `cockpit.path_traversal` | `cockpit.path_traversal` |
| `receipt_oversized` | `cockpit.receipt_oversized` | `cockpit.receipt_oversized` |

These constants are defined in `cockpitctl-domain::cockpit_codes`.

## Finding Codes

Finding codes (`cockpit.*`) are documented in [finding-codes.md](../../docs/reference/finding-codes.md). They are not duplicated here.

Additional cockpit-synthesized finding codes that do not have a corresponding reason token:
- `cockpit.receipt_inconsistent` — receipt claimed counts don't match findings
- `cockpit.sensors_truncated` — sensor discovery hit the `max_receipts` safety limit

## Stability

- Reason tokens are part of the cockpitctl API contract.
- The closed set (cockpit-synthesized) is never renamed or removed.
- New tokens may be added in minor versions.
- The format rule (`^[a-z0-9_]+$`) is frozen.
- Identity tuples are frozen once published.

## See Also

- [Identity Specification](identity-spec.md) — vocabulary and fingerprint rules
- [Finding Codes](../../docs/reference/finding-codes.md) — detailed `cockpit.*` code documentation
