# Safety Limits

cockpitctl treats receipts as untrusted input. This page documents the safety measures.

## Why Untrusted?

See [Trust Boundaries](../explanation/trust-boundaries.md) for the full rationale.

In summary: sensors may be compromised, buggy, or malicious. cockpitctl must not:
- Exhaust memory
- Write outside expected paths
- Execute arbitrary code
- Leak information through path traversal

## Path Restrictions

### Path Traversal Prevention

Sensor IDs containing `..` are rejected:

```
artifacts/../../../etc/passwd   # REJECTED
artifacts/my-sensor/report.json # OK
```

If a path traversal is detected:
- The sensor is skipped
- A `cockpit.path_traversal` finding is emitted
- Processing continues with other sensors

### Symlink Handling

cockpitctl does not follow symlinks that point outside the artifacts directory:

```
artifacts/
  legit-sensor/report.json          # OK (regular file)
  symlink-sensor -> ../../../tmp/   # REJECTED (escapes root)
  internal-link -> ../other/        # OK (stays in artifacts)
```

## Size Limits

### Receipt File Size

Default maximum: **2 MB**

Receipts larger than this limit:
- Are not read
- Generate a `cockpit.receipt_oversized` finding
- Do not cause cockpitctl to crash

Configuration (future):
```toml
[policy]
max_receipt_size_bytes = 2097152  # 2MB
```

### Receipt Count

Default maximum: **100 sensors**

If more than 100 sensor directories are found:
- Only the first 100 (lexically) are processed
- A warning is logged
- Processing continues

### Finding Limits

Per-sensor finding limit: configurable via `max_per_sensor_findings`

```toml
[policy]
max_per_sensor_findings = 20
```

Findings beyond this limit:
- Are not included in highlights
- The sensor is marked as `truncated: true`
- A truncation note appears in the comment

### Highlight Limits

Global highlight limit: configurable via `max_highlights`

```toml
[policy]
max_highlights = 7
```

## Memory Bounds

cockpitctl is designed for O(receipts + findings) memory usage:

- Receipts are processed one at a time
- Large `data` payloads are not retained in memory
- Findings are capped before aggregation

Expected memory for typical PRs: < 50 MB

## No Execution

cockpitctl never executes:
- Commands from `repro` fields (display only)
- Content from `data` fields
- Scripts or binaries from artifacts

The `repro` field is rendered in the comment for humans to copy; cockpitctl does not run it.

## Graceful Degradation

When limits are hit or errors occur, cockpitctl:
1. Emits a finding describing the issue
2. Continues processing other sensors
3. Produces output with available information
4. Uses appropriate exit code

This "surface problems, don't hide them" approach prevents "green by omission."

## See Also

- [Trust Boundaries](../explanation/trust-boundaries.md) - Security model
- [Finding Codes](finding-codes.md) - Error finding details
