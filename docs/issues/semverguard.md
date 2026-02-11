# semverguard — cockpitctl Protocol Adoption

## Overview

semverguard detects semver violations (breaking API changes without version bumps). Key integration concerns: distinguishing "baseline missing" (skip) from "actual semver violation" (fail), routing deep API diff reports through `data`/`artifacts[]`, and stable finding codes for violation categories.

## P0 — Receipt Basics

- [ ] Emit `sensor.report.v1` envelope with all required fields
- [ ] Write to `artifacts/semverguard/report.json`
- [ ] `tool.name = "semverguard"`, `tool.version` follows semver
- [ ] `verdict.status` ∈ {pass, warn, fail, skip}; matches findings
- [ ] `verdict.counts` matches actual findings array tallies
- [ ] `run.started_at` is valid ISO 8601
- [ ] Finding `code` values are stable (e.g., `semverguard.break.type_removed`, `semverguard.break.field_changed`)
- [ ] Finding `severity` ∈ {info, warn, error}
- [ ] Finding `location.path` is repo-relative
- [ ] Exit 0 on success; non-zero only on tool crash

## P1 — Conformance & Ecosystem

- [ ] `verdict.reasons` tokens (e.g., `breaking_change`, `no_baseline`)
- [ ] Tool-error identity for `tool_error` reason
- [ ] Baseline missing → `verdict.status = "skip"` with reason `no_baseline`, not `fail`
- [ ] Actual semver violation → `verdict.status = "fail"` with reason `breaking_change`
- [ ] Deterministic output given identical crate source + baseline
- [ ] Golden test committed
- [ ] Canonical finding order
- [ ] Deep API diff data in `data` or `artifacts[]` pointer, not inlined in findings
- [ ] `run.capabilities`: `"semver": true`, optionally `"baseline": true`
- [ ] CI conformance gate with conformctl

## CI Snippet

```yaml
- name: Run semverguard
  run: |
    semverguard check \
      --baseline baseline-api.json \
      --out artifacts/semverguard/report.json

- name: Conformance gate
  run: |
    conformctl check \
      --report artifacts/semverguard/report.json \
      --sensor-id semverguard \
      --all
```

## Acceptance Criteria

- `conformctl check --all` exits 0
- Missing baseline → `skip`, not `fail`
- Breaking changes correctly classified by finding code
- Deep diff data in `data`/`artifacts[]`, not bloating findings
- Golden test passes
