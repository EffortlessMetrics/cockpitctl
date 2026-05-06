# Clippy policy

`cockpitctl` uses Clippy as a governed engineering surface, not as a local taste
file. The root workspace manifest owns the active lint block, while
`policy/clippy-lints.toml` records the active policy and planned Rust upgrade
flips.

## Baseline

The workspace baseline is panic-free for production and tests. It forbids
unchecked `unwrap`, `expect`, panic macros, string/indexing footguns, silent
failure patterns, and silent lint suppression. It also enables reviewability
lints for formatting, allocation shape, result handling, and API contracts.

## No test carveouts

Do not add Clippy test carveouts such as `allow-unwrap-in-tests`,
`allow-expect-in-tests`, `allow-panic-in-tests`, `allow-indexing-slicing-in-tests`,
or `allow-dbg-in-tests` to `clippy.toml`. Tests should return `Result` where
fallible setup is needed.

```rust
#[test]
fn parses_fixture() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = std::fs::read_to_string("tests/fixtures/input.rs")?;
    let parsed = parse(&fixture)?;

    ensure_eq(parsed.items.len(), 3, "fixture should expose three items")?;

    Ok(())
}
```

## Suppressions

Prefer fixing the code. If a narrow exception is necessary, use
`#[expect(..., reason = "...")]` at the smallest possible scope. Broad
`#[allow(...)]` suppressions are not part of the standard policy; expiring debt
belongs in `policy/clippy-debt.toml`.

## Policy files

- `policy/clippy-lints.toml` is the machine-readable active and planned lint
  ledger.
- `policy/clippy-debt.toml` is reserved for reviewed, expiring lint exceptions.
- `policy/no-panic-allowlist.toml` documents the semantic TOML shape for any
  panic-family exceptions that remain during migration.
- `policy/non-rust-allowlist.toml` documents non-Rust surfaces with owners,
  reasons, classifications, and CI coverage.
- `clippy.toml` is only for repo-local Clippy configuration such as disallowed
  methods/types/macros. It must not weaken the workspace policy.

## Gate

Run:

```bash
cargo run -p xtask -- check-lint-policy
```

The gate checks MSRV alignment, workspace lint inheritance, active/planned lint
consistency, no test carveouts, and required debt metadata.
