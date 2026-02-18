# cockpitctl-io

Filesystem adapters and operational safety boundaries for cockpitctl.

## Scope
- Implements ingest ports using filesystem-backed adapters.
- Resolves layout and canonical output paths under `artifacts/`.
- Enforces safety controls (path traversal rejection, size caps, sensor count caps).
- Provides JSON Schema validation adapters for strict mode.
- Runs optional post-process hooks and buildfix actuator commands.

## Key exports
- `FsLayout`, `FsReceiptSource`, `FsPolicySource`, `FsOutputSink`
- `JsonSchemaValidator`
- `run_hooks`, `run_buildfix_actuator`, `load_policy_signing_key`
