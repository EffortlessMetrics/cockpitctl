# xtask

Internal workspace task runner for cockpitctl maintenance workflows.

## Scope
- Schema sync checks.
- Conformance harness utilities.
- Fixture generation and refresh workflows.

## Usage
- `cargo run -p xtask -- schema-sync-check`
- `cargo run -p xtask -- fixtures-help`

This crate is `publish = false` and is intended for repository maintenance only.
