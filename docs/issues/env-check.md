# env-check — cockpitctl Protocol Adoption

## Overview

env-check probes the build environment (compiler versions, installed tools, platform details). Key integration concerns: running without a predefined spec (discover and report), declaring probe capability keys, and writing a `comment.md` with repro commands for environment mismatches.

## P0 — Receipt Basics

- [ ] Emit `sensor.report.v1` envelope with all required fields
- [ ] Write to `artifacts/env-check/report.json`
- [ ] `tool.name = "env-check"`, `tool.version` follows semver
- [ ] `verdict.status` ∈ {pass, warn, fail, skip}; matches findings
- [ ] `verdict.counts` matches actual findings array tallies
- [ ] `run.started_at` is valid ISO 8601
- [ ] Finding `code` values are stable (e.g., `env-check.probe.rustc_version`, `env-check.probe.missing_tool`)
- [ ] Finding `severity` ∈ {info, warn, error}
- [ ] No path traversal in finding locations
- [ ] Exit 0 on success; non-zero only on tool crash

## P1 — Conformance & Ecosystem

- [ ] `verdict.reasons` tokens (e.g., `env_mismatch`, `tool_missing`)
- [ ] Tool-error identity for `tool_error` reason
- [ ] No-spec mode: when no spec is provided, report discovered values as `info` findings, verdict `pass`
- [ ] Deterministic output given identical environment
- [ ] Golden test committed (pin env values via mocks or snapshots)
- [ ] Canonical finding order
- [ ] `run.capabilities`: declare probe keys (e.g., `"rustc": true`, `"cargo": true`, `"node": true`)
- [ ] `run.host` populated: `os`, `arch`
- [ ] `comment.md` with repro commands: `artifacts/env-check/comment.md` containing commands to reproduce the detected environment state
- [ ] `data._cockpit` hints: key environment values for PR comment (e.g., `{ "rustc": "1.82.0", "os": "linux" }`)
- [ ] CI conformance gate with conformctl

## CI Snippet

```yaml
- name: Run env-check
  run: env-check probe --out artifacts/env-check/

- name: Conformance gate
  run: |
    conformctl check \
      --report artifacts/env-check/report.json \
      --sensor-id env-check \
      --all
```

## Acceptance Criteria

- `conformctl check --all` exits 0
- No-spec mode produces `pass` with `info` findings
- `comment.md` written with repro commands
- `run.host` and `run.capabilities` populated
- Golden test passes
