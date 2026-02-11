# covguard — cockpitctl Protocol Adoption

## Overview

covguard measures code coverage and reports regressions. Key integration concerns: handling missing LCOV gracefully (skip vs fail), path normalization across platforms, and routing heavy payloads (per-file coverage maps) through `data` and `artifacts[]` rather than bloating findings.

## P0 — Receipt Basics

- [ ] Emit `sensor.report.v1` envelope with all required fields
- [ ] Write to `artifacts/covguard/report.json`
- [ ] `tool.name = "covguard"`, `tool.version` follows semver
- [ ] `verdict.status` ∈ {pass, warn, fail, skip}; matches findings
- [ ] `verdict.counts` matches actual findings array tallies
- [ ] `run.started_at` is valid ISO 8601
- [ ] Finding `code` values are stable (e.g., `covguard.coverage.below_threshold`, `covguard.coverage.no_tests`)
- [ ] Finding `severity` ∈ {info, warn, error}
- [ ] Finding `location.path` is repo-relative, forward-slash only
- [ ] Exit 0 on success; non-zero only on tool crash

## P1 — Conformance & Ecosystem

- [ ] `verdict.reasons` tokens (e.g., `below_threshold`, `no_baseline`, `lcov_missing`)
- [ ] Tool-error identity for `tool_error` reason
- [ ] When LCOV is missing: emit `verdict.status = "skip"` with reason `lcov_missing`, not a crash
- [ ] Deterministic output given identical LCOV + config
- [ ] Golden test committed
- [ ] Canonical finding order
- [ ] Path normalization: convert backslashes to forward slashes, strip common prefixes
- [ ] Heavy payloads (per-file coverage maps) in `data`, not as individual findings
- [ ] `data._cockpit.coverage_pct` for PR comment rendering
- [ ] `artifacts[]` pointer for LCOV: `{ "kind": "lcov", "path": "artifacts/covguard/lcov.info" }` if retained
- [ ] `run.capabilities`: `"lcov": true`, optionally `"git": true`
- [ ] `run.git` populated: `head_sha`, `base_sha` for regression detection
- [ ] CI conformance gate with conformctl

## CI Snippet

```yaml
- name: Run covguard
  run: |
    covguard check \
      --lcov lcov.info \
      --base ${{ github.event.pull_request.base.sha }} \
      --head ${{ github.sha }} \
      --out artifacts/covguard/report.json

- name: Conformance gate
  run: |
    conformctl check \
      --report artifacts/covguard/report.json \
      --sensor-id covguard \
      --all
```

## Acceptance Criteria

- `conformctl check --all` exits 0
- Missing LCOV produces `skip` verdict, not a crash
- `data._cockpit.coverage_pct` present in receipt
- Path separators normalized to `/`
- Golden test passes
