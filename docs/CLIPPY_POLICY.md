# Clippy policy

`cockpitctl` uses the Effortless Metrics Rust lint platform policy. The goal is
not local taste; it is a governed engineering surface that keeps the workspace
panic-free, avoids silent failure, rejects unreviewed suppression, and records
future Rust/Clippy upgrade flips before the MSRV moves.

## Active baseline

The root `Cargo.toml` owns the active workspace lint block. Every workspace
crate inherits that block with:

```toml
[lints]
workspace = true
```

The baseline covers:

- panic-free production and test code (`unwrap`, `expect`, `panic!`, `todo!`,
  `unimplemented!`, `unreachable!`, and related Result/Option collapse lints);
- AST, parser, UTF-8, indexing, and slice safety;
- silent-failure prevention (`let _ =`, ignored `Result::ok`, ignored
  `map_err`, and similar footguns);
- async/concurrency, unsafe/memory, numeric, file/process/path, API, and trait
  correctness lints;
- reviewability lints that reduce allocation/control-flow/formatting noise; and
- suppression governance lints.

`policy/clippy-lints.toml` is the machine-readable ledger for that policy. It
records every active lint plus planned Rust 1.94 and 1.95 flips. CI should run
`cargo run -p xtask -- check-lint-policy` to verify the manifest, ledger, and
source tree stay coherent.

## No test carveouts

The standard is workspace panic-free, not just production panic-free. Do not add
Clippy test carveouts such as:

```toml
allow-unwrap-in-tests = true
allow-expect-in-tests = true
allow-panic-in-tests = true
allow-indexing-slicing-in-tests = true
allow-dbg-in-tests = true
```

Tests should return `Result` and use fallible helpers instead of `unwrap`,
`expect`, or panic-driven setup.

## Suppression style

Use `#[expect(..., reason = "...")]` for a narrow, reviewed suppression. Do not
use broad `#[allow(...)]` attributes. If a suppression represents temporary
workspace debt, add a matching `policy/clippy-debt.toml` entry with `lint`,
`path`, `owner`, `reason`, and `expires`.

Example:

```rust
#[expect(
    clippy::too_many_arguments,
    reason = "Conformance CLI mirrors stable command-line flags; refactor after parser split."
)]
fn conformance_entrypoint(/* ... */) {}
```

## Repo-local `clippy.toml`

`clippy.toml` is only for repo-specific policy hooks such as disallowed methods,
disallowed types, or disallowed macros. It must not weaken the workspace baseline
or configure test carveouts.

## Future lint flips

The policy ledger tracks planned Rust 1.94 and 1.95 lints before they become
active. `check-lint-policy` fails if those planned lints are activated before the
workspace MSRV reaches their `activate_when_msrv` value, making upgrade gates
explicit and reviewable.
