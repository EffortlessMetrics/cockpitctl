# cockpitctl-feature-state

Shared definitions for cockpitctl runtime feature toggles and expected runtime state.
Used by CLI orchestration and BDD steps to keep feature flag behavior aligned.

## Architecture

This crate belongs to the **infrastructure layer**. It provides feature-state
primitives consumed by the CLI and test harnesses.

## Further reading

See the [cockpitctl repository](https://github.com/cockpitctl/cockpitctl) for full
documentation and architecture overview.

## License

Licensed under either of [Apache License, Version 2.0](../../LICENSE-APACHE) or
[MIT License](../../LICENSE-MIT), at your option.
