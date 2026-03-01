# cockpitctl-io-buildfix

Buildfix actuator adapter boundary extracted from `cockpitctl-io`.

## Scope

- Runs an external buildfix actuator command as a subprocess.
- Serializes `BuildfixApplyRequest` to the command's stdin as JSON.
- Parses `BuildfixActuatorResult` from the command's stdout.
- Enforces a configurable timeout and deterministic result ordering.

## Architecture

This crate belongs to the **I/O adapter layer**. It bridges domain buildfix
decisions to external actuator commands via process spawning.

## Key exports

- `run_buildfix_actuator` — execute the configured actuator and return results.

## Usage

```rust
use cockpitctl_io_buildfix::run_buildfix_actuator;
use cockpitctl_types::BuildfixActuatorConfig;

let config = BuildfixActuatorConfig {
    command: "my-buildfix-tool apply".into(),
    timeout_ms: 30_000,
};
let result = run_buildfix_actuator(&config, &request)?;
println!("applied: {:?}", result.applied_fix_ids);
```

## Further reading

See the [cockpitctl repository](https://github.com/cockpitctl/cockpitctl) for full
documentation and architecture overview.

## License

Licensed under either of [Apache License, Version 2.0](../../LICENSE-APACHE) or
[MIT License](../../LICENSE-MIT), at your option.
