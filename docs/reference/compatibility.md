# Compatibility Promise

This document defines the formal compatibility promise for cockpitctl v1 contracts.

## Overview

cockpitctl follows [Semantic Versioning 2.0.0](https://semver.org/). Within a major version, we guarantee backward compatibility for all public contracts. Consumers can upgrade minor and patch versions without breaking their integrations.

## Versioned Contracts

The following artifacts are covered by this compatibility promise:

| Contract | Version | Location |
|----------|---------|----------|
| Sensor report schema | `sensor.report.v1` | `schemas/sensor.report.v1.json` |
| Cockpit report schema | `cockpit.report.v1` | `schemas/cockpit.report.v1.json` |
| Comment template | `cockpit.comment.v1` | `templates/cockpit.comment.v1.md` |
| CLI interface | v1 | `cockpitctl` binary |
| Exit codes | v1 | Documented in [Exit Codes](exit-codes.md) |

## SemVer Policy

### Major Version (X.0.0)

A major version bump indicates breaking changes. We will:

- Announce breaking changes at least one minor version in advance
- Provide migration documentation
- Maintain the previous major version with security fixes for at least 6 months

### Minor Version (0.X.0)

A minor version bump indicates new features or non-breaking changes:

- New optional fields in schemas
- New CLI flags or commands
- New configuration options with backward-compatible defaults
- Performance improvements
- Bug fixes that don't change documented behavior

### Patch Version (0.0.X)

A patch version bump indicates:

- Bug fixes
- Documentation improvements
- Internal refactoring with no behavior changes

## Schema Stability

### sensor.report.v1

The `sensor.report.v1` schema is the input contract for sensors.

**Stable guarantees:**

- Required fields remain required
- Required field types do not change
- Enum values (`pass`, `warn`, `fail`, `skip` for status; `info`, `warn`, `error` for severity) remain valid
- The `data` field (top-level and per-finding) remains opaque and accepts any JSON value

**Allowed changes (non-breaking):**

- Adding new optional fields to any object
- Adding new optional sub-objects
- Widening type constraints (e.g., increasing max length)
- Adding new enum values (consumers must handle unknown values gracefully)

**Breaking changes (require major version):**

- Removing or renaming any field
- Changing a field's type
- Making an optional field required
- Removing enum values
- Changing the meaning of existing fields

### cockpit.report.v1

The `cockpit.report.v1` schema is the output contract for consumers.

**Same guarantees apply as `sensor.report.v1`**, plus:

- The `policy` snapshot structure remains stable
- Sensor summaries in the `sensors` array maintain their structure
- The `highlights` array structure remains stable

### Extension Rules

Both schemas use `"additionalProperties": false` at each level. This means:

1. Producers must only emit known fields
2. New fields are introduced as schema updates
3. Consumers should use lenient parsing and ignore unknown fields from future versions

For tool-specific or extension data, use the `data` fields:

- Top-level `data` for report-wide metadata
- Per-finding `data` for finding-specific payload

These fields are explicitly untyped and cockpitctl treats them as opaque.

## Comment Template Stability

The `cockpit.comment.v1.md` template defines the PR comment structure.

**Stable guarantees:**

- Comment markers (`<!-- cockpit:begin -->` and `<!-- cockpit:end -->`) remain stable
- Section headings remain stable
- Table column order remains stable

**Allowed changes (non-breaking):**

- Adding new sections
- Adding new columns to tables (at the end)
- Changing emoji or formatting within cells
- Changing text between stable markers

**Breaking changes (require major version):**

- Changing comment markers
- Removing sections
- Reordering or removing table columns
- Changing section heading text

## CLI Stability

### Commands

**Stable commands:**

| Command | Purpose |
|---------|---------|
| `ingest` | Process receipts and produce report |
| `init` | Generate starter configuration |
| `validate` | Validate JSON against schemas |

**Allowed changes (non-breaking):**

- Adding new commands
- Adding new flags to existing commands
- Adding new flag aliases

**Breaking changes (require major version):**

- Removing commands
- Removing flags
- Changing flag semantics
- Changing default values in backward-incompatible ways

### Exit Codes

Exit codes are part of the stable interface.

| Code | Meaning | Stability |
|------|---------|-----------|
| `0` | Pass | Stable |
| `1` | Runtime error | Stable |
| `2` | Policy failure | Stable |

**Guarantees:**

- Exit code meanings do not change
- The mapping from verdict to exit code (documented in [Exit Codes](exit-codes.md)) remains stable
- New exit codes may be added for new failure modes

## Deprecation Process

When a feature or field needs to be removed:

### Timeline

1. **Deprecation announcement**: Feature marked deprecated in release notes
2. **Deprecation warning**: CLI emits warning when deprecated feature is used (minimum 1 minor version)
3. **Removal**: Feature removed in next major version

### Communication

- Deprecations are announced in:
  - Release notes
  - CLI warnings
  - Documentation (with `[DEPRECATED]` markers)
- Migration guides are provided before removal

### Schema Field Deprecation

Deprecated schema fields:

1. Remain in the schema with a `"deprecated": true` annotation (or description note)
2. Continue to be accepted and processed
3. Are removed only in major versions

## Determinism Guarantees

Output determinism is part of the compatibility promise. See [Determinism](determinism.md) for details.

**Guaranteed stable:**

- Sort order of findings, highlights, and sensors
- JSON formatting (2-space indent, definition order, trailing newline)
- Fingerprint derivation algorithm
- Exit code for identical inputs

## What Is Not Covered

The following are explicitly not part of the stability promise:

- Internal crate APIs (anything not re-exported as public)
- Debug/verbose output format
- Error message text (codes are stable, messages are not)
- Performance characteristics
- Undocumented behavior
- Private fields in config (those prefixed with `_`)

## Reporting Compatibility Issues

If you believe a release has broken compatibility:

1. Check the release notes for announced changes
2. Open an issue with:
   - cockpitctl version (before and after)
   - Minimal reproduction case
   - Expected vs actual behavior
   - Reference to this compatibility document

## See Also

- [Sensor Report Schema](sensor-report-schema.md) - Input format specification
- [Cockpit Report Schema](cockpit-report-schema.md) - Output format specification
- [Exit Codes](exit-codes.md) - Exit code semantics
- [CLI Reference](cli.md) - Command and flag documentation
- [Determinism](determinism.md) - Ordering guarantees
