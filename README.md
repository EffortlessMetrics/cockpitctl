# cockpitctl

[![CI](https://github.com/EffortlessMetrics/cockpitctl/actions/workflows/ci.yml/badge.svg)](https://github.com/EffortlessMetrics/cockpitctl/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/cockpitctl.svg)](https://crates.io/crates/cockpitctl)
[![License](https://img.shields.io/badge/license-Apache--2.0%20%2F%20MIT-blue)](LICENSE-APACHE)

`cockpitctl` is the **director** for the Effortless Metrics PR cockpit.

It does one job: **ingest receipts** emitted by sensors and **render one merge surface**
(one aggregate report + one PR comment) under strict budgets.

## What it is
- Ingest + normalize receipts (`artifacts/*/report.json`)
- Apply composition policy (`cockpit.toml`)
- Produce:
  - `artifacts/cockpit/report.json` (`cockpit.report.v1`)
  - `artifacts/cockpit/comment.md` (comment contract v1)
- SARIF v2.1.0 export for IDE/CI integration
- GitHub Actions annotations (`::error`, `::warning`, `::notice`)

## What it is not
- Not a runner/orchestrator (no installing tools, no running sensors)
- Not a GitHub bot (workflow owns posting comments/checks)
- Not an adapter soup (tool-specific payload is opaque; contract lives in the envelope)

## Installation

### From GitHub Releases

Download pre-built binaries for Linux, macOS (x64/ARM64), and Windows from
[GitHub Releases](https://github.com/EffortlessMetrics/cockpitctl/releases).

### From source

```bash
cargo install cockpitctl
```

### As a GitHub Action

```yaml
- uses: EffortlessMetrics/cockpitctl@v0
  with:
    artifacts-path: artifacts
    config-path: cockpit.toml
```

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

## Supported sensors

Any tool that emits a `sensor.report.v1` receipt can participate. Known sensors
with fixtures or integration documentation:

| Sensor | Description |
|--------|-------------|
| `builddiag` | Build diagnostics (compiler errors/warnings) |
| `diffguard` | Diff-based change detection |
| `tokmd` | LLM/AI handoff manifest generation ([protocol](docs/issues/tokmd.md)) |
| `linter` | Generic linter output |
| `secaudit` | Security audit findings |
| `coverage` | Code coverage reporting |

To add a new sensor, emit `artifacts/<sensor_id>/report.json` conforming to
`sensor.report.v1` and validate with `conformctl check --all`.

## Repository layout

```
crates/
  cockpitctl-types          # stable DTOs + embedded schemas
  cockpitctl-domain         # pure policy logic, sorting, synthesis
  cockpitctl-domain-buildfix  # buildfix domain logic
  cockpitctl-domain-signing   # policy signing domain logic
  cockpitctl-domain-trend     # trend analysis domain logic
  cockpitctl-ingest          # use case + ports + exit semantics
  cockpitctl-io              # filesystem adapters + safety guards
  cockpitctl-io-buildfix     # buildfix I/O adapters
  cockpitctl-io-hooks        # hook execution adapters
  cockpitctl-io-policy-signing # policy signing I/O adapters
  cockpitctl-io-schema       # schema validation adapters
  cockpitctl-render          # deterministic markdown + annotation rendering
  cockpitctl-sarif           # SARIF v2.1.0 export
  cockpitctl-conform         # conformance checking library
  cockpitctl-feature-state   # feature flag state management
  cockpitctl-feature-grid    # BDD feature toggle grid
  cockpitctl-core            # facade crate (re-exports all microcrates)
  cockpitctl-cli             # `cockpitctl` CLI package (binary + compatibility lib)
  conformctl                 # standalone conformance checker CLI
xtask/                       # schema sync, conformance harness, fixture tooling
contracts/
  schemas/                   # JSON Schemas (source-of-truth)
  docs/                      # protocol specifications (tokens, identity)
docs/                        # user-facing documentation (Diataxis)
fixtures/                    # golden fixture inputs + expected outputs
templates/                   # comment contract template
fuzz/                        # cargo-fuzz harness (optional)
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
cargo run -p xtask -- smoke-test-release v0.3.0

# Windows PowerShell
cargo run -p xtask -- smoke-test-release v0.3.0
```

See [`docs/how-to/smoke-test-release.md`](docs/how-to/smoke-test-release.md) for details.

## GitHub Action

Use cockpitctl as a reusable GitHub Action to aggregate sensor receipts and
post a cockpit PR comment automatically.

```yaml
- uses: EffortlessMetrics/cockpitctl@v0
  with:
    artifacts-path: artifacts          # default: artifacts
    config-path: cockpit.toml         # default: cockpit.toml
    schema-validation: strict         # optional: lax (default) or strict
    github-annotations: 'true'       # optional: emit ::error/::warning annotations
    labels: 'security,performance'   # optional: comma-separated PR labels
    version: latest                  # optional: version tag or "latest"
    post-comment: 'true'             # optional: post/update the PR comment
    fail-on-error: 'true'            # optional: fail the step on non-zero exit
```

### Outputs

| Output         | Description                                        |
| -------------- | -------------------------------------------------- |
| `exit-code`    | Exit code from cockpitctl (0=pass, 2=policy fail)  |
| `verdict`      | Overall verdict (`pass`, `warn`, `fail`, `skip`)   |
| `report-path`  | Path to the generated `report.json`                |
| `comment-path` | Path to the generated `comment.md`                 |

### Example workflow

```yaml
name: Cockpit
on: [pull_request]
jobs:
  cockpit:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      # ... run your sensors, producing artifacts/<sensor>/report.json ...
      - uses: EffortlessMetrics/cockpitctl@v0
        id: cockpit
        with:
          artifacts-path: artifacts
          config-path: cockpit.toml
      - run: echo "Verdict was ${{ steps.cockpit.outputs.verdict }}"
```

## Test coverage

cockpitctl has 2900+ tests across 19 crates, spanning all major modalities:

| Modality | Description |
|----------|-------------|
| Unit tests | Per-crate tests for all 19 microcrates |
| Integration tests | Cross-crate pipeline, IO adapters, domain logic |
| E2E tests | CLI invocations, exit codes, config precedence |
| BDD scenarios | Cucumber/Gherkin scenarios for ingest, validate, init |
| Golden/snapshot | Deterministic output verification via insta |
| Property-based | Proptest across types, domain, ingest, render, IO, conform |
| Fuzz testing | 12 cargo-fuzz targets with corpus seeds |
| Stress tests | Caps, budgets, and load testing |
| Doc tests | Executable examples in rustdoc for public APIs |
| Benchmarks | Performance benchmarks for critical paths |
| Mutation testing | cargo-mutants configuration for test quality |
| Cross-platform | Path normalization tests for Windows/Unix |
| CLI completeness | Help, version, and snapshot tests for CLI interface |

## CLI usage

```bash
cockpitctl ingest --artifacts artifacts --config cockpit.toml
cockpitctl init --path cockpit.toml          # Write starter config
cockpitctl validate --input report.json       # Validate receipt/report
cockpitctl explain <CODE|all>                 # Explain finding codes

# Standalone conformance checker
conformctl check --report report.json --all --sensor-id builddiag
conformctl check-dir --dir artifacts --all --validate-cockpit
```

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT License](LICENSE-MIT) at your option.

## Links

- [CHANGELOG](CHANGELOG.md) — Recent changes and release history
- [ROADMAP](ROADMAP.md) — Planned and completed work
- [Documentation](docs/README.md) — Full Diataxis-structured docs
- [Release Gate Checklist](RELEASE_READY_GATE_CHECKLIST.md) — Release process verification
- [Contracts](contracts/) — JSON schemas and protocol specs
