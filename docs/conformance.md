# Conformance

Conformance is how the ecosystem stays coherent as sensors evolve independently.

`cockpitctl` enforces conformance indirectly:
- it can parse receipts into the envelope DTOs
- it surfaces invalid/missing receipts as cockpit-level findings

## What conformance means here

A receipt is conformant if:
- it matches the envelope shape (`sensor.report.v1`)
- it only extends via `data` fields
- it uses stable finding codes
- it is deterministic for a fixed input

## Suggested checks in sensor repos

- “shape test”: serialize a sample report and validate it parses as `SensorReport`
- “golden test”: run the sensor on a fixed fixture repo state and compare the receipt
- “no panics”: fuzz the sensor input parsing if it accepts complex inputs

## cockpitctl validate

The `validate` subcommand is intentionally small:
- does it parse as a sensor report or cockpit report?

If you want full JSON Schema validation, add it as an optional feature
(and keep it out of the hot-path ingest unless you *really* need it).
