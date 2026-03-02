# cockpitctl-domain-codes

`cockpitctl-domain-codes` is a focused domain microcrate for the cockpit finding-code catalog.

It provides:

- Stable code constants in `cockpit_codes`
- `CodeExplanation` metadata
- Lookup helpers: `all_codes()` and `explain_code()`

This keeps explanatory text and code IDs decoupled from ingest/report synthesis logic.
