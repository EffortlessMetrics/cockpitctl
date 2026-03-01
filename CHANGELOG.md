# Changelog

All notable changes to cockpitctl are documented here.

This project adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

#### Architecture
- Extract 9 new microcrates for clean SRP architecture (PR #9):
  - `cockpitctl-domain-buildfix` — Buildfix domain logic
  - `cockpitctl-domain-signing` — Policy signing domain logic
  - `cockpitctl-domain-trend` — Trend analysis domain logic
  - `cockpitctl-io-hooks` — Hook execution adapters
  - `cockpitctl-io-schema` — Schema validation adapters
  - `cockpitctl-io-buildfix` — Buildfix I/O adapters
  - `cockpitctl-io-policy-signing` — Policy signing I/O adapters
  - `cockpitctl-feature-state` — Feature flag state management
  - `cockpitctl-feature-grid` — BDD feature toggle grid
- Feature-gated builds: hooks, buildfix, policy-signing, and schema are opt-in
  features (`default = []`, opt-in from CLI)

#### Features & Integrations
- tokmd sensor integration fixture and documentation (PR #43)
- GitHub Action improvements: `schema-validation` and `annotations` inputs,
  SHA256SUMS.txt checksum verification, `toolpack` manifest-based installation
  (PR #50)
- Packaging hygiene check scripts for CI (`check_state.js`, `check_state.ps1`)
  (PR #50)

#### Testing expansion
- Comprehensive unit tests across all 19 microcrates (PR #9)
- Doc tests across all public API crates (PR #25)
- Edge case and error path coverage for safety-critical code (PR #26)
- Cross-crate integration tests: full pipeline, determinism, and safety (PR #29)
- Fuzz testing expansion: 6 targets covering receipt parsing, policy evaluation,
  and rendering (PR #30)
- Property-based testing (proptest) across 5 core crates — types, domain,
  ingest, render, and IO (PR #31)
- Golden/snapshot test expansion: 29 test functions, 34 snapshot files covering
  normal, edge, error, and multi-sensor scenarios (PR #32)
- Runnable examples for `cockpitctl-core` and `cockpitctl-types` crates (PR #33)
- E2E test expansion: 47 new end-to-end tests covering CLI invocations, config
  precedence, exit codes, and output validation (PR #34)
- Renderer budget and marker regression tests — 18 tests (PR #41)
- Safety boundary tests for IO crate — 35 tests (PR #42)
- Conformance expansion with snapshot tests and E2E (PR #44)
- CLI precedence contract E2E tests — 22 tests (PR #45)
- Determinism golden tests for ordering stability (PR #46)
- Domain microcrate unit tests: buildfix, signing, and trend (PR #47)
- IO adapter integration tests (PR #48)
- SARIF, explain, and feature-grid test coverage expansion (PR #49)
- Init, validate, and error message E2E hardening (PR #51)
- Total test count: 1200+ tests across 19 crates

#### CI hardening
- No-default-features build and test steps in CI (PR #11)
- Security audit workflow: weekly schedule plus on dependency changes (PR #27)
- MSRV (minimum supported Rust version) verification in CI (PR #27)
- Code coverage reporting with cargo-tarpaulin (PR #36)
- cargo-deny for license and advisory checking (PR #37)

#### Documentation
- Architecture documentation aligned with 19-crate layout (PR #10)
- Per-crate `README.md` files for crates.io readiness (PR #28)
- Runnable doc-tested examples for core and types crates (PR #33)
- Improved rustdoc coverage across core crates (PR #40)

#### Release infrastructure
- 9-tier dependency-ordered publish in release workflow
- CI packaging verification for all 19 crates

### Fixed
- Feature gating: feature flags now properly propagate from CLI to feature-state crate
- CI packaging: all 19 crates included in cargo package verification
- Release workflow: 9-tier dependency-ordered publish with proper index waits

## [0.3.0] - 2026-02-17

### Added

- Buildfix auto-apply safety gating:
  - `buildfix.auto_apply`
  - `buildfix.max_auto_apply_safety`
  - `buildfix.require_matched_finding`
  - optional `buildfix.actuator` command + timeout
- Actuator integration for buildfix apply:
  - `cockpitctl ingest` can execute an external actuator with
    `buildfix.apply.request.v1` on stdin
  - writes `artifacts/cockpit/buildfix.apply.json`
  - surfaces apply evidence under `data._buildfix_apply` in `cockpit.report.v1`
- CLI overrides for buildfix apply controls:
  - `--buildfix-auto-apply`
  - `--buildfix-max-auto-apply-safety`
  - `--buildfix-actuator`
  - `--buildfix-actuator-timeout-ms`
- Policy snapshot signing:
  - `[policy_signing]` config (`enabled`, `algorithm`, `key_path`, `key_env`, `key_id`)
  - deterministic HMAC-SHA256 signature over canonical `report.policy` snapshot bytes
  - evidence in `cockpit.report.v1` under `data._policy_signature`
  - sidecar output `artifacts/cockpit/policy.signature.json`
- CLI overrides for policy signing:
  - `--policy-sign`
  - `--policy-sign-key-path`
  - `--policy-sign-key-env`
  - `--policy-sign-key-id`

## [0.2.1] - 2026-02-14

### Fixed

- Release workflow: `build-binaries` job now depends on `quality-gate` to
  prevent binary builds from starting before lint/format checks pass.
- Release workflow: fixed `dtolnay/rust-action` typo to `dtolnay/rust-toolchain`
  in the `build-binaries` job.

## [0.2.0] - 2026-02-10

### Added

- **cockpitctl-core facade crate** — single dependency for downstream consumers
  that re-exports all microcrate APIs (`types`, `domain`, `ingest`, `render`,
  `io`). Eliminates the need to wire individual microcrates.
- **Reason token validation** — `verdict.reasons[]` and `capabilities.*.reason`
  tokens are now validated against `^[a-z0-9_]+$`. The `xtask conform
  --reason-lint` flag and `conform-dir --reason-lint` enforce this at fixture
  and CI time.
- **`xtask conform` command** — conformance harness that validates sensor
  receipts against the protocol. Flags:
  - `--path-hygiene` — rejects absolute paths, backslashes, and `..` traversal
    in finding locations
  - `--ordering` — verifies findings are in canonical sort order
  - `--reason-lint` — validates reason token format
  - `--survivability` — requires explanatory findings or reasons when
    `status=fail`
  - `--tool-error-identity` — requires canonical `check_id`/`code` when
    `tool_error` reason is present
  - `--golden <FILE>` — determinism check against a golden file
  - `--all` — enables all checks
- **`xtask conform-dir` command** — batch conformance validation across all
  sensors in an artifacts directory, with optional `--validate-cockpit` for the
  cockpit report and `--allow-missing-report` to skip sensors without receipts.
- **`xtask validate-schemas` command** — JSON Schema meta-validation for schema
  files, with optional `--fix` for consistent formatting.
- **Structured capabilities map** — `run.capabilities` is now a
  `BTreeMap<String, Capability>` with `status` and optional `reason` fields
  (clean break from the previous representation).
- **Token registry** (`contracts/docs/tokens.md`) — canonical registry of all
  reason tokens (sensor-emitted, cockpit-synthesized, policy-derived, and
  capability reasons) with identity tuples and stability guarantees.
- **Identity specification** (`contracts/docs/identity-spec.md`) — vocabulary
  for severity levels, verdict status, safety levels, fingerprint derivation,
  and code stability rules.
- **Embedded JSON schemas** — `sensor.report.v1` and `cockpit.report.v1`
  schemas are compiled into the binary via `include_str!`, enabling offline
  `--strict` validation without external schema files.
- **New fixtures** — `mixed_verdicts`, `skip_receipt`, `tool_error` covering
  additional edge cases (multi-sensor verdict composition, skip status handling,
  tool runtime error identity).
- **Schema sync tooling** — `xtask schema-sync-check` verifies crate-local
  schema copies match `contracts/schemas/`; `schema-sync-fix` copies them.
- **Multi-platform binary builds** — CI workflow produces binaries for Linux,
  macOS, and Windows.
- **Release workflow** — GitHub Actions workflow for publishing crates to
  crates.io.
- **BDD test scenarios** — extended to 20+ scenarios including label gating,
  schema validation modes, mixed verdicts, and tool errors.
- **Finding codes reference** (`docs/reference/finding-codes.md`) — complete
  documentation of all `cockpit.*` finding codes with examples and severity
  rules.

### Changed

- `cockpit.report.v1` now includes `run.capabilities` as an ordered map and
  `verdict.reasons` at both aggregate and per-sensor levels.
- Sensor report `verdict.reasons` is now surfaced in the cockpit report per
  sensor entry.
- JSON schema files include `buildfix.plan.v1.json` in the sync set.
