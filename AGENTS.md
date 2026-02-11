# AGENTS.md

This file is the one-page, drift-resistant description of cockpitctl and its core contracts.

## cockpitctl in one page

### What it is

`cockpitctl` is an offline director that compiles many sensor receipts into:

- Machine evidence: `artifacts/cockpit/report.json` (`cockpit.report.v1`)
- Human surface: `artifacts/cockpit/comment.md` (deterministic markdown)
- CI semantics: exit code (`0` pass, `2` policy fail, `1` runtime error)

### What it is not

- Not a sensor runner
- Not a GitHub API client
- Not an orchestrator (that is CI/Flow Studio)

## The three contracts that matter

### 1) Receipt contract (inputs)

Sensors write exactly one canonical receipt each:

```
artifacts/<sensor_id>/report.json   # sensor.report.v1
```

Lax mode means: skip JSON Schema validation. It does not mean accept invalid shape.
Serde parsing can still fail and should surface as a cockpit finding.

### 2) Cockpit contract (outputs)

`cockpitctl ingest` always writes (even on exit code `2`):

```
artifacts/cockpit/report.json    # cockpit.report.v1
artifacts/cockpit/comment.md     # stable markers, budgeted
```

Everything higher-level should consume `cockpit.report.v1` as the canonical evidence object.
The comment is just one renderer.

### 3) Precedence contract (policy vs CLI)

Config is the default. CLI only overrides when explicitly provided.

- `cockpit.toml` supplies defaults (`schema_validation = "lax"` unless set)
- CLI `--schema-validation ...` overrides only if the user actually passes it

## How it interfaces with the cockpit stack

`cockpitctl` is the bottom layer. It produces a stable cockpit receipt that everything else
can treat as evidence:

- PR bots: post `comment.md` (or render from `report.json`)
- Dashboards: ingest `cockpit.report.v1`
- Flow Studio: store/compare/diff cockpit receipts across runs, attach provenance, aggregate runs

The rule: higher layers should not parse 10 sensor formats. They should parse one cockpit format.

## Repo shape and responsibilities (clean boundaries)

Workspace layout:

- `cockpitctl-types`: stable DTOs, rankings, embedded schemas
- `cockpitctl-domain`: pure determinism, selection logic, normalization
- `cockpitctl-ingest`: orchestration + ports + precedence + exit semantics
- `cockpitctl-io`: filesystem adapters + safety limits + traversal protection
- `cockpitctl-render`: markdown renderer + budgets + stable markers
- `cockpitctl-cli`: clap + subcommands (only clap crate)
- `xtask`: schema checks, fixture tooling

Key internal invariants (what tests are buying you):

- Deterministic ordering and capping
- Stable PR surface (goldens + BDD)
- Safety is "controlled findings", not crashes (oversize/traversal)

## Definition of done (operational)

1. Strict validation works from any working directory
   - No runtime dependence on repo-root `schemas/`; use embedded schema bytes
2. Warnings really are errors
   - No `#[allow(deprecated)]` hiding harness issues
3. Packaging is boring
   - `cargo package --list -p cockpitctl` ships no fixtures or docs junk
   - Embedded schemas are included in `cockpitctl-types`
   - CI enforces `cargo run -p xtask -- schema-sync-check`
4. The precedence contract is reflected everywhere
   - Code, tests, docs, and BDD features

## Ops appendix (agent essentials)

Build and test:

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

CLI usage:

```bash
cockpitctl ingest --artifacts artifacts --config cockpit.toml
cockpitctl init --path cockpit.toml          # Write starter config
cockpitctl validate --input report.json       # Validate receipt/report
```

Determinism requirements (must be byte-stable):

- Sensor discovery: lexical order
- Findings sort: severity desc -> sensor_id -> path -> line -> code -> message
- Highlights sort: severity desc -> blocking-first -> sensor_id -> path -> line -> code

Safety constraints (untrusted receipts):

- Refuse path traversal (`..`) in sensor IDs
- Avoid following symlinks out of artifacts root
- Cap receipt file size (2MB default)
- Cap number of receipts processed

Fixture regeneration:

```bash
cargo run -p xtask -- fixtures-help
```
