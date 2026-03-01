# cockpitctl-io-hooks

Post-processor hook I/O boundary extracted from `cockpitctl-io`.

## Scope

- Runs post-processor hooks as subprocesses after ingest completes.
- Feeds the cockpit report JSON to each hook via stdin.
- Collects comment sections and extra output files from hook stdout.
- Sorts sections deterministically by (order, name).

## Architecture

This crate belongs to the **I/O adapter layer**. It bridges the ingest
pipeline to user-defined hook commands via process spawning.

## Key exports

- `run_hooks` — execute configured hooks and collect comment sections.
- `PostProcessOutput`, `CommentSection`, `OutputFile` — hook output types.

## Usage

```rust
use cockpitctl_io_hooks::run_hooks;

let sections = run_hooks(&hooks, &report_json, &output_sink)?;
for section in &sections {
    println!("{}: {}", section.name, section.content);
}
```

## Further reading

See the [cockpitctl repository](https://github.com/cockpitctl/cockpitctl) for full
documentation and architecture overview.

## License

Licensed under either of [Apache License, Version 2.0](../../LICENSE-APACHE) or
[MIT License](../../LICENSE-MIT), at your option.
