# Determinism

cockpitctl guarantees byte-stable output given identical inputs. This page documents the sort keys and ordering rules.

## Why Determinism Matters

See [Determinism Design](../explanation/determinism-design.md) for the rationale.

## Guarantees

Given identical inputs (receipts + config):
- `cockpit.report.json` is byte-identical
- `comment.md` is byte-identical
- Exit code is identical

## Sort Keys

### Sensor Discovery Order

Sensors are discovered in **lexical order** by sensor ID.

```
artifacts/
  alpha/report.json     # processed first
  beta/report.json      # processed second
  zeta/report.json      # processed last
```

### Findings Sort Order

Findings are sorted by this composite key:

```
severity DESC → sensor_id ASC → path ASC → line ASC → code ASC → message ASC
```

| Priority | Field | Direction | Notes |
|----------|-------|-----------|-------|
| 1 | severity | descending | error > warn > info |
| 2 | sensor_id | ascending | lexical |
| 3 | path | ascending | lexical |
| 4 | line | ascending | numeric |
| 5 | code | ascending | lexical |
| 6 | message | ascending | lexical |

Example order:
1. `error` from `alpha` at `src/a.rs:10`
2. `error` from `alpha` at `src/a.rs:20`
3. `error` from `beta` at `src/a.rs:5`
4. `warn` from `alpha` at `src/a.rs:1`

### Highlights Sort Order

Highlights use a slightly different key that prioritizes blocking sensors:

```
severity DESC → blocking DESC → sensor_id ASC → path ASC → line ASC → code ASC
```

| Priority | Field | Direction | Notes |
|----------|-------|-----------|-------|
| 1 | severity | descending | error > warn > info |
| 2 | blocking | descending | blocking first |
| 3 | sensor_id | ascending | lexical |
| 4 | path | ascending | lexical |
| 5 | line | ascending | numeric |
| 6 | code | ascending | lexical |

### Sensor Order in Report

Sensors in the output report are ordered by:

```
section_order index ASC → sensor_id ASC
```

Sensors in configured sections appear first (in section order), then any "Other" section sensors in lexical order.

## Fingerprint Derivation

When a finding lacks a `fingerprint` field, cockpitctl derives one:

```
sha256(sensor_id + code + path + line + col + message)
```

This derived fingerprint is used for:
- Deduplication in highlights
- Stable identification across runs

## JSON Formatting

- Pretty-printed with 2-space indentation
- Keys in definition order (not sorted alphabetically)
- No trailing whitespace
- Single trailing newline

## Comment Formatting

- Sections in `section_order` order
- Sensors within sections in lexical order
- Highlights in highlight sort order
- Tables have consistent column widths per run (may vary between runs)

## Testing Determinism

The test suite includes:
- Golden/snapshot tests that verify byte-stability
- Property-based tests that verify ordering is stable for random inputs
- Mutation testing that catches ordering bugs

## See Also

- [Determinism Design](../explanation/determinism-design.md) - Why this matters
