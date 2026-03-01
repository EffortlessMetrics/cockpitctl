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
- `artifacts/cockpit/buildfix.apply.json` (when buildfix apply is evaluated)
- `artifacts/cockpit/policy.signature.json` (when policy signing is enabled)

## Policy (`cockpit.toml`)
See: `docs/reference/config.md` for full reference.

## Contracts
- `contracts/schemas/sensor.report.v1.json` (input bus)
- `contracts/schemas/cockpit.report.v1.json` (director output)
- `contracts/schemas/buildfix.plan.v1.json` (fix plan)
- `templates/cockpit.comment.v1.md` (comment contract)
- `contracts/docs/tokens.md` (reason token registry)
- `contracts/docs/identity-spec.md` (vocabulary and fingerprint rules)

## Repository layout

```
crates/
  cockpitctl-types     # stable DTOs + embedded schemas
  cockpitctl-domain    # pure policy logic, sorting, synthesis
  cockpitctl-ingest    # use case + ports + exit semantics
  cockpitctl-io        # filesystem adapters + safety guards
  cockpitctl-render    # deterministic markdown + annotation rendering
  cockpitctl-sarif     # SARIF v2.1.0 export
  cockpitctl-validate  # validation use case (schema + parse checks)
  cockpitctl-conform   # conformance checking library
  cockpitctl-core      # facade crate (re-exports all microcrates)
  cockpitctl-cli       # `cockpitctl` CLI package (binary + compatibility lib)
  conformctl           # standalone conformance checker CLI
xtask/                 # schema sync, conformance harness, fixture tooling
contracts/
  schemas/             # JSON Schemas (source-of-truth)
  docs/                # protocol specifications (tokens, identity)
docs/                  # user-facing documentation (Diataxis)
fixtures/              # golden fixture inputs + expected outputs
templates/             # comment contract template
fuzz/                  # cargo-fuzz harness (optional)
```

## Development (recommended)
- Unit tests: `cargo test --workspace --all-targets`
- Snapshots/goldens: `cargo test -p cockpitctl --test ingest_golden`
- BDD: `cargo test -p cockpitctl --test bdd` (optional; see `crates/cockpitctl-cli/features/`)
- Conformance: `cargo run -p xtask -- conform --report <file> --all --sensor-id <id>`
- Conformance (batch): `cargo run -p xtask -- conform-dir --dir artifacts --all --validate-cockpit`
- Schema sync: `cargo run -p xtask -- schema-sync-check`
- Fuzz: `cargo fuzz run parse_receipt` (optional)
- Mutation: `cargo mutants` (optional)

## Release validation

Before announcing a release, validate it using only published artifacts:

```bash
# Unix/macOS
./scripts/smoke-test-release.sh v0.3.0

# Windows PowerShell
.\scripts\smoke-test-release.ps1 -Tag v0.3.0
```

See [`docs/how-to/smoke-test-release.md`](docs/how-to/smoke-test-release.md) for details.

See `CHANGELOG.md` for recent changes, `ROADMAP.md` for planned work, and `docs/` for full documentation.
