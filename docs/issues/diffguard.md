# diffguard — cockpitctl Protocol Adoption

## Overview

diffguard is a **diff-aware policy sensor** — it scans the PR diff for forbidden patterns, secret leaks, and policy violations. Key integration concerns: receipt-first output (findings must land in the receipt, not just stdout), git capabilities declaration, and optional SARIF in extras.

## P0 — Receipt Basics

- [ ] Emit `sensor.report.v1` envelope with all required fields
- [ ] Write to `artifacts/diffguard/report.json`
- [ ] `tool.name = "diffguard"`, `tool.version` follows semver
- [ ] `verdict.status` ∈ {pass, warn, fail, skip}; matches findings
- [ ] `verdict.counts` matches actual findings array tallies
- [ ] `run.started_at` is valid ISO 8601
- [ ] Finding `code` values are stable (e.g., `diffguard.policy.forbidden_api`, `diffguard.secret.detected`)
- [ ] Finding `severity` ∈ {info, warn, error}
- [ ] Finding `location.path` is repo-relative; no `..` or absolute paths
- [ ] Exit 0 on success; non-zero only on tool crash

## P1 — Conformance & Ecosystem

- [ ] `verdict.reasons` tokens match `^[a-z0-9_]+$` (e.g., `forbidden_api_detected`, `secret_leak`)
- [ ] Tool-error identity for `tool_error` reason
- [ ] Deterministic output given identical diff + config
- [ ] Golden test committed
- [ ] Canonical finding order
- [ ] `run.capabilities`: declare `"git": true`; optionally `"sarif": true`
- [ ] `run.git` populated: `repo`, `head_sha`, `base_sha`, `head_ref`, `base_ref`
- [ ] If SARIF output is generated, add `artifacts[]` pointer: `{ "kind": "sarif", "path": "artifacts/diffguard/results.sarif", "media_type": "application/sarif+json" }`
- [ ] CI conformance gate with conformctl

## CI Snippet

```yaml
- name: Run diffguard
  run: |
    diffguard check \
      --base ${{ github.event.pull_request.base.sha }} \
      --head ${{ github.sha }} \
      --out artifacts/diffguard/report.json

- name: Conformance gate
  run: |
    conformctl check \
      --report artifacts/diffguard/report.json \
      --sensor-id diffguard \
      --all
```

## Acceptance Criteria

- `conformctl check --all` exits 0 for diffguard receipt
- `run.git` fields populated in CI receipts
- SARIF artifact pointer present when SARIF generation is enabled
- Golden test passes
