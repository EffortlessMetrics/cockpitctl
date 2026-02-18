# cockpitctl-conform

Conformance checking library for cockpitctl receipt contracts.

## Scope
- Validates `sensor.report.v1` against embedded schema.
- Runs optional extended checks (ordering, path hygiene, reason lint, survivability, identity).
- Validates `cockpit.report.v1` schema and cockpit-specific extended checks.

## Key exports
- `ConformChecks`, `ConformResult`, `Violation`
- `conform_single`, `validate_cockpit_schema`, `check_cockpit_extended`
- Utility checks in `checks` module (ordering, tokens, path hygiene, determinism)
