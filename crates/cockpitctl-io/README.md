# cockpitctl-io

Filesystem adapters and operational safety boundaries for cockpitctl.

## Scope
- Implements ingest ports using filesystem-backed adapters.
- Resolves layout and canonical output paths under `artifacts/`.
- Enforces safety controls (path traversal rejection, size caps, sensor count caps).
- Focuses on filesystem-backed ingest adapters and safety boundaries.
- Re-exports execution/schema adapters from dedicated microcrates for compatibility.

## Key exports
- `FsLayout`, `FsReceiptSource`, `FsPolicySource`, `FsOutputSink`
- `JsonSchemaValidator` (from `cockpitctl-schema`)
- `run_hooks`, `run_buildfix_actuator`, `load_policy_signing_key` (from `cockpitctl-exec`)
