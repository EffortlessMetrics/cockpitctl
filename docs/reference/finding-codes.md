# Finding Codes

cockpitctl generates findings with `cockpit.*` codes to surface issues with receipt processing.

## cockpit.* Codes

### cockpit.missing_receipt

Emitted when an expected sensor did not produce a receipt.

```json
{
  "severity": "warn",
  "code": "cockpit.missing_receipt",
  "message": "Expected receipt from sensor 'builddiag' but none was found"
}
```

**Severity depends on policy:**
- `"warn"` if sensor's `missing = "warn"`
- `"error"` if sensor's `missing = "fail"`
- Not emitted if `missing = "skip"`

**When it appears:**
- Policy defines the sensor as expected
- No `artifacts/<sensor_id>/report.json` found

### cockpit.invalid_receipt

Emitted when a receipt exists but cannot be parsed.

```json
{
  "severity": "error",
  "code": "cockpit.invalid_receipt",
  "message": "Failed to parse receipt for sensor 'builddiag': expected object at line 1",
  "data": {
    "error": "expected object at line 1 column 1",
    "path": "artifacts/builddiag/report.json"
  }
}
```

**Severity:** Always `"error"`

**When it appears:**
- Receipt file exists but is not valid JSON
- Receipt JSON doesn't match `sensor.report.v1` schema
- Receipt has unknown top-level fields (strict parsing)

### cockpit.receipt_inconsistent

Emitted when a receipt's claimed counts don't match its findings.

```json
{
  "severity": "info",
  "code": "cockpit.receipt_inconsistent",
  "message": "Receipt counts for 'builddiag' don't match findings: claimed 3 errors, found 2"
}
```

**Severity:** Always `"info"` (advisory)

**Behavior:** cockpitctl uses computed counts for aggregation, not claimed counts.

### cockpit.receipt_oversized

Emitted when a receipt exceeds size limits.

```json
{
  "severity": "error",
  "code": "cockpit.receipt_oversized",
  "message": "Receipt for 'builddiag' exceeds size limit (5MB > 2MB)"
}
```

**Severity:** Always `"error"`

**When it appears:**
- Receipt file size exceeds configured maximum (default 2MB)
- Receipt is not processed to avoid memory exhaustion

### cockpit.path_traversal

Emitted when a sensor ID contains path traversal.

```json
{
  "severity": "error",
  "code": "cockpit.path_traversal",
  "message": "Sensor ID '../escape' contains path traversal"
}
```

**Severity:** Always `"error"`

**When it appears:**
- Sensor ID in config or discovered path contains `..`
- The sensor is skipped for security

## Code Stability

Finding codes are part of cockpitctl's API:
- Codes are never renamed
- Deprecated codes may be aliased but continue to work
- New codes may be added in minor versions

## See Also

- [Safety Limits](safety-limits.md) - Size caps and path rules
- [Debug Failing Ingest](../how-to/debug-failing-ingest.md) - Troubleshooting
