# Determinism Design

cockpitctl guarantees byte-stable output. This document explains why that matters and how it's achieved.

## Why Determinism Matters

### Reproducible Builds

Given the same inputs:
```
artifacts/ (unchanged)
cockpit.toml (unchanged)
```

cockpitctl produces:
```
artifacts/cockpit/report.json (identical)
artifacts/cockpit/comment.md (identical)
exit code (identical)
```

This enables:
- Debugging: "Run it again with the same inputs"
- Caching: If inputs match, outputs can be cached
- Verification: Compare outputs across environments

### Stable PR Comments

Without determinism, PR comments flicker:
- Finding order changes randomly
- GitHub re-renders the comment
- Reviewers lose their place
- Noise obscures signal

With determinism:
- Comments only change when inputs change
- Updates are meaningful diffs
- Review history is preserved

### Meaningful Output Diffs

When outputs are deterministic, you can diff them:

```bash
diff old/report.json new/report.json
```

Changes in the diff represent real changes in inputs or policy, not random noise.

### Easier Debugging

Non-determinism is a debugging nightmare:
- "It worked yesterday"
- "It fails on CI but passes locally"
- "The output is different every time"

Determinism eliminates a class of bugs.

## Sources of Non-Determinism

Non-determinism sneaks in through:

1. **Hash table iteration**: `HashMap` iteration order is randomized
2. **File system order**: `readdir` order varies by filesystem
3. **Timestamp variations**: "Generated at" timestamps differ
4. **Floating point**: Different hardware produces different results
5. **Random IDs**: UUIDs, random fingerprints

cockpitctl addresses each of these.

## How Determinism is Achieved

### Explicit Sorting

All collections are sorted before output:

```rust
// Not this (non-deterministic):
for sensor in sensors { ... }

// This (deterministic):
let mut sensors: Vec<_> = sensors.into_iter().collect();
sensors.sort_by_key(|s| &s.id);
for sensor in sensors { ... }
```

Sort keys are documented in [Determinism Reference](../reference/determinism.md).

### Lexical Sensor Discovery

Sensors are discovered in lexical order:

```rust
let mut entries: Vec<_> = fs::read_dir(artifacts)?.collect();
entries.sort_by_key(|e| e.file_name());
```

This is independent of filesystem implementation.

### No Timestamps in Output

The output does not include:
- "Generated at" timestamps
- Random run IDs
- Anything that varies without input changes

The `run.started_at` field reflects when sensors ran, not when cockpitctl ran.

### Derived Fingerprints

When findings lack fingerprints, they're derived deterministically:

```rust
let fingerprint = sha256(
    sensor_id + code + message + path + line
);
```

Same inputs always produce the same fingerprint.

### Stable JSON Formatting

JSON output uses:
- 2-space indentation
- Keys in definition order (not sorted)
- No trailing whitespace
- Single trailing newline

This is enforced by consistent serialization settings.

## Testing Determinism

### Golden Tests

The primary determinism test: snapshot comparison.

```rust
#[test]
fn ingest_happy_path() {
    let output = run_ingest("fixtures/happy_path");
    assert_eq!(output.report, include_str!("expected/report.json"));
    assert_eq!(output.comment, include_str!("expected/comment.md"));
}
```

If the output changes, the test fails.

### Property Tests

Property-based tests verify ordering is stable:

```rust
#[test]
fn findings_order_is_stable(findings: Vec<Finding>) {
    let sorted1 = sort_findings(findings.clone());
    let sorted2 = sort_findings(findings);
    assert_eq!(sorted1, sorted2);
}
```

Run with thousands of random inputs.

### Mutation Testing

Mutation testing catches ordering bugs:

```bash
cargo mutants --workspace
```

If a mutation removes a sort and tests still pass, there's a gap.

## The Cost

Determinism has a cost:
- Explicit sorting instead of hash-based lookups
- More careful data structure choices
- Testing overhead

For cockpitctl, this cost is acceptable. The benefits (stable comments, reproducible builds, easier debugging) outweigh the overhead.

## Exceptions

Some things are intentionally non-deterministic:

- **Verbose logging**: Timing information, debug output
- **Error messages**: May include system-specific details

These don't appear in the output files.

## See Also

- [Determinism Reference](../reference/determinism.md) - Sort keys and ordering
- [Why cockpitctl](why-cockpitctl.md) - Design philosophy
