# cockpitctl-core

Facade crate that re-exports cockpitctl microcrates through a single dependency.

## Scope
- Re-exports `types`, `domain`, `explain`, `ingest`, `io`, `render`, and `sarif` modules.
- Provides flattened access to commonly used types and helpers, including finding-code explanations.

## When to use this crate
- You want cockpitctl as a library without wiring each microcrate manually.

## Key exports
- `IngestUseCase`, `IngestRequest`, `IngestResult`
- `CockpitConfig`, `CockpitReport`, `SensorReport`
- `explain::explain_code`, `explain::all_codes`
- `render_comment`, `render_github_annotations`
- `cockpit_report_to_sarif_json`
