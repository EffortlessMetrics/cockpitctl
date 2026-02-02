# cockpitctl design

This document describes **how** we meet the requirements, using a BDD + hexagonal clean architecture
approach with microcrates and heavy testing.

## Architecture style

- **Hexagonal / Clean architecture**:
  - domain logic is isolated from IO
  - dependencies point inward
  - adapters live at the edges

- **Microcrate workspace**:
  small crates with single responsibilities and clean dependency graphs.
  This keeps change localized and tests fast.

## Crate map

```
cockpitctl-types   : DTOs + stable IDs + shared ordering helpers
cockpitctl-domain  : policy evaluation + highlight selection + normalization helpers
cockpitctl-ingest  : use case boundary (ports + orchestrating domain)
cockpitctl-render  : comment renderer (contract v1)
cockpitctl-io      : filesystem adapters (receipt discovery + read/write)
cockpitctl-cli     : CLI wiring (clap) + process exit mapping
xtask              : schema + fixture tooling
```

Design rule: **domain crates must not depend on clap, filesystem, or network**.

## Core entities

### Sensor receipt (`sensor.report.v1`)

A sensor receipt is treated as an immutable record.
`cockpitctl` only relies on envelope fields:
- verdict
- findings (severity, code, message, optional location)
- tool metadata (name/version)
Everything else (`data`) is opaque.

### Policy config (`cockpit.toml`)

Policy is declarative:
- what is blocking
- what missing means
- budgets (highlights, per-sensor caps)
- section order (comment UI)
- optional label gates

### Aggregate report (`cockpit.report.v1`)

A shallow “receipt of receipts”:
- overall verdict
- per-sensor summaries
- capped highlights
- policy snapshot used to compute the verdict

## Use case: `ingest`

The `ingest` use case is implemented in `cockpitctl-ingest` as a pure orchestration function
with ports for IO.

**Ports (traits)**:
- `ReceiptSource`: list sensors, read report bytes, check for comment.md
- `PolicySource`: load and parse cockpit.toml
- `OutputSink`: write cockpit report and comment

**Flow**:
1. Load policy (optional). If absent, build a default policy from discovered receipts.
2. Determine expected sensors:
   - if policy defines sensors: those are expected
   - else: discovered receipts define expected sensors (no “missing”)
3. For each expected sensor:
   - if receipt missing: synthesize a summary + a cockpit finding (`cockpit.missing_receipt`)
   - if receipt invalid: synthesize a summary + a cockpit finding (`cockpit.invalid_receipt`)
   - else: parse receipt, normalize, cap per-sensor surfaced findings
4. Compute overall verdict:
   - consider only blocking sensors
   - apply warn-as-fail
   - label gates can effectively skip a sensor
5. Select highlights:
   - union of surfaced findings
   - dedupe by fingerprint (or derived key)
   - stable sort and cap to `max_highlights`
6. Render comment (`cockpitctl-render`) using:
   - policy section order
   - per-sensor repro lines (if provided)
   - links to artifacts
   - truncation markers
7. Write outputs and return exit code.

## Normalization and correctness rules

### Path normalization

`cockpitctl` must treat paths as protocol-level identities:
- repo-relative
- forward slashes
- no `./` prefix

It should not rewrite sensor findings, but it may normalize for display and fingerprinting.

### Counts reconciliation

Receipts are untrusted.
When `verdict.counts` disagrees with findings, record `receipt_inconsistent` as a reason
and prefer computed counts for aggregate summaries.

### Deterministic ordering

Sort keys are part of the contract.

Findings:
`severity desc → sensor_id → path → line → code → message`

Sensors:
`section order → sensor_id`

Highlights:
`severity desc → blocking-first → sensor_id → path → line → code`

### Truncation

Truncation is treated as an explicit event:
- `sensor_summary.truncated = true`
- comment shows “top N shown; see artifacts”

## Comment contract (renderer)

The renderer produces a stable PR comment:

- markers for sticky updates:
  `<!-- cockpit:begin -->` / `<!-- cockpit:end -->`
- summary table first
- highlights section second
- then sections in configured order
- each section:
  - short summary
  - links to sensor artifacts
  - optional one-line repro command

`cockpitctl` should never inline full sensor markdown.
If sensors produce `comment.md`, cockpit links to it.

## Validation mode

`cockpitctl validate` is a developer tool:
- validate a receipt against the envelope (by parsing into DTOs)
- optional JSON schema validation (feature-gated)

## Testing strategy

This repo is intentionally test-heavy.

### Unit tests (fast)
- domain rules (overall verdict, missing receipt behavior)
- ordering and stable sorting
- fingerprint derivation

### Snapshot / golden tests (most important)
- fixtures under `fixtures/**`
- `ingest` produces byte-stable:
  - cockpit.report.json
  - comment.md

### BDD (behavior)
- cucumber features in `features/`
- scenarios:
  - missing receipt produces warning/failure per policy
  - invalid receipt surfaces as cockpit finding
  - highlight capping and truncation messaging

### Property-based tests (proptest)
- ordering is stable for random finding sets
- highlight selection is deterministic
- policy evaluation is monotonic wrt additional non-blocking sensors

### Fuzzing (cargo-fuzz)
- fuzz receipt JSON parsing and diff-like large inputs
- invariants: no panics, bounded allocations, graceful error reporting

### Mutation testing (cargo-mutants)
- focus on domain logic:
  - missing receipt policy
  - warn-as-fail
  - highlight capping
  - fingerprint derivation
