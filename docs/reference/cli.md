# CLI Reference

Complete reference for cockpitctl commands and flags.

## Global Options

```
--verbose    Enable verbose logging (discovery, parsing, policy decisions)
--help       Print help information
--version    Print version information
```

## Commands

### ingest

Read sensor receipts and produce an aggregate cockpit report.

```bash
cockpitctl ingest [OPTIONS]
```

**Options:**

| Flag | Default | Description |
|------|---------|-------------|
| `--artifacts <PATH>` | `artifacts` | Directory containing sensor receipts |
| `--config <PATH>` | `cockpit.toml` | Path to policy configuration file |
| `--output <PATH>` | `artifacts/cockpit` | Directory for cockpit outputs |

**Outputs:**

- `<output>/report.json` - Aggregate report (`cockpit.report.v1`)
- `<output>/comment.md` - PR comment for posting

**Exit Codes:**

| Code | Meaning |
|------|---------|
| `0` | Pass (or warn when `warn_is_fail = false`) |
| `2` | Policy failure (blocking sensor failed or warn-as-fail triggered) |
| `1` | Runtime error (cannot read/write required paths, config error) |

**Example:**

```bash
# Standard usage
cockpitctl ingest --artifacts artifacts --config cockpit.toml

# Custom paths
cockpitctl ingest --artifacts ./ci-artifacts --config ./config/cockpit.toml

# Verbose mode for debugging
cockpitctl ingest --verbose
```

### init

Write a starter `cockpit.toml` configuration file.

```bash
cockpitctl init [OPTIONS]
```

**Options:**

| Flag | Default | Description |
|------|---------|-------------|
| `--path <PATH>` | `cockpit.toml` | Where to write the config file |

**Behavior:**

- Does not overwrite existing files
- Creates a minimal valid configuration

**Example:**

```bash
cockpitctl init --path cockpit.toml
```

### validate

Validate a receipt or report against the expected schema.

```bash
cockpitctl validate [OPTIONS]
```

**Options:**

| Flag | Default | Description |
|------|---------|-------------|
| `--input <PATH>` | required | Path to JSON file to validate |

**Behavior:**

- Parses the file as either `sensor.report.v1` or `cockpit.report.v1`
- Reports parse errors with location information
- Does not perform full JSON Schema validation by default

**Example:**

```bash
# Validate a sensor receipt
cockpitctl validate --input artifacts/builddiag/report.json

# Validate a cockpit report
cockpitctl validate --input artifacts/cockpit/report.json
```

## Environment Variables

cockpitctl does not fetch CI context automatically. Supply context via flags or environment variables if needed for metadata population.

## See Also

- [Exit Codes](exit-codes.md) - Detailed exit code semantics
- [Config Reference](config.md) - cockpit.toml format
