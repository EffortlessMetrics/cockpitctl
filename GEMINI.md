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
*   **`cockpitctl-ingest`**: Use case boundary. Defines ports (`ReceiptSource`, `PolicySource`, `OutputSink`).
*   **`cockpitctl-io`**: Adapters implementing ports (filesystem access, safety limits).
*   **`cockpitctl-render`**: Markdown renderer for the PR comment.
*   **`cockpitctl-conform`**: Validation library (schema, path hygiene).
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

**Current Focus (Near-term):**
-   Configurable receipt size limits.
-   `cockpitctl explain` command.
-   Reusable GitHub Action.
-   GitHub Annotation output.
