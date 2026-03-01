# cockpitctl-core

Facade crate that re-exports cockpitctl microcrates through a single dependency.

## Scope
- Re-exports `types`, `domain`, `ingest`, `io`, `exec`, `schema`, `render`, and `sarif` modules.
- Provides flattened access to commonly used types and helpers.

## When to use this crate
- You want cockpitctl as a library without wiring each microcrate manually.

## Key exports
- `IngestUseCase`, `IngestRequest`, `IngestResult`
- `CockpitConfig`, `CockpitReport`, `SensorReport`
- `render_comment`, `render_github_annotations`
- `run_hooks`, `run_buildfix_actuator`, `JsonSchemaValidator`
- `cockpit_report_to_sarif_json`
