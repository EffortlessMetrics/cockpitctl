# Debug Failing Ingest

This guide helps troubleshoot common cockpitctl issues.

## Exit Code 1: Runtime Error

Exit code 1 means cockpitctl itself failed.

### Cannot Find Config

```
Error: Failed to load config from 'cockpit.toml': No such file
```

**Solutions:**
- Verify `cockpit.toml` exists at the specified path
- Use `--config` to specify a different path
- Run without config (discovery mode): cockpitctl works without `cockpit.toml`

### Cannot Read Artifacts

```
Error: Failed to read artifacts directory: Permission denied
```

**Solutions:**
- Check directory permissions
- Verify the path exists: `ls -la artifacts/`
- Use `--artifacts` to specify correct path

### Cannot Write Output

```
Error: Failed to write report: artifacts/cockpit/report.json
```

**Solutions:**
- Ensure `artifacts/cockpit/` directory exists or can be created
- Check write permissions
- Use `--artifacts` to point to a writable artifacts root

## Exit Code 2: Policy Failure

Exit code 2 means policy evaluation failed (expected behavior).

### Blocking Sensor Failed

Check the comment or report for which sensor failed:

```json
{
  "sensors": [
    {
      "id": "builddiag",
      "blocking": true,
      "verdict": { "status": "fail" }
    }
  ]
}
```

**Solutions:**
- Fix the issues the sensor found
- Temporarily make the sensor non-blocking (not recommended)
- Check if the sensor result is valid

### Missing Required Receipt

Look for `cockpit.missing_receipt`:

```json
{
  "highlights": [
    {
      "finding": {
        "code": "cockpit.missing_receipt",
        "message": "Expected receipt from sensor 'builddiag'"
      }
    }
  ]
}
```

**Solutions:**
- Ensure the sensor ran: check CI logs
- Verify receipt path: `artifacts/builddiag/report.json`
- Check sensor ID matches config exactly
- Change `missing` policy if receipt is legitimately absent

### warn_is_fail Triggered

If sensors only warn but the build fails:

```toml
[policy]
warn_is_fail = true  # Warnings from blocking sensors cause failure
```

**Solutions:**
- Fix the warnings
- Set `warn_is_fail = false` if warnings are acceptable
- Make the sensor non-blocking

## Invalid Receipt Errors

### Parse Errors

```json
{
  "finding": {
    "code": "cockpit.invalid_receipt",
    "message": "Failed to parse receipt for 'builddiag': expected object at line 1"
  }
}
```

**Solutions:**
- Validate the receipt JSON: `jq . artifacts/builddiag/report.json`
- Check for truncated files
- Verify the sensor is producing valid `sensor.report.v1`

### Schema Mismatches

```json
{
  "finding": {
    "code": "cockpit.schema_violation",
    "message": "schema violation: /verdict is required"
  }
}
```

**Solutions:**
- Run strict validation directly: `cockpitctl validate --input artifacts/builddiag/report.json --strict`
- Ensure sensor version matches expected schema
- Fix the sensor output shape, then re-run ingest

## Path Traversal Errors

```json
{
  "finding": {
    "code": "cockpit.path_traversal",
    "message": "Sensor ID '../escape' contains path traversal"
  }
}
```

**Cause:** Sensor ID in config or discovered path contains `..`

**Solutions:**
- Fix the sensor ID in `cockpit.toml`
- Investigate how the path was created (possible attack)

## Oversized Receipt

```json
{
  "finding": {
    "code": "cockpit.receipt_oversized",
    "message": "Receipt for 'big-sensor' exceeds size limit (5MB > 2MB)"
  }
}
```

**Solutions:**
- Reduce receipt size (remove verbose data payloads)
- Increase size limit in policy when needed:
  ```toml
  [policy]
  max_receipt_size_bytes = 5242880 # 5 MB
  ```
- Check for runaway data generation in sensor

## Collect Debug Signals

Use strict validate and inspect cockpit outputs:

```bash
# Validate one receipt in strict mode
cockpitctl validate --input artifacts/builddiag/report.json --strict

# Run ingest and inspect generated highlights
cockpitctl ingest --artifacts artifacts --config cockpit.toml
jq '.highlights[]?.finding.code' artifacts/cockpit/report.json
```

## Validate Individual Receipts

Test receipt parsing in isolation:

```bash
cockpitctl validate --input artifacts/builddiag/report.json
```

This helps isolate whether the issue is:
- Receipt format (validate fails)
- Policy configuration (validate passes, ingest fails)

## Check File Existence

```bash
# List all receipts
ls -la artifacts/*/report.json

# Check specific sensor
cat artifacts/builddiag/report.json | jq .verdict
```

## Compare to Expected

If output changed unexpectedly:

```bash
diff expected/report.json artifacts/cockpit/report.json
```

Look for:
- New/missing sensors
- Changed verdicts
- Different highlight selection

## See Also

- [Exit Codes](../reference/exit-codes.md) - Exit code meanings
- [Finding Codes](../reference/finding-codes.md) - cockpit.* findings
- [Validate Receipts](validate-receipts.md) - Using the validate command
