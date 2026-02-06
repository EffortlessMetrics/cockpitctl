# Identity and Vocabulary Specification

This document defines the stable vocabulary and identity rules for the cockpitctl protocol.

## Severity Levels

Findings use a three-level severity scale:

| Level   | Meaning                                      |
|---------|----------------------------------------------|
| `info`  | Informational; no action required            |
| `warn`  | Warning; may require attention               |
| `error` | Error; requires action                       |

Severity is frozen. No other values are permitted.

## Verdict Status

The overall verdict uses a four-state model:

| Status | Meaning                                       |
|--------|-----------------------------------------------|
| `pass` | All checks passed                             |
| `warn` | Warnings present but not blocking             |
| `fail` | Blocking issues found                         |
| `skip` | Sensor was skipped (not run or not required)  |

Status is frozen. No other values are permitted.

## Safety Levels (Buildfix)

Fixes in `buildfix.plan.v1` declare their safety:

| Level     | Meaning                                      |
|-----------|----------------------------------------------|
| `safe`    | No side effects; safe to apply automatically |
| `guarded` | Requires confirmation before applying        |
| `unsafe`  | May break things; use with caution           |

## Fingerprint Derivation

Fingerprints uniquely identify findings for deduplication and tracking.

### Requirements

1. **Deterministic**: Same input produces same fingerprint
2. **Stable**: Does not change across runs for the same issue
3. **Unique**: Different issues produce different fingerprints

### Recommended Algorithm

Sensors SHOULD derive fingerprints from:
- `code` (required)
- `check_id` (if present)
- `location.path` (if present)
- `location.line` (if present)
- Message content (optional, may reduce stability)

### Example

```
SHA256(code || "|" || path || ":" || line)
```

## Code Stability

The `code` field identifies the type of issue:

- Must be non-empty
- Should be stable across tool versions
- Should be unique per issue type within a sensor
- Should use snake_case or kebab-case

### Examples

Good: `unused-variable`, `missing_import`, `E0001`
Bad: `error`, `issue_42`, `line_100_problem`

## Check ID

The optional `check_id` field provides additional categorization:

- Groups related codes
- Used for filtering and reporting
- Should be stable and meaningful

### Examples

- `clippy::pedantic`
- `security/injection`
- `performance`

## See Also

- [Token Registry](tokens.md) — canonical reason tokens and identity tuples
