# cockpitctl-domain-buildfix

Buildfix planning and selection domain boundary extracted from `cockpitctl-domain`.

## Scope

- Matches fixes from a buildfix plan to report findings by sensor, fingerprint, and code.
- Selects fixes eligible for auto-apply under a configured safety gate.
- Deterministic sort order: safety rank → sensor ID → fix ID.

## Architecture

This crate belongs to the **domain layer**. It contains pure logic with no filesystem,
network, or CLI dependencies.

## Key exports

- `match_buildfix_plan` — match plan fixes against report highlights.
- `select_auto_apply_fixes` — filter fixes by safety level and match status.

## Usage

```rust
use cockpitctl_domain_buildfix::{match_buildfix_plan, select_auto_apply_fixes};
use cockpitctl_types::SafetyLevel;

let summary = match_buildfix_plan("clippy", &plan, &highlights);
let auto = select_auto_apply_fixes(&summary, SafetyLevel::Safe, true);
```

## Further reading

See the [cockpitctl repository](https://github.com/cockpitctl/cockpitctl) for full
documentation and architecture overview.

## License

Licensed under either of [Apache License, Version 2.0](../../LICENSE-APACHE) or
[MIT License](../../LICENSE-MIT), at your option.
