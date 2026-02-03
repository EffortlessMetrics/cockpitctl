# Customize the PR Comment

This guide shows how to configure the PR comment that cockpitctl generates.

## Comment Structure

The generated comment has these sections:

```markdown
<!-- cockpit:begin -->
## Cockpit

### Summary
| Sensor | Status | Blocking | Notes |
...

### Highlights (capped)
1. **sensor**: `code` at `path:line` — "message"
...

### Sections
#### Section Name
- sensor: summary (link)
...

<!-- cockpit:end -->
```

## Section Ordering

Control section order in `cockpit.toml`:

```toml
[policy]
section_order = [
  "Highlights",
  "Repo contract",
  "Dependencies",
  "Policy",
  "Tests",
  "Diagnostics",
  "Performance",
  "Environment",
  "Other"
]
```

Sensors appear in their configured section:

```toml
[sensors.builddiag]
section = "Repo contract"

[sensors.covguard]
section = "Tests"
```

Sensors without a section go to "Other".

## Highlight Count

Control how many highlights appear:

```toml
[policy]
max_highlights = 7
```

Highlights are:
- Selected from all sensors
- Sorted by severity (errors first)
- Blocking sensors prioritized
- Deduplicated by fingerprint

Set to `0` to disable highlights entirely.

## Per-Sensor Finding Limits

Control findings surfaced per sensor:

```toml
[policy]
max_per_sensor_findings = 20
```

When exceeded:
- Sensor is marked as `truncated: true`
- Comment shows "top N shown; see artifacts"
- Full findings remain in the receipt

## Repro Commands

Add local reproduction commands:

```toml
[sensors.builddiag]
repro = "cargo run -p builddiag -- check"

[sensors.diffguard]
repro = "diffguard diff HEAD~1"
```

These appear in the comment:

```markdown
#### Repo contract
- builddiag: 2 issues ([report](artifacts/builddiag/report.json))
  ```
  cargo run -p builddiag -- check
  ```
```

## Sticky Comments

The comment uses markers for "sticky" updates:

```markdown
<!-- cockpit:begin -->
...
<!-- cockpit:end -->
```

When posting with `gh pr comment --edit-last`, GitHub updates the existing comment instead of creating a new one.

## Minimal Comment

For the smallest useful comment:

```toml
[policy]
max_highlights = 3
max_per_sensor_findings = 5
section_order = ["Highlights", "Other"]
```

## Verbose Comment

For maximum detail:

```toml
[policy]
max_highlights = 20
max_per_sensor_findings = 50
section_order = [
  "Highlights",
  "Repo contract",
  "Dependencies",
  "Policy",
  "Tests",
  "Diagnostics",
  "Performance",
  "Environment",
  "Other"
]
```

## Hiding Sections

To hide a section, don't assign any sensors to it:

```toml
[policy]
# No "Performance" in section_order hides it
section_order = ["Highlights", "Repo contract", "Tests", "Other"]
```

Or assign sensors to "Other":

```toml
[sensors.perf-bench]
section = "Other"  # Won't appear in a "Performance" section
```

## Linking to Artifacts

The comment links to:
- `artifacts/<sensor>/report.json` for each sensor
- `artifacts/<sensor>/comment.md` if it exists

Ensure these are uploaded as CI artifacts for links to work.

## Comment Contract

The comment format is versioned. cockpitctl v1 produces:
- Markers: `<!-- cockpit:begin -->` / `<!-- cockpit:end -->`
- Summary table format
- Highlights list format
- Section structure

Breaking changes require a contract version bump.

## See Also

- [Config Reference](../reference/config.md) - Full policy options
- [Integrate with GitHub Actions](integrate-github-actions.md) - Posting comments
