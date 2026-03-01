# cockpitctl-feature-grid

Shared feature-toggle grid definitions used by CLI BDD and future interoperability
layers.

The crate intentionally keeps feature-gating data and expected runtime state logic
in one place so feature matrices, BDD assertions, and runtime checks stay aligned.
