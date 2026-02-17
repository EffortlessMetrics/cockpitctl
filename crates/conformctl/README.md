# conformctl

Standalone conformance checker CLI for cockpitctl artifacts.

## Commands
- `check`: validate a single sensor receipt with selectable checks.
- `check-dir`: scan an artifacts directory and validate sensor receipts in batch.

## Typical usage
- `conformctl check --report artifacts/<sensor>/report.json --all --sensor-id <sensor>`
- `conformctl check-dir --dir artifacts --all --validate-cockpit`

## Exit behavior
- exit code `0` when all requested checks pass
- non-zero when any conformance check fails or runtime errors occur
