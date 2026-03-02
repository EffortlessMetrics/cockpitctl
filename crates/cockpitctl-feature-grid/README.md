# cockpitctl-feature-grid

Shared feature-toggle grid definitions used by CLI BDD and future interoperability
layers.

## Scope

The crate intentionally keeps the canonical feature matrix in one place so
BDD fixtures and parity tests stay aligned. Runtime evaluation helpers live in
`cockpitctl-feature-runtime`.

## Architecture

This crate belongs to the **infrastructure layer**. It is consumed by CLI and test
code but has no domain or I/O dependencies.

## Further reading

See the [cockpitctl repository](https://github.com/cockpitctl/cockpitctl) for full
documentation and architecture overview.

## License

Licensed under either of [Apache License, Version 2.0](../../LICENSE-APACHE) or
[MIT License](../../LICENSE-MIT), at your option.
