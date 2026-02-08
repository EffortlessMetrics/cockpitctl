# Determinism Contract

All cockpitctl output must be byte-stable given identical inputs. This document specifies the ordering, deduplication, truncation, and timestamp rules that guarantee determinism.

## Findings Sort Key

Per-sensor findings are sorted by the following composite key (ascending, with severity descending):

1. `severity` — descending: `error` (0) > `warn` (1) > `info` (2)
2. `sensor_id` — lexical ascending
3. `location.path` — lexical ascending (empty string if absent)
4. `location.line` — numeric ascending (`u32::MAX` if absent)
5. `code` — lexical ascending
6. `message` — lexical ascending

Implementation: `FindingSortKey` in `cockpitctl-types`, applied by `sort_findings()` in `cockpitctl-domain`.

## Highlight Sort Key

Highlights (cross-sensor top findings) are sorted by:

1. `severity` — descending: `error` (0) > `warn` (1) > `info` (2)
2. `blocking` — blocking sensors first (0) before non-blocking (1)
3. `sensor_id` — lexical ascending
4. `location.path` — lexical ascending (empty string if absent)
5. `location.line` — numeric ascending (`u32::MAX` if absent)
6. `code` — lexical ascending
7. `message` — lexical ascending

Implementation: `select_highlights()` in `cockpitctl-domain`.

## Sensor Summary Order

Sensor summaries in the cockpit report are sorted by:

1. `section_order` rank — index in `policy.section_order` (unmatched sections sort last via `usize::MAX`)
2. `id` — lexical ascending within the same section

Section assignment: each sensor's `section` field from policy config, defaulting to `"Other"` if absent.

Implementation: `sort_sensor_summaries()` in `cockpitctl-domain`.

## Deduplication

Highlights are deduplicated by fingerprint before sorting and truncation.

- If a finding has an explicit `fingerprint` field, that value is used.
- If absent, a derived fingerprint is computed via SHA-256 over the concatenation (newline-separated):
  - `sensor_id`
  - `code`
  - `message`
  - `location.path` (if present)
  - `location.line` (if present, as decimal string)

First occurrence wins; duplicates are discarded.

Implementation: `derive_fingerprint()` in `cockpitctl-domain`.

## Truncation Budgets

Three policy-level caps control output size:

| Field | Default | Scope |
|-------|---------|-------|
| `max_highlights` | 7 | Total highlights in the cockpit report |
| `max_per_sensor_findings` | 20 | Findings surfaced per sensor |
| `max_annotations` | 25 | Annotations rendered in the PR comment |

- Truncation is applied after sorting and deduplication.
- When truncation occurs, the sensor summary sets `truncated: true`.
- The PR comment notes truncation inline when annotation limits are hit.

## Reproducible Timestamps

cockpitctl reads timestamps in the following priority order:

1. **`COCKPITCTL_STARTED_AT`** — if set, used verbatim as `run.started_at` (RFC 3339 string).
2. **`SOURCE_DATE_EPOCH`** — if set, parsed as a Unix timestamp (seconds since epoch) and formatted as RFC 3339.
3. **`now_utc()`** — wall-clock time (non-reproducible fallback).

For golden tests and CI reproducibility, set `COCKPITCTL_STARTED_AT` to a fixed value.

## Invariant

Identical inputs (same receipts, same `cockpit.toml`, same environment variables) must produce byte-identical `cockpit/report.json` and `cockpit/comment.md`.

## See Also

- [Identity Specification](identity-spec.md) — vocabulary and fingerprint rules
- [Comment ABI](comment-abi.md) — PR comment determinism guarantee
