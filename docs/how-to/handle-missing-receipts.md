# Handle Missing Receipts

This guide explains how to configure cockpitctl's behavior when expected sensor receipts are missing.

## The Problem

When a sensor is expected but doesn't produce a receipt:
- The sensor may have crashed
- The sensor may have been skipped
- The CI step may have failed
- The receipt path may be misconfigured

cockpitctl needs to know what to do in each case.

## Missing Behavior Options

Configure per-sensor in `cockpit.toml`:

```toml
[sensors.builddiag]
missing = "fail"    # Missing = failure

[sensors.covguard]
missing = "warn"    # Missing = warning

[sensors.perf-bench]
missing = "skip"    # Missing = silently skip
```

### fail

The strictest option. Use for critical sensors.

```toml
[sensors.builddiag]
blocking = true
missing = "fail"
```

Behavior:
- `cockpit.missing_receipt` finding with severity `error`
- Sensor status shown as `fail`
- If sensor is blocking, cockpit verdict is `fail`
- Exit code 2

Use when: The sensor must always run and missing results indicate a problem.

### warn

A middle ground. Surface the issue without failing.

```toml
[sensors.typecheck]
blocking = true
missing = "warn"
```

Behavior:
- `cockpit.missing_receipt` finding with severity `warn`
- Sensor status shown as `skip`
- Visible in comment and report
- Exit code depends on `warn_is_fail` and other sensors

Use when: Missing is noteworthy but not necessarily a failure.

### skip

Silent omission. Use for optional sensors.

```toml
[sensors.perf-bench]
blocking = false
missing = "skip"
```

Behavior:
- No finding generated
- Sensor not shown in output
- No impact on verdict

Use when: The sensor is truly optional and absence is expected.

## Common Patterns

### Required Sensors with Fallback

```toml
[sensors.builddiag]
blocking = true
missing = "fail"    # Must always run
```

### Label-Gated Sensors

```toml
[sensors.perf-bench]
blocking = false
missing = "skip"    # Expected to be absent without label
require_label = "run-perf"
```

### Informational Sensors

```toml
[sensors.env-check]
blocking = false
missing = "skip"    # Nice to have, not required
```

### New Sensors Being Rolled Out

```toml
[sensors.new-lint]
blocking = false
missing = "warn"    # Surface issues during rollout
```

Once stable, change to:

```toml
[sensors.new-lint]
blocking = true
missing = "fail"
```

## Diagnosing Missing Receipts

When a receipt is missing, check:

1. **Did the sensor run?**
   - Check CI logs for the sensor step
   - Verify the sensor command succeeded

2. **Is the path correct?**
   - Expected: `artifacts/<sensor_id>/report.json`
   - Check for typos in sensor ID

3. **Is the sensor configured?**
   - If using policy, the sensor must be in `cockpit.toml`
   - Undeclared sensors in policy mode are unknown, not missing

## No Policy Behavior

Without `cockpit.toml`:
- All receipts in `artifacts/*/report.json` are discovered
- All discovered sensors are informational
- No "missing" concept (nothing is expected)

This is useful for bootstrapping before defining policy.

## See Also

- [Config Reference](../reference/config.md) - Full policy options
- [Finding Codes](../reference/finding-codes.md) - cockpit.missing_receipt details
- [Debug Failing Ingest](debug-failing-ingest.md) - Troubleshooting
