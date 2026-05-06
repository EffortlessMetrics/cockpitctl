# xtask

Internal workspace task runner for cockpitctl maintenance workflows.

## Scope
- Schema sync checks and validation.
- Conformance harness utilities.
- Fixture generation and refresh workflows.
- Example file synchronization.
- Lint policy ledger and debt hygiene checks.

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
| `check-lint-policy` | Validate Clippy policy ledgers, workspace lint metadata, and debt hygiene |

## Usage

```bash
cargo run -p xtask -- schema-sync-check
cargo run -p xtask -- validate-schemas
cargo run -p xtask -- conform --report report.json --all --sensor-id builddiag
cargo run -p xtask -- conform-dir --dir artifacts --all --validate-cockpit
cargo run -p xtask -- fixtures-help
cargo run -p xtask -- check-lint-policy
```

This crate is `publish = false` and is intended for repository maintenance only.
