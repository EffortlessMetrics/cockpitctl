# Fuzzing

This directory is a scaffold for `cargo-fuzz` (optional).

Typical setup:

```bash
cargo install cargo-fuzz
cargo fuzz init
cargo fuzz run parse_receipt
```

Targets should focus on robustness and invariants:
- JSON parsing must not panic
- memory usage is bounded (use file size caps at IO boundary)
- invalid input yields a surfaced `cockpit.invalid_receipt` instead of crashing
