# Composition Model

cockpitctl composes independent sensor receipts into a unified cockpit. This document explains how that composition works.

## The Core Idea

Sensors are independent observers. Each runs separately, produces a receipt, and knows nothing about other sensors.

cockpitctl is the composer:
1. Gathers all receipts
2. Applies policy
3. Produces a unified view

This separation allows sensors to evolve independently while maintaining a coherent merge decision.

## The Receipt Contract

Sensors emit receipts conforming to `sensor.report.v1`:

```json
{
  "schema": "sensor.report.v1",
  "tool": { "name": "builddiag", "version": "0.5.0" },
  "run": { "started_at": "..." },
  "verdict": { "status": "pass", "counts": {...} },
  "findings": [...],
  "data": { /* tool-specific, opaque to cockpit */ }
}
```

cockpitctl only relies on the envelope fields. The `data` field can contain anything; cockpitctl passes it through without interpretation.

## Expected vs Discovered Sensors

There are two modes of operation:

### With Policy

When `cockpit.toml` defines sensors:
```toml
[sensors.builddiag]
blocking = true
missing = "fail"

[sensors.covguard]
blocking = false
missing = "skip"
```

These sensors are **expected**. If a receipt is missing:
- `builddiag` missing → failure (because `missing = "fail"`)
- `covguard` missing → silently skipped (because `missing = "skip"`)

### Without Policy

When no policy exists, sensors are **discovered**:
- cockpitctl scans `artifacts/*/report.json`
- All discovered sensors are informational (non-blocking)
- No "missing" concept (nothing is expected)

This allows zero-config bootstrapping: just run sensors and run cockpitctl.

## Verdict Composition

Each sensor has a verdict: `pass`, `warn`, `fail`, or `skip`.

The overall cockpit verdict is computed from **blocking sensors only**:

```
For each blocking sensor:
  if verdict == fail:
    overall = fail
  if verdict == warn and warn_is_fail:
    overall = fail
  if verdict == warn and not warn_is_fail:
    overall = max(overall, warn)

If no blocking sensor failed/warned:
  overall = pass
```

Non-blocking sensors contribute to the comment but not the exit code.

## Highlight Selection

Highlights are the most important findings across all sensors. Selection works as follows:

1. **Collect**: Gather findings from all sensors (up to `max_per_sensor_findings` each)
2. **Deduplicate**: Remove duplicates by fingerprint
3. **Sort**: Order by severity (desc), blocking status, sensor, location
4. **Cap**: Keep only `max_highlights` findings

This ensures:
- Errors from blocking sensors appear first
- No single sensor dominates highlights
- Output is bounded and stable

## Section Organization

The PR comment organizes sensors into sections:

```toml
[policy]
section_order = ["Highlights", "Repo contract", "Dependencies", "Tests", "Other"]

[sensors.builddiag]
section = "Repo contract"
```

Sensors appear in their configured section. Sensors without a section go to "Other".

Typical sections map to mental models:
- **Repo contract**: Build, required files, structure
- **Dependencies**: Lockfile, policy enforcement
- **Policy**: Diff-scoped rules, guardrails
- **Tests**: Coverage, test gates
- **Diagnostics**: Lint deltas, typecheck deltas
- **Performance**: Optional, label-gated
- **Environment**: Informational

## The Aggregation

cockpitctl produces `cockpit.report.v1`:

```json
{
  "schema": "cockpit.report.v1",
  "verdict": { /* overall */ },
  "sensors": [
    { "id": "builddiag", "verdict": {...}, "truncated": false },
    { "id": "covguard", "verdict": {...}, "truncated": true }
  ],
  "highlights": [...],
  "policy": { /* snapshot of applied policy */ }
}
```

This is a "receipt of receipts": it summarizes what was observed without duplicating full findings.

## Information Flow

```
┌──────────┐     ┌──────────┐     ┌──────────┐
│ Sensor A │     │ Sensor B │     │ Sensor C │
│ receipt  │     │ receipt  │     │ (missing)│
└────┬─────┘     └────┬─────┘     └────┬─────┘
     │                │                │
     ▼                ▼                ▼
┌─────────────────────────────────────────────┐
│                  cockpitctl                 │
│  ┌─────────────────────────────────────┐    │
│  │           Load Policy               │    │
│  └─────────────────────────────────────┘    │
│                    │                        │
│  ┌─────────────────┴───────────────────┐    │
│  │         For each expected sensor    │    │
│  │   - Parse receipt (or note missing) │    │
│  │   - Cap findings                    │    │
│  │   - Generate summary               │    │
│  └─────────────────────────────────────┘    │
│                    │                        │
│  ┌─────────────────┴───────────────────┐    │
│  │      Compute overall verdict        │    │
│  │      Select highlights              │    │
│  │      Render comment                 │    │
│  └─────────────────────────────────────┘    │
└─────────────────────────────────────────────┘
                     │
                     ▼
    ┌────────────────────────────────┐
    │  cockpit.report.v1             │
    │  comment.md                    │
    │  exit code                     │
    └────────────────────────────────┘
```

## See Also

- [Why cockpitctl](why-cockpitctl.md) - Design philosophy
- [Write a Conformant Sensor](../how-to/write-conformant-sensor.md) - Building sensors
- [Config Reference](../reference/config.md) - Policy configuration
