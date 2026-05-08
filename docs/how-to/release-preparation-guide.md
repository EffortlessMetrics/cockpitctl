# Release Preparation Guide

This guide provides comprehensive instructions for preparing and executing a cockpitctl release.

## Table of Contents

- [Overview](#overview)
- [Pre-Release Verification](#pre-release-verification)
- [Release Execution](#release-execution)
- [Post-Release Validation](#post-release-validation)
- [Troubleshooting](#troubleshooting)

## Overview

The cockpitctl release process is automated via GitHub Actions and triggered by pushing a version tag (e.g., `v0.3.0`). The workflow consists of five jobs:

1. **Quality Gate** — Code formatting and linting checks
2. **Publish** — Publishes crates to crates.io in dependency order
3. **Build Binaries** — Builds cross-platform binaries (4 platforms × 2 binaries)
4. **Test Binaries** — Smoke tests every built binary on its native runner
5. **GitHub Release** — Creates GitHub Release with SHA256 checksums

### Release Artifacts

Each release produces:

- **Crates.io packages**: 10 crates published to crates.io
  - `cockpitctl-types`
  - `cockpitctl-conform`
  - `cockpitctl-domain`
  - `cockpitctl-render`
  - `cockpitctl-ingest`
  - `cockpitctl-io`
  - `cockpitctl-sarif`
  - `cockpitctl-core`
  - `cockpitctl`
  - `conformctl`

- **GitHub Release assets**: 8 binaries + checksums
  - `cockpitctl-linux-x64`
  - `cockpitctl-darwin-x64`
  - `cockpitctl-darwin-arm64`
  - `cockpitctl-windows-x64.exe`
  - `conformctl-linux-x64`
  - `conformctl-darwin-x64`
  - `conformctl-darwin-arm64`
  - `conformctl-windows-x64.exe`
  - `SHA256SUMS.txt`

## Pre-Release Verification

Before triggering a release, complete all verification steps.

### 1. Code Quality Verification

Run all quality checks locally to ensure they pass:

```bash
# Format check
cargo fmt --all -- --check

# Lint check (warnings as errors)
cargo clippy --workspace --all-targets -- -D warnings

# Run all tests
cargo test --workspace --all-targets
```

**Expected result**: All commands should complete without errors.

### 2. Version Consistency Check

Verify version consistency across all files:

```bash
# Check workspace Cargo.toml version
grep "version = " Cargo.toml

# The version should match the tag you plan to create (e.g., 0.3.0 for v0.3.0)
```

Files to verify:
- [`Cargo.toml`](../../Cargo.toml) — `[workspace.package]` version
- [`CHANGELOG.md`](../../CHANGELOG.md) — Version heading format

### 3. Changelog Update

Update [`CHANGELOG.md`](../../CHANGELOG.md) to prepare for release:

1. Move all items from `[Unreleased]` to the new version section
2. Add the release date in ISO format (YYYY-MM-DD)
3. Create a new `[Unreleased]` section for future changes

Example:

```markdown
## [Unreleased]

## [0.3.0] - 2026-02-15

### Added
- New feature description

### Changed
- Modified behavior description

### Fixed
- Bug fix description
```

### 4. Documentation Review

Verify all documentation is up to date:

- [`README.md`](../../README.md) — Installation instructions, quick start
- [`docs/reference/`](../../docs/reference/) — All reference docs
- [`docs/how-to/`](../../docs/how-to/) — All how-to guides
- [`AGENTS.md`](../../AGENTS.md) — Core contracts (if changed)

### 5. Schema Synchronization Check

Ensure embedded schemas are synchronized with source schemas:

```bash
cargo run -p xtask -- schema-sync-check
```

**Expected result**: Command exits 0 and reports schemas are in sync.

### 6. Golden Tests Verification

Run golden/snapshot tests to ensure deterministic behavior:

```bash
cargo test -p cockpitctl --test ingest_golden
```

**Expected result**: All golden tests pass.

### 7. Package Validation + Dry-Run Strategy

Validate package contents locally before tagging:

```bash
cargo package --list -p cockpitctl-types
cargo package --list -p cockpitctl-conform
cargo package --list -p cockpitctl-domain
cargo package --list -p cockpitctl-render
cargo package --list -p cockpitctl-ingest
cargo package --list -p cockpitctl-io
cargo package --list -p cockpitctl-sarif
cargo package --list -p cockpitctl-core
cargo package --list -p cockpitctl
cargo package --list -p conformctl
```

Optional local publish dry-run:

```bash
cargo publish --dry-run -p cockpitctl-types
```

`cargo publish --dry-run` for dependent crates can fail locally before release
because the new dependency versions are not on crates.io yet. The release
workflow handles this by running dry-run + publish in dependency tiers.

**Expected result**: `cargo package --list` succeeds for all crates, and CI dry-runs pass during the release workflow.

### 8. Binary Build Verification

Build release binaries locally to verify they compile:

```bash
# Build for your platform
cargo build --release -p cockpitctl
cargo build --release -p conformctl

# Verify binaries work
./target/release/cockpitctl --version
./target/release/conformctl --version
```

### 9. Fixture Regeneration Check

If any fixtures were modified, ensure they are regenerated correctly:

```bash
cargo run -p xtask -- fixtures-help
```

### 10. Security Audit

Run a security audit on dependencies:

```bash
cargo audit
```

**Expected result**: No vulnerabilities or only acceptable ones.

## Release Execution

Once all pre-release verification steps pass, execute the release.

### 1. Create and Push Tag

```bash
# Create the version tag
git tag v0.3.0

# Push the tag to trigger the release workflow
git push origin v0.3.0
```

**Note**: The tag format must be `v*` (e.g., `v0.3.0`, `v1.0.0`). The workflow will verify that the tag version matches the version in [`Cargo.toml`](../../Cargo.toml).

### 2. Monitor Release Workflow

1. Navigate to the [GitHub Actions tab](https://github.com/EffortlessMetrics/cockpitctl/actions)
2. Find the "Release" workflow triggered by your tag
3. Monitor the progress of all five jobs:
   - `quality-gate` — Should complete in ~2-3 minutes
   - `publish` — Should complete in ~5-8 minutes (includes 30s waits between tiers)
   - `build-binaries` — Should complete in ~5-10 minutes (8 matrix builds in parallel)
   - `test-binaries` — Should complete in ~3-5 minutes
   - `github-release` — Should complete in ~1-2 minutes

**Total expected time**: 15-30 minutes

### 3. Verify Workflow Success

Ensure all jobs complete successfully:

- ✅ Quality Gate: `cargo fmt` and `cargo clippy` pass
- ✅ Publish: All 10 crates published to crates.io
- ✅ Build Binaries: All 8 binaries built successfully
- ✅ Test Binaries: All binaries pass smoke tests
- ✅ GitHub Release: Release created with all assets

### 4. Verify crates.io Publications

Visit crates.io to verify all packages are published:

- https://crates.io/crates/cockpitctl-types
- https://crates.io/crates/cockpitctl-conform
- https://crates.io/crates/cockpitctl-domain
- https://crates.io/crates/cockpitctl-render
- https://crates.io/crates/cockpitctl-ingest
- https://crates.io/crates/cockpitctl-io
- https://crates.io/crates/cockpitctl-sarif
- https://crates.io/crates/cockpitctl-core
- https://crates.io/crates/cockpitctl
- https://crates.io/crates/conformctl

### 5. Verify GitHub Release

Visit the GitHub release page:

```
https://github.com/EffortlessMetrics/cockpitctl/releases/tag/v0.3.0
```

Verify:
- Release notes are generated (via `generate_release_notes: true`)
- All 8 binaries are attached
- `SHA256SUMS.txt` is attached
- Checksums are correct

## Post-Release Validation

After the release is complete, perform validation steps.

### 1. Smoke Test Release Artifacts

Run the smoke test xtask to validate published artifacts:

```bash
cargo run -p xtask -- smoke-test-release v0.3.0
```

The smoke test validates:
- Binary downloads from GitHub Release
- Version output is correct
- `cockpitctl ingest` works on test fixtures
- `conformctl check` works on test receipts

**Expected result**: All tests pass.

### 2. Verify Binary Checksums

Download and verify checksums:

```bash
# Download checksums
curl -fsSL https://github.com/EffortlessMetrics/cockpitctl/releases/download/v0.3.0/SHA256SUMS.txt -o SHA256SUMS.txt

# Download a binary
curl -fsSL https://github.com/EffortlessMetrics/cockpitctl/releases/download/v0.3.0/cockpitctl-linux-x64 -o cockpitctl-linux-x64
chmod +x cockpitctl-linux-x64

# Verify checksum
sha256sum -c SHA256SUMS.txt --ignore-missing
```

**Expected result**: Checksum verification succeeds.

### 3. Test crates.io Installation

Test installing from crates.io:

```bash
# Create a temporary test project
mkdir test-cockpitctl && cd test-cockpitctl
cargo init

# Add cockpitctl as a dependency
cargo add cockpitctl

# Build the project
cargo build

# Clean up
cd ..
rm -rf test-cockpitctl
```

**Expected result**: Project builds successfully with the published version.

### 4. Cross-Platform Binary Testing

If you have access to multiple platforms, test binaries on each:

- **Linux x64**: Test `cockpitctl-linux-x64` and `conformctl-linux-x64`
- **macOS x64**: Test `cockpitctl-darwin-x64` and `conformctl-darwin-x64`
- **macOS ARM64**: Test `cockpitctl-darwin-arm64` and `conformctl-darwin-arm64`
- **Windows x64**: Test `cockpitctl-windows-x64.exe` and `conformctl-windows-x64.exe`

For each binary:
```bash
./cockpitctl --version
./cockpitctl ingest --artifacts fixtures/happy_path/artifacts --config fixtures/happy_path/cockpit.toml
```

### 5. Verify Documentation Links

Check that all documentation links are working:

- README installation instructions
- How-to guides reference correct versions
- Reference documentation is up to date

### 6. Announce Release

Once all validation passes, announce the release:

1. Update project status in relevant channels
2. Post release notes to appropriate forums/mailing lists
3. Notify downstream consumers of the new version

## Troubleshooting

### Quality Gate Failures

**Problem**: `cargo fmt` or `cargo clippy` fails.

**Solution**:
```bash
# Fix formatting issues
cargo fmt --all

# Fix clippy warnings (warnings are errors)
cargo clippy --workspace --all-targets --fix --allow-dirty --allow-staged
```

Commit fixes and create a new tag.

### Version Mismatch

**Problem**: Tag version doesn't match Cargo.toml version.

**Solution**:
```bash
# Update version in Cargo.toml
# Then delete and recreate the tag
git tag -d v0.3.0
git push origin :refs/tags/v0.3.0
git tag v0.3.0
git push origin v0.3.0
```

### Publish Failures

**Problem**: Crates.io publish fails.

**Common causes**:
- Version already published (use `cargo yank` if needed)
- Network issues (retry)
- Validation errors (fix and retry)

**Solution**: Check the specific error message and address accordingly.

### Binary Build Failures

**Problem**: Cross-platform binary build fails.

**Solution**: Check the specific platform's build log. Common issues:
- Target-specific dependencies
- Platform-specific code paths
- Compilation errors on specific targets

### Test Binary Failures

**Problem**: Smoke test fails on a specific platform.

**Solution**:
1. Download the binary locally
2. Test manually to reproduce the issue
3. Check fixture paths and permissions
4. Verify binary was built correctly

### Rollback Procedure

If a critical issue is discovered after release:

1. **Yank the crates** (if necessary):
   ```bash
   cargo yank --vers 0.3.0 cockpitctl
   cargo yank --vers 0.3.0 cockpitctl-core
   # ... repeat for all crates
   ```

2. **Delete the GitHub release** (if necessary):
   ```bash
   gh release delete v0.3.0 --yes
   ```

3. **Prepare a patch release** (e.g., `v0.3.1`):
   - Fix the issue
   - Update CHANGELOG
   - Create new tag
   - Trigger new release

## Related Documentation

- [Release Runbook](./release-runbook.md) — Step-by-step execution guide
- [Release Manager Checklist](./release-manager-checklist.md) — Comprehensive checklist
- [Smoke Testing a Release](./smoke-test-release.md) — Post-release validation
- [Release-Ready Gate Checklist](../../RELEASE_READY_GATE_CHECKLIST.md) — Workflow verification
