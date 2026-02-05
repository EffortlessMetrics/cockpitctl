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
| `--label <LABEL>` | (none) | PR labels for label-gate evaluation (repeatable) |
| `--schema-validation <MODE>` | (unset) | Schema validation mode: `lax` or `strict` (overrides config when provided) |

**Schema Validation Modes:**

- **`lax` (config default):** Skip JSON Schema validation. Receipts only need to parse as valid JSON matching the serde structure. Faster, but schema errors surface as `cockpit.invalid_receipt` with less detail.
- **`strict`:** Validate receipts against `schemas/sensor.report.v1.json` before parsing. Schema violations surface as `cockpit.schema_violation` with detailed field-level errors. Useful during sensor development or strict CI pipelines.

> **Note:** The CLI flag only overrides config when explicitly provided. If unset, `cockpit.toml` controls the mode.

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
| `--strict` | default | Perform JSON Schema validation |
| `--lax` | (none) | Skip JSON Schema validation |

**Behavior:**

- In `--strict` mode (default), validates against the embedded JSON Schemas
- In `--lax` mode, only parses JSON into the Rust struct shapes

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
