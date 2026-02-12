# Release-Ready Gate Checklist

> **Status**: ✅ ALL GATES IMPLEMENTED AND OPERATIONAL
>
> This checklist maps directly to the jobs and steps in `.github/workflows/release.yml` to ensure a complete release pipeline. All release gates have been implemented and are functioning correctly.

## Overview

The release workflow consists of five jobs triggered by pushing a `v*` tag:

1. **quality-gate** — `cargo fmt` and `cargo clippy` checks (must pass before publish) ✅
2. **publish** — Publishes crates to crates.io in dependency order with dry-run validation ✅
3. **build-binaries** — Builds cross-platform binaries (4 platforms x 2 binaries) ✅
4. **test-binaries** — Smoke tests every built binary on its native runner ✅
5. **github-release** — Creates GitHub Release with SHA256 checksums ✅

## Job/Step Mapping and Release Guarantees

### 1. Quality Gate Job (`quality-gate`)

| Step | Release Guarantee | Status |
|------|-------------------|--------|
| Install Rust toolchain (with rustfmt, clippy) | Correct toolchain | ✅ |
| `cargo fmt --all -- --check` | Code formatting | ✅ |
| `cargo clippy --workspace --all-targets -- -D warnings` | Lint-clean code | ✅ |

### 2. Publish Job (`publish`)

Requires `quality-gate` to pass first.

| Step | Release Guarantee | Status |
|------|-------------------|--------|
| Verify version tag matches Cargo.toml | Version consistency | ✅ |
| `cargo test --workspace --all-targets` | All tests pass | ✅ |
| Dry-run publish per crate | Catches packaging errors before real publish | ✅ |
| Publish in dependency order | crates.io dependency resolution | ✅ |
| 30s waits between tiers | crates.io index propagation | ✅ |

**Publish order:** types → conform → domain → render → ingest → io → core → cockpitctl → conformctl

### 3. Build Binaries Job (`build-binaries`)

Requires `quality-gate` to pass first.

| Step | Release Guarantee | Status |
|------|-------------------|--------|
| `cargo build --release --target <target>` | Optimized binaries | ✅ |
| Rename to consistent asset names | Predictable download URLs | ✅ |
| Upload via `actions/upload-artifact@v4` | Artifact preservation | ✅ |

**Matrix Targets:**
- Linux x64 (cockpitctl & conformctl)
- macOS x64 (cockpitctl & conformctl)
- macOS ARM64 (cockpitctl & conformctl)
- Windows x64 (cockpitctl & conformctl)

### 4. Test Binaries Job (`test-binaries`)

Requires `build-binaries` to complete first.

| Step | Release Guarantee | Status |
|------|-------------------|--------|
| Download built artifact | Binary retrieval | ✅ |
| `--version` check | Binary loads and prints version | ✅ |
| `cockpitctl ingest` on happy_path fixture | End-to-end ingest works | ✅ |
| `conformctl check` on fixture receipt | End-to-end conformance works | ✅ |

### 5. GitHub Release Job (`github-release`)

Requires both `publish` and `test-binaries` to complete.

| Step | Release Guarantee | Status |
|------|-------------------|--------|
| Download all artifacts | Binary retrieval | ✅ |
| Generate SHA256SUMS.txt | Integrity verification | ✅ |
| Create release via `softprops/action-gh-release@v1` | Release with assets | ✅ |

## Release-Ready Questions and Answers

### Are all quality gates (fmt, clippy, test) run before publishing?
✅ **YES** — The `quality-gate` job runs `cargo fmt` and `cargo clippy` and must pass before `publish` starts. The `publish` job then runs `cargo test`.

### Does `cargo publish --dry-run` succeed for every intended crate?
✅ **YES** — Each crate has a dry-run step immediately before its real publish step.

### Are checksums generated and attached to the release?
✅ **YES** — `SHA256SUMS.txt` is generated from all binaries and uploaded as a release asset.

