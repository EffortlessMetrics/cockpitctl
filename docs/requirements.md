# cockpitctl requirements

This document is the **source of truth** for what `cockpitctl` must do.

## Purpose

`cockpitctl` is the **director**: it turns many independent sensor receipts into a **single merge decision**
and a **single PR surface**, with strict noise budgets.

It exists so teams do not have to reason about a pile of tools.
They reason about **one cockpit**.

## Users

- Reviewers: want a short, stable PR comment that points to evidence.
- Maintainers: want deterministic output, bounded noise, and predictable policy.
- CI operators: want a single step that gates merges (exit code) and writes canonical artifacts.
- Tool authors: want a stable bus so sensors can ship independently.

## Non-goals

`cockpitctl` is deliberately narrow.

- **No running sensors** (no installs, no subprocess orchestration by default).
- **No network** (no GitHub API calls; posting comments is a workflow concern).
- **No tool-specific parsing beyond the envelope** (`data` is opaque).
- **No policy baked into the binary** (policy is config; defaults only).
- **No “smart triage”** that changes meaning across versions without a schema bump.

## Inputs

### Required
- `artifacts/` directory containing zero or more sensor receipts at:
  - `artifacts/<sensor_id>/report.json` (canonical)
- Optional policy file:
  - `cockpit.toml`

### Optional
- `artifacts/<sensor_id>/comment.md` (sensor-provided markdown, link-only)
- CI context (base/head SHA, labels, workflow URLs) **supplied out-of-band**.
  `cockpitctl` may accept them as flags/env vars to populate metadata, but must not fetch them.

## Outputs

`cockpitctl ingest` MUST produce:

- `artifacts/cockpit/report.json` conforming to `cockpit.report.v1`
- `artifacts/cockpit/comment.md` conforming to the comment contract v1

`cockpitctl` MUST be deterministic:
given identical inputs (receipts + config), outputs are byte-stable.

## Contracts (treated as API)

`cockpitctl` MUST treat these as stable interfaces:

- `schemas/sensor.report.v1.json` (sensor receipt envelope)
- `schemas/cockpit.report.v1.json` (director output)
- `templates/cockpit.comment.v1.md` (comment contract)

### Receipt envelope semantics

- `verdict.status` is one of: `pass | warn | fail | skip`
- Tool/runtime failure is represented as:
  - process exit code `1`
  - receipt emitted where possible with `verdict.status="fail"`, `reasons=["tool_error"]`
  - one canonical finding `tool.runtime_error`

`cockpitctl` MUST NOT invent additional verdict states.

### Extension discipline

Sensor receipts MUST be treated as strict at the top level.
Tool-specific payload MUST only appear under:
- report-level `data`
- finding-level `data`

`cockpitctl` MUST NOT depend on tool-specific payload fields.

## Composition policy

Composition policy is read from `cockpit.toml`.

Policy MUST support:
- blocking vs informational sensors
- missing receipt behavior per sensor: `skip | warn | fail`
- warn-as-fail (global or per sensor)
- budgets:
  - max highlights (global)
  - max surfaced findings per sensor
  - max annotations (global)
  - per-section caps (optional; staged)

Policy SHOULD support:
- label gates (enable a sensor only when a label is present)
- section ordering
- per-sensor repro command lines

## Ingest behavior

`cockpitctl` MUST:
- discover receipts at canonical paths
- parse and normalize receipts
- tolerate missing receipts and invalid receipts (surface them as findings, do not crash)
- compute per-sensor verdicts and overall verdict
- select and cap highlights deterministically
- render a PR comment that is:
  - short
  - stable (no random ordering)
  - link-first (details live in artifacts)
  - clearly indicates truncation

## Exit code semantics

- `0`: overall verdict passes (or warns when warn-as-fail is false)
- `2`: overall verdict fails due to policy (blocking failures or warn-as-fail)
- `1`: cockpitctl runtime error (cannot read/write required paths, cannot load config, etc.)

`cockpitctl` SHOULD still write `artifacts/cockpit/*` on policy failures.
It MAY refuse to write outputs on runtime errors where correctness cannot be guaranteed.

## Determinism requirements

- Sensor discovery order MUST be stable (lexical).
- Findings ordering MUST be stable using a documented sort key:
  `severity desc → sensor_id → path → line → code → message`.
- Highlight selection and truncation MUST be stable.
- Output JSON formatting MUST be stable (pretty printing is fine; just be consistent).

## Safety and robustness

Treat receipts as untrusted inputs.

- Refuse path traversal (`..`) in sensor IDs or receipt paths.
- Avoid following symlinks out of the artifacts root.
- Bound memory and time:
  - cap maximum receipt file size read (configurable; sane default)
  - cap number of receipts processed (configurable)
- Parsing errors MUST be surfaced as findings (`cockpit.invalid_receipt`) and included in output.

## Performance targets

- O(number_of_receipts + number_of_findings) ingestion.
- Expected runtime: under ~100ms for typical PRs (excluding IO).
- Must not read large tool-specific artifacts by default.
