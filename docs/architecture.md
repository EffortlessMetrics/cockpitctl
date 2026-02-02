# cockpitctl architecture plan

This is the “system view” of `cockpitctl`: what it owns, how it composes with the other repos, and where the seams are.

## Position in the ecosystem

`cockpitctl` is the **director**.

- Sensors emit receipts to `artifacts/<sensor>/report.json`.
- `cockpitctl ingest` reads receipts and produces:
  - `artifacts/cockpit/report.json` (`cockpit.report.v1`)
  - `artifacts/cockpit/comment.md`

It is intentionally not a runner.
Tool installs, CI lanes, posting comments/checks live in a workflow/action.

## Truth layer boundaries

`cockpitctl` does not generate truth.
It composes truth that already exists in receipts.

It must not:
- execute builds/tests
- interpret tool-specific payloads
- fetch from the network
- mutate the repo

## Canonical artifact layout

```
artifacts/
  <sensor_id>/
    report.json        # required if sensor ran
    comment.md         # optional, linked by cockpit
    extras/...         # tool-specific
  cockpit/
    report.json        # cockpit.report.v1 (always written by ingest)
    comment.md         # PR comment contract v1
```

This layout is what makes ingestion boring.

## Contracts

`cockpitctl` treats these as ABI-level interfaces:

- `sensor.report.v1` receipt envelope
- `cockpit.report.v1` aggregate report
- PR comment contract v1 (section order + caps + markers)

## CLI surface

- `cockpitctl ingest`
  - reads artifacts + config
  - writes cockpit outputs
  - exit codes: 0/2/1 (pass/policy fail/runtime error)

- `cockpitctl validate`
  - developer tool to validate receipts and cockpit report structure

- `cockpitctl init`
  - writes a starter `cockpit.toml` (does not overwrite)

## Data flow

```
artifacts/*/report.json  +  cockpit.toml
             |
             v
       [ingest + normalize]
             |
             v
  cockpit.report.v1  +  cockpit comment.md
```

The director “compresses”:
- apply policy
- cap findings
- select highlights
- render stable sections

## Failure behavior

`cockpitctl` must surface problems without hiding them:

- missing expected receipt → synthesized sensor summary + cockpit finding
- invalid receipt JSON → synthesized sensor summary + cockpit finding
- runtime error (cannot read/write) → exit 1

The cockpit should avoid “green by omission”:
if policy expects a sensor and it is missing, it is visible.

## Security / robustness

Treat receipts as untrusted:
- refuse path traversal
- avoid symlink escapes
- cap receipt size and number processed
- never execute any content derived from receipts

## Observability

- `--verbose` logs:
  - discovery results
  - parse errors per sensor
  - policy decisions (blocking/missing/warn-as-fail)
  - highlight selection and truncation decisions

Avoid noisy logs by default; the report is the record.

## Compatibility and evolution

- Envelope changes are versioned: breaking changes require `v2`.
- Finding codes are stable: never rename, only deprecate/alias.
- `cockpit.report.v1` is additive-only within `v1`.
- Comment contract breaking changes bump contract marker (v2).

## Conformance harness

Every sensor repo should run conformance checks, but `cockpitctl` is where they become visible.

`cockpitctl` maintains:
- golden fixtures (mixed receipts → expected cockpit outputs)
- schema files (input + output)
