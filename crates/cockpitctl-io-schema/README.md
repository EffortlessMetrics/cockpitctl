# cockpitctl-io-schema

Schema validation adapter boundary extracted from `cockpitctl-io`.

## Scope

- Provides JSON Schema validation for sensor and cockpit reports.
- Implements the `SchemaValidator` port from `cockpitctl-ingest`.
- Supports construction from embedded schemas, file paths, or custom schema values.

## Architecture

This crate belongs to the **I/O adapter layer**. It bridges the ingest pipeline's
schema validation port to the `jsonschema` library.

## Key exports

- `JsonSchemaValidator` — schema validator implementing `SchemaValidator`.
  - `::sensor_report_v1()` — validator for `sensor.report.v1`.
  - `::cockpit_report_v1()` — validator for `cockpit.report.v1`.
  - `::from_file(path)` — validator from a schema file.
  - `::from_schema(value)` — validator from a JSON value.

## Usage

```rust
use cockpitctl_io_schema::JsonSchemaValidator;
use cockpitctl_ingest::SchemaValidator;

let validator = JsonSchemaValidator::sensor_report_v1()?;
let result = validator.validate_receipt(report_bytes)?;
```

## Further reading

See the [cockpitctl repository](https://github.com/cockpitctl/cockpitctl) for full
documentation and architecture overview.

## License

Licensed under either of [Apache License, Version 2.0](../../LICENSE-APACHE) or
[MIT License](../../LICENSE-MIT), at your option.
