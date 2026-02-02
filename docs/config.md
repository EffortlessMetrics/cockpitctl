# cockpit.toml configuration

`cockpit.toml` is where merge governance lives.
Sensors emit observations. `cockpitctl` applies policy.

## Minimal shape

```toml
[policy]
warn_is_fail = false
max_highlights = 7
max_per_sensor_findings = 20
max_annotations = 25
section_order = ["Highlights", "Repo contract", "Other"]

[sensors.builddiag]
blocking = true
missing = "fail"
section = "Repo contract"
repro = "builddiag check --profile team"

[sensors.env-check]
blocking = false
missing = "skip"
section = "Environment"
```

## policy

- `warn_is_fail`: when true, a warning from a blocking sensor fails the cockpit.
- `max_highlights`: global cap on cross-sensor highlights in the comment.
- `max_per_sensor_findings`: cap on findings surfaced per sensor (receipt can still carry more).
- `max_annotations`: global cap for annotation emitters (future/optional).
- `section_order`: stable order of sections in the comment.

## sensors.<id>

- `blocking`: if true, this sensor participates in overall verdict.
- `missing`: what to do when the sensor is expected but its receipt is missing.
  - `skip` | `warn` | `fail`
- `section`: where the sensor appears in the PR comment.
- `repro`: a one-line command to reproduce locally.
- `require_label`: only enforce (or only run) when a label is present.
  The director treats this as “effectively skipped” when absent (policy-driven).
