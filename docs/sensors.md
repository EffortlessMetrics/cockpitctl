# Sensors and composition

`cockpitctl` assumes a simple contract:

- Sensors write receipts to `artifacts/<sensor_id>/report.json`.
- The director reads those receipts, applies policy, and produces one cockpit.

## What a sensor must do

A sensor repo should:
- emit a `sensor.report.v1`-shaped envelope
- keep finding codes stable
- treat `data` as an extension point (schema it, but don't rely on the director)

A sensor must not:
- write outside its own artifacts directory
- assume the director will parse its internal payload

## Typical sensor categories

These map well to PR-review mental models:

- **Repo contract**: build graph, required files, structure constraints
- **Dependencies**: lockfile / policy enforcement
- **Policy**: diff-scoped rules and guardrails (the highest ROI category)
- **Tests**: coverage and test-related gates
- **Diagnostics**: lint deltas, typecheck deltas, localized warnings
- **Performance**: optional, label-gated
- **Environment**: informational, for diagnosis

## Suggested default sections

The default section order in `Policy` is:

- Highlights
- Repo contract
- Dependencies
- Policy
- Tests
- Diagnostics
- Performance
- Environment
- Other

You can override via `policy.section_order`.

## Recommended adoption tiers

- Tier 0: run sensors; save receipts; manually inspect artifacts
- Tier 1: add `cockpitctl ingest` and post `comment.md` as a sticky PR comment
- Tier 2: mark a small set of sensors blocking in `cockpit.toml`
- Tier 3: expand blocking set; add label gates for expensive checks
- Tier 4: conformance in sensor repos; golden fixtures in cockpitctl for “system tests”
