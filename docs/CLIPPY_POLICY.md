# Clippy policy

Cockpitctl treats Clippy as a governed engineering surface, not as local taste
hidden in individual crate manifests. The policy has three layers:

1. **Workspace lint baseline** in the root `Cargo.toml`.
2. **Machine-readable ledger** in `policy/clippy-lints.toml`.
3. **Reviewed exception ledgers** under `policy/` for temporary debt and other
   policy allowlists.

## Active baseline

The workspace baseline is panic-free across production and test code, prevents
silent failures, bans silent lint suppression, and includes reviewability lints
that keep parser and reporting code easier to inspect. The active Cargo lint
block is mirrored in `policy/clippy-lints.toml` so automation can verify that the
manifest and policy ledger stay coherent.

The current workspace MSRV remains `1.92` until the toolchain ratchet PR lands.
The ledger already tracks the planned Rust 1.94 and 1.95 flips so the upgrade is
reviewable before the compiler bump.

## No test carveouts

Do not add Clippy test carveouts to `clippy.toml`, including:

- `allow-unwrap-in-tests = true`
- `allow-expect-in-tests = true`
- `allow-panic-in-tests = true`
- `allow-indexing-slicing-in-tests = true`
- `allow-dbg-in-tests = true`

Tests should return `Result` and use checked assertions/helpers instead of
`unwrap`, `expect`, or panic-driven setup where practical.

## Suppressions

Use narrow `#[expect(..., reason = "...")]` suppressions when a lint exception is
intentional and local. Do not use broad `#[allow]` attributes as a replacement for
policy review. Temporary repo debt belongs in `policy/clippy-debt.toml` with an
owner, path, lint, reason, and expiry.

## Repo-local overlays

Keep `clippy.toml` for repo-specific additions such as disallowed methods,
disallowed types, or disallowed macros. Do not use it to weaken the workspace
panic-free posture.

## Policy checks

Run these checks before merging policy changes:

```bash
cargo run -p xtask -- check-lint-policy
cargo run -p xtask -- check-file-policy
cargo run -p xtask -- check-no-panic-family
cargo run -p xtask -- policy-report
```

`check-lint-policy` verifies the root MSRV, active/planned lint ledger, Clippy
carveouts, and debt metadata. `check-file-policy` and `check-no-panic-family`
validate the structured TOML allowlist schemas used by follow-up policy gates.
