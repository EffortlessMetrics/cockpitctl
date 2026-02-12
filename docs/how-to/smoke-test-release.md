# Smoke Testing a Release

This guide describes how to validate a cockpitctl release using only published artifacts. This is the final validation step before announcing a release to sensor teams.

## Overview

The smoke test validates three key components:

1. **conformctl binary** - Downloads and runs against a sample receipt
2. **cockpitctl binary** - Downloads and runs `ingest` on test artifacts
3. **Composite action** - Validates the GitHub Action works with pinned tag inputs

## Prerequisites

- `curl` or `wget` for downloading binaries
- `jq` for JSON validation (optional but recommended)
- `gh` CLI for composite action testing (optional)

## Automated Smoke Test Script

The easiest way to validate a release is using the provided smoke test script:

```bash
# Test a specific tag
./scripts/smoke-test-release.sh v0.2.0

# The script will:
# 1. Detect your platform
# 2. Download conformctl and cockpitctl binaries
# 3. Run functional tests on both binaries
# 4. Provide instructions for testing the composite action
```

## Manual Smoke Testing

If you prefer to test manually, follow these steps:

### 1. Test conformctl

```bash
# Download conformctl for your platform
# Linux x64
curl -fsSL https://github.com/EffortlessMetrics/cockpitctl/releases/download/v0.2.0/conformctl-linux-x64 -o conformctl
chmod +x conformctl

# macOS ARM64
curl -fsSL https://github.com/EffortlessMetrics/cockpitctl/releases/download/v0.2.0/conformctl-darwin-arm64 -o conformctl
chmod +x conformctl

# Windows x64
curl -fsSL https://github.com/EffortlessMetrics/cockpitctl/releases/download/v0.2.0/conformctl-windows-x64.exe -o conformctl.exe

# Verify version
./conformctl --version

# Test against a sample receipt
cat > test-receipt.json << 'EOF'
{
  "schema": "test-sensor.report.v1",
  "tool": {
    "name": "test-sensor",
    "version": "1.0.0"
  },
  "run": {
    "started_at": "2024-01-01T00:00:00Z"
  },
  "verdict": {
    "status": "pass",
    "counts": { "info": 0, "warn": 0, "error": 0 },
    "reasons": []
  },
  "findings": []
}
EOF

./conformctl check --report test-receipt.json --sensor-id test-sensor
```

### 2. Test cockpitctl

```bash
# Download cockpitctl for your platform
# Linux x64
curl -fsSL https://github.com/EffortlessMetrics/cockpitctl/releases/download/v0.2.0/cockpitctl-linux-x64 -o cockpitctl
chmod +x cockpitctl

# macOS ARM64
curl -fsSL https://github.com/EffortlessMetrics/cockpitctl/releases/download/v0.2.0/cockpitctl-darwin-arm64 -o cockpitctl
chmod +x cockpitctl

# Windows x64
curl -fsSL https://github.com/EffortlessMetrics/cockpitctl/releases/download/v0.2.0/cockpitctl-windows-x64.exe -o cockpitctl.exe

# Verify version
./cockpitctl --version

# Test ingest on happy_path fixtures
cd fixtures/happy_path
../cockpitctl ingest --artifacts artifacts --config cockpit.toml

# Verify outputs exist
ls artifacts/cockpit/report.json
ls artifacts/cockpit/comment.md

# Verify report structure
jq '.verdict.status' artifacts/cockpit/report.json  # Should be "pass"
```

### 3. Test Composite Action

Create a test workflow in a GitHub repository:

```yaml
# .github/workflows/test-cockpitctl.yml
name: Test cockpitctl Action
on: workflow_dispatch
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Test cockpitctl action
        uses: EffortlessMetrics/cockpitctl@v0.2.0
        with:
          artifacts-path: fixtures/happy_path/artifacts
          config-path: fixtures/happy_path/cockpit.toml
          version: v0.2.0
          post-comment: false
          fail-on-error: true
```

Run the workflow manually via the GitHub UI and verify it completes successfully.

## Validation Checklist

Before announcing a release, confirm:

- [ ] `conformctl` downloads and runs successfully
- [ ] `conformctl check` validates a sample receipt
- [ ] `cockpitctl` downloads and runs successfully
- [ ] `cockpitctl ingest` produces `report.json` and `comment.md`
- [ ] The composite action runs with a pinned tag
- [ ] All platforms work (test on at least one platform per OS)

## Release Assets

The release includes the following assets:

### cockpitctl binaries
- `cockpitctl-linux-x64` - Linux x86_64
- `cockpitctl-darwin-x64` - macOS Intel
- `cockpitctl-darwin-arm64` - macOS Apple Silicon
- `cockpitctl-windows-x64.exe` - Windows x86_64

### conformctl binaries
- `conformctl-linux-x64` - Linux x86_64
- `conformctl-darwin-x64` - macOS Intel
- `conformctl-darwin-arm64` - macOS Apple Silicon
- `conformctl-windows-x64.exe` - Windows x86_64

## No Vendoring Required

The smoke test proves that users don't need to:

- Clone the repository
- Install Rust toolchain
- Build from source
- Download any additional dependencies

All functionality is contained in the pre-compiled binaries.

## Troubleshooting

### Binary download fails
- Verify the tag exists: `gh release view v0.2.0`
- Check the asset name matches your platform

### `cockpitctl ingest` fails
- Verify the artifacts directory structure matches expectations
- Check that `cockpit.toml` is present and valid
- Review the error message for specific issues

### Composite action fails
- Ensure the action repository is public
- Verify the tag exists in the action repository
- Check GitHub Actions logs for detailed errors

## Next Steps

After successful smoke testing:

1. Update the [CHANGELOG.md](../../CHANGELOG.md) with release notes
2. Create a GitHub release with the tag
3. Announce to sensor teams with:
   - Release notes
   - Migration guide (if breaking changes)
   - Link to documentation