### Can the binaries run end-to-end on all supported platforms?
✅ **YES** — The `test-binaries` job downloads each binary on its native runner and exercises `cockpitctl ingest` and `conformctl check`.

### Does the tag trigger the build matrix correctly?
✅ **YES** — Triggers on `push: tags: - 'v*'` and the matrix covers all 8 binary variants.

### Are GitHub Release assets attached with consistent naming?
✅ **YES** — Assets follow the pattern `{binary}-{platform}[-exe]`:
- `cockpitctl-linux-x64`, `cockpitctl-darwin-x64`, `cockpitctl-darwin-arm64`, `cockpitctl-windows-x64.exe`
- `conformctl-linux-x64`, `conformctl-darwin-x64`, `conformctl-darwin-arm64`, `conformctl-windows-x64.exe`

## Verified Fixes

### Malformed JSON Handling
✅ **CONFIRMED** — Malformed JSON produces a `cockpit.invalid_receipt` finding. Parseable but schema-invalid JSON produces a `cockpit.schema_violation` finding. Neither causes a runtime abort. See `crates/cockpitctl-io/src/lib.rs`.

### Determinism
✅ **CONFIRMED** — Documented in `docs/reference/determinism.md`. Covers sensor discovery order, findings sort, highlights sort, and JSON formatting rules.

## Current Release Readiness Status

| Category | Status | Confidence | Implementation Date |
|----------|--------|------------|---------------------|
| Quality Gates (fmt, clippy, test) | ✅ IMPLEMENTED | High | 2026-02-10 |
| Dry-Run Publishing | ✅ IMPLEMENTED | High | 2026-02-10 |
| Core Publishing (dependency order) | ✅ IMPLEMENTED | High | 2026-02-10 |
| Binary Building (4 platforms) | ✅ IMPLEMENTED | High | 2026-02-10 |
| E2E Binary Testing | ✅ IMPLEMENTED | High | 2026-02-10 |
| SHA256 Checksums | ✅ IMPLEMENTED | High | 2026-02-10 |
| GitHub Release | ✅ IMPLEMENTED | High | 2026-02-10 |
| Malformed JSON Survivability | ✅ IMPLEMENTED | High | 2026-02-10 |
| Determinism Guarantees | ✅ IMPLEMENTED | High | 2026-02-10 |

## Release Process Summary

The release process is fully automated and triggered by version tags:

1. Ensure all changes are merged to `main`
2. Update `CHANGELOG.md` — move items from `[Unreleased]` to `[X.Y.Z] - YYYY-MM-DD`
3. Bump version in workspace `Cargo.toml` if needed
4. Tag: `git tag v0.2.0 && git push origin v0.2.0`
5. Monitor the release workflow in GitHub Actions
6. After release, run `scripts/smoke-test-release.sh v0.2.0` (or `.ps1` on Windows) to validate published artifacts

## Comprehensive Release Documentation

For detailed release procedures, refer to the following documentation:

- **[Release Preparation Guide](docs/how-to/release-preparation-guide.md)** — Comprehensive guide covering pre-release verification, release execution, and post-release validation
- **[Release Manager Checklist](docs/how-to/release-manager-checklist.md)** — Detailed checklist for release managers covering all phases of the release process
- **[Release Runbook](docs/how-to/release-runbook.md)** — Step-by-step guide for executing a release with troubleshooting procedures
- **[Smoke Testing a Release](docs/how-to/smoke-test-release.md)** — Post-release validation using published artifacts

## Additional Release Considerations

### Version Management

- All crates use the workspace version defined in `[workspace.package]` in [`Cargo.toml`](Cargo.toml:17)
- Version consistency is verified by the release workflow before publishing
- Follow Semantic Versioning (semver) for all releases

### Release Frequency

- Releases are tag-driven and can be created at any time
- No scheduled release cadence — release when features are ready
- Consider downstream impact when planning releases

