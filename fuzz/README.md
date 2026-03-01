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

### sarif_convert

Fuzzes SARIF conversion from `CockpitReport`. Parses arbitrary bytes as a
`CockpitReport`, then converts to SARIF format via `cockpit_report_to_sarif`
and `cockpit_report_to_sarif_json`.

**Goal:** Ensure the SARIF conversion logic never panics on any valid `CockpitReport`.

```bash
cargo +nightly fuzz run sarif_convert
```

### render_comment

Fuzzes PR comment rendering from `CockpitReport`. Parses arbitrary bytes as a
`CockpitReport`, then renders a markdown comment via `render_comment`.

**Goal:** Ensure the markdown rendering logic never panics on any valid `CockpitReport`.

```bash
cargo +nightly fuzz run render_comment
```

### fuzz_sensor_id

Fuzzes sensor ID validation and path traversal checks. Exercises
`is_valid_sensor_id`, `check_sensor_id_format`, and `check_path_hygiene`
with arbitrary byte strings.

**Goal:** Ensure safety-critical ID validation functions never panic on any input.

```bash
cargo +nightly fuzz run fuzz_sensor_id
```

### fuzz_schema_validate

Fuzzes JSON schema validation with arbitrary input. Exercises `conform_single`
(full conformance checks) and `validate_cockpit_schema` against the embedded
JSON schemas.

**Goal:** Ensure schema validation logic never panics on any input.

```bash
cargo +nightly fuzz run fuzz_schema_validate
```

## Running Fuzz Tests

Basic usage (runs indefinitely until stopped or crash found):

```bash
cargo +nightly fuzz run parse_receipt
cargo +nightly fuzz run parse_config
cargo +nightly fuzz run sarif_convert
cargo +nightly fuzz run render_comment
cargo +nightly fuzz run fuzz_sensor_id
cargo +nightly fuzz run fuzz_schema_validate
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

- `corpus/parse_receipt/` — Valid `SensorReport` JSON samples and edge cases
- `corpus/parse_config/` — Valid `CockpitConfig` TOML samples and edge cases
- `corpus/sarif_convert/` — `CockpitReport` JSON for SARIF conversion (valid, minimal, many-findings, invalid)
- `corpus/render_comment/` — `CockpitReport` JSON for rendering (valid, empty, large, single-finding)
- `corpus/fuzz_sensor_id/` — Sensor ID strings (valid, traversal attacks, special chars, long, empty)
- `corpus/fuzz_schema_validate/` — JSON for schema validation (valid receipt, invalid schema, missing fields, extras, malformed)

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
- Edge cases: empty strings, huge numbers, deeply nested data, Unicode paths
- UTF-8 boundary conditions
- Serialization round-trip stability

### parse_config

- TOML parsing robustness (malformed TOML, invalid syntax)
- Serde deserialization of CockpitConfig and nested types
- Edge cases in policy values (max_highlights, section_order, etc.)
- Sensor policy parsing (blocking, missing, section fields)
- Buildfix and hooks configuration parsing
- Serialization round-trip stability

### sarif_convert

- CockpitReport deserialization followed by SARIF struct conversion
- JSON serialization of the SARIF output
- Edge cases: empty sensors/highlights, many findings, missing fields

### render_comment

- CockpitReport deserialization followed by markdown rendering
- Budget/truncation logic with large reports
- Edge cases: empty reports, single finding, missing sensors

### fuzz_sensor_id

- `is_valid_sensor_id` with arbitrary strings
- `check_sensor_id_format` with arbitrary strings
- `check_path_hygiene` with synthetic reports containing fuzzed paths
- Path traversal prevention (../, special chars, null bytes)

### fuzz_schema_validate

- `conform_single` with all conformance checks enabled
- `validate_cockpit_schema` for cockpit report validation
- Invalid JSON, wrong schema versions, missing required fields

## Integration with CI

For CI integration, run fuzzing for a bounded time:

```bash
# Quick smoke test (60 seconds per target)
for target in parse_receipt parse_config sarif_convert render_comment fuzz_sensor_id fuzz_schema_validate; do
  cargo +nightly fuzz run "$target" -- -max_total_time=60
done

# Or check that it builds and seeds are valid
cargo +nightly fuzz build
cargo +nightly fuzz run parse_receipt corpus/parse_receipt/ -- -runs=0
```

## Invariants Being Tested

From CLAUDE.md, receipts are untrusted input. The fuzzer helps ensure:

1. JSON parsing must not panic
2. TOML parsing must not panic
3. SARIF conversion must not panic on valid CockpitReport
4. Comment rendering must not panic on valid CockpitReport
5. Sensor ID validation must not panic (safety-critical for path traversal)
6. Schema validation must not panic on any input
7. Memory usage is bounded (via file size caps at IO boundary)
8. Invalid input yields proper errors instead of crashes

## Tips

- Start with a short run to verify setup works
- Use `RUST_BACKTRACE=1` for better crash diagnostics
- The fuzzer automatically minimizes crash inputs
- Regularly sync corpus between developers for better coverage
