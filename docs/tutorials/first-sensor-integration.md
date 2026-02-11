# First Sensor Integration

This tutorial walks you through adding cockpitctl to a real repository with sensors.

## What You'll Learn

- How to structure your artifacts directory
- How to run sensors and save receipts
- How to invoke cockpitctl in CI

## Prerequisites

- Completed [Getting Started](getting-started.md)
- A repository with CI (GitHub Actions shown)
- At least one tool that can produce receipts

## Step 1: Understand the Flow

The typical CI flow:

```
1. Run sensors (each produces artifacts/<sensor>/report.json)
2. Run cockpitctl ingest
3. Post comment to PR
4. Fail or pass based on exit code
```

cockpitctl doesn't run sensors. It reads what they produce.

## Step 2: Create the Artifacts Directory

Add `.gitignore` entry:

```gitignore
# CI artifacts
artifacts/
```

Create a placeholder so the directory structure is documented:

```bash
mkdir -p artifacts/.gitkeep
echo "Sensor receipts go here during CI" > artifacts/README.md
```

## Step 3: Adapt Your First Sensor

Choose a tool that runs in your CI. You need to:
1. Run the tool
2. Capture its output as `sensor.report.v1`

### Option A: Tool Already Produces Receipts

Some tools output cockpitctl-compatible receipts natively:

```bash
my-tool check --format=cockpit --output artifacts/my-tool/report.json
```

### Option B: Wrap an Existing Tool

Create a wrapper script that converts output:

```bash
#!/bin/bash
# scripts/run-clippy-sensor.sh

mkdir -p artifacts/clippy

# Run clippy and capture output
cargo clippy --message-format=json 2>&1 > clippy-raw.json

# Convert to sensor.report.v1 (using a converter script)
python scripts/clippy-to-receipt.py clippy-raw.json > artifacts/clippy/report.json
```

A simple converter might look like:

```python
# scripts/clippy-to-receipt.py
import json
import sys
from datetime import datetime

def convert(clippy_output):
    findings = []
    errors = 0
    warnings = 0

    for line in open(clippy_output):
        msg = json.loads(line)
        if msg.get("reason") == "compiler-message":
            # Convert clippy message to finding
            finding = {
                "severity": "warn" if "warning" in msg else "error",
                "code": f"clippy.{msg.get('message', {}).get('code', {}).get('code', 'unknown')}",
                "message": msg.get("message", {}).get("message", ""),
                "location": {
                    "path": msg.get("message", {}).get("spans", [{}])[0].get("file_name", ""),
                    "line": msg.get("message", {}).get("spans", [{}])[0].get("line_start", 1)
                }
            }
            findings.append(finding)
            if finding["severity"] == "error":
                errors += 1
            else:
                warnings += 1

    return {
        "schema": "sensor.report.v1",
        "tool": {"name": "clippy", "version": "1.0.0"},
        "run": {"started_at": datetime.utcnow().isoformat() + "Z"},
        "verdict": {
            "status": "fail" if errors > 0 else ("warn" if warnings > 0 else "pass"),
            "counts": {"info": 0, "warn": warnings, "error": errors}
        },
        "findings": findings
    }

if __name__ == "__main__":
    print(json.dumps(convert(sys.argv[1]), indent=2))
```

### Option C: Start Simple

For testing, create a passing sensor:

```bash
mkdir -p artifacts/placeholder
cat > artifacts/placeholder/report.json << 'EOF'
{
  "schema": "sensor.report.v1",
  "tool": {"name": "placeholder", "version": "1.0.0"},
  "run": {"started_at": "2024-01-15T10:00:00Z"},
  "verdict": {"status": "pass", "counts": {"info": 0, "warn": 0, "error": 0}},
  "findings": []
}
EOF
```

## Step 4: Add CI Workflow

Create `.github/workflows/cockpit.yml`:

```yaml
name: Cockpit

on:
  pull_request:
    branches: [main]

jobs:
  checks:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      # Install cockpitctl
      - name: Install cockpitctl
        run: |
          cargo install cockpitctl
          # Or download pre-built binary

      # Run your sensors
      - name: Run clippy sensor
        run: scripts/run-clippy-sensor.sh

      # Run cockpitctl
      - name: Aggregate with cockpitctl
        id: cockpit
        continue-on-error: true
        run: cockpitctl ingest --artifacts artifacts

      # Post comment
      - name: Post PR comment
        if: always()
        env:
          GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}
        run: |
          gh pr comment ${{ github.event.pull_request.number }} \
            --body-file artifacts/cockpit/comment.md \
            --edit-last || \
          gh pr comment ${{ github.event.pull_request.number }} \
            --body-file artifacts/cockpit/comment.md

      # Upload artifacts for debugging
      - name: Upload artifacts
        if: always()
        uses: actions/upload-artifact@v4
        with:
          name: cockpit
          path: artifacts/

      # Fail if cockpit failed
      - name: Check result
        if: steps.cockpit.outcome == 'failure'
        run: exit 1
```

## Step 5: Test Locally

Before pushing, test locally:

```bash
# Run your sensor
scripts/run-clippy-sensor.sh

# Verify receipt
cat artifacts/clippy/report.json | jq .verdict

# Run cockpitctl
cockpitctl ingest --artifacts artifacts
echo "Exit code: $?"

# Check outputs
cat artifacts/cockpit/comment.md
```

## Step 6: Push and Verify

```bash
git add .
git commit -m "Add cockpitctl integration"
git push origin feature/add-cockpit
```

Open a PR and verify:
- CI runs successfully
- Comment appears on the PR
- Artifacts are uploaded

## Step 7: Add Policy (Optional)

Once working, add `cockpit.toml`:

```toml
[policy]
warn_is_fail = false
max_highlights = 7

[sensors.clippy]
blocking = true
missing = "warn"
section = "Diagnostics"
repro = "cargo clippy"
```

Commit and push. The comment will now show:
- Clippy as a blocking sensor
- Section organization
- Repro command

## Troubleshooting

### Receipt Not Found

```
cockpit.missing_receipt: Expected receipt from sensor 'clippy'
```

- Check sensor step ran successfully
- Verify path: `artifacts/clippy/report.json`
- Check sensor ID matches config

### Invalid Receipt

```
cockpit.invalid_receipt: Failed to parse receipt for 'clippy'
```

- Validate JSON: `jq . artifacts/clippy/report.json`
- Check against schema
- Use `cockpitctl validate`

### Comment Not Posted

- Check `GH_TOKEN` has write permissions
- Verify PR number is correct
- Check workflow has `pull-requests: write` permission

## What You've Learned

- CI flow: sensors → cockpitctl → comment
- How to adapt existing tools to produce receipts
- Basic GitHub Actions integration
- Local testing workflow

## Next Steps

- [Adding Policy](adding-policy.md) - Configure blocking and sections
- [Integrate with GitHub Actions](../how-to/integrate-github-actions.md) - Advanced CI patterns
- [Write a Conformant Sensor](../how-to/write-conformant-sensor.md) - Build a proper sensor
