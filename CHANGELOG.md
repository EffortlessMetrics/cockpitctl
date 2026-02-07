# Changelog

All notable changes to cockpitctl are documented here.

This project adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

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
