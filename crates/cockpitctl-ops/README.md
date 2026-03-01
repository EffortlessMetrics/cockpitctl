# cockpitctl-ops

Operational adapters for cockpitctl.

## Scope
- Runs optional post-process hooks with deterministic output ordering.
- Runs optional buildfix actuator commands for auto-apply flows.
- Loads policy signing keys from file/env with normalization.

## Key exports
- `run_hooks`, `run_buildfix_actuator`, `load_policy_signing_key`
- `PostProcessOutput`, `CommentSection`, `OutputFile`
