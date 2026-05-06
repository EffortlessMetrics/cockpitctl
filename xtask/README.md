# xtask

Internal workspace task runner for cockpitctl maintenance workflows.

## Scope
- Schema sync checks and validation.
- Conformance harness utilities.
- Fixture generation and refresh workflows.
- Example file synchronization.
- Governed lint and policy ledger checks.

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
| `check-lint-policy` | Verify Clippy lint ledger, active Cargo lints, planned flips, and debt metadata |
| `check-file-policy` | Verify structured non-Rust file allowlist metadata |
| `check-no-panic-family` | Verify structured panic-family allowlist metadata |
| `policy-report` | Run all policy ledger checks and print a combined summary |

## Usage

```bash
cargo run -p xtask -- schema-sync-check
cargo run -p xtask -- validate-schemas
cargo run -p xtask -- conform --report report.json --all --sensor-id builddiag
cargo run -p xtask -- conform-dir --dir artifacts --all --validate-cockpit
cargo run -p xtask -- fixtures-help
cargo run -p xtask -- check-lint-policy
cargo run -p xtask -- check-file-policy
cargo run -p xtask -- check-no-panic-family
cargo run -p xtask -- policy-report
```

This crate is `publish = false` and is intended for repository maintenance only.
