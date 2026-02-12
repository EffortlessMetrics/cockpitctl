# Reference

Reference documentation provides **technical descriptions** of the machinery and how to operate it. It is information-oriented and assumes you know what you're looking for.

## CLI

### [CLI Commands](cli.md)

Complete reference for all cockpitctl and conformctl commands and flags.

## Configuration

### [cockpit.toml Reference](config.md)

Full specification of the configuration file format.

### [Toolpack Manifest](toolpack.md)

The `toolpack.json` manifest for declaring tools and binary assets with versioned URLs and SHA256 checksums.

## Schemas

### [Sensor Report Schema](sensor-report-schema.md)

The `sensor.report.v1` envelope that sensors emit.

### [Cockpit Report Schema](cockpit-report-schema.md)

The `cockpit.report.v1` aggregate report that cockpitctl produces.

## Behavior

### [Exit Codes](exit-codes.md)

What each exit code means and when it's returned.

### [Finding Codes](finding-codes.md)

The `cockpit.*` codes that cockpitctl generates.

### [Determinism](determinism.md)

Sort keys and ordering guarantees.

### [Safety Limits](safety-limits.md)

Size caps, path restrictions, and robustness measures.

## Contracts

### [Compatibility Promise](compatibility.md)

SemVer policy, schema stability guarantees, and deprecation process for v1 contracts.

## Related

- [How-to Guides](../how-to/README.md) - For task-oriented guidance
- [Explanation](../explanation/README.md) - For conceptual background
