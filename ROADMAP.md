# Roadmap

This roadmap describes planned work for cockpitctl. Items are grouped by theme
and roughly ordered by priority. Nothing here is a commitment — priorities shift
as the project and its users evolve.

See `CHANGELOG.md` for what has already shipped.

## Completed milestones

These milestones from the original implementation plan are done:

- **M0 — Repo scaffold + contracts.** Workspace microcrates, schemas, comment
  template, CI.
- **M1 — Ingest core.** Receipt discovery, parsing, policy evaluation, missing
  and invalid receipt handling, verdict computation, `cockpit.report.v1` output.
- **M2 — Rendering contract.** Summary table, highlights, section ordering,
  repro lines, truncation, sticky markers.
- **M3 — Determinism, caps, robustness.** Stable sorting, derived fingerprints,
  receipt size/count caps, symlink/path-traversal protection, counts
  reconciliation.
- **M4 — Conformance harness + BDD.** `validate` command, `xtask conform` /
  `conform-dir`, 20+ BDD scenarios, fuzz harness, mutation testing guidance.
- **M5 — Distribution and ecosystem.** Multi-platform binaries, release
  workflow, Diataxis documentation, compatibility promise, reusable GitHub
  Action (`action.yml`).

---

## Near-term

### Configurable receipt size limit

`max_receipt_size_bytes` in `cockpit.toml` so teams can raise or lower the 2 MB
default per-pipeline without rebuilding.

### `cockpitctl explain` command

A diagnostic command that, given a finding code (e.g. `cockpit.missing_receipt`),
prints what it means, why it fires, and how to fix it. Useful for onboarding
sensor authors and debugging CI failures.

### Annotation output

Emit GitHub Actions `::warning` / `::error` workflow commands (or a SARIF file)
so findings appear as inline annotations on the PR diff, not just in the
comment. Controlled by `max_annotations` in policy.

---

## Mid-term

### SARIF export

Optional `--format sarif` flag on `ingest` that writes a SARIF log alongside the
cockpit report. Enables integration with GitHub Code Scanning, VS Code SARIF
Viewer, and other static-analysis dashboards.

### Buildfix plan integration

The `buildfix.plan.v1` schema is already defined. Future work:
- Ingest fix plans alongside sensor receipts.
- Surface fix suggestions (with safety levels: `safe`, `guarded`, `unsafe`) in
  the PR comment.
- Gate auto-apply on the safety level.

### Trend tracking

Compare the current cockpit report against a baseline (e.g. from the base
branch) to surface regressions and improvements:
- "Coverage dropped from 85 % → 82 %"
- "3 new findings since base"

Requires a baseline store or artifact download step.

### Plugin / extension hooks

Allow external tools to contribute post-processing steps (e.g. custom comment
sections, badge generation) without forking the director. Likely implemented as
a trait in `cockpitctl-core` that downstream crates can implement.

---

## Long-term / exploratory

### Policy snapshot signing

Cryptographic signature over the policy snapshot embedded in the cockpit report,
providing tamper evidence that the verdict was computed under a known policy.

### Receipt streaming

For very large monorepos with hundreds of sensors, stream receipts instead of
loading all into memory. This relaxes the current O(receipts) memory bound.

### Web dashboard

A lightweight UI that reads `cockpit.report.v1` files and renders an interactive
dashboard — historical trends, per-sensor drill-down, policy diff view.

---

## How to contribute

If you want to work on any of these items, open an issue to discuss the design
before sending a PR. See `CLAUDE.md` for build and test commands.
