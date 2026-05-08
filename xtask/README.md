# xtask

Internal workspace task runner for cockpitctl maintenance workflows.

## Scope
- Schema sync checks and validation.
- Conformance harness utilities.
- Fixture generation and refresh workflows.
- Example file synchronization.
- Packaging, release dry-run, smoke-test, and feature matrix automation that replaces shell/PowerShell scripts.

## Commands

| Command | Description |
|---------|-------------|
| `schema-sync-check` | Check that crate-local schema copies match `contracts/schemas/` |
| `schema-sync-fix` | Copy `contracts/schemas/*.json` → `crates/cockpitctl-types/schemas/` |
| `schema-check` | Basic schema sanity checks (JSON parse + required fields) |
| `validate-schemas` | Validate that JSON Schema files are valid JSON and conform to JSON Schema spec |
| `example-sync-check` | Check that crate-local `cockpit.toml.example` matches workspace root copy |
| `example-sync-fix` | Copy `cockpit.toml.example` → `crates/cockpitctl-cli/cockpit.toml.example` |
| `conform` | Conformance harness: validate sensor receipts against the protocol |
| `conform-dir` | Validate every sensor receipt in an `artifacts/` directory at once |
| `fixtures-help` | Print instructions for regenerating golden fixtures |
| `check-packaging` | Verify publishable crates ship no junk files and have required metadata |
| `release-dry-run` | Simulate a full crates.io publish in dependency order |
| `smoke-test-release <tag>` | Download and exercise published release binaries |
| `feature-matrix-check [--quick]` | Build `cockpitctl` across supported feature combinations |

## Usage

```bash
cargo run -p xtask -- schema-sync-check
cargo run -p xtask -- validate-schemas
cargo run -p xtask -- conform --report report.json --all --sensor-id builddiag
cargo run -p xtask -- conform-dir --dir artifacts --all --validate-cockpit
cargo run -p xtask -- fixtures-help
cargo run -p xtask -- check-packaging
cargo run -p xtask -- release-dry-run
cargo run -p xtask -- smoke-test-release v0.3.0
cargo run -p xtask -- feature-matrix-check --quick
```

This crate is `publish = false` and is intended for repository maintenance only.
