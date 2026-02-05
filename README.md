# cockpitctl

`cockpitctl` is the **director** for the Effortless Metrics PR cockpit.

It does one job: **ingest receipts** emitted by sensors and **render one merge surface**
(one aggregate report + one PR comment) under strict budgets.

## What it is
- Ingest + normalize receipts (`artifacts/*/report.json`)
- Apply composition policy (`cockpit.toml`)
- Produce:
  - `artifacts/cockpit/report.json` (`cockpit.report.v1`)
  - `artifacts/cockpit/comment.md` (comment contract v1)

## What it is not
- Not a runner/orchestrator (no installing tools, no running sensors)
- Not a GitHub bot (workflow owns posting comments/checks)
- Not an adapter soup (tool-specific payload is opaque; contract lives in the envelope)

## Quickstart (local)

```bash
# from repo root
cockpitctl ingest --artifacts artifacts --config cockpit.toml
```

Outputs:
- `artifacts/cockpit/report.json`
- `artifacts/cockpit/comment.md`

## Policy (`cockpit.toml`)
See: `docs/config.md` and `cockpit.toml.example`.

## Contracts
- `schemas/sensor.report.v1.json` (input bus)
- `schemas/cockpit.report.v1.json` (director output)
- `templates/cockpit.comment.v1.md` (comment contract)

## Repository layout

```
crates/
  cockpitctl-types     # DTOs (sensor + cockpit), stable IDs
  cockpitctl-domain    # policy evaluation, ordering, highlight selection
  cockpitctl-ingest    # use case + ports (clean/hexagonal boundary)
  cockpitctl-render    # PR comment renderer (budgeted)
  cockpitctl-io        # filesystem adapters (read receipts, write outputs)
  cockpitctl-cli       # `cockpitctl` binary (clap)
    features/          # cucumber feature files
xtask/                 # schema + fixture tooling
docs/                  # requirements/design/architecture/implementation plan
schemas/               # JSON Schemas (source-of-truth)
fixtures/              # golden fixture inputs + expected outputs
templates/             # comment contract template
fuzz/                  # cargo-fuzz harness (optional)
```

## Development (recommended)
- Unit tests: `cargo test`
- Snapshots/goldens: `cargo test -p cockpitctl --test ingest_golden`
- BDD: `cargo test -p cockpitctl --test bdd` (optional; see `crates/cockpitctl-cli/features/`)
- Fuzz: `cargo fuzz run parse_receipt` (optional)
- Mutation: `cargo mutants` (optional)

See `docs/implementation_plan.md` for the staged plan and “Definition of Done”.
