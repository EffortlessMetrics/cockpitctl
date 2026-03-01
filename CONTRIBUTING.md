# Contributing to cockpitctl

Welcome, and thank you for considering a contribution to cockpitctl!

cockpitctl is an offline director that compiles sensor receipts into deterministic
merge evidence — one report, one PR comment, one exit code. See the
[README](README.md) for the full project overview.

## Getting Started

### Prerequisites

- **Rust 1.92+** — install via [rustup](https://rustup.rs/). The repo pins the
  toolchain in `rust-toolchain.toml`, so `rustup` will install the right version
  automatically.
- **Git**

### Clone and build

```bash
git clone https://github.com/EffortlessMetrics/cockpitctl.git
cd cockpitctl
cargo build
```

### Run tests

```bash
cargo test --workspace --all-targets
```

## Development Workflow

1. **Create a branch** from `main`:
   ```bash
   git checkout -b my-feature main
   ```

2. **Make changes** following the architecture (see [Architecture Guide](#architecture-guide) below).

3. **Format**:
   ```bash
   cargo fmt --all
   ```

4. **Lint** (CI treats warnings as errors):
   ```bash
   cargo clippy --workspace --all-targets -- -D warnings
   ```

5. **Test**:
   ```bash
   cargo test --workspace --all-targets
   ```

6. **Schema sync check** (required if you touch contracts or embedded schemas):
   ```bash
   cargo run -p xtask -- schema-sync-check
   ```

7. **Submit a PR** against `main`.

## Architecture Guide

cockpitctl uses a hexagonal (clean) architecture split across 20 workspace
members. Dependencies point **inward** — domain crates never depend on CLI,
filesystem, or network adapters.

```
Layer          Crates
─────          ──────
Types          cockpitctl-types (DTOs, embedded schemas, ordering helpers)
Domain         cockpitctl-domain (policy, selection, normalization)
               cockpitctl-domain-buildfix, cockpitctl-domain-signing,
               cockpitctl-domain-trend
Conformance    cockpitctl-conform (schema validation, path hygiene)
Features       cockpitctl-feature-state, cockpitctl-feature-grid
Use Case       cockpitctl-ingest (orchestration, ports, exit semantics)
Rendering      cockpitctl-render (markdown + budgets)
               cockpitctl-sarif (SARIF v2.1.0 export)
I/O Adapters   cockpitctl-io, cockpitctl-io-buildfix, cockpitctl-io-hooks,
               cockpitctl-io-policy-signing, cockpitctl-io-schema
Facade         cockpitctl-core (re-exports all microcrates)
Binaries       cockpitctl-cli (clap entry point)
               conformctl (standalone conformance checker)
Dev Tooling    xtask (schema sync, fixture generation, conformance harness)
```

See [AGENTS.md](AGENTS.md) for the full architecture description and contract
details.

## Testing

cockpitctl has 1800+ tests across the workspace. The main test modalities:

| Modality | How to run | Notes |
|----------|-----------|-------|
| **Unit tests** | `cargo test --workspace --all-targets` | Per-crate, run always |
| **Integration tests** | Same as above | Live in `tests/` directories |
| **E2E tests** | Same as above (uses `assert_cmd`) | CLI invocations, exit codes |
| **BDD scenarios** | `cargo test -p cockpitctl --test bdd` | Cucumber/Gherkin in `crates/cockpitctl-cli/features/` |
| **Property tests** | Same as unit tests (uses `proptest`) | Randomized invariant checking |
| **Snapshot/golden tests** | `cargo test -p cockpitctl --test ingest_golden` | Deterministic output via `insta` |
| **Fuzz targets** | `cargo fuzz run parse_receipt` | Requires nightly; optional |
| **Mutation testing** | `cargo mutants --workspace` | Optional, expensive |

When adding new behavior, include tests in the appropriate modality. Golden tests
are especially important for anything that affects `report.json` or `comment.md`
output.

## Code Style

- **Edition**: Rust 2024
- **Formatting**: `cargo fmt --all` — enforced in CI via `cargo fmt --all -- --check`
- **Linting**: `cargo clippy --workspace --all-targets -- -D warnings` — all
  warnings are errors in CI
- **Doc comments**: all public items should have `///` doc comments
- **Dependencies**: audited via `cargo-deny` (`deny.toml`); only permissively
  licensed crates are allowed

## Contract Changes

Contracts live in `contracts/schemas/` and are the source of truth. To modify a
contract:

1. **Edit the schema** in `contracts/schemas/` (e.g., `sensor.report.v1.json`
   or `cockpit.report.v1.json`).
2. **Run schema sync check** to verify embedded schemas stay in sync:
   ```bash
   cargo run -p xtask -- schema-sync-check
   ```
3. **Update DTOs** in `cockpitctl-types` to match the schema change.
4. **Add or update conformance tests** to cover the new/changed fields.
5. **Update golden fixtures** if output changes (`cargo test` will show `insta`
   snapshot diffs; review and accept with `cargo insta review`).

The comment template lives at `templates/cockpit.comment.v1.md`. Changes there
should be validated against the golden tests.

## Determinism Requirements

All cockpitctl output **must be byte-stable** given identical inputs. This is a
core invariant — the same receipts and config must always produce the same report,
comment, and exit code.

Canonical orderings:

- **Sensor discovery**: lexical order by sensor ID
- **Findings sort**: severity desc → sensor\_id → path → line → code → message
- **Highlights sort**: severity desc → blocking-first → sensor\_id → path → line → code

If your change affects ordering or output, run the golden tests and verify the
diffs are intentional:

```bash
cargo test -p cockpitctl --test ingest_golden
```

## Pull Request Guidelines

- **Small, focused PRs** are preferred over large omnibus changes.
- **All CI checks must pass**: fmt, clippy, tests, schema-sync-check, cargo-deny.
- **Include tests** for any new behavior or bug fix.
- **Update `CHANGELOG.md`** for user-visible changes (new features, bug fixes,
  breaking changes).
- **Update documentation** if your change affects CLI usage, contracts, or
  architecture.

## License

cockpitctl is dual-licensed under [MIT](LICENSE-MIT) and
[Apache-2.0](LICENSE-APACHE). By contributing, you agree that your contributions
will be licensed under the same terms.
