# Hexagonal Architecture

cockpitctl uses a hexagonal (ports and adapters) architecture with microcrates. This document explains the design.

## The Pattern

Hexagonal architecture separates:
- **Domain logic**: Pure business rules, no I/O
- **Ports**: Interfaces the domain needs
- **Adapters**: Implementations of those interfaces

Dependencies point inward: adapters depend on ports, ports depend on domain, domain depends on nothing external.

## Why This Architecture?

1. **Testability**: Domain logic can be tested without filesystem or network
2. **Flexibility**: Adapters can be swapped (filesystem, in-memory, remote)
3. **Clarity**: Clear boundaries prevent accidental coupling
4. **Change isolation**: Modifying I/O doesn't touch business rules

## Crate Map

```
┌──────────────────────────┐              ┌──────────────────┐
│     cockpitctl-cli       │              │    conformctl     │
│  (binary, clap, wiring)  │              │ (standalone bin)  │
└──────────────────────────┘              └──────────────────┘
              │                                    │
  ┌───────────┼───────────────┐                    │
  ▼           ▼               ▼                    │
┌──────┐ ┌──────────┐ ┌──────────┐                 │
│  io  │ │  ingest  │ │  render  │                 │
└──────┘ └──────────┘ └──────────┘                 │
  │           │             │                      │
  └───────────┼─────────────┘                      │
              ▼                                    │
       ┌────────────┐    ┌─────────────┐           │
       │   domain   │    │   conform   │◄──────────┘
       └────────────┘    └─────────────┘
              │                │
              └───────┬────────┘
                      ▼
               ┌────────────┐
               │    types   │
               └────────────┘

         cockpitctl-core re-exports all microcrates
```

### cockpitctl-types

DTOs, stable IDs, and shared ordering helpers.

- No external dependencies except `serde` and `time`
- Defines: `SensorReport`, `CockpitReport`, `Finding`, `Verdict`, etc.
- Implements: `Ord` for deterministic sorting

### cockpitctl-domain

Policy evaluation, highlight selection, normalization.

- Dependencies: `cockpitctl-types`, `sha2`, `hex`
- No filesystem, no clap, no network
- Pure functions that transform data

Key responsibilities:
- Compute overall verdict from sensor verdicts
- Apply warn-as-fail logic
- Select and cap highlights
- Derive fingerprints for findings without them
- Normalize paths for display

### cockpitctl-ingest

Use case boundary with ports (traits).

- Dependencies: `cockpitctl-types`, `cockpitctl-domain`
- Defines ports (traits) that adapters implement
- Orchestrates the ingest flow

**Ports:**

```rust
trait ReceiptSource {
    fn list_sensors(&self) -> Vec<SensorId>;
    fn read_report(&self, id: &SensorId) -> Result<Vec<u8>>;
    fn has_comment(&self, id: &SensorId) -> bool;
}

trait PolicySource {
    fn load(&self) -> Result<Policy>;
}

trait OutputSink {
    fn write_report(&self, report: &CockpitReport) -> Result<()>;
    fn write_comment(&self, comment: &str) -> Result<()>;
}
```

### cockpitctl-render

PR comment renderer with stable markers and truncation.

- Dependencies: `cockpitctl-types`
- Produces `comment.md` content
- Handles section ordering, truncation markers, repro lines

### cockpitctl-io

Filesystem adapters implementing the ports.

- Dependencies: `cockpitctl-ingest` (for port traits)
- Implements `ReceiptSource`, `PolicySource`, `OutputSink`
- Handles path safety, symlink checking, size limits

### cockpitctl-ops

Operational adapters for process execution and key loading.

- Dependencies: `cockpitctl-ingest`, `cockpitctl-types`
- Runs post-processing hooks and buildfix actuator commands
- Loads policy signing key material from path/env with normalization

### cockpitctl-cli

Binary entry point.

- Dependencies: all crates
- Uses `clap` for argument parsing
- Wires adapters to use cases
- Maps results to exit codes

### cockpitctl-conform

Conformance checking library.

- Dependencies: `cockpitctl-types`
- Schema validation, path hygiene, ordering, reason lint checks
- Used by both `conformctl` and `xtask`

### cockpitctl-core

Facade crate.

- Re-exports all microcrate APIs as a single dependency
- Convenience for downstream consumers

### conformctl

Standalone conformance checker binary.

- Dependencies: `cockpitctl-types`, `cockpitctl-conform`, `clap`
- `check` (single report) and `check-dir` (batch) subcommands
- Independent of the full cockpitctl workspace

### xtask

Development tooling.

- Schema checks, conformance harness, fixture regeneration
- Not part of the main binary

## The Rule

**Domain crates must not depend on clap, filesystem, or network.**

This is enforced by crate boundaries. If `cockpitctl-domain` tried to import `std::fs`, it would need to add that import, making the violation visible in review.

## Data Flow

```
1. CLI parses arguments
2. CLI creates adapters (FilesystemReceiptSource, TomlPolicySource, etc.)
3. CLI calls ingest use case with adapters
4. Ingest calls ports to load policy and discover receipts
5. Ingest calls domain to evaluate policy and select highlights
6. Ingest calls render to produce comment
7. Ingest calls output sink to write results
8. CLI maps result to exit code
```

## Testing at Each Layer

| Crate | Test Type | What's Tested |
|-------|-----------|---------------|
| types | Unit | Serialization, ordering |
| conform | Unit | Schema validation, path hygiene, ordering |
| domain | Unit, property | Business rules, determinism |
| ingest | Unit with mock ports | Use case flow |
| render | Snapshot | Comment format stability |
| io | Integration | Filesystem behavior |
| cli | Golden/BDD | End-to-end behavior |

## See Also

- [Why cockpitctl](why-cockpitctl.md) - Design philosophy
- [Composition Model](composition-model.md) - How sensors work together
