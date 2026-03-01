# Roadmap

This roadmap describes planned work for cockpitctl. Items are grouped by theme
and roughly ordered by priority. Nothing here is a commitment — priorities shift
as the project and its users evolve.

See `CHANGELOG.md` for what has already shipped.

## Completed milestones

These milestones from the original implementation plan are done:

- **M0 — Repo scaffold + contracts.** Workspace microcrates, schemas, comment
  template, CI.
- **M1 — Ingest core.** Receipt discovery, parsing, policy evaluation, missing
  and invalid receipt handling, verdict computation, `cockpit.report.v1` output.
- **M2 — Rendering contract.** Summary table, highlights, section ordering,
  repro lines, truncation, sticky markers.
- **M3 — Determinism, caps, robustness.** Stable sorting, derived fingerprints,
  receipt size/count caps, symlink/path-traversal protection, counts
  reconciliation.
- **M4 — Conformance harness + BDD.** `validate` command, `xtask conform` /
  `conform-dir`, 20+ BDD scenarios, fuzz harness, mutation testing guidance.
- **M5 — Distribution and ecosystem.** Multi-platform binaries, release
  workflow, Diataxis documentation, compatibility promise, reusable GitHub
  Action (`action.yml`).

---

## Recently completed roadmap items

### Configurable receipt size limit

Implemented as `policy.max_receipt_size_bytes` in `cockpit.toml`, wired through
filesystem adapters so teams can raise/lower the default cap per pipeline.

### `cockpitctl explain` command

Implemented as `cockpitctl explain <CODE|all>` with cockpit finding-code
explanations sourced from `cockpitctl-domain`.

### Annotation output

Implemented via `--github-annotations` and deterministic capping through
`policy.max_annotations`.

### SARIF export

Implemented via `cockpitctl ingest --format sarif`, writing
`artifacts/cockpit/sarif.json`.

### Trend tracking

Implemented via `--baseline <path>` plus deterministic diffing/rendering for
verdict/count/finding deltas.

### Plugin / extension hooks

Implemented through `[[hooks]]` post-processors that can emit extra files and
comment sections (now appended before cockpit sticky end markers).

### Buildfix plan integration

Implemented:
- Ingest `plan.json` alongside receipts.
- Match fixes to surfaced findings.
- Surface buildfix summaries in `cockpit.report.v1` data and the PR comment.
- Gate auto-apply on safety levels (`safe`, `guarded`, `unsafe`) and matched
  findings policy.
- Integrate external actuator execution with deterministic evidence output in
  `artifacts/cockpit/buildfix.apply.json` and `cockpit.report.v1` data.

### Policy snapshot signing

Implemented:
- Configurable signing policy via `[policy_signing]` in `cockpit.toml`.
- HMAC-SHA256 signing over canonical `report.policy` snapshot bytes.
- Signature evidence in `cockpit.report.v1` data (`_policy_signature`) and
  deterministic sidecar output in `artifacts/cockpit/policy.signature.json`.
- Optional CLI overrides for enabling signing and selecting key source/ID.

### 19-crate microcrate extraction and test expansion

Implemented:
- 9 new microcrates extracted for clean SRP (domain-buildfix, domain-signing,
  domain-trend, io-hooks, io-schema, io-buildfix, io-policy-signing,
  feature-state, feature-grid).
- Comprehensive test expansion: doc tests, edge-case/error-path tests,
  cross-crate integration tests, 6 fuzz targets, property-based testing across
  5 crates, 29 golden/snapshot tests (34 snap files), and 47 E2E tests.
- Feature gating: `default = []`, all optional features opt-in from CLI.

### CI hardening

Implemented:
- No-default-features build and test steps to verify feature isolation.
- Security audit workflow (weekly + on dependency changes).
- MSRV (minimum supported Rust version) verification in CI.
- Code coverage reporting with cargo-tarpaulin (PR #36).
- cargo-deny for license and advisory checking (PR #37).
- Benchmark, examples, and doc test compilation checks (PR #68).
- Full CI pipeline: fmt, clippy, tests, doc tests, benchmarks, examples,
  no-default-features, schema-sync, packaging, conformance, dependency
  isolation, security audit, MSRV, cargo-deny.

### Documentation and crates.io readiness

Implemented:
- Architecture documentation aligned with 19-crate layout.
- Per-crate `README.md` files for all published crates.
- Runnable doc-tested examples for core and types crates.
- Executable doc tests for public APIs (PR #58).
- 9-tier dependency-ordered publish in release workflow.

### Comprehensive test expansion

Implemented (1800+ tests across 19 crates):
- 13 test modalities: unit, integration, E2E, BDD, golden/snapshot,
  property-based, fuzz (6 targets with corpus seeds), stress, doc tests,
  benchmarks, mutation testing, cross-platform, CLI completeness.
- Cross-platform path normalization tests (PR #73).
- CLI completeness and help/version tests — 28 tests (PR #74).
- Conform crate property-based test expansion (PR #72).
- Stress and load tests for caps and budgets (PR #67).
- Feature flag isolation and compilation matrix tests (PR #59).

---

## Near-term focus

### Release candidate finalization
- Final documentation polish and CHANGELOG alignment
- Verify all CI gates pass on tagged release
- Smoke test published artifacts on all platforms

### Community readiness
- Contributing guide (`CONTRIBUTING.md`)
- Issue templates for bug reports and feature requests
- crates.io publish and documentation hosting

---

## Long-term / exploratory

### Receipt streaming

For very large monorepos with hundreds of sensors, stream receipts instead of
loading all into memory. This relaxes the current O(receipts) memory bound.

### Web dashboard

A lightweight UI that reads `cockpit.report.v1` files and renders an interactive
dashboard — historical trends, per-sensor drill-down, policy diff view.

---

## How to contribute

If you want to work on any of these items, open an issue to discuss the design
before sending a PR. See `CLAUDE.md` for build and test commands.
