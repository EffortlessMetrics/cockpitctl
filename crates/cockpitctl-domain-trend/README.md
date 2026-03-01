# cockpitctl-domain-trend

Trend computation domain boundary extracted from `cockpitctl-domain`.

## Scope

- Computes the delta between a baseline and current cockpit report.
- Identifies new, resolved, and unchanged findings across runs.
- Tracks verdict changes and count deltas per sensor.

## Architecture

This crate belongs to the **domain layer**. It contains pure comparison logic
with no filesystem, network, or CLI dependencies.

## Key exports

- `compute_trend` — compute `TrendDelta` between baseline and current reports.

## Usage

```rust
use cockpitctl_domain_trend::compute_trend;

let delta = compute_trend(&baseline_report, &current_report);
println!("new findings: {}", delta.new_findings.len());
println!("resolved:     {}", delta.resolved_findings.len());
```

## Further reading

See the [cockpitctl repository](https://github.com/cockpitctl/cockpitctl) for full
documentation and architecture overview.

## License

Licensed under either of [Apache License, Version 2.0](../../LICENSE-APACHE) or
[MIT License](../../LICENSE-MIT), at your option.
