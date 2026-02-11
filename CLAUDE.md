# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**cockpitctl** is a "director" application that ingests sensor receipts and produces a single merge decision surface. It reads receipts from `artifacts/<sensor>/report.json`, applies composition policy from `cockpit.toml`, and outputs an aggregate report and PR comment.

Key constraints: No running sensors, no network calls, treats receipts as untrusted input, tool-specific payload is opaque.

## Build & Test Commands

```bash
# Build
cargo build

# Run all tests
cargo test --workspace --all-targets

# Format check
cargo fmt --all -- --check

# Lint (CI treats warnings as errors)
cargo clippy --workspace --all-targets -- -D warnings

# Golden/snapshot tests only
cargo test -p cockpitctl --test ingest_golden

# BDD tests (optional)
cargo test -p cockpitctl --test bdd

# Fuzzing (optional)
cargo fuzz run parse_receipt

# Mutation testing (optional, expensive)
cargo mutants --workspace
```

## Architecture

Hexagonal/Clean architecture with microcrates. Dependencies point inward; domain crates must not depend on clap, filesystem, or network.

```
cockpitctl-types    → DTOs, stable IDs, ordering helpers (no external deps except serde/time)
cockpitctl-conform  → Conformance checking library (schema validation, path hygiene, ordering, etc.)
cockpitctl-domain   → Policy evaluation, highlight selection, normalization (uses sha2/hex)
cockpitctl-ingest   → Use case boundary with ports (traits): ReceiptSource, PolicySource, OutputSink
cockpitctl-render   → PR comment renderer with stable markers and truncation
cockpitctl-io       → Filesystem adapters implementing the ports
cockpitctl-cli      → Binary entry point (clap), wires adapters to use case
conformctl          → Standalone conformance checker binary (depends only on types + conform)
xtask               → Schema checks, fixture tooling (delegates conformance to cockpitctl-conform)
```

## Key Contracts

- `schemas/sensor.report.v1.json` - Input receipt envelope
- `schemas/cockpit.report.v1.json` - Director output
- `templates/cockpit.comment.v1.md` - Comment contract

Verdict states: `pass | warn | fail | skip` (no others allowed)

Exit codes: `0` (pass), `2` (policy fail), `1` (runtime error)

## Determinism Requirements

All output must be byte-stable given identical inputs:
- Sensor discovery: lexical order
- Findings sort: `severity desc → sensor_id → path → line → code → message`
- Highlights sort: `severity desc → blocking-first → sensor_id → path → line → code`

## Safety Constraints

Receipts are untrusted:
- Refuse path traversal (`..`) in sensor IDs
- Avoid following symlinks out of artifacts root
- Cap receipt file size (2MB default)
- Cap number of receipts processed

## CLI Usage

```bash
cockpitctl ingest --artifacts artifacts --config cockpit.toml
cockpitctl init --path cockpit.toml          # Write starter config
cockpitctl validate --input report.json       # Validate receipt/report

# Standalone conformance checker (no cockpitctl workspace needed)
conformctl check --report report.json --all --sensor-id builddiag
conformctl check-dir --dir artifacts --all --validate-cockpit
```

## Fixture Regeneration

```bash
cargo run -p xtask -- fixtures-help
```
