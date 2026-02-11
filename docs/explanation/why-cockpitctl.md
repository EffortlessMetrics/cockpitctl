# Why cockpitctl

This document explains the problem space, design philosophy, and deliberate constraints of cockpitctl.

## The Problem

Modern repositories run many checks: linters, type checkers, test suites, security scanners, coverage tools, policy enforcers. Each tool produces its own output format, has its own failure modes, and demands its own attention.

Reviewers face:
- Multiple CI jobs to check
- Inconsistent output formats
- Noisy comments that obscure signal
- "Green by omission" when a tool silently fails

Maintainers face:
- Flaky merge gates from tool instability
- Unpredictable comment churn
- No unified policy for what blocks merges
- Difficulty adding new tools without disrupting workflow

## The Solution

cockpitctl is a **director**: it turns many independent sensor receipts into a **single merge decision** and a **single PR surface**, with strict noise budgets.

Teams reason about **one cockpit**, not a pile of tools.

## Users

cockpitctl serves four audiences:

| User | Wants |
|------|-------|
| Reviewers | Short, stable PR comment pointing to evidence |
| Maintainers | Deterministic output, bounded noise, predictable policy |
| CI operators | Single step that gates merges (exit code) and writes canonical artifacts |
| Tool authors | Stable bus so sensors can ship independently |

## Design Philosophy

### Composition, Not Orchestration

cockpitctl does not run tools. It reads receipts that tools produce.

This separation means:
- Tool authors control their execution (installs, runtimes, caching)
- cockpitctl focuses on composition policy
- Failures are isolated: a broken tool doesn't break cockpitctl

### Receipts as Truth

Sensors emit receipts; cockpitctl composes them. cockpitctl does not:
- Execute builds or tests
- Interpret tool-specific payloads
- Fetch from the network
- Mutate the repository

This keeps the director simple, fast, and auditable.

### Policy is Configuration

cockpitctl has no baked-in opinions about what should block merges. All policy comes from `cockpit.toml`:
- Which sensors are blocking
- What missing means
- How much noise is acceptable

This makes policy explicit, version-controlled, and reviewable.

### Determinism as Feature

Given identical inputs, cockpitctl produces byte-identical outputs. This is not an accident; it's a requirement.

Determinism enables:
- Reproducible builds
- Meaningful diffs of outputs
- Stable PR comments (no spurious updates)
- Easier debugging

### Surface Problems, Don't Hide Them

When something goes wrong (missing receipt, invalid JSON, oversized file), cockpitctl:
1. Emits a finding describing the issue
2. Continues processing
3. Includes the problem in the output

"Green by omission" is the enemy. If a sensor is expected but missing, that's visible.

## Non-Goals

cockpitctl is deliberately narrow:

| Non-goal | Rationale |
|----------|-----------|
| Running sensors | Tool installs, runtimes, and subprocess orchestration are workflow concerns |
| Network calls | Posting comments is a workflow concern; cockpitctl produces artifacts |
| Tool-specific parsing | The `data` field is opaque; cockpitctl only knows the envelope |
| Baked-in policy | Policy is configuration, not code |
| "Smart triage" | No magic that changes meaning across versions without a schema bump |

## The Ecosystem

cockpitctl sits at the center of an ecosystem:

```
┌─────────┐  ┌─────────┐  ┌─────────┐
│ Sensor  │  │ Sensor  │  │ Sensor  │
│   A     │  │   B     │  │   C     │
└────┬────┘  └────┬────┘  └────┬────┘
     │            │            │
     ▼            ▼            ▼
     artifacts/<sensor>/report.json
                  │
                  ▼
            ┌──────────┐
            │ cockpit  │ ← cockpit.toml
            │   ctl    │
            └────┬─────┘
                 │
                 ▼
    artifacts/cockpit/report.json
    artifacts/cockpit/comment.md
                 │
                 ▼
         ┌───────────────┐
         │ CI Workflow   │
         │ (post comment)│
         └───────────────┘
```

Sensors ship independently. cockpitctl ships independently. Workflows wire them together.

## See Also

- [Hexagonal Architecture](hexagonal-architecture.md) - How the code is organized
- [Composition Model](composition-model.md) - How sensors compose
- [Trust Boundaries](trust-boundaries.md) - Security model
