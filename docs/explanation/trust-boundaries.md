# Trust Boundaries

cockpitctl treats sensor receipts as untrusted input. This document explains the security model.

## The Threat Model

Receipts arrive from sensors that:
- May be compromised by supply chain attacks
- May have bugs that produce malformed output
- May be crafted by malicious actors (in PR scenarios)
- May try to escape their sandbox

cockpitctl must:
- Not crash on malformed input
- Not exhaust memory
- Not write outside expected paths
- Not execute arbitrary code
- Continue processing despite individual failures

## Trust Levels

```
┌─────────────────────────────────────────────────────┐
│  Trusted: cockpitctl binary, cockpit.toml          │
│  (reviewed, in main branch)                         │
└─────────────────────────────────────────────────────┘
                         │
                         ▼
┌─────────────────────────────────────────────────────┐
│  Untrusted: sensor receipts                         │
│  (produced by tools, may be from PRs)              │
└─────────────────────────────────────────────────────┘
```

### Trusted

- **cockpitctl binary**: Built from reviewed code
- **cockpit.toml**: Checked into the repository, reviewed

Policy is trusted because it's version-controlled. Changes to policy are visible in PRs.

### Untrusted

- **Sensor receipts**: Produced by tools that may be:
  - Buggy (emit malformed JSON)
  - Compromised (emit malicious paths)
  - Oversized (attempt DoS)

cockpitctl assumes receipts may be hostile.

## Defenses

### Path Traversal Prevention

Sensor IDs and paths are validated:

```
artifacts/../../../etc/passwd   → REJECTED
artifacts/my-sensor/../other    → REJECTED
artifacts/my-sensor/report.json → OK
```

The `..` pattern is rejected anywhere in sensor IDs.

### Symlink Containment

Symlinks are not followed if they escape the artifacts directory:

```
artifacts/
  real-sensor/report.json           → OK
  escape -> /etc/                   → REJECTED
  internal -> other-sensor/         → OK (stays inside)
```

### Size Limits

Receipts exceeding 2MB (default) are not read:

- Prevents memory exhaustion
- Surfaces as a `cockpit.receipt_oversized` finding
- Processing continues with other sensors

### Parsing Strictness

Receipt envelopes are parsed strictly:

- Unknown top-level fields cause parse errors
- Invalid JSON causes parse errors
- Missing required fields cause parse errors

All errors surface as findings, not crashes.

### No Execution

cockpitctl never executes:

- Commands from `repro` fields (display only)
- Content from `data` fields (opaque)
- Scripts from artifacts

Even if a receipt contains:
```json
{ "repro": "rm -rf /", "data": { "exec": "malicious" } }
```

cockpitctl only displays the repro string; it doesn't run it.

### Resource Bounds

Processing is bounded:

- O(receipts + findings) memory
- Maximum 100 sensors processed
- Maximum `max_per_sensor_findings` findings per sensor
- Maximum `max_highlights` in output

## Failure Behavior

When defenses trigger:

1. **Emit finding**: Problem is visible in output
2. **Continue**: Other sensors still process
3. **No crash**: cockpitctl remains stable

Example: Invalid receipt

```json
{
  "sensors": [
    {
      "id": "bad-sensor",
      "present": true,
      "errors": ["parse error at line 1: expected object"],
      "verdict": { "status": "fail", ... }
    }
  ],
  "highlights": [
    {
      "sensor_id": "cockpit",
      "finding": {
        "code": "cockpit.invalid_receipt",
        "message": "Failed to parse receipt for 'bad-sensor'"
      }
    }
  ]
}
```

The problem is surfaced; it's not hidden.

## What cockpitctl Trusts

- **File existence**: If a file exists at a path, its content is read
- **Filesystem permissions**: OS-level access control
- **Configuration**: `cockpit.toml` from the repository

## What cockpitctl Does Not Trust

- **Receipt content**: May be malformed or malicious
- **Receipt metadata**: Claimed counts, paths, etc.
- **Sensor IDs**: May contain traversal attempts
- **Data payloads**: Completely opaque

## Counts Reconciliation

Receipts may claim counts that don't match findings:

```json
{
  "verdict": { "counts": { "error": 5 } },
  "findings": [
    { "severity": "error", ... },
    { "severity": "error", ... }
  ]
}
```

cockpitctl:
- Computes actual counts from findings
- Uses computed counts for aggregation
- Emits `cockpit.receipt_inconsistent` (informational)

## See Also

- [Safety Limits](../reference/safety-limits.md) - Specific limits
- [Finding Codes](../reference/finding-codes.md) - Error findings
- [Why cockpitctl](why-cockpitctl.md) - Design philosophy
