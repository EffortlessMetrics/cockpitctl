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

Emitted when a receipt exists but cannot be parsed as valid JSON or doesn't match the expected serde structure.

```json
{
  "severity": "error",
  "code": "cockpit.invalid_receipt",
  "message": "Invalid receipt for sensor `builddiag` at `artifacts/builddiag/report.json`: expected object at line 1",
  "help": "Validate that the sensor wrote JSON matching sensor.report.v1."
}
```

**Severity:** Always `"error"`

**When it appears:**
- Receipt file exists but is not valid JSON
- Receipt JSON doesn't match the Rust struct structure (missing required fields)
- Occurs in lax mode or when schema validation passes but serde parsing fails

### cockpit.schema_violation

Emitted when a receipt fails JSON Schema validation (only in strict mode).

```json
{
  "severity": "error",
  "code": "cockpit.schema_violation",
  "message": "Receipt for sensor `builddiag` at `artifacts/builddiag/report.json` does not conform to sensor.report.v1 schema: /tool: \"version\" is a required property",
  "help": "Ensure the sensor output matches the JSON schema at schemas/sensor.report.v1.json."
}
```

**Severity:** Always `"error"`

**When it appears:**
- Effective schema validation is `strict` (from config or explicit CLI override)
- Receipt is valid JSON but violates the JSON Schema
- Provides detailed field-level validation errors

**Difference from invalid_receipt:**
- `schema_violation` catches schema issues early with detailed errors
- `invalid_receipt` is a fallback for parse errors (less specific)
- A receipt may pass schema validation but fail serde parsing if schemas diverge

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

### cockpit.sensors_truncated

Emitted when sensor discovery is truncated due to safety limits.

```json
{
  "severity": "warn",
  "code": "cockpit.sensors_truncated",
  "message": "Sensor discovery was truncated: processed 100 of 150 sensors found. Increase max_receipts limit if needed.",
  "help": "This is a safety limit to prevent DoS. Consider reviewing why so many sensors exist."
}
```

**Severity:** Always `"warn"`

**When it appears:**
- Number of discovered sensors exceeds `max_receipts` limit (default: 100)
- Only the first N sensors (lexically sorted) are processed

## Code Stability

Finding codes are part of cockpitctl's API:
- Codes are never renamed
- Deprecated codes may be aliased but continue to work
- New codes may be added in minor versions

## See Also

- [Safety Limits](safety-limits.md) - Size caps and path rules
- [Debug Failing Ingest](../how-to/debug-failing-ingest.md) - Troubleshooting
