# cockpitctl-feature-grid

Shared feature-toggle grid definitions used by CLI BDD and future interoperability
layers.

## Scope

The crate intentionally keeps feature-gating data and expected runtime state logic
in one place so feature matrices, BDD assertions, and runtime checks stay aligned.

## Architecture

This crate belongs to the **infrastructure layer**. It is consumed by CLI and test
code but has no domain or I/O dependencies.

## Further reading

See the [cockpitctl repository](https://github.com/cockpitctl/cockpitctl) for full
documentation and architecture overview.

## License

Licensed under either of [Apache License, Version 2.0](../../LICENSE-APACHE) or
[MIT License](../../LICENSE-MIT), at your option.
