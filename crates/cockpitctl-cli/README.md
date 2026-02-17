# cockpitctl

CLI binary for offline cockpit receipt ingestion.

## Commands
- `ingest`: compile sensor receipts into cockpit outputs.
- `init`: write starter `cockpit.toml`.
- `validate`: parse/validate a receipt or cockpit report JSON file.
- `explain`: explain cockpit synthesis codes.

## Outputs
- `artifacts/cockpit/report.json` (`cockpit.report.v1`)
- `artifacts/cockpit/comment.md`
- exit code `0` (pass), `2` (policy fail), `1` (runtime error)

## Notes
- This package also includes a compatibility library facade (`src/lib.rs`) that re-exports `cockpitctl-core`.
