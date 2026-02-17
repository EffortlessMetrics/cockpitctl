# Integrate with GitHub Actions

This guide shows how to set up a complete CI workflow with cockpitctl.

## Prerequisites

- A repository with sensors that produce receipts
- cockpitctl binary available (built or downloaded)
- GitHub Actions enabled

## Using the Composite Action (Recommended)

The simplest way to integrate cockpitctl is the provided composite action:

```yaml
name: CI

on:
  pull_request:
    branches: [main]

jobs:
  checks:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      # Run your sensors first...
      - name: Run builddiag
        run: builddiag check --output artifacts/builddiag/report.json

      # Then aggregate with the composite action
      - name: Cockpit
        uses: EffortlessMetrics/cockpitctl@v0.3.0
        with:
          artifacts-path: artifacts
          config-path: cockpit.toml
          post-comment: true
          fail-on-error: true
```

The action handles binary download, ingest, and PR comment posting in one step.
See [`action.yml`](../../action.yml) for all available inputs and outputs.

## Manual Workflow

If you need more control, wire the steps yourself:

## Basic Workflow

```yaml
name: CI

on:
  pull_request:
    branches: [main]

jobs:
  checks:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      # Run your sensors (examples)
      - name: Run builddiag
        run: |
          builddiag check --output artifacts/builddiag/report.json

      - name: Run diffguard
        run: |
          diffguard diff HEAD~1 --output artifacts/diffguard/report.json

      # Run cockpitctl
      - name: Aggregate results
        id: cockpit
        continue-on-error: true
        run: |
          cockpitctl ingest --artifacts artifacts --config cockpit.toml

      # Post the comment
      - name: Post PR comment
        if: always() && github.event_name == 'pull_request'
        env:
          GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}
        run: |
          gh pr comment ${{ github.event.pull_request.number }} \
            --body-file artifacts/cockpit/comment.md \
            --edit-last || \
          gh pr comment ${{ github.event.pull_request.number }} \
            --body-file artifacts/cockpit/comment.md

      # Fail the job if cockpit failed
      - name: Check result
        if: steps.cockpit.outcome == 'failure'
        run: exit 1
```

## Understanding the Steps

### 1. Run Sensors

Each sensor runs independently and writes its receipt:

```yaml
- name: Run builddiag
  run: builddiag check --output artifacts/builddiag/report.json
```

Sensors are responsible for:
- Their own installation and setup
- Producing `sensor.report.v1` receipts
- Writing to `artifacts/<sensor_id>/report.json`

### 2. Run cockpitctl

```yaml
- name: Aggregate results
  id: cockpit
  continue-on-error: true
  run: cockpitctl ingest --artifacts artifacts --config cockpit.toml
```

Key points:
- `continue-on-error: true` lets the workflow continue even if cockpitctl returns exit code 2 (policy failure)
- We capture the outcome with `id: cockpit` for later

### 3. Post the Comment

```yaml
- name: Post PR comment
  if: always() && github.event_name == 'pull_request'
  run: |
    gh pr comment ${{ github.event.pull_request.number }} \
      --body-file artifacts/cockpit/comment.md \
      --edit-last || \
    gh pr comment ${{ github.event.pull_request.number }} \
      --body-file artifacts/cockpit/comment.md
```

This creates a "sticky" comment:
- `--edit-last` updates the existing comment if one exists
- Falls back to creating a new comment if none exists
- Uses the cockpit markers (`<!-- cockpit:begin -->`) to identify the comment

### 4. Fail the Job

```yaml
- name: Check result
  if: steps.cockpit.outcome == 'failure'
  run: exit 1
```

This fails the job if cockpitctl returned a non-zero exit code.

## Caching cockpitctl

If building from source:

```yaml
- name: Cache cargo
  uses: actions/cache@v4
  with:
    path: |
      ~/.cargo/bin/
      ~/.cargo/registry/
      target/
    key: ${{ runner.os }}-cargo-${{ hashFiles('**/Cargo.lock') }}

- name: Install cockpitctl
  run: cargo install --path crates/cockpitctl-cli
```

Or download a pre-built binary:

```yaml
- name: Install cockpitctl
  run: |
    curl -L https://github.com/your-org/cockpitctl/releases/download/v0.3.0/cockpitctl-linux-x64 -o cockpitctl
    chmod +x cockpitctl
    sudo mv cockpitctl /usr/local/bin/
```

## Upload Artifacts

Preserve the cockpit output for debugging:

```yaml
- name: Upload cockpit artifacts
  if: always()
  uses: actions/upload-artifact@v4
  with:
    name: cockpit-report
    path: artifacts/cockpit/
```

## Label-Gated Sensors

For expensive checks that should only run with a label:

```yaml
- name: Run performance benchmarks
  if: contains(github.event.pull_request.labels.*.name, 'run-perf')
  run: |
    perf-bench run --output artifacts/perf-bench/report.json
```

Configure in `cockpit.toml`:

```toml
[sensors.perf-bench]
blocking = false
missing = "skip"
require_label = "run-perf"
```

## Matrix Builds

If sensors run in parallel jobs:

```yaml
jobs:
  sensors:
    strategy:
      matrix:
        sensor: [builddiag, diffguard, covguard]
    steps:
      - run: ${{ matrix.sensor }} check --output artifacts/${{ matrix.sensor }}/report.json
      - uses: actions/upload-artifact@v4
        with:
          name: receipt-${{ matrix.sensor }}
          path: artifacts/${{ matrix.sensor }}/

  cockpit:
    needs: sensors
    steps:
      - uses: actions/download-artifact@v4
        with:
          pattern: receipt-*
          path: artifacts/
          merge-multiple: true
      - run: cockpitctl ingest
```

## See Also

- [CLI Reference](../reference/cli.md) - cockpitctl commands
- [Exit Codes](../reference/exit-codes.md) - Understanding exit codes
- [Handle Missing Receipts](handle-missing-receipts.md) - When sensors don't run
