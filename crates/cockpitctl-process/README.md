# cockpitctl-process

Process execution adapters for cockpitctl:

- Post-processing hook execution (`run_hooks`)
- Buildfix actuator execution (`run_buildfix_actuator`)
- Policy signing key material loading (`load_policy_signing_key`)

This crate isolates subprocess orchestration concerns from filesystem adapters to preserve SRP boundaries.
