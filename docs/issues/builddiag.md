# builddiag — cockpitctl Protocol Adoption

## Overview

builddiag is the **reference sensor** — the first to adopt the cockpitctl protocol end-to-end. It checks MSRV, toolchain, resolver settings, and publish readiness. Because it's the reference, it must demonstrate every P0 and P1 contract requirement.

## P0 — Receipt Basics

- [ ] Emit `sensor.report.v1` envelope with all required fields
- [ ] Write to `artifacts/builddiag/report.json`
- [ ] `tool.name = "builddiag"`, `tool.version` follows semver
- [ ] `verdict.status` ∈ {pass, warn, fail, skip}; matches findings
- [ ] `verdict.counts` matches actual findings array tallies
- [ ] `run.started_at` is valid ISO 8601
- [ ] Finding `code` values are stable (e.g., `builddiag.msrv.missing`, `builddiag.resolver.v1`)
- [ ] Finding `severity` ∈ {info, warn, error}
- [ ] No path traversal in `location.path`; repo-relative only
- [ ] Exit 0 on success; non-zero only on tool crash

## P1 — Conformance & Determinism

- [ ] `verdict.reasons` tokens match `^[a-z0-9_]+$`
- [ ] Tool-error identity: `tool_error` reason → finding with `check_id: "tool.runtime"`, `code: "runtime_error"`
- [ ] Deterministic output: same inputs → byte-identical receipt
- [ ] Golden test committed: fixture + expected receipt in CI
- [ ] Findings sorted in canonical order (severity desc → sensor_id → path → line → code → message)
- [ ] `artifacts[]` pointers for any extra outputs
- [ ] `run.capabilities` declared (e.g., `"msrv": true`, `"resolver": true`, `"publish": true`)
- [ ] CI conformance gate: `conformctl check --report artifacts/builddiag/report.json --sensor-id builddiag --all`

## CI Snippet

```yaml
- name: Run builddiag
  run: builddiag check --profile team --out artifacts/builddiag/report.json

- name: Conformance gate
  run: |
    conformctl check \
      --report artifacts/builddiag/report.json \
      --sensor-id builddiag \
      --all
```

## Acceptance Criteria

- `conformctl check --report artifacts/builddiag/report.json --sensor-id builddiag --all` exits 0
- Golden test passes in CI
- cockpitctl ingest completes without warnings for builddiag receipt
