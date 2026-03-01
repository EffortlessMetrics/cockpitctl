# Gemini Context: cockpitctl

This file provides context for Gemini when working with the `cockpitctl` repository.

## Project Overview

**cockpitctl** is the "director" for the Effortless Metrics PR cockpit. It is an offline CLI tool that ingests receipts (reports) from various sensors and produces a single, deterministic merge decision surface (report + PR comment).

-   **Role:** Director / Aggregator.
-   **Input:** Sensor receipts (`artifacts/<sensor>/report.json`).
-   **Policy:** Composition rules defined in `cockpit.toml`.
-   **Output:**
    -   Machine: `artifacts/cockpit/report.json` (`cockpit.report.v1`)
    -   Human: `artifacts/cockpit/comment.md` (Markdown summary)
-   **Constraints:** No network calls, no sensor execution, strict resource budgets, untrusted input handling.

## Architecture

The project follows a **Hexagonal / Clean Architecture** with a workspace of microcrates. Dependencies point inward.

### Crate Structure
*   **`cockpitctl-types`**: Core DTOs, stable IDs, embedded schemas. No external deps except serde/time.
*   **`cockpitctl-domain`**: Pure business logic (policy evaluation, ordering, normalization).
*   **`cockpitctl-domain-buildfix`**: Buildfix domain logic.
*   **`cockpitctl-domain-signing`**: Policy signing domain logic.
*   **`cockpitctl-domain-trend`**: Trend analysis domain logic.
*   **`cockpitctl-ingest`**: Use case boundary. Defines ports (`ReceiptSource`, `PolicySource`, `OutputSink`).
*   **`cockpitctl-io`**: Adapters implementing ports (filesystem access, safety limits).
*   **`cockpitctl-io-buildfix`**: Buildfix I/O adapters.
*   **`cockpitctl-io-hooks`**: Hook execution adapters.
*   **`cockpitctl-io-policy-signing`**: Policy signing I/O adapters.
*   **`cockpitctl-io-schema`**: Schema validation adapters.
*   **`cockpitctl-render`**: Markdown renderer for the PR comment.
*   **`cockpitctl-sarif`**: SARIF v2.1.0 export.
*   **`cockpitctl-conform`**: Validation library (schema, path hygiene).
*   **`cockpitctl-feature-state`**: Feature flag state management.
*   **`cockpitctl-feature-grid`**: BDD feature toggle grid.
*   **`cockpitctl-core`**: Facade crate — re-exports all microcrates as one dependency.
*   **`cockpitctl-cli`**: Binary entry point (using `clap`). Wires adapters to the use case.
*   **`conformctl`**: Standalone conformance checking binary.
*   **`xtask`**: Dev tooling (schema sync, fixture generation).

## Key Contracts

1.  **Receipt Contract (Input):** Sensors write to `artifacts/<sensor_id>/report.json`.
2.  **Cockpit Contract (Output):** `cockpitctl` always produces `cockpit.report.v1` and `comment.md`.
3.  **Policy Contract:** `cockpit.toml` defines acceptance criteria.

**Verdict States:** `pass`, `warn`, `fail`, `skip`.
**Exit Codes:** `0` (pass), `2` (policy fail), `1` (runtime error).

## Development Workflow

### Build & Run
-   **Build:** `cargo build`
-   **Run:** `cargo run -p cockpitctl -- ingest --artifacts artifacts --config cockpit.toml`

### Testing & Verification
-   **Unit Tests:** `cargo test --workspace --all-targets`
-   **Golden Tests:** `cargo test -p cockpitctl --test ingest_golden` (Snapshots)
-   **BDD Tests:** `cargo test -p cockpitctl --test bdd`
-   **Formatting:** `cargo fmt --all -- --check`
-   **Linting:** `cargo clippy --workspace --all-targets -- -D warnings`
-   **Conformance:** `cargo run -p xtask -- conform-dir --dir artifacts --all --validate-cockpit`

### Fixtures
Regenerate fixtures using `xtask`:
```bash
cargo run -p xtask -- fixtures-help
```

## Key Invariants

### Determinism
Output must be byte-stable for identical inputs.
-   **Sensor Discovery:** Lexical order.
-   **Findings Sort:** `severity desc` -> `sensor_id` -> `path` -> `line` -> `code` -> `message`.
-   **Highlights Sort:** `severity desc` -> `blocking-first` -> `sensor_id` -> `path` -> `line` -> `code`.

### Safety
Receipts are treated as untrusted input.
-   No path traversal (`..`).
-   No following symlinks outside the artifact root.
-   Strict file size limits (default 2MB).
-   Strict processing time/count limits.

## Project Status

**Completed Milestones (M0-M5):**
-   Core Ingest & Rendering
-   Determinism & Safety Caps
-   Conformance Harness & BDD
-   Distribution & Documentation

**Recently Completed:**
-   19-crate microcrate extraction with feature-gated builds
-   1200+ tests across all crates (unit, integration, golden, BDD, E2E)
-   Code coverage with cargo-tarpaulin, cargo-deny for license/advisory checking
-   GitHub Action with schema-validation, annotations, and SHA256 checksum verification
-   SARIF export, trend tracking, buildfix integration, policy signing

**Long-term / Exploratory:**
-   Receipt streaming for large monorepos
-   Web dashboard for interactive report viewing
