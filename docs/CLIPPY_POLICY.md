# Clippy policy

`cockpitctl` uses a governed lint policy instead of ad-hoc local lint taste. The
policy has three layers:

1. a shared workspace lint block in `Cargo.toml`;
2. a machine-readable ledger in `policy/clippy-lints.toml`; and
3. temporary, expiring debt in `policy/clippy-debt.toml`.

The current rollout is infrastructure-first: the strict baseline is recorded in
workspace metadata and policy files so follow-up PRs can make crate inheritance
and debt burn-down reviewable instead of surprising.

## Policy goals

The common baseline is designed around these guarantees:

- **Panic-free production and tests**: no unchecked `unwrap`, `expect`, `panic!`,
  `todo!`, `unimplemented!`, `unreachable!`, or debug macros as normal control
  flow.
- **Parser-safe defaults**: string slicing, unchecked indexing, and UTF-8/byte
  boundary hazards are denied by default.
- **No silent failure**: ignored futures, ignored must-use values, swallowed
  errors, and lock/future footguns are denied.
- **Suppression governance**: broad `#[allow]` suppressions are not policy. Use
  narrow `#[expect(..., reason = "...")]` only when the exception is reviewed.
- **Upgrade tracking**: Rust 1.94 and 1.95 lint flips are tracked before the MSRV
  changes so the upgrade is a planned ratchet.

## Suppression style

Prefer fixing code over suppressing lints. When a suppression is unavoidable, use
`#[expect]` with a reason and keep the scope as small as possible:

```rust
#[expect(
    clippy::too_many_arguments,
    reason = "Conformance CLI adapter mirrors command-line flags one-for-one."
)]
fn run_conformance_adapter(/* ... */) {
    // ...
}
```

Do not add Clippy test carveouts in `clippy.toml`, including:

- `allow-unwrap-in-tests = true`
- `allow-expect-in-tests = true`
- `allow-panic-in-tests = true`
- `allow-indexing-slicing-in-tests = true`
- `allow-dbg-in-tests = true`

Tests should move toward `Result`-returning helpers and explicit assertion
helpers instead of panic-driven setup.

## Policy files

### `policy/clippy-lints.toml`

This is the source-of-truth ledger for active lints and planned Rust 1.94/1.95
flips. `cargo run -p xtask -- check-lint-policy` verifies that active entries
match the root workspace lint block and that planned entries stay planned until
the MSRV reaches their activation version.

### `policy/clippy-debt.toml`

Temporary exceptions live here. Every debt entry must include:

- `lint`
- `path`
- `owner`
- `reason`
- `expires`

Expired debt fails the policy check.

### `policy/no-panic-allowlist.toml`

This file reserves the semantic allowlist shape for panic-family exceptions:
identity is `path + family + selector`, while `last_seen` line/column data is
advisory only. It is the migration target for follow-up no-panic checks.

### `policy/non-rust-allowlist.toml`

This file documents non-Rust surfaces that are intentionally part of the repo,
who owns them, and what CI coverage keeps them honest.

## Local overlay

`clippy.toml` is reserved for repo-specific `disallowed-*` policy and must not
weaken the shared baseline or add test carveouts.
