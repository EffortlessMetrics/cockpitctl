# cockpitctl-ingest

Ingest use case for composing sensor receipts into a cockpit report.

## Scope
- Defines the application flow from discovered receipts to deterministic outputs.
- Enforces precedence contract (`cockpit.toml` defaults, CLI explicit overrides).
- Emits `cockpit.report.v1` and rendered comment content through output ports.
- Maps verdict outcomes to process exit semantics.

## Boundaries
- Does not read or write the filesystem directly.
- Does not parse CLI arguments.
- Depends on adapters for receipt IO, config loading, output writing, and schema validation.

## Key exports
- `IngestUseCase`, `IngestRequest`, `IngestResult`
- Re-exported port contracts from `cockpitctl-ports` for compatibility
