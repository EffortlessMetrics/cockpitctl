# cockpitctl-ingest-precedence

Small pure helpers for policy-vs-CLI precedence in ingest.

This crate centralizes the precedence contract:

- config defaults come from `cockpit.toml`
- CLI values override only when explicitly provided