### Rollback Procedures

If a critical issue is discovered after release:

1. **Yank the crates** from crates.io:
   ```bash
   cargo yank --vers 0.2.0 cockpitctl
   # Repeat for all published crates
   ```

2. **Delete the GitHub release** (if necessary):
   ```bash
   gh release delete v0.2.0 --yes
   ```

3. **Prepare a patch release** (e.g., `v0.2.1`):
   - Fix the issue
   - Update CHANGELOG
   - Create new tag
   - Trigger new release

See the [Release Manager Checklist](docs/how-to/release-manager-checklist.md#rollback-checklist-emergency-use-only) for complete rollback procedures.

### Release Artifacts

Each release produces:

**Crates.io packages (9 crates):**
- `cockpitctl-types` — Core DTOs and embedded schemas
- `cockpitctl-conform` — Conformance checking library
- `cockpitctl-domain` — Pure determinism and selection logic
- `cockpitctl-render` — Markdown renderer
- `cockpitctl-ingest` — Orchestration and ports
- `cockpitctl-io` — Filesystem adapters
- `cockpitctl-core` — Facade crate
- `cockpitctl` — CLI binary
- `conformctl` — Standalone conformance checker

**GitHub Release assets (9 files):**
- `cockpitctl-linux-x64`
- `cockpitctl-darwin-x64`
- `cockpitctl-darwin-arm64`
- `cockpitctl-windows-x64.exe`
- `conformctl-linux-x64`
- `conformctl-darwin-x64`
- `conformctl-darwin-arm64`
- `conformctl-windows-x64.exe`
- `SHA256SUMS.txt`

### Supported Platforms

The release workflow builds and tests binaries for:

- **Linux x64** (`x86_64-unknown-linux-gnu`)
- **macOS x64** (`x86_64-apple-darwin`)
- **macOS ARM64** (`aarch64-apple-darwin`)
- **Windows x64** (`x86_64-pc-windows-msvc`)

### Release Workflow Timing

Typical workflow execution times:

- Quality Gate: ~2-3 minutes
- Publish: ~5-8 minutes (includes 30s waits between tiers)
- Build Binaries: ~5-10 minutes (8 matrix builds in parallel)
- Test Binaries: ~3-5 minutes
- GitHub Release: ~1-2 minutes

**Total expected time**: 15-30 minutes

### Pre-Release Checklist Summary

Before creating a release tag, ensure:

- [ ] All quality checks pass locally (`cargo fmt`, `cargo clippy`, `cargo test`)
- [ ] Version is consistent across [`Cargo.toml`](Cargo.toml:17) and intended tag
- [ ] [`CHANGELOG.md`](CHANGELOG.md) is updated with release notes
- [ ] Documentation is current and accurate
- [ ] Schema sync check passes: `cargo run -p xtask -- schema-sync-check`
- [ ] Golden tests pass: `cargo test -p cockpitctl --test ingest_golden`
- [ ] Dry-run publishes succeed for all crates
- [ ] Security audit passes: `cargo audit`

### Post-Release Validation Summary

After the release is complete:

- [ ] Run smoke test script: `./scripts/smoke-test-release.sh v0.2.0`
- [ ] Verify checksums match downloaded binaries
- [ ] Test crates.io installation: `cargo add cockpitctl`
- [ ] Test binaries on available platforms
- [ ] Verify documentation links are working
- [ ] Announce release to team and community

## Related Documentation

- [Release Preparation Guide](docs/how-to/release-preparation-guide.md) — Comprehensive release guide
- [Release Manager Checklist](docs/how-to/release-manager-checklist.md) — Detailed release checklist
- [Release Runbook](docs/how-to/release-runbook.md) — Step-by-step execution guide
- [Smoke Testing a Release](docs/how-to/smoke-test-release.md) — Post-release validation
- [AGENTS.md](AGENTS.md) — Core contracts and project architecture
