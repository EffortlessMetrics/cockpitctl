# cockpitctl-domain

Pure domain logic for deterministic cockpit decisions.

## Scope
- Policy evaluation for missing/invalid/present sensor receipts.
- Deterministic sorting and highlight selection.
- Cockpit report synthesis helpers.
- Trend and baseline diff helpers.

## Boundaries
- No filesystem access.
- No command execution.
- No CLI parsing.

## Key exports
- `summarize_sensor_report`, `select_highlights`, `build_cockpit_report`
- `overall_verdict`, `compute_trend`
- `explain_code`, `all_codes`
