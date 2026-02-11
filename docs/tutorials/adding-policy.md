# Adding Policy

This tutorial walks you through creating and configuring `cockpit.toml` for your repository.

## What You'll Learn

- Policy file structure
- How to mark sensors as blocking
- How to configure missing receipt behavior
- How to set budgets and section order

## Prerequisites

- Completed [Getting Started](getting-started.md)
- cockpitctl working with at least one sensor

## Step 1: Generate a Starter Config

Use the `init` command:

```bash
cockpitctl init --path cockpit.toml
```

This creates a minimal config:

```toml
[policy]
warn_is_fail = false
max_highlights = 7
max_per_sensor_findings = 20
```

## Step 2: Understand the Structure

The config has two main sections:

```toml
[policy]           # Global settings
# ...

[sensors.<id>]     # Per-sensor settings
# ...
```

## Step 3: Add Your Sensors

For each sensor that writes to `artifacts/<id>/report.json`, add a section:

```toml
[sensors.builddiag]
blocking = true
missing = "fail"

[sensors.clippy]
blocking = true
missing = "warn"

[sensors.coverage]
blocking = false
missing = "skip"
```

### Choosing blocking

Ask: "Should a failure in this sensor block the merge?"

| Sensor Type | Typically blocking? |
|-------------|---------------------|
| Build checks | Yes |
| Type errors | Yes |
| Security scans | Yes |
| Coverage | Depends on policy |
| Performance | Usually no |
| Informational | No |

### Choosing missing

Ask: "What if this sensor doesn't produce a receipt?"

| Scenario | Use |
|----------|-----|
| Sensor must always run | `"fail"` |
| Sensor should run but might not | `"warn"` |
| Sensor is optional | `"skip"` |

## Step 4: Organize into Sections

Group sensors by category:

```toml
[policy]
section_order = [
  "Highlights",
  "Build",
  "Security",
  "Quality",
  "Tests",
  "Other"
]

[sensors.builddiag]
blocking = true
missing = "fail"
section = "Build"

[sensors.security-scan]
blocking = true
missing = "fail"
section = "Security"

[sensors.clippy]
blocking = true
missing = "warn"
section = "Quality"

[sensors.coverage]
blocking = false
missing = "skip"
section = "Tests"
```

The PR comment will organize findings by section.

## Step 5: Add Repro Commands

Help developers reproduce locally:

```toml
[sensors.builddiag]
blocking = true
missing = "fail"
section = "Build"
repro = "cargo build --all-targets"

[sensors.clippy]
blocking = true
missing = "warn"
section = "Quality"
repro = "cargo clippy --all-targets -- -D warnings"
```

These appear in the PR comment:

```markdown
#### Quality
- clippy: 2 warnings
  ```
  cargo clippy --all-targets -- -D warnings
  ```
```

## Step 6: Configure Budgets

Control output size:

```toml
[policy]
max_highlights = 7           # Top findings in highlights section
max_per_sensor_findings = 20 # Findings shown per sensor
max_annotations = 25         # For annotation emitters (future)
```

### Tuning Highlights

- **Too few**: Important issues may be hidden
- **Too many**: Comment becomes noisy

Start with 7, adjust based on team feedback.

### Tuning Per-Sensor

- **Too few**: Developers don't see all issues
- **Too many**: One noisy sensor dominates

Start with 20, adjust based on typical finding counts.

## Step 7: Configure warn_is_fail

Decide if warnings from blocking sensors should fail the build:

```toml
[policy]
warn_is_fail = false  # Warnings don't fail
# or
warn_is_fail = true   # Warnings fail
```

Recommendation:
- Start with `false` during rollout
- Change to `true` when team is ready for stricter enforcement

## Step 8: Add Label Gates (Optional)

For expensive checks that shouldn't run on every PR:

```toml
[sensors.perf-benchmark]
blocking = false
missing = "skip"
section = "Performance"
require_label = "run-perf"
```

When `run-perf` label is absent, the sensor is treated as "effectively skipped."

Note: cockpitctl doesn't fetch labels. Your CI must skip the sensor step when the label is absent.

## Example Complete Config

```toml
# cockpit.toml

[policy]
warn_is_fail = false
max_highlights = 7
max_per_sensor_findings = 20
section_order = [
  "Highlights",
  "Build",
  "Security",
  "Quality",
  "Tests",
  "Performance",
  "Other"
]

# Critical - must always pass
[sensors.builddiag]
blocking = true
missing = "fail"
section = "Build"
repro = "cargo build --all-targets"

[sensors.security-audit]
blocking = true
missing = "fail"
section = "Security"
repro = "cargo audit"

# Important - should pass
[sensors.clippy]
blocking = true
missing = "warn"
section = "Quality"
repro = "cargo clippy -- -D warnings"

[sensors.tests]
blocking = true
missing = "fail"
section = "Tests"
repro = "cargo test"

# Informational
[sensors.coverage]
blocking = false
missing = "skip"
section = "Tests"
repro = "cargo tarpaulin"

# Optional, label-gated
[sensors.perf-benchmark]
blocking = false
missing = "skip"
section = "Performance"
require_label = "run-perf"
repro = "cargo bench"
```

## Step 9: Test Your Config

Run locally:

```bash
# With all sensors
cockpitctl ingest --artifacts artifacts --config cockpit.toml

# Check comment structure
cat artifacts/cockpit/comment.md

# Check exit code
echo "Exit: $?"
```

Test edge cases:
- Remove a blocking sensor's receipt → should fail
- Add warnings to a blocking sensor → check warn_is_fail behavior

## Step 10: Commit and Iterate

```bash
git add cockpit.toml
git commit -m "Add cockpit policy configuration"
```

Policy evolves with your team:
- Start permissive, tighten over time
- Add new sensors as informational first
- Promote to blocking after stabilization

## What You've Learned

- Policy structure: global settings and per-sensor config
- Blocking determines merge gates
- Missing determines absent behavior
- Sections organize the PR comment
- Budgets control output size

## Next Steps

- [Config Reference](../reference/config.md) - All configuration options
- [Handle Missing Receipts](../how-to/handle-missing-receipts.md) - Advanced missing patterns
- [Customize PR Comment](../how-to/customize-pr-comment.md) - Comment tuning
