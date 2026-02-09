# lintdiff — cockpitctl Protocol Adoption

## Overview

lintdiff ingests compiler/linter diagnostics and reports only issues **introduced in the diff**. Key integration concerns: graceful handling of missing diagnostics input (skip, not crash), stable finding code mapping from tool-native codes, and optional SARIF output as an artifact.

## P0 — Receipt Basics

- [ ] Emit `sensor.report.v1` envelope with all required fields
- [ ] Write to `artifacts/lintdiff/report.json`
- [ ] `tool.name = "lintdiff"`, `tool.version` follows semver
- [ ] `verdict.status` ∈ {pass, warn, fail, skip}; matches findings
- [ ] `verdict.counts` matches actual findings array tallies
- [ ] `run.started_at` is valid ISO 8601
- [ ] Finding `code` values are stable (e.g., `lintdiff.clippy.needless_return`, `lintdiff.rustc.unused_variable`)
- [ ] Finding `severity` ∈ {info, warn, error}
- [ ] Finding `location.path` is repo-relative; no `..` or absolute paths
- [ ] Exit 0 on success; non-zero only on tool crash

## P1 — Conformance & Ecosystem

- [ ] `verdict.reasons` tokens (e.g., `new_warnings`, `no_diagnostics`)
- [ ] Tool-error identity for `tool_error` reason
- [ ] When diagnostics input is missing: `verdict.status = "skip"` with reason `no_diagnostics`, not a crash
- [ ] Deterministic output given identical diagnostics + diff
- [ ] Golden test committed
- [ ] Canonical finding order
- [ ] Stable code mapping: tool-native lint codes → `lintdiff.<tool>.<rule>` (never change the mapping)
- [ ] Optional SARIF: if generated, add `artifacts[]` pointer `{ "kind": "sarif", "path": "artifacts/lintdiff/results.sarif", "media_type": "application/sarif+json" }`
- [ ] `run.capabilities`: `"git": true`, optionally `"sarif": true`
- [ ] `run.git` populated: `head_sha`, `base_sha`
- [ ] CI conformance gate with conformctl

## CI Snippet

```yaml
- name: Run lintdiff
  run: |
    lintdiff ingest \
      --diagnostics clippy.jsonl \
      --base ${{ github.event.pull_request.base.sha }} \
      --head ${{ github.sha }} \
      --out artifacts/lintdiff/report.json

- name: Conformance gate
  run: |
    conformctl check \
      --report artifacts/lintdiff/report.json \
      --sensor-id lintdiff \
      --all
```

## Acceptance Criteria

- `conformctl check --all` exits 0
- Missing diagnostics input → `skip` verdict, not crash
- Finding codes are deterministically mapped from tool-native codes
- Golden test passes
