# cockpitctl-exec

Process execution adapters for cockpitctl.

This crate owns SRP runtime concerns that were previously bundled into filesystem adapters:

- Post-processor hook execution (`run_hooks`)
- Buildfix actuator execution (`run_buildfix_actuator`)
- Policy signing key material loading (`load_policy_signing_key`)
