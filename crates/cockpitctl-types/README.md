# cockpitctl-types

Stable DTOs, enums, and embedded schemas for cockpitctl contracts.

## Scope
- Data model for `sensor.report.v1`, `cockpit.report.v1`, and related policy/buildfix payloads.
- Deterministic primitives such as severity/status ranking helpers.
- Embedded JSON schema bytes exposed as constants.

## Boundaries
- No filesystem access.
- No CLI dependencies.
- No network calls.

## Key exports
- `SensorReport`, `CockpitReport`, `CockpitConfig`
- `Finding`, `Highlight`, `SensorSummary`, `Verdict`
- `SENSOR_REPORT_V1_SCHEMA_JSON`, `COCKPIT_REPORT_V1_SCHEMA_JSON`
