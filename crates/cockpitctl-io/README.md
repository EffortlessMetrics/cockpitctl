# cockpitctl-io

Filesystem adapters and operational safety boundaries for cockpitctl ingest ports.

## Scope
- Implements ingest ports using filesystem-backed adapters.
- Resolves layout and canonical output paths under `artifacts/`.
- Enforces safety controls (path traversal rejection, size caps, sensor count caps).

## Key exports
- `FsLayout`, `FsReceiptSource`, `FsPolicySource`, `FsOutputSink`
