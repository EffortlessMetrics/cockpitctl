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

### fuzz_domain_pipeline

Fuzzes the full domain pipeline: `summarize_sensor_report` → `select_highlights`
→ `build_cockpit_report`. Parses arbitrary bytes as a `SensorReport`, then runs
the complete summarization, highlight selection, and report construction chain.

**Goal:** Ensure the domain pipeline (sort, cap, fingerprint, verdict aggregation)
never panics on any valid `SensorReport`.

```bash
cargo +nightly fuzz run fuzz_domain_pipeline
```

### fuzz_render_annotations

Fuzzes annotation rendering paths: `render_annotations`,
`render_github_annotations`, and `append_comment_sections`. Parses arbitrary
bytes as a `CockpitReport`, then exercises all annotation rendering code paths.

**Goal:** Ensure annotation rendering and comment section appending never panic
on any valid `CockpitReport`.

```bash
cargo +nightly fuzz run fuzz_render_annotations
```

### fuzz_fingerprint

Fuzzes fingerprint derivation and finding sort logic: `derive_fingerprint`,
`finding_sort_key`, `sort_findings`, `cap_findings`, and `compute_counts`.
Accepts both single `Finding` JSON and `Vec<Finding>` arrays.

**Goal:** Ensure deterministic ordering and fingerprinting never panic on any
valid `Finding` data.

```bash
cargo +nightly fuzz run fuzz_fingerprint
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
cargo +nightly fuzz run fuzz_domain_pipeline
cargo +nightly fuzz run fuzz_render_annotations
cargo +nightly fuzz run fuzz_fingerprint
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
- `corpus/fuzz_domain_pipeline/` — `SensorReport` JSON for full domain pipeline (valid, inconsistent counts, edge cases)
- `corpus/fuzz_render_annotations/` — `CockpitReport` JSON for annotation rendering (empty, truncated, special chars, blocking)
- `corpus/fuzz_fingerprint/` — `Finding` JSON for fingerprint/sort logic (single, arrays, duplicates, edge cases)

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

### fuzz_domain_pipeline

- Full domain pipeline: summarize → select highlights → build report
- Finding sort, cap, and fingerprint derivation on untrusted SensorReport data
- Inconsistent verdict counts detection and handling
- Overall verdict aggregation from sensor summaries

### fuzz_render_annotations

- `render_annotations` with arbitrary highlight data
- `render_github_annotations` (workflow command format)
- `append_comment_sections` with rendered comment + arbitrary sections
- Edge cases: empty highlights, special characters in messages, truncation

### fuzz_fingerprint

- `derive_fingerprint` with arbitrary sensor ID and Finding data
- `finding_sort_key` derivation for deterministic ordering
- `sort_findings` on Vec<Finding> (sort stability)
- `cap_findings` at various limits (0, 1, 100, usize::MAX)
- `compute_counts` on arbitrary finding slices

## Integration with CI

For CI integration, run fuzzing for a bounded time:

```bash
# Quick smoke test (60 seconds per target)
for target in parse_receipt parse_config sarif_convert render_comment fuzz_sensor_id fuzz_schema_validate fuzz_domain_pipeline fuzz_render_annotations fuzz_fingerprint; do
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
7. Domain pipeline (sort, cap, fingerprint, verdict) must not panic
8. Annotation rendering must not panic on any valid CockpitReport
9. Fingerprint derivation must not panic on any valid Finding
10. Memory usage is bounded (via file size caps at IO boundary)
11. Invalid input yields proper errors instead of crashes

## Tips

- Start with a short run to verify setup works
- Use `RUST_BACKTRACE=1` for better crash diagnostics
- The fuzzer automatically minimizes crash inputs
- Regularly sync corpus between developers for better coverage
