# Validate Receipts

This guide shows how to use cockpitctl's validate command to check receipt and report structure.

## The validate Command

```bash
cockpitctl validate --input <file> [--strict|--lax]
```

Validates that a JSON file conforms to either:
- `sensor.report.v1` (sensor receipt)
- `cockpit.report.v1` (cockpit report)

**Modes:**
- `--strict` (default): Validate against embedded JSON Schemas.
- `--lax`: Skip JSON Schema validation; only parse JSON into Rust struct shapes.

## Validating Sensor Receipts

```bash
cockpitctl validate --input artifacts/builddiag/report.json
```

**Success (strict):**
```
ok: validated as sensor.report.v1
```

**Failure:**
```
Error: Invalid receipt
  - Missing required field: verdict
  - Unknown field: extra_field (sensor.report.v1 is strict)
```

## Validating Cockpit Reports

```bash
cockpitctl validate --input artifacts/cockpit/report.json
```

The validator auto-detects the schema from the `schema` field.

## Common Validation Errors

### Missing Required Fields

```
Error: Missing required field: verdict.counts
```

**Fix:** Ensure the receipt includes all required fields:
- `schema`
- `tool` (with `name`, `version`)
- `run` (with `started_at`)
- `verdict` (with `status`, `counts`)
- `findings`

### Unknown Fields

```
Error: Unknown field: my_custom_field
```

sensor.report.v1 is strict at the top level. Tool-specific data must go in `data` fields.

**Fix:** Move custom fields into `data`:

```json
{
  "data": {
    "my_custom_field": "value"
  }
}
```

### Invalid Enum Value

```
Error: Invalid verdict.status: "error"
  Expected one of: pass, warn, fail, skip
```

**Fix:** Use only valid enum values.

### Type Mismatches

```
Error: Expected integer for verdict.counts.error, got string
```

**Fix:** Check field types against the schema.

## Batch Validation

Validate all receipts in a directory:

```bash
for f in artifacts/*/report.json; do
  echo "Validating $f"
  cockpitctl validate --input "$f" || echo "FAILED: $f"
done
```

## Pre-Commit Validation

Add validation to sensor CI:

```yaml
- name: Validate receipt
  run: |
    cockpitctl validate --input artifacts/my-sensor/report.json
```

This catches schema violations before they reach cockpitctl.

## Schema Files

The canonical schemas are:
- `contracts/schemas/sensor.report.v1.json`
- `contracts/schemas/cockpit.report.v1.json`

For full JSON Schema validation beyond cockpitctl:

```bash
# Using ajv-cli
npx ajv validate -s contracts/schemas/sensor.report.v1.json -d artifacts/sensor/report.json
```

## What validate Checks

The validate command performs:
- JSON syntax validation
- Required field presence
- Type checking
- Enum value validation
- Strict top-level field checking (in strict mode)

It does **not** check:
- Semantic consistency (counts matching findings)
- Cross-references
- Business logic rules

## Use Cases

### Sensor Development

Validate receipts as you build the sensor:

```bash
cargo run --bin my-sensor -- check > receipt.json
cockpitctl validate --input receipt.json
```

### Debugging Ingest Failures

When `cockpit.invalid_receipt` appears:

```bash
cockpitctl validate --input artifacts/failing-sensor/report.json
```

Get detailed error messages outside the ingest flow.

### CI Quality Gates

Require valid receipts before aggregation:

```yaml
- name: Validate all receipts
  run: |
    find artifacts -name "report.json" -exec cockpitctl validate --input {} \;
```

## See Also

- [Sensor Report Schema](../reference/sensor-report-schema.md) - Full schema reference
- [Write a Conformant Sensor](write-conformant-sensor.md) - Building valid sensors
- [Debug Failing Ingest](debug-failing-ingest.md) - Troubleshooting
