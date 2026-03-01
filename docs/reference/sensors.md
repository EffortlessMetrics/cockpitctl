# Known compatible sensors

cockpitctl is **sensor-agnostic**: any tool that writes a receipt conforming to
the [`sensor.report.v1`](sensor-report-schema.md) schema can participate in the
cockpit. The sensors listed below have been tested end-to-end with cockpitctl.

## Sensors

| Sensor | Purpose | Crate |
|--------|---------|-------|
| **builddiag** | Build diagnostics (warnings/errors) | [crates.io](https://crates.io/crates/builddiag) |
| **diffguard** | Diff policy enforcement | [crates.io](https://crates.io/crates/diffguard) |
| **tokmd** | Token counting for LLM context budgets | [crates.io](https://crates.io/crates/tokmd) |

## Writing your own sensor

Any program can act as a sensor. It must:

1. Write a single file to `artifacts/<sensor_id>/report.json`
2. Conform to the `sensor.report.v1` JSON schema
3. Include a `verdict` with `status` (`pass` | `warn` | `fail` | `skip`) and `counts`
4. List any `findings` with `severity`, `code`, and `message`

See the [sensor report schema reference](sensor-report-schema.md) for the full
field listing. The `fixtures/` directory in the repository contains example
receipts for each known sensor.
