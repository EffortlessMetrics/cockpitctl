# Fuzzing

This directory contains fuzz targets for cockpitctl using [cargo-fuzz](https://github.com/rust-fuzz/cargo-fuzz) (libFuzzer).

## Prerequisites

Install cargo-fuzz (requires nightly Rust):

```bash
rustup install nightly
cargo +nightly install cargo-fuzz
```

## Available Targets

### parse_receipt

Fuzzes JSON deserialization of `SensorReport` (the input receipt envelope).

**Goal:** Ensure that no combination of bytes causes a panic in receipt parsing.

```bash
cargo +nightly fuzz run parse_receipt
```

### parse_config

Fuzzes TOML deserialization of `CockpitConfig` (cockpit.toml policy config).

**Goal:** Ensure that no combination of bytes causes a panic in config parsing.

```bash
cargo +nightly fuzz run parse_config
```

## Running Fuzz Tests

Basic usage (runs indefinitely until stopped or crash found):

```bash
cargo +nightly fuzz run parse_receipt
cargo +nightly fuzz run parse_config
```

Run for a specific duration:

```bash
cargo +nightly fuzz run parse_receipt -- -max_total_time=60
cargo +nightly fuzz run parse_config -- -max_total_time=60
```

Run with multiple jobs (parallel fuzzing):

```bash
cargo +nightly fuzz run parse_receipt -- -jobs=4 -workers=4
```

## Corpus

Seed inputs are stored in `corpus/<target>/`:

- `corpus/parse_receipt/` - Valid SensorReport JSON samples
- `corpus/parse_config/` - Valid CockpitConfig TOML samples

The fuzzer will use these as starting points and mutate them to explore edge cases.

To add new seeds from the fixtures:

```bash
cp fixtures/happy_path/artifacts/*/report.json fuzz/corpus/parse_receipt/
cp fixtures/*/cockpit.toml fuzz/corpus/parse_config/
```

## Crash Artifacts

When a crash is found, the input that caused it is saved to `artifacts/<target>/`:

```bash
# Reproduce a crash
cargo +nightly fuzz run parse_receipt artifacts/parse_receipt/crash-<hash>
```

## Coverage

To generate coverage reports (useful for understanding what code paths the fuzzer is exercising):

```bash
cargo +nightly fuzz coverage parse_receipt
```

## What the Fuzzer Tests

### parse_receipt

- JSON parsing robustness (malformed JSON, truncated input, nested structures)
- Serde deserialization of all SensorReport fields
- Edge cases: empty strings, huge numbers, deeply nested data
- UTF-8 boundary conditions
- Serialization round-trip stability

### parse_config

- TOML parsing robustness (malformed TOML, invalid syntax)
- Serde deserialization of CockpitConfig and nested types
- Edge cases in policy values (max_highlights, section_order, etc.)
- Sensor policy parsing (blocking, missing, section fields)
- Serialization round-trip stability

## Integration with CI

For CI integration, run fuzzing for a bounded time:

```bash
# Quick smoke test (60 seconds)
cargo +nightly fuzz run parse_receipt -- -max_total_time=60

# Or check that it builds and seeds are valid
cargo +nightly fuzz build
cargo +nightly fuzz run parse_receipt corpus/parse_receipt/ -- -runs=0
```

## Invariants Being Tested

From CLAUDE.md, receipts are untrusted input. The fuzzer helps ensure:

1. JSON parsing must not panic
2. Memory usage is bounded (via file size caps at IO boundary)
3. Invalid input yields proper errors instead of crashes

## Tips

- Start with a short run to verify setup works
- Use `RUST_BACKTRACE=1` for better crash diagnostics
- The fuzzer automatically minimizes crash inputs
- Regularly sync corpus between developers for better coverage
