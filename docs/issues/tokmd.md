# tokmd — cockpitctl Protocol Adoption

## Overview

tokmd produces a **handoff manifest** — a structured summary of the repo suitable for LLM/AI consumption. Key integration concerns: declare git + handoff capabilities, write deterministic packs, and route manifest content through `artifacts[]` pointers rather than stuffing it into findings.

## P0 — Receipt Basics

- [ ] Emit `sensor.report.v1` envelope with all required fields
- [ ] Write to `artifacts/tokmd/report.json`
- [ ] `tool.name = "tokmd"`, `tool.version` follows semver
- [ ] `verdict.status` ∈ {pass, warn, fail, skip}; matches findings
- [ ] `verdict.counts` matches actual findings array tallies
- [ ] `run.started_at` is valid ISO 8601
- [ ] Finding `code` values are stable (e.g., `tokmd.manifest.generated`, `tokmd.pack.oversized`)
- [ ] Finding `severity` ∈ {info, warn, error}
- [ ] No path traversal in finding locations
- [ ] Exit 0 on success; non-zero only on tool crash

## P1 — Conformance & Ecosystem

- [ ] `verdict.reasons` tokens (e.g., `manifest_generated`, `pack_truncated`)
- [ ] Tool-error identity for `tool_error` reason
- [ ] Deterministic output: same repo state → byte-identical receipt and manifest
- [ ] Golden test committed
- [ ] Canonical finding order
- [ ] `artifacts[]` pointers for manifest and pack files: `{ "kind": "manifest", "path": "artifacts/tokmd/manifest.json" }`
- [ ] `run.capabilities`: `"git": true`, `"handoff": true`
- [ ] `run.git` populated when available
- [ ] Pack files are deterministic (stable key ordering, no timestamps in content)
- [ ] CI conformance gate with conformctl

## CI Snippet

```yaml
- name: Run tokmd
  run: tokmd pack --out artifacts/tokmd/

- name: Conformance gate
  run: |
    conformctl check \
      --report artifacts/tokmd/report.json \
      --sensor-id tokmd \
      --all
```

## Acceptance Criteria

- `conformctl check --all` exits 0
- Manifest and pack files listed in `artifacts[]`
- Deterministic output verified by golden test
- `run.capabilities` includes `git` and `handoff`
