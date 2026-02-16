# Release Runbook

This runbook provides step-by-step instructions for executing a cockpitctl release. Follow these procedures to ensure a successful release.

## Table of Contents

- [Prerequisites](#prerequisites)
- [Pre-Release Checklist](#pre-release-checklist)
- [Release Execution](#release-execution)
- [Post-Release Validation](#post-release-validation)
- [Troubleshooting](#troubleshooting)
- [Rollback Procedures](#rollback-procedures)

## Prerequisites

Before executing a release, ensure you have:

- **Git access** with push permissions to the repository
- **GitHub CLI** (`gh`) installed for release management
- **Rust toolchain** installed for local verification
- **crates.io token** configured in GitHub Actions secrets
- **Access to multiple platforms** (optional, for cross-platform testing)

### Required Tools

```bash
# Verify git is installed
git --version

# Verify GitHub CLI is installed
gh --version

# Verify Rust toolchain
rustc --version
cargo --version
```

## Pre-Release Checklist

Complete all items in this checklist before creating a release tag.

### Step 1: Verify Code Quality

```bash
# Check code formatting
cargo fmt --all -- --check

# Run clippy (warnings as errors)
cargo clippy --workspace --all-targets -- -D warnings

# Run all tests
cargo test --workspace --all-targets
```

**Expected output**: All commands complete without errors.

**If any check fails**:
- Fix the issues
- Commit the fixes
- Re-run the checks

### Step 2: Verify Version Consistency

```bash
# Check workspace version
grep "version = " Cargo.toml
```

**Verify**:
- The version matches your intended release (e.g., `0.2.1`)
- The version follows semantic versioning

**Example output**:
```
[workspace.package]
version = "0.2.1"
```

### Step 3: Update CHANGELOG.md

Edit [`CHANGELOG.md`](../../CHANGELOG.md):

1. Move all items from `[Unreleased]` to the new version section
2. Add the release date in ISO format (YYYY-MM-DD)
3. Create a new `[Unreleased]` section

**Example**:
```markdown
## [Unreleased]

## [0.2.1] - 2026-02-15

### Added
- New feature description

### Changed
- Modified behavior description

### Fixed
- Bug fix description
```

### Step 4: Verify Schema Synchronization

```bash
cargo run -p xtask -- schema-sync-check
```

**Expected output**: No output (exit code 0)

**If check fails**:
```bash
# Fix schema sync issues
cargo run -p xtask -- schema-sync-fix
```

### Step 5: Run Golden Tests

```bash
cargo test -p cockpitctl --test ingest_golden
```

**Expected output**: All tests pass

### Step 6: Dry-Run Publish Validation

Run dry-run publishes in dependency order:

```bash
cargo publish --dry-run -p cockpitctl-types
cargo publish --dry-run -p cockpitctl-conform
cargo publish --dry-run -p cockpitctl-domain
cargo publish --dry-run -p cockpitctl-render
cargo publish --dry-run -p cockpitctl-ingest
cargo publish --dry-run -p cockpitctl-io
cargo publish --dry-run -p cockpitctl-sarif
cargo publish --dry-run -p cockpitctl-core
cargo publish --dry-run -p cockpitctl
cargo publish --dry-run -p conformctl
```

**Expected output**: All dry-runs complete successfully

### Step 7: Build and Test Locally

```bash
# Build release binaries
cargo build --release -p cockpitctl
cargo build --release -p conformctl

# Test binaries
./target/release/cockpitctl --version
./target/release/conformctl --version

# Test ingest functionality
./target/release/cockpitctl ingest --artifacts fixtures/happy_path/artifacts --config fixtures/happy_path/cockpit.toml
```

**Expected output**: All commands complete successfully

### Step 8: Security Audit

```bash
cargo audit
```

**Expected output**: No critical or high vulnerabilities

**If vulnerabilities are found**:
- Review and assess severity
- Update dependencies if necessary
- Document accepted risks

### Step 9: Commit All Changes

```bash
# Stage changes
git add CHANGELOG.md Cargo.toml

# Commit with descriptive message
git commit -m "Prepare release v0.2.1"
```

**Commit message format**:
- Use semantic commit messages
- Include version number
- Reference relevant issues if applicable

### Step 10: Push to Main

```bash
git push origin main
```

**Verify**:
- All changes are pushed
- CI workflow passes

## Release Execution

### Step 1: Create Version Tag

```bash
# Create the tag
git tag v0.2.1

# Verify the tag
git tag -l v*
git show v0.2.1
```

**Tag format**: Must be `v*` (e.g., `v0.2.1`, `v1.0.0`)

### Step 2: Push Tag to Trigger Release

```bash
git push origin v0.2.1
```

**This action triggers**:
- GitHub Actions release workflow
- Automated quality gates
- Crates.io publishing
- Binary builds
- End-to-end tests

### Step 3: Monitor Release Workflow

1. **Navigate to GitHub Actions**:
   ```bash
   gh run list --workflow=release.yml
   ```

2. **Open the workflow run**:
   ```bash
   gh run view --workflow=release.yml
   ```

3. **Monitor each job**:

   **Quality Gate Job** (~2-3 minutes)
   - `cargo fmt` check
   - `cargo clippy` check
   - Must pass before publishing

   **Publish Job** (~5-8 minutes)
   - Version verification
   - All tests
   - Dry-run publishes
   - Crates.io publishes (with 30s waits between tiers)

   **Build Binaries Job** (~5-10 minutes)
   - 8 matrix builds in parallel:
     - cockpitctl: linux-x64, darwin-x64, darwin-arm64, windows-x64
     - conformctl: linux-x64, darwin-x64, darwin-arm64, windows-x64

   **Test Binaries Job** (~3-5 minutes)
   - Download each binary
   - Test `--version`
   - Test `cockpitctl ingest`
   - Test `conformctl check`

   **GitHub Release Job** (~1-2 minutes)
   - Download all artifacts
   - Generate SHA256SUMS.txt
   - Create GitHub release

**Total expected time**: 15-30 minutes

### Step 4: Verify Workflow Success

Ensure all jobs complete successfully:

```bash
# Check workflow status
gh run view --workflow=release.yml
```

**Verify**:
- ✅ Quality Gate: Passed
- ✅ Publish: Passed
- ✅ Build Binaries: Passed
- ✅ Test Binaries: Passed
- ✅ GitHub Release: Passed

### Step 5: Verify crates.io Publications

Visit each crate page to verify publication:

```bash
# Open crates.io pages
gh repo view --web
```

**Verify these crates are published**:
- https://crates.io/crates/cockpitctl-types
- https://crates.io/crates/cockpitctl-conform
- https://crates.io/crates/cockpitctl-domain
- https://crates.io/crates/cockpitctl-render
- https://crates.io/crates/cockpitctl-ingest
- https://crates.io/crates/cockpitctl-io
- https://crates.io/crates/cockpitctl-core
- https://crates.io/crates/cockpitctl
- https://crates.io/crates/conformctl

### Step 6: Verify GitHub Release

```bash
# Open the release page
gh release view v0.2.1 --web
```

**Verify**:
- Release notes are generated
- All 8 binaries are attached:
  - `cockpitctl-linux-x64`
  - `cockpitctl-darwin-x64`
  - `cockpitctl-darwin-arm64`
  - `cockpitctl-windows-x64.exe`
  - `conformctl-linux-x64`
  - `conformctl-darwin-x64`
  - `conformctl-darwin-arm64`
  - `conformctl-windows-x64.exe`
- `SHA256SUMS.txt` is attached

## Post-Release Validation

### Step 1: Run Smoke Test Script

**Unix/Linux/macOS**:
```bash
./scripts/smoke-test-release.sh v0.2.1
```

**Windows PowerShell**:
```powershell
./scripts/smoke-test-release.ps1 v0.2.1
```

**Expected output**: All smoke tests pass

### Step 2: Verify Checksums

```bash
# Download checksums
curl -fsSL https://github.com/EffortlessMetrics/cockpitctl/releases/download/v0.2.1/SHA256SUMS.txt -o SHA256SUMS.txt

# Download a binary
curl -fsSL https://github.com/EffortlessMetrics/cockpitctl/releases/download/v0.2.1/cockpitctl-linux-x64 -o cockpitctl-linux-x64
chmod +x cockpitctl-linux-x64

# Verify checksum
sha256sum -c SHA256SUMS.txt --ignore-missing
```

**Expected output**: `cockpitctl-linux-x64: OK`

### Step 3: Test crates.io Installation

```bash
# Create test project
mkdir test-cockpitctl && cd test-cockpitctl
cargo init

# Add cockpitctl dependency
cargo add cockpitctl

# Build project
cargo build

# Clean up
cd ..
rm -rf test-cockpitctl
```

**Expected output**: Build succeeds

### Step 4: Cross-Platform Testing (Optional)

If you have access to multiple platforms, test each binary:

**Linux x64**:
```bash
curl -fsSL https://github.com/EffortlessMetrics/cockpitctl/releases/download/v0.2.1/cockpitctl-linux-x64 -o cockpitctl
chmod +x cockpitctl
./cockpitctl --version
./cockpitctl ingest --artifacts fixtures/happy_path/artifacts --config fixtures/happy_path/cockpit.toml
```

**macOS x64**:
```bash
curl -fsSL https://github.com/EffortlessMetrics/cockpitctl/releases/download/v0.2.1/cockpitctl-darwin-x64 -o cockpitctl
chmod +x cockpitctl
./cockpitctl --version
```

**macOS ARM64**:
```bash
curl -fsSL https://github.com/EffortlessMetrics/cockpitctl/releases/download/v0.2.1/cockpitctl-darwin-arm64 -o cockpitctl
chmod +x cockpitctl
./cockpitctl --version
```

**Windows x64**:
```powershell
curl -fsSL https://github.com/EffortlessMetrics/cockpitctl/releases/download/v0.2.1/cockpitctl-windows-x64.exe -o cockpitctl.exe
.\cockpitctl.exe --version
```

### Step 5: Verify Documentation

- [ ] README installation instructions work
- [ ] All documentation links are valid
- [ ] Examples use current version syntax

### Step 6: Announce Release

1. **Update project status**:
   - Update README with latest version
   - Update badges if applicable

2. **Announce to team**:
   - Send release notes to internal team
   - Highlight breaking changes

3. **Announce to community**:
   - Post to relevant forums
   - Update project website
   - Social media announcement

## Troubleshooting

### Quality Gate Failures

**Symptom**: `cargo fmt` or `cargo clippy` fails

**Solution**:
```bash
# Fix formatting
cargo fmt --all

# Fix clippy warnings
cargo clippy --workspace --all-targets --fix --allow-dirty --allow-staged

# Commit fixes
git add -A
git commit -m "Fix quality gate issues"
git push origin main

# Delete and recreate tag
git tag -d v0.2.1
git push origin :refs/tags/v0.2.1
git tag v0.2.1
git push origin v0.2.1
```

### Version Mismatch

**Symptom**: Tag version doesn't match Cargo.toml version

**Solution**:
```bash
# Check versions
git tag -l v*
grep "version = " Cargo.toml

# Update Cargo.toml if needed
# Then delete and recreate tag
git tag -d v0.2.1
git push origin :refs/tags/v0.2.1
git tag v0.2.1
git push origin v0.2.1
```

### Publish Failures

**Symptom**: Crates.io publish fails

**Common causes**:
- Version already published
- Network issues
- Validation errors

**Solution**:
```bash
# Check if version already exists
cargo search cockpitctl

# If version exists, yank it
cargo yank --vers 0.2.1 cockpitctl

# Retry publish (may need to wait for index update)
cargo publish -p cockpitctl --token $CRATES_IO_TOKEN
```

### Binary Build Failures

**Symptom**: Cross-platform binary build fails

**Solution**:
1. Check the specific platform's build log
2. Identify the error
3. Fix the issue in the code
4. Create a patch release

**Common issues**:
- Target-specific dependencies
- Platform-specific code paths
- Compilation errors on specific targets

### Test Binary Failures

**Symptom**: Smoke test fails on a specific platform

**Solution**:
```bash
# Download the binary locally
curl -fsSL https://github.com/EffortlessMetrics/cockpitctl/releases/download/v0.2.1/cockpitctl-linux-x64 -o cockpitctl
chmod +x cockpitctl

# Test manually
./cockpitctl --version
./cockpitctl ingest --artifacts fixtures/happy_path/artifacts --config fixtures/happy_path/cockpit.toml

# Check fixture paths and permissions
ls -la fixtures/happy_path/
```

### Workflow Timeout

**Symptom**: Workflow times out

**Solution**:
1. Check GitHub Actions logs for the stuck step
2. Identify the cause (network, resource limits, etc.)
3. Retry the workflow

```bash
# Re-run the workflow
gh run rerun <run-id>
```

## Rollback Procedures

**Use rollback procedures only for critical issues that cannot be patched quickly.**

### Step 1: Assess Severity

Before rolling back, assess:
- Is the issue critical?
- Can it be patched quickly?
- What is the downstream impact?

### Step 2: Yank Crates from crates.io

```bash
# Yank in reverse dependency order
cargo yank --vers 0.2.1 conformctl
cargo yank --vers 0.2.1 cockpitctl
cargo yank --vers 0.2.1 cockpitctl-core
cargo yank --vers 0.2.1 cockpitctl-io
cargo yank --vers 0.2.1 cockpitctl-ingest
cargo yank --vers 0.2.1 cockpitctl-render
cargo yank --vers 0.2.1 cockpitctl-domain
cargo yank --vers 0.2.1 cockpitctl-conform
cargo yank --vers 0.2.1 cockpitctl-types
```

### Step 3: Delete GitHub Release

```bash
gh release delete v0.2.1 --yes
```

### Step 4: Delete Tag (Optional)

**Only delete the tag if no one has pulled it**:

```bash
git tag -d v0.2.1
git push origin :refs/tags/v0.2.1
```

### Step 5: Prepare Patch Release

1. **Fix the critical issue**
2. **Update CHANGELOG** with patch notes
3. **Bump patch version** (e.g., `0.2.1` → `0.2.2`)
4. **Create new tag**
5. **Execute normal release process**

```bash
# Update version in Cargo.toml
# Update CHANGELOG

# Commit changes
git add -A
git commit -m "Prepare patch release v0.2.1"

# Create and push tag
git tag v0.2.1
git push origin main
git push origin v0.2.1
```

### Step 6: Announce Patch Release

- Notify users of the critical issue
- Provide upgrade instructions
- Document the fix

## Quick Reference

### Essential Commands

```bash
# Pre-release verification
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets
cargo run -p xtask -- schema-sync-check
cargo test -p cockpitctl --test ingest_golden

# Dry-run publish
cargo publish --dry-run -p <crate-name>

# Create release
git tag v0.2.1
git push origin v0.2.1

# Monitor workflow
gh run list --workflow=release.yml
gh run view --workflow=release.yml

# Smoke test
./scripts/smoke-test-release.sh v0.2.1

# Rollback
cargo yank --vers 0.2.1 cockpitctl
gh release delete v0.2.1 --yes
```

### Release Timeline

| Phase | Duration | Actions |
|-------|----------|---------|
| Pre-Release | 30-60 min | Verification, testing, documentation |
| Tag Creation | <1 min | Create and push tag |
| Quality Gate | 2-3 min | fmt, clippy |
| Publish | 5-8 min | Tests, dry-run, publish |
| Build Binaries | 5-10 min | 8 matrix builds |
| Test Binaries | 3-5 min | Smoke tests |
| GitHub Release | 1-2 min | Create release |
| Post-Release | 15-30 min | Validation, announcement |

### Contact Information

For release-related issues:
- **GitHub Issues**: https://github.com/EffortlessMetrics/cockpitctl/issues
- **Documentation**: See [Release Preparation Guide](./release-preparation-guide.md)
- **Checklist**: See [Release Manager Checklist](./release-manager-checklist.md)

## Related Documentation

- [Release Preparation Guide](./release-preparation-guide.md) — Comprehensive release guide
- [Release Manager Checklist](./release-manager-checklist.md) — Detailed release checklist
- [Smoke Testing a Release](./smoke-test-release.md) — Post-release validation
- [Release-Ready Gate Checklist](../../RELEASE_READY_GATE_CHECKLIST.md) — Workflow verification
