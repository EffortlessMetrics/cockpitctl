# depguard — cockpitctl Protocol Adoption

## Overview

depguard audits dependency trees for vulnerabilities, license issues, and policy violations. Key integration concerns: large dependency trees should go in `data` (not bloat findings), ratchet-friendly defaults for advisory counts, and structured fix hints in finding `data`.

## P0 — Receipt Basics

- [ ] Emit `sensor.report.v1` envelope with all required fields
- [ ] Write to `artifacts/depguard/report.json`
- [ ] `tool.name = "depguard"`, `tool.version` follows semver
- [ ] `verdict.status` ∈ {pass, warn, fail, skip}; matches findings
- [ ] `verdict.counts` matches actual findings array tallies
- [ ] `run.started_at` is valid ISO 8601
- [ ] Finding `code` values are stable (e.g., `depguard.vuln.critical`, `depguard.license.forbidden`)
- [ ] Finding `severity` ∈ {info, warn, error}
- [ ] No path traversal in finding locations
- [ ] Exit 0 on success; non-zero only on tool crash

## P1 — Conformance & Ecosystem

- [ ] `verdict.reasons` tokens (e.g., `vuln_found`, `license_violation`)
- [ ] Tool-error identity for `tool_error` reason
- [ ] Deterministic output given identical lockfile + config
- [ ] Golden test committed
- [ ] Canonical finding order
- [ ] Large dependency trees in `data` (top-level), not as individual findings — keeps receipt compact
- [ ] Structured fix hints: `finding.data.fix` with `{ "action": "upgrade", "from": "1.0.0", "to": "1.1.0" }` where available
- [ ] Ratchet-friendly: advisory-level findings default to `warn` not `error`, so teams can adopt incrementally
- [ ] `run.capabilities`: `"lockfile": true`, optionally `"audit_db": true`
- [ ] CI conformance gate with conformctl

## CI Snippet

```yaml
- name: Run depguard
  run: |
    depguard check \
      --profile team \
      --scope diff \
      --out artifacts/depguard/report.json

- name: Conformance gate
  run: |
    conformctl check \
      --report artifacts/depguard/report.json \
      --sensor-id depguard \
      --all
```

## Acceptance Criteria

- `conformctl check --all` exits 0
- Dependency tree data in `data`, not findings
- Fix hints present in finding `data.fix` where applicable
- Golden test passes
