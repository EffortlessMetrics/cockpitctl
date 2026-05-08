# Release Manager Checklist

This comprehensive checklist guides release managers through preparing, executing, and validating a cockpitctl release.

## Table of Contents

- [Phase 1: Pre-Release Preparation](#phase-1-pre-release-preparation)
- [Phase 2: Pre-Release Verification](#phase-2-pre-release-verification)
- [Phase 3: Release Execution](#phase-3-release-execution)
- [Phase 4: Post-Release Validation](#phase-4-post-release-validation)
- [Phase 5: Post-Release Tasks](#phase-5-post-release-tasks)

## Phase 1: Pre-Release Preparation

### Planning

- [ ] **Determine release version**
  - [ ] Review [`CHANGELOG.md`](../../CHANGELOG.md) for unreleased changes
  - [ ] Decide on version bump (major/minor/patch) based on semver
  - [ ] Document breaking changes if major version bump

- [ ] **Schedule release window**
  - [ ] Coordinate with team for release timing
  - [ ] Ensure no conflicting changes in progress
  - [ ] Plan for potential rollback

### Code Review

- [ ] **Review all pending changes**
  - [ ] All PRs for this release are merged to `main`
  - [ ] No unmerged changes intended for this release
  - [ ] Release branch (if used) is up to date

- [ ] **Verify feature completeness**
  - [ ] All planned features are implemented
  - [ ] All planned bug fixes are implemented
  - [ ] Documentation is updated for new features

## Phase 2: Pre-Release Verification

### Code Quality Verification

- [ ] **Run formatting check**
  ```bash
  cargo fmt --all -- --check
  ```
  - [ ] No formatting errors

- [ ] **Run clippy lint check**
  ```bash
  cargo clippy --workspace --all-targets -- -D warnings
  ```
  - [ ] No clippy warnings

- [ ] **Run all tests**
  ```bash
  cargo test --workspace --all-targets
  ```
  - [ ] All tests pass

### Version Consistency Checks

- [ ] **Verify workspace version in [`Cargo.toml`](../../Cargo.toml)**
  ```bash
  grep "version = " Cargo.toml
  ```
  - [ ] Version matches planned release (e.g., `0.3.0`)

- [ ] **Verify individual crate versions**
  ```bash
  grep "version = " crates/*/Cargo.toml
  ```
  - [ ] All crates use workspace version or correct dependency versions

- [ ] **Verify no hardcoded versions in dependencies**
  - [ ] All crates use workspace dependencies where appropriate

### Documentation Updates

- [ ] **Update [`CHANGELOG.md`](../../CHANGELOG.md)**
  - [ ] Move items from `[Unreleased]` to new version section
  - [ ] Add release date in ISO format (YYYY-MM-DD)
  - [ ] Create new `[Unreleased]` section
  - [ ] Categorize changes: Added, Changed, Deprecated, Removed, Fixed, Security

- [ ] **Review [`README.md`](../../README.md)**
  - [ ] Installation instructions are current
  - [ ] Quick start examples work
  - [ ] Version references are updated

- [ ] **Review reference documentation**
  - [ ] [`docs/reference/cli.md`](../../docs/reference/cli.md) — CLI commands are documented
  - [ ] [`docs/reference/config.md`](../../docs/reference/config.md) — Configuration options are current
  - [ ] [`docs/reference/compatibility.md`](../../docs/reference/compatibility.md) — Compatibility notes are updated

- [ ] **Review how-to guides**
  - [ ] [`docs/how-to/`](../../docs/how-to/) — All guides are accurate
  - [ ] Examples use current version syntax

### Schema and Contract Verification

- [ ] **Run schema sync check**
  ```bash
  cargo run -p xtask -- schema-sync-check
  ```
  - [ ] No schema sync errors

- [ ] **Run golden tests**
  ```bash
  cargo test -p cockpitctl --test ingest_golden
  ```
  - [ ] All golden tests pass

- [ ] **Run BDD tests** (optional)
  ```bash
  cargo test -p cockpitctl --test bdd
  ```
  - [ ] All BDD tests pass

### Package Verification

- [ ] **Run local package validation for all crates**
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
  - [ ] All package listings complete successfully
  - [ ] No packaging warnings or errors

- [ ] **Optionally dry-run first publish tier**
  ```bash
  cargo publish --dry-run -p cockpitctl-types
  ```
  - [ ] First-tier dry-run succeeds
  - [ ] CI release workflow will perform per-crate dry-runs before each publish step

- [ ] **Verify packaging contents**
  ```bash
  cargo package --list -p cockpitctl
  ```
  - [ ] No large fixtures or docs junk included
  - [ ] Expected source/tests/schemas are present
  - [ ] All necessary files are included
  - [ ] Embedded schemas are included

### Binary Build Verification

- [ ] **Build release binaries locally**
  ```bash
  cargo build --release -p cockpitctl
  cargo build --release -p conformctl
  ```
  - [ ] Build completes without errors

- [ ] **Verify binary functionality**
  ```bash
  ./target/release/cockpitctl --version
  ./target/release/conformctl --version
  ```
  - [ ] Version output is correct

- [ ] **Test binary on fixtures**
  ```bash
  ./target/release/cockpitctl ingest --artifacts fixtures/happy_path/artifacts --config fixtures/happy_path/cockpit.toml
  ```
  - [ ] Ingest completes successfully

### Security Verification

- [ ] **Run security audit**
  ```bash
  cargo audit
  ```
  - [ ] No critical vulnerabilities
  - [ ] No high vulnerabilities (or documented exceptions)

- [ ] **Review dependency changes**
  - [ ] Understand any new dependencies
  - [ ] Verify licenses are acceptable

### Integration Verification

- [ ] **Verify GitHub Actions workflows**
  - [ ] [`.github/workflows/ci.yml`](../../.github/workflows/ci.yml) is up to date
  - [ ] [`.github/workflows/release.yml`](../../.github/workflows/release.yml) is up to date
  - [ ] Secrets are configured (CARGO_REGISTRY_TOKEN)

- [ ] **Verify composite action** (if applicable)
  - [ ] [`action.yml`](../../action.yml) version references are correct
  - [ ] Action inputs are documented

## Phase 3: Release Execution

### Tag Creation

- [ ] **Create version tag**
  ```bash
  git tag v0.3.0
  ```
  - [ ] Tag follows `v*` pattern

- [ ] **Verify tag locally**
  ```bash
  git tag -l v*
  git show v0.3.0
  ```
  - [ ] Tag points to correct commit
  - [ ] Tag annotation is correct

- [ ] **Push tag to origin**
  ```bash
  git push origin v0.3.0
  ```
  - [ ] Tag push succeeds

### Workflow Monitoring

- [ ] **Navigate to GitHub Actions**
  - [ ] Release workflow is triggered
  - [ ] Workflow run is visible

- [ ] **Monitor Quality Gate job**
  - [ ] `cargo fmt` check passes
  - [ ] `cargo clippy` check passes
  - [ ] Job completes successfully

- [ ] **Monitor Publish job**
  - [ ] Version tag matches Cargo.toml
  - [ ] All tests pass
  - [ ] All dry-runs pass
  - [ ] All crates publish in order:
    - [ ] cockpitctl-types
    - [ ] cockpitctl-conform
    - [ ] cockpitctl-domain
    - [ ] cockpitctl-render
    - [ ] cockpitctl-ingest
    - [ ] cockpitctl-io
    - [ ] cockpitctl-sarif
    - [ ] cockpitctl-core
    - [ ] cockpitctl
    - [ ] conformctl
  - [ ] Job completes successfully

- [ ] **Monitor Build Binaries job**
  - [ ] All 8 matrix builds complete:
    - [ ] cockpitctl-linux-x64
    - [ ] cockpitctl-darwin-x64
    - [ ] cockpitctl-darwin-arm64
    - [ ] cockpitctl-windows-x64.exe
    - [ ] conformctl-linux-x64
    - [ ] conformctl-darwin-x64
    - [ ] conformctl-darwin-arm64
    - [ ] conformctl-windows-x64.exe
  - [ ] All artifacts uploaded
  - [ ] Job completes successfully

- [ ] **Monitor Test Binaries job**
  - [ ] All 8 binaries are downloaded
  - [ ] All `--version` checks pass
  - [ ] All `cockpitctl ingest` tests pass
  - [ ] All `conformctl check` tests pass
  - [ ] Job completes successfully

- [ ] **Monitor GitHub Release job**
  - [ ] All artifacts downloaded
  - [ ] SHA256SUMS.txt generated
  - [ ] Release created with all assets
  - [ ] Job completes successfully

### Release Verification

- [ ] **Verify crates.io publications**
  - [ ] All 10 crates are published:
    - [ ] https://crates.io/crates/cockpitctl-types
    - [ ] https://crates.io/crates/cockpitctl-conform
    - [ ] https://crates.io/crates/cockpitctl-domain
    - [ ] https://crates.io/crates/cockpitctl-render
    - [ ] https://crates.io/crates/cockpitctl-ingest
    - [ ] https://crates.io/crates/cockpitctl-io
    - [ ] https://crates.io/crates/cockpitctl-sarif
    - [ ] https://crates.io/crates/cockpitctl-core
    - [ ] https://crates.io/crates/cockpitctl
    - [ ] https://crates.io/crates/conformctl

- [ ] **Verify GitHub Release**
  - [ ] Release exists at `https://github.com/EffortlessMetrics/cockpitctl/releases/tag/v0.3.0`
  - [ ] Release notes are generated
  - [ ] All 8 binaries are attached
  - [ ] SHA256SUMS.txt is attached
  - [ ] Checksums are correct

## Phase 4: Post-Release Validation

### Smoke Testing

- [ ] **Run smoke test xtask**
  ```bash
  cargo run -p xtask -- smoke-test-release v0.3.0
  ```
  - [ ] All smoke tests pass

### Checksum Verification

- [ ] **Download checksums**
  ```bash
  curl -fsSL https://github.com/EffortlessMetrics/cockpitctl/releases/download/v0.3.0/SHA256SUMS.txt -o SHA256SUMS.txt
  ```
  - [ ] Checksums file downloaded

- [ ] **Download and verify a binary**
  ```bash
  curl -fsSL https://github.com/EffortlessMetrics/cockpitctl/releases/download/v0.3.0/cockpitctl-linux-x64 -o cockpitctl-linux-x64
  chmod +x cockpitctl-linux-x64
  sha256sum -c SHA256SUMS.txt --ignore-missing
  ```
  - [ ] Checksum verification succeeds

### crates.io Installation Test

- [ ] **Create test project**
  ```bash
  mkdir test-cockpitctl && cd test-cockpitctl
  cargo init
  ```
  - [ ] Test project created

- [ ] **Add cockpitctl dependency**
  ```bash
  cargo add cockpitctl
  ```
  - [ ] Dependency added successfully

- [ ] **Build test project**
  ```bash
  cargo build
  ```
  - [ ] Build succeeds

- [ ] **Clean up**
  ```bash
  cd ..
  rm -rf test-cockpitctl
  ```
  - [ ] Cleanup complete

### Cross-Platform Testing (if available)

- [ ] **Linux x64**
  - [ ] `cockpitctl-linux-x64` runs
  - [ ] `conformctl-linux-x64` runs
  - [ ] Smoke tests pass

- [ ] **macOS x64**
  - [ ] `cockpitctl-darwin-x64` runs
  - [ ] `conformctl-darwin-x64` runs
  - [ ] Smoke tests pass

- [ ] **macOS ARM64**
  - [ ] `cockpitctl-darwin-arm64` runs
  - [ ] `conformctl-darwin-arm64` runs
  - [ ] Smoke tests pass

- [ ] **Windows x64**
  - [ ] `cockpitctl-windows-x64.exe` runs
  - [ ] `conformctl-windows-x64.exe` runs
  - [ ] Smoke tests pass

### Documentation Verification

- [ ] **Verify README links**
  - [ ] Installation links work
  - [ ] Example commands work

- [ ] **Verify reference docs**
  - [ ] All links are valid
  - [ ] Content is current

- [ ] **Verify how-to guides**
  - [ ] Examples work with new version
  - [ ] No outdated instructions

## Phase 5: Post-Release Tasks

### Release Announcement

- [ ] **Update project status**
  - [ ] Update project README with latest version
  - [ ] Update badges if applicable

- [ ] **Announce to team**
  - [ ] Notify internal team of release
  - [ ] Share release notes

- [ ] **Announce to community**
  - [ ] Post to relevant forums/mailing lists
  - [ ] Update project website if applicable
  - [ ] Social media announcement

### Post-Release Maintenance

- [ ] **Create maintenance branch** (if using LTS model)
  - [ ] Branch created from release tag
  - [ ] Branch configured for patches

- [ ] **Update development branch**
  - [ ] Bump version to next development version
  - [ ] Update CHANGELOG with new `[Unreleased]` section

- [ ] **Close release issues**
  - [ ] Close all issues included in release
  - [ ] Update issue tracker

### Release Retrospective

- [ ] **Document any issues**
  - [ ] Note any problems encountered
  - [ ] Document solutions for future reference

- [ ] **Update release process**
  - [ ] Update documentation if process changed
  - [ ] Update checklists if needed

## Rollback Checklist (Emergency Use Only)

**Only use this section if a critical issue is discovered after release.**

### Assess Severity

- [ ] **Determine rollback necessity**
  - [ ] Issue severity is critical
  - [ ] Issue cannot be patched quickly
  - [ ] Downstream impact is significant

### Crates.io Yank

- [ ] **Yank all crates** (in reverse dependency order)
  ```bash
  cargo yank --vers 0.3.0 conformctl
  cargo yank --vers 0.3.0 cockpitctl
  cargo yank --vers 0.3.0 cockpitctl-core
  cargo yank --vers 0.3.0 cockpitctl-io
  cargo yank --vers 0.3.0 cockpitctl-ingest
  cargo yank --vers 0.3.0 cockpitctl-render
  cargo yank --vers 0.3.0 cockpitctl-domain
  cargo yank --vers 0.3.0 cockpitctl-conform
  cargo yank --vers 0.3.0 cockpitctl-types
  ```
  - [ ] All crates yanked

### GitHub Release Deletion

- [ ] **Delete GitHub release**
  ```bash
  gh release delete v0.3.0 --yes
  ```
  - [ ] Release deleted

- [ ] **Delete tag** (optional, only if no one has pulled it)
  ```bash
  git tag -d v0.3.0
  git push origin :refs/tags/v0.3.0
  ```
  - [ ] Tag deleted

### Patch Release Preparation

- [ ] **Fix the critical issue**
  - [ ] Bug fix implemented
  - [ ] Tests added/updated

- [ ] **Prepare patch release**
  - [ ] Update CHANGELOG with patch notes
  - [ ] Bump patch version (e.g., 0.3.0 → 0.3.1)
  - [ ] Create new tag

- [ ] **Execute patch release**
  - [ ] Follow normal release process
  - [ ] Announce patch release

## Related Documentation

- [Release Preparation Guide](./release-preparation-guide.md) — Comprehensive release guide
- [Release Runbook](./release-runbook.md) — Step-by-step execution guide
- [Smoke Testing a Release](./smoke-test-release.md) — Post-release validation
- [Release-Ready Gate Checklist](../../RELEASE_READY_GATE_CHECKLIST.md) — Workflow verification
