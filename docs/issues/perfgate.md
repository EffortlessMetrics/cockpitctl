# perfgate — cockpitctl Protocol Adoption

## Overview

perfgate runs performance benchmarks and detects regressions. Key integration concerns: handling missing baselines gracefully (no baseline ≠ failure), distinguishing counts-based vs sample-based metrics, and proper tool-error identity when the benchmark harness crashes.

## P0 — Receipt Basics

- [ ] Emit `sensor.report.v1` envelope with all required fields
- [ ] Write to `artifacts/perfgate/report.json`
- [ ] `tool.name = "perfgate"`, `tool.version` follows semver
- [ ] `verdict.status` ∈ {pass, warn, fail, skip}; matches findings
- [ ] `verdict.counts` matches actual findings array tallies
- [ ] `run.started_at` is valid ISO 8601
- [ ] Finding `code` values are stable (e.g., `perfgate.regression.throughput`, `perfgate.regression.latency_p99`)
- [ ] Finding `severity` ∈ {info, warn, error}
- [ ] No path traversal in finding locations
- [ ] Exit 0 on success; non-zero only on tool crash

## P1 — Conformance & Ecosystem

- [ ] `verdict.reasons` tokens (e.g., `regression_detected`, `no_baseline`, `tool_error`)
- [ ] Tool-error identity: benchmark harness crash → finding with `check_id: "tool.runtime"`, `code: "runtime_error"`
- [ ] No baseline handling: `verdict.status = "skip"` with reason `no_baseline`, not `fail`
- [ ] Deterministic output given identical benchmark results + config
- [ ] Golden test committed
- [ ] Canonical finding order
- [ ] Distinguish metric types in `finding.data`: `{ "metric_type": "counter" | "sample", "value": ..., "threshold": ... }`
- [ ] Heavy benchmark data (histograms, traces) in `data` or `artifacts[]`, not findings
- [ ] `run.capabilities`: `"benchmark": true`, optionally `"baseline": true`
- [ ] CI conformance gate with conformctl

## CI Snippet

```yaml
- name: Run perfgate
  run: |
    perfgate check \
      --results bench-results.json \
      --baseline baseline.json \
      --out artifacts/perfgate/report.json

- name: Conformance gate
  run: |
    conformctl check \
      --report artifacts/perfgate/report.json \
      --sensor-id perfgate \
      --all
```

## Acceptance Criteria

- `conformctl check --all` exits 0
- Missing baseline → `skip` verdict with `no_baseline` reason
- Benchmark harness crash → `tool_error` with canonical identity
- Golden test passes
