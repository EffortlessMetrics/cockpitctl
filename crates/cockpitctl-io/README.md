# cockpitctl-io

Filesystem adapters and operational safety boundaries for cockpitctl.

## Scope
- Implements ingest ports using filesystem-backed adapters.
- Resolves layout and canonical output paths under `artifacts/`.
- Enforces safety controls (path traversal rejection, size caps, sensor count caps).
- Re-exports execution and validation adapters from dedicated SRP microcrates.

## Key exports
- `FsLayout`, `FsReceiptSource`, `FsPolicySource`, `FsOutputSink`
- Re-exported validation adapter: `JsonSchemaValidator` (from `cockpitctl-validate`)
- Re-exported execution adapters: `run_hooks`, `run_buildfix_actuator`, `load_policy_signing_key` (from `cockpitctl-exec`)
