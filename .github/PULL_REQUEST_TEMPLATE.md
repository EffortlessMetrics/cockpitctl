## Summary of changes

<!-- Describe what this PR does and why. -->

## Related issue(s)

<!-- Link related issues: Fixes #123, Relates to #456 -->

## Test plan

<!-- What tests were added or run to verify the changes? -->

- [ ] Added new tests
- [ ] Ran `cargo test --workspace --all-targets`

## Contract impact

<!-- Do these changes affect schemas, DTOs, or CLI behavior? If yes, describe. -->

- [ ] No contract changes
- [ ] Updated schema(s) in `contracts/schemas/`
- [ ] Changed DTOs in `cockpitctl-types`
- [ ] Changed CLI flags or exit codes

## Determinism receipt

<!-- Do these changes affect cockpit output (report.json, comment.md)? -->

- [ ] No output changes
- [ ] Updated golden fixtures (`cargo test -p cockpitctl --test ingest_golden`)

## Checklist

- [ ] `cargo fmt --all -- --check` passes
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` passes
- [ ] `cargo test --workspace --all-targets` passes
- [ ] `cargo run -p xtask -- schema-sync-check` passes
