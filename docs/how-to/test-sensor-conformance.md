# Test Sensor Conformance

This guide shows how to verify your sensor produces correct, conformant receipts.

## Why Conformance Matters

Conformance ensures:
- cockpitctl can parse your receipts
- Output is deterministic
- Finding codes are stable
- The ecosystem stays coherent as sensors evolve

## Conformance Checklist

A receipt is conformant if it:

- [ ] Matches `sensor.report.v1` envelope shape
- [ ] Only extends via `data` fields
- [ ] Uses stable finding codes
- [ ] Is deterministic for fixed input
- [ ] Doesn't write outside its artifacts directory

## Shape Test

Verify receipts parse correctly:

```rust
#[test]
fn receipt_parses_as_sensor_report() {
    let json = include_str!("fixtures/sample_receipt.json");
    let report: SensorReport = serde_json::from_str(json)
        .expect("should parse as SensorReport");

    assert_eq!(report.schema, "sensor.report.v1");
    assert_eq!(report.tool.name, "my-sensor");
}
```

Or use cockpitctl:

```bash
cockpitctl validate --input artifacts/my-sensor/report.json
```

## Golden Test

Compare output against a known-good receipt:

```rust
#[test]
fn golden_receipt() {
    // Run sensor on fixed input
    let output = run_sensor("fixtures/test-repo");

    // Compare to expected
    let expected = include_str!("expected/report.json");
    assert_eq!(output, expected);
}
```

Key points:
- Use a fixed, committed test fixture
- Commit the expected output
- Diff failures show exactly what changed

## Determinism Test

Verify repeated runs produce identical output:

```rust
#[test]
fn deterministic_output() {
    let run1 = run_sensor("fixtures/test-repo");
    let run2 = run_sensor("fixtures/test-repo");

    assert_eq!(run1, run2, "Output should be identical");
}
```

## Finding Code Stability

Track finding codes in a file:

```
# finding-codes.txt
my-sensor.coverage.below-threshold
my-sensor.coverage.no-tests
my-sensor.style.long-function
```

Test that codes don't disappear:

```rust
#[test]
fn finding_codes_stable() {
    let expected_codes: HashSet<_> = include_str!("finding-codes.txt")
        .lines()
        .filter(|l| !l.starts_with('#'))
        .collect();

    let receipt = run_sensor("fixtures/all-findings");
    let actual_codes: HashSet<_> = receipt.findings
        .iter()
        .map(|f| f.code.as_str())
        .collect();

    let removed: Vec<_> = expected_codes.difference(&actual_codes).collect();
    assert!(removed.is_empty(), "Codes were removed: {:?}", removed);
}
```

## Fuzz Testing

Fuzz input parsing to catch panics:

```rust
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(input) = std::str::from_utf8(data) {
        // Should not panic
        let _ = parse_input(input);
    }
});
```

Run with cargo-fuzz:

```bash
cargo +nightly fuzz run parse_input
```

## Integration with cockpitctl

Test that cockpitctl can process your receipts:

```bash
# Setup
mkdir -p artifacts/my-sensor
my-sensor check > artifacts/my-sensor/report.json

# Verify ingest works
cockpitctl ingest --artifacts artifacts

# Check exit code
if [ $? -eq 1 ]; then
    echo "Runtime error - receipt may be invalid"
    exit 1
fi
```

## CI Integration

Add conformance tests to sensor CI:

```yaml
name: Conformance

on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Run sensor on fixture
        run: my-sensor check fixtures/test-repo

      - name: Validate receipt shape
        run: cockpitctl validate --input artifacts/my-sensor/report.json

      - name: Compare to golden
        run: diff expected/report.json artifacts/my-sensor/report.json
```

## Schema Validation

For strict schema validation beyond cockpitctl:

```bash
# Install ajv-cli
npm install -g ajv-cli

# Validate against schema
ajv validate \
  -s node_modules/cockpitctl/schemas/sensor.report.v1.json \
  -d artifacts/my-sensor/report.json
```

## Regression Testing

When fixing bugs in your sensor:

1. Create a fixture that reproduces the bug
2. Capture the (fixed) expected output
3. Commit both as a regression test

```rust
#[test]
fn regression_issue_42() {
    // This fixture triggered the bug
    let output = run_sensor("fixtures/regression-42");
    let expected = include_str!("expected/regression-42.json");
    assert_eq!(output, expected);
}
```

## Version Compatibility

When updating sensor versions:

1. Run both versions on the same input
2. Diff the outputs
3. Verify changes are intentional
4. Update golden tests

```bash
# Old version
git checkout v1.0.0
my-sensor check fixtures/test-repo > old.json

# New version
git checkout main
my-sensor check fixtures/test-repo > new.json

# Compare
diff old.json new.json
```

## See Also

- [Write a Conformant Sensor](write-conformant-sensor.md) - Building sensors
- [Sensor Report Schema](../reference/sensor-report-schema.md) - Schema reference
- [Validate Receipts](validate-receipts.md) - Using the validate command
