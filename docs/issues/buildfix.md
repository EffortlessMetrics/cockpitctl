# buildfix — cockpitctl Protocol Adoption

## Overview

buildfix is an **actuator-as-sensor** — it proposes fixes (patches, config changes) and reports what it did. Key integration concerns: it plays the sensor role (emits a receipt) while also producing actionable outputs (plans, patches, comments). Route plan/patch/comment through `artifacts[]` pointers; use `finding_refs` to link fix findings back to originating sensor findings.

## P0 — Receipt Basics

- [ ] Emit `sensor.report.v1` envelope with all required fields
- [ ] Write to `artifacts/buildfix/report.json`
- [ ] `tool.name = "buildfix"`, `tool.version` follows semver
- [ ] `verdict.status` ∈ {pass, warn, fail, skip}; matches findings
- [ ] `verdict.counts` matches actual findings array tallies
- [ ] `run.started_at` is valid ISO 8601
- [ ] Finding `code` values are stable (e.g., `buildfix.plan.generated`, `buildfix.patch.applied`, `buildfix.patch.failed`)
- [ ] Finding `severity` ∈ {info, warn, error}
- [ ] No path traversal in finding locations
- [ ] Exit 0 on success; non-zero only on tool crash

## P1 — Conformance & Ecosystem

- [ ] `verdict.reasons` tokens (e.g., `plan_generated`, `patch_applied`, `no_fixes_available`)
- [ ] Tool-error identity for `tool_error` reason
- [ ] Deterministic output given identical inputs + fix rules
- [ ] Golden test committed
- [ ] Canonical finding order
- [ ] `artifacts[]` pointers for all outputs:
  - `{ "kind": "plan", "path": "artifacts/buildfix/plan.json", "media_type": "application/json" }`
  - `{ "kind": "patch", "path": "artifacts/buildfix/fix.patch", "media_type": "text/x-diff" }`
  - `{ "kind": "comment", "path": "artifacts/buildfix/comment.md", "media_type": "text/markdown" }`
- [ ] `finding.data.finding_refs` links fix findings to originating findings: `{ "sensor_id": "builddiag", "code": "builddiag.msrv.missing", "fingerprint": "..." }`
- [ ] `run.capabilities`: `"plan": true`, `"patch": true`, `"comment": true`
- [ ] CI conformance gate with conformctl

## CI Snippet

```yaml
- name: Run buildfix
  run: |
    buildfix propose \
      --artifacts artifacts \
      --out artifacts/buildfix/

- name: Conformance gate
  run: |
    conformctl check \
      --report artifacts/buildfix/report.json \
      --sensor-id buildfix \
      --all
```

## Acceptance Criteria

- `conformctl check --all` exits 0
- Plan, patch, and comment files listed in `artifacts[]`
- `finding.data.finding_refs` links back to originating sensor findings
- Golden test passes
- cockpitctl ingest handles buildfix receipt without warnings
