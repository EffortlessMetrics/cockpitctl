# CLI Reference

Complete reference for `cockpitctl` and `conformctl`.

## Global Options

`cockpitctl` supports standard clap flags:

```text
-h, --help       Print help information
-V, --version    Print version information
```

## Commands

### ingest

Read sensor receipts and produce aggregate outputs.

```bash
cockpitctl ingest [OPTIONS]
```

**Options:**

| Flag | Default | Description |
|------|---------|-------------|
| `--artifacts <PATH>` | `artifacts` | Artifacts root containing sensor receipts (`<artifacts>/<sensor_id>/report.json`) |
| `--config <PATH>` | `cockpit.toml` | Path to policy configuration file |
| `--label <LABEL>` | (none) | PR labels for label-gate evaluation (repeatable) |
| `--schema-validation <MODE>` | (unset) | Schema validation mode: `lax` or `strict` (overrides config only when provided) |
| `--github-annotations` | `false` | Emit GitHub Actions workflow command annotations to stdout |
| `--format <FORMAT>` | `cockpit` | Output format: `cockpit` or `sarif` |
| `--baseline <PATH>` | (none) | Path to a previous `cockpit.report.v1` file for trend comparison |
| `--buildfix-auto-apply` | `false` | Enable buildfix auto-apply for this run |
| `--buildfix-max-auto-apply-safety <LEVEL>` | (unset) | Override safety gate: `safe`, `guarded`, or `unsafe` |
| `--buildfix-actuator <COMMAND>` | (unset) | Override actuator command used for auto-apply |
| `--buildfix-actuator-timeout-ms <MS>` | (unset) | Override actuator timeout in milliseconds |
| `--policy-sign` | `false` | Enable policy snapshot signing for this run |
| `--policy-sign-key-path <PATH>` | (unset) | Override signing key file path |
| `--policy-sign-key-env <VAR>` | (unset) | Override signing key env var name |
| `--policy-sign-key-id <ID>` | (unset) | Override signing key identifier |

**Schema Validation Modes:**

- **`lax` (config default):** Skip JSON Schema validation. Receipts must still parse as JSON and deserialize into the receipt shape.
- **`strict`:** Validate against embedded `sensor.report.v1` schema bytes. Schema violations surface as `cockpit.schema_violation` findings.

> **Note:** Config is the default. CLI only overrides when explicitly passed.
> This includes buildfix apply controls and policy signing controls.

**Outputs:**

- `<artifacts>/cockpit/report.json` - Aggregate report (`cockpit.report.v1`)
- `<artifacts>/cockpit/comment.md` - Deterministic PR comment
- `<artifacts>/cockpit/sarif.json` - SARIF output (only when `--format sarif`)
- `<artifacts>/cockpit/buildfix.apply.json` - Buildfix auto-apply evidence (when buildfix apply is evaluated)
- `<artifacts>/cockpit/policy.signature.json` - Policy signature evidence (when policy signing is enabled)

**Exit Codes:**

| Code | Meaning |
|------|---------|
| `0` | Pass (or warn when `warn_is_fail = false`) |
| `2` | Policy failure (blocking sensor failed or warn-as-fail triggered) |
| `1` | Runtime error (I/O/config/command failure) |

**Examples:**

```bash
# Standard usage
cockpitctl ingest --artifacts artifacts --config cockpit.toml

# Force strict schema validation this run
cockpitctl ingest --schema-validation strict

# Include PR labels for label-gated sensors
cockpitctl ingest --label needs-perf-test

# Emit GitHub annotations and SARIF
cockpitctl ingest --github-annotations --format sarif

# Compute trend output against a baseline report
cockpitctl ingest --baseline artifacts-prev/cockpit/report.json

# Auto-apply safe buildfix plans with an explicit actuator command
cockpitctl ingest --buildfix-auto-apply --buildfix-actuator "buildfix-actuator --apply"

# Sign the policy snapshot using a key from environment
cockpitctl ingest --policy-sign --policy-sign-key-env COCKPITCTL_POLICY_SIGNING_KEY --policy-sign-key-id ci-key
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

Validate a receipt or report input.

```bash
cockpitctl validate [OPTIONS] --input <PATH>
```

**Options:**

| Flag | Default | Description |
|------|---------|-------------|
| `--input <PATH>` | required | Path to JSON file to validate |
| `--strict` | default | Perform JSON Schema validation |
| `--lax` | (none) | Skip JSON Schema validation; parse-only mode |

**Behavior:**

- In strict mode, validates against embedded JSON schemas.
- In lax mode, parses into Rust DTO shapes only.

**Example:**

```bash
# Validate a sensor receipt
cockpitctl validate --input artifacts/builddiag/report.json

# Validate a cockpit report
cockpitctl validate --input artifacts/cockpit/report.json
```

### explain

Explain cockpit finding codes.

```bash
cockpitctl explain <CODE|all>
```

**Examples:**

```bash
# Explain one code
cockpitctl explain cockpit.missing_receipt

# List all known cockpit codes
cockpitctl explain all
```

---

## conformctl

`conformctl` validates sensor receipts and cockpit reports against protocol rules.

### check

Validate a single sensor report.

```bash
conformctl check [OPTIONS] --report <PATH>
```

**Options:**

| Flag | Default | Description |
|------|---------|-------------|
| `--report <PATH>` | required | Path to the sensor report JSON file |
| `--sensor-id <ID>` | (none) | Expected sensor ID to verify |
| `--all` | (none) | Enable all checks |
| `--path-hygiene` | (none) | Reject absolute paths, backslashes, `..` traversal |
| `--ordering` | (none) | Verify findings are in canonical sort order |
| `--reason-lint` | (none) | Validate reason token format (`^[a-z0-9_]+$`) |
| `--survivability` | (none) | Require explanatory findings/reasons when `status=fail` |
| `--golden <FILE>` | (none) | Determinism check against a golden file |
| `--sensor-id-format` | (none) | Validate sensor ID format (`[a-zA-Z0-9_-]+`) |
| `--artifact-pointers` | (none) | Validate artifact pointer structure |
| `--tool-error-identity` | (none) | Require canonical `check_id`/`code` for `tool_error` |

**Example:**

```bash
conformctl check --report artifacts/builddiag/report.json --all --sensor-id builddiag
```

### check-dir

Validate all sensor reports in an artifacts directory.

```bash
conformctl check-dir [OPTIONS] --dir <PATH>
```

**Options:**

| Flag | Default | Description |
|------|---------|-------------|
| `--dir <PATH>` | required | Artifacts directory to scan |
| `--all` | (none) | Enable all checks |
| `--validate-cockpit` | (none) | Also validate the cockpit report |
| `--presence-semantics` | (none) | Validate presence semantics |
| All `check` flags | (none) | Applied to each discovered report |

**Example:**

```bash
conformctl check-dir --dir artifacts --all --validate-cockpit
```

## See Also

- [Exit Codes](exit-codes.md) - Detailed exit code semantics
- [Config Reference](config.md) - `cockpit.toml` format
