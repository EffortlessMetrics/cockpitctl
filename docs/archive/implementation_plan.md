# cockpitctl implementation plan

This is the staged, test-heavy plan to take `cockpitctl` from skeleton to "boring infrastructure".

## Milestone 0 — Repo scaffold + contracts

**Goal:** compile-ready workspace, docs, schemas, and a tiny CLI.

- [ ] Workspace microcrates created (types/domain/ingest/render/io/cli/xtask)
- [ ] Schemas committed:
  - `schemas/sensor.report.v1.json`
  - `schemas/cockpit.report.v1.json`
- [ ] Comment contract template committed:
  - `templates/cockpit.comment.v1.md`
- [ ] `cockpit.toml.example` committed
- [ ] `cockpitctl ingest` exists (no-op skeleton) and writes placeholder files
- [ ] CI runs: fmt, clippy, test

**Definition of done**
- "Hello ingest" produces `artifacts/cockpit/*` deterministically.

## Milestone 1 — Ingest core (receipt bus)

**Goal:** read receipts, normalize, compute verdicts.

- [ ] Receipt discovery (canonical paths)
- [ ] Receipt parsing into DTOs (serde)
- [ ] Policy load from `cockpit.toml` (toml)
- [ ] Missing receipt behavior per policy
- [ ] Invalid receipt surfaced as `cockpit.invalid_receipt`
- [ ] Per-sensor summary generation
- [ ] Overall verdict computation (blocking + warn-as-fail)
- [ ] Output `cockpit.report.v1` written

**Tests**
- Golden fixture: happy path with 2–3 sensor receipts
- Golden fixture: missing expected receipt
- Golden fixture: invalid receipt JSON

## Milestone 2 — Rendering contract (PR comment)

**Goal:** produce a stable, budgeted `comment.md`.

- [ ] Summary table (sensor, status, blocking, counts, links)
- [ ] Highlights selection + dedupe + cap
- [ ] Section ordering from config
- [ ] Repro lines per sensor (config)
- [ ] Truncation markers
- [ ] Sticky markers (`<!-- cockpit:begin -->` / `<!-- cockpit:end -->`)

**Tests**
- Snapshot/golden: comment output stable byte-for-byte

## Milestone 3 — Determinism, caps, and robustness

**Goal:** eliminate churn and failure spikes.

- [ ] Stable sorting utilities for sensors/findings/highlights
- [ ] Derived fingerprint for findings missing fingerprint
- [ ] Receipt size caps and sensor count caps
- [ ] Symlink escape protections (no traversal outside artifacts dir)
- [ ] "Counts reconciliation" (receipt_inconsistent reason)

**Tests**
- Property tests for ordering determinism
- Unit tests for fingerprint derivation
- Fixtures for truncation behavior

## Milestone 4 — Conformance harness + BDD

**Goal:** lock behavior, not just types.

- [ ] `cockpitctl validate` command
- [ ] BDD scenarios (cucumber):
  - missing receipts
  - warn-as-fail
  - label-gated sensors
  - highlight cap
- [ ] Fuzz harness (cargo-fuzz) for JSON parsing robustness
- [ ] Mutation testing (cargo-mutants) guidance and CI job (optional)

## Milestone 5 — Distribution and ecosystem integration

**Goal:** make adoption a one-paste workflow without contaminating the director.

- [ ] Release binaries (Linux/macOS/Windows)
- [ ] Reusable workflow/composite action (separate repo or `workflow/` folder)
- [ ] Documentation:
  - how to run sensors
  - how to run ingest
  - how to post sticky comment + check summary
- [ ] Compatibility promise documented for v1 contracts

## Backlog / later

- JSON schema validation (optional feature)
- `cockpitctl explain` for cockpit-level codes (missing/invalid)
- SARIF render for highlights (optional)
- "Policy snapshot signing" (if you need tamper-evidence)
