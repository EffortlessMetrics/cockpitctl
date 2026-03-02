# cockpitctl-feature-runtime

Small SRP crate for runtime feature state helpers used by BDD and CLI tests.

Provides:

- `feature_runtime_present` for evaluating compile-time + disable-flag state.
- `parse_feature_state` for parsing BDD tokens like `present`/`absent`.
