# Sensor Authoring Checklist

A prioritized checklist for building sensors that integrate with cockpitctl. Complete **P0** before your first PR; tackle **P1** before GA; **P2** when you need it.

Cross-references link to the existing docs rather than duplicating content.

---

## P0 — Must ship (blocks first integration)

- [ ] **Emit `sensor.report.v1` envelope** — every field in the [schema reference](../reference/sensor-report-schema.md) marked *required* is present.
- [ ] **Write to `artifacts/<sensor_id>/report.json`** — sensor ID matches `[a-zA-Z0-9_-]+`, no `..` in path.
- [ ] **Set `tool.name` + `tool.version`** — name is the binary/crate name, version follows semver.
- [ ] **`verdict.status`** is one of `pass | warn | fail | skip`. Match findings: errors → `fail`, warnings only → `warn`, clean → `pass`.
- [ ] **`verdict.counts`** matches the actual findings array (info/warn/error tallies).
- [ ] **`run.started_at`** is a valid ISO 8601 timestamp.
- [ ] **Finding `code` values are stable** — never rename; deprecate and alias.
- [ ] **Finding `severity`** is one of `info | warn | error`.
- [ ] **No path traversal** — finding `location.path` values are repo-relative, no `..`, no absolute paths, no backslashes.
- [ ] **Exit 0** on success; non-zero only on tool crash (cockpitctl reads the receipt, not the exit code).

## P1 — Should ship (blocks GA / conformctl --all pass)

- [ ] **Reason tokens** — `verdict.reasons` entries match `^[a-z0-9_]+$`. Use `tool_error` for crashes. See [token registry](../../contracts/docs/tokens.md).
- [ ] **Tool-error identity** — when emitting `tool_error`, include a finding with `check_id: "tool.runtime"`, `code: "runtime_error"`. See [error handling](write-conformant-sensor.md#error-handling).
- [ ] **Deterministic output** — same inputs → byte-identical receipt. Pin timestamps via `COCKPITCTL_STARTED_AT` or `SOURCE_DATE_EPOCH` in tests.
- [ ] **Golden test** — commit a fixture + expected receipt; `diff` or `conformctl check --golden` in CI.
- [ ] **Canonical finding order** — sort findings by `severity desc → sensor_id → path → line → code → message`. Checked by `conformctl check --ordering --sensor-id <id>`.
- [ ] **`artifacts[]` pointers** — if the sensor writes extra files (SARIF, LCOV, patches), list them in the top-level `artifacts` array with `kind`, `path`, and optional `media_type`.
- [ ] **CI gate with conformctl** — add a step that runs `conformctl check --report artifacts/<sensor_id>/report.json --sensor-id <id> --all` (see CI snippet below).
- [ ] **`run.capabilities`** — declare what the sensor can provide (e.g., `"git": true`, `"sarif": true`). Keys are free-form; document yours in your sensor README.

## P2 — Nice-to-have (polish / ecosystem leverage)

- [ ] **`data._cockpit` hints** — surface key metrics for the PR comment (e.g., `{ "coverage_pct": 85.5 }`). cockpitctl passes this through; renderers may use it.
- [ ] **`run.git` context** — populate `repo`, `head_sha`, `base_sha`, `head_ref`, `base_ref` when available.
- [ ] **`run.ci` context** — populate `provider`, `run_id`, `run_url`, `job` when running in CI.
- [ ] **Optional `comment.md`** — write `artifacts/<sensor_id>/comment.md` for rich, sensor-specific detail; cockpitctl links but doesn't inline.
- [ ] **Fuzz the parser** — `cargo fuzz run parse_<input>` or equivalent.
- [ ] **Fingerprint stability** — if you set `finding.fingerprint`, use a deterministic hash (SHA-256 hex, 64 chars) over `(sensor_id, code, message, path, line)`.

---

## Canonical shapes (quick reference)

### `artifacts[]` entry

```json
{
  "kind": "sarif",
  "path": "artifacts/lintdiff/results.sarif",
  "media_type": "application/sarif+json"
}
```

### Tool-error identity finding

```json
{
  "severity": "error",
  "check_id": "tool.runtime",
  "code": "runtime_error",
  "message": "Failed to parse config: invalid TOML at line 5"
}
```

### `run.capabilities` example

```json
{
  "capabilities": {
    "git": true,
    "sarif": true,
    "fix_hints": false
  }
}
```

### `data._cockpit` hint

```json
{
  "data": {
    "_cockpit": {
      "coverage_pct": 85.5,
      "files_changed": 12
    },
    "internal_metrics": { "..." : "..." }
  }
}
```

---

## CI snippet — conformctl gate

Add this step after your sensor runs:

```yaml
- name: Conformance gate
  run: |
    conformctl check \
      --report artifacts/<sensor_id>/report.json \
      --sensor-id <sensor_id> \
      --all
```

For multi-sensor repos or integration tests:

```yaml
- name: Conformance gate (all sensors)
  run: |
    conformctl check-dir \
      --dir artifacts \
      --all \
      --validate-cockpit
```

---

## Vocabulary

| Term | Meaning |
|------|---------|
| **receipt** | The `sensor.report.v1` JSON file a sensor emits |
| **finding** | A single observation (issue, metric, annotation) |
| **verdict** | Aggregate pass/warn/fail/skip for the sensor run |
| **reason token** | Machine-readable tag in `verdict.reasons` (`^[a-z0-9_]+$`) |
| **artifact pointer** | Entry in `artifacts[]` referencing an extra output file |
| **capability** | Key in `run.capabilities` advertising sensor features |
| **sensor ID** | Directory name under `artifacts/`; matches `[a-zA-Z0-9_-]+` |
| **fingerprint** | Stable hash for dedup; SHA-256 hex, 64 chars |
| **golden test** | Byte-level comparison of receipt against committed expected output |
| **conformctl** | Standalone binary that validates receipts against the protocol |

---

## See Also

- [Write a Conformant Sensor](write-conformant-sensor.md) — step-by-step authoring guide
- [Test Sensor Conformance](test-sensor-conformance.md) — testing patterns + xtask harness
- [Sensor Report Schema](../reference/sensor-report-schema.md) — full field reference
- [Composition Model](../explanation/composition-model.md) — how sensors compose
