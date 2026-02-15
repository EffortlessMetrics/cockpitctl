# Getting Started

This tutorial walks you through your first run of cockpitctl with sample data.

## What You'll Learn

- What sensor receipts look like
- How to run `cockpitctl ingest`
- How to interpret the output

## Prerequisites

- cockpitctl installed (see Installation below)
- A terminal/command line

## Installation

### From Source

```bash
git clone https://github.com/your-org/cockpitctl
cd cockpitctl
cargo build --release
```

The binary is at `target/release/cockpitctl`.

### Pre-built Binary

Download from releases:

```bash
curl -L https://github.com/your-org/cockpitctl/releases/download/v0.2.1/cockpitctl-linux-x64 -o cockpitctl
chmod +x cockpitctl
```

## Step 1: Create Sample Receipts

Create a directory structure with sample sensor receipts.

```bash
mkdir -p artifacts/builddiag artifacts/covguard
```

Create `artifacts/builddiag/report.json`:

```json
{
  "schema": "sensor.report.v1",
  "tool": {
    "name": "builddiag",
    "version": "0.5.0"
  },
  "run": {
    "started_at": "2024-01-15T10:30:00Z"
  },
  "verdict": {
    "status": "pass",
    "counts": {
      "info": 1,
      "warn": 0,
      "error": 0
    }
  },
  "findings": [
    {
      "severity": "info",
      "code": "builddiag.build.success",
      "message": "Build completed successfully"
    }
  ]
}
```

Create `artifacts/covguard/report.json`:

```json
{
  "schema": "sensor.report.v1",
  "tool": {
    "name": "covguard",
    "version": "1.2.0"
  },
  "run": {
    "started_at": "2024-01-15T10:31:00Z"
  },
  "verdict": {
    "status": "warn",
    "counts": {
      "info": 0,
      "warn": 1,
      "error": 0
    }
  },
  "findings": [
    {
      "severity": "warn",
      "code": "covguard.coverage.below-threshold",
      "message": "Coverage is 75%, below 80% threshold",
      "location": {
        "path": "src/new_feature.rs",
        "line": 1
      }
    }
  ]
}
```

Your structure should be:

```
artifacts/
  builddiag/
    report.json
  covguard/
    report.json
```

## Step 2: Run cockpitctl

Run the ingest command:

```bash
cockpitctl ingest --artifacts artifacts
```

You should see output like:

```
Discovered 2 sensors: builddiag, covguard
Overall verdict: warn
Wrote artifacts/cockpit/report.json
Wrote artifacts/cockpit/comment.md
```

Exit code is `0` (pass/warn without warn-as-fail).

## Step 3: Examine the Output

### The Report

Look at `artifacts/cockpit/report.json`:

```bash
cat artifacts/cockpit/report.json
```

Key sections:
- `verdict`: Overall status (`warn`)
- `sensors`: Summary of each sensor
- `highlights`: Most important findings
- `policy`: What policy was applied

### The Comment

Look at `artifacts/cockpit/comment.md`:

```bash
cat artifacts/cockpit/comment.md
```

This is what would be posted to a PR:

```markdown
<!-- cockpit:begin -->
## Cockpit

### Summary

| Sensor | Status | Blocking | Notes |
|---|---:|---:|---|
| builddiag | ✅ pass | no | |
| covguard | ⚠️ warn | no | |

### Highlights

1. **covguard**: `covguard.coverage.below-threshold` at `src/new_feature.rs:1` — "Coverage is 75%, below 80% threshold"

<!-- cockpit:end -->
```

## Step 4: Add Policy

Create `cockpit.toml` to customize behavior:

```toml
[policy]
warn_is_fail = false
max_highlights = 5

[sensors.builddiag]
blocking = true
missing = "fail"
section = "Build"

[sensors.covguard]
blocking = false
missing = "skip"
section = "Tests"
```

Run again:

```bash
cockpitctl ingest --artifacts artifacts --config cockpit.toml
```

Now the comment shows sections and blocking status.

## Step 5: Simulate a Failure

Edit `artifacts/builddiag/report.json` to have a failing verdict:

```json
{
  "verdict": {
    "status": "fail",
    "counts": {
      "info": 0,
      "warn": 0,
      "error": 1
    }
  },
  "findings": [
    {
      "severity": "error",
      "code": "builddiag.build.failed",
      "message": "Build failed: missing dependency"
    }
  ]
}
```

Run again:

```bash
cockpitctl ingest --artifacts artifacts --config cockpit.toml
echo "Exit code: $?"
```

Exit code is now `2` (policy failure) because:
- `builddiag` is `blocking = true`
- `builddiag` verdict is `fail`

## What You've Learned

- Sensor receipts follow `sensor.report.v1` schema
- `cockpitctl ingest` aggregates receipts
- Output includes a report and PR comment
- Policy in `cockpit.toml` controls blocking behavior
- Exit code reflects overall verdict

## Next Steps

- [First Sensor Integration](first-sensor-integration.md) - Add cockpitctl to a real repo
- [Adding Policy](adding-policy.md) - Configure `cockpit.toml` in depth
- [CLI Reference](../reference/cli.md) - All command options
