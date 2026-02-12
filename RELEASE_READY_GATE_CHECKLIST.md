# Release-Ready Gate Checklist

This checklist maps directly to the jobs and steps in `.github/workflows/release.yml` to ensure a complete release pipeline.

## Overview

The release workflow consists of five jobs triggered by pushing a `v*` tag:

1. **quality-gate** — `cargo fmt` and `cargo clippy` checks (must pass before publish)
2. **publish** — Publishes crates to crates.io in dependency order with dry-run validation
3. **build-binaries** — Builds cross-platform binaries (4 platforms x 2 binaries)
4. **test-binaries** — Smoke tests every built binary on its native runner
5. **github-release** — Creates GitHub Release with SHA256 checksums

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
✅ **CONFIRMED** — Malformed JSON produces a finding (`cockpit.invalid_receipt`), not a runtime abort. See `crates/cockpitctl-io/src/lib.rs`.

### Determinism
✅ **CONFIRMED** — Documented in `docs/reference/determinism.md`. Covers sensor discovery order, findings sort, highlights sort, and JSON formatting rules.

## Current Release Readiness Status

| Category | Status | Confidence |
|----------|--------|------------|
| Quality Gates (fmt, clippy, test) | ✅ Complete | High |
| Dry-Run Publishing | ✅ Complete | High |
| Core Publishing (dependency order) | ✅ Complete | High |
| Binary Building (4 platforms) | ✅ Complete | High |
| E2E Binary Testing | ✅ Complete | High |
| SHA256 Checksums | ✅ Complete | High |
| GitHub Release | ✅ Complete | High |

## Release Process

1. Ensure all changes are merged to `main`
2. Update `CHANGELOG.md` — move items from `[Unreleased]` to `[X.Y.Z] - YYYY-MM-DD`
3. Bump version in workspace `Cargo.toml` if needed
4. Tag: `git tag v0.2.0 && git push origin v0.2.0`
5. Monitor the release workflow in GitHub Actions
6. After release, run `scripts/smoke-test-release.sh v0.2.0` (or `.ps1` on Windows) to validate published artifacts
