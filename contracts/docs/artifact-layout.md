# Artifact Layout Specification

This document defines the standard layout for cockpitctl artifacts.

## Directory Structure

```
artifacts/
├── <sensor_id>/
│   ├── report.json      # Required: sensor receipt (sensor.report.v1)
│   └── comment.md       # Optional: sensor-specific PR comment fragment
└── cockpit/
    ├── report.json      # Output: director aggregate (cockpit.report.v1)
    └── comment.md       # Output: merged PR comment
```

## Sensor Receipts

Each sensor produces a receipt at `artifacts/<sensor_id>/report.json`.

- **Schema**: `sensor.report.v1` (see `contracts/schemas/sensor.report.v1.json`)
- **Required**: `schema`, `tool`, `run`, `verdict`, `findings`
- **Size limit**: 2MB default (configurable)

## Director Output

The director (cockpitctl) produces:

- `artifacts/cockpit/report.json` - Aggregate report (cockpit.report.v1)
- `artifacts/cockpit/comment.md` - Merged PR comment

## Safety Constraints

Sensor IDs are untrusted input. The director enforces:

1. **No path traversal**: Sensor IDs must not contain `..`
2. **No directory separators**: No `/` or `\` in sensor IDs
3. **No symlink following**: Symlinks out of artifacts root are not followed
4. **Size limits**: Receipt files capped at 2MB default
5. **Count limits**: Maximum 100 sensors processed by default

## Sensor ID Validation

A valid sensor ID:
- Is non-empty
- Contains no `..` sequences
- Contains no `/` or `\` characters
- Uses alphanumeric characters, hyphens, and underscores

## Optional Comment Fragments

Sensors may provide `artifacts/<sensor_id>/comment.md` for custom PR comment content.

The director merges these fragments according to `section_order` policy.
