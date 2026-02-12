# Release-Ready Gate Checklist

This checklist maps directly to the jobs and steps in `.github/workflows/release.yml` to ensure a complete release pipeline.

## Overview

The release workflow consists of three main jobs:
1. **publish** - Publishes crates to crates.io in dependency order
2. **build-binaries** - Builds cross-platform binaries for GitHub Release
3. **github-release** - Creates GitHub Release with binaries as assets

## Job/Step Mapping and Release Guarantees

### 1. Publish Job (`publish`)

| Step | Release Guarantee | Status | Notes | 
|------|-------------------|--------|-------|
| Checkout code | ✅ Source code available | ✅ | Uses `actions/checkout@v4` |
| Install Rust toolchain | ✅ Correct Rust version with fmt/clippy | ✅ | Uses `dtolnay/rust-action@stable` |
| Cache cargo registry | ⚠️ Build performance | ✅ | Caches `~/.cargo/registry` and `~/.cargo/git` |
| Verify version tag matches Cargo.toml | ✅ Version consistency | ✅ | Critical for release integrity |
| Run tests | ✅ Code quality | ⚠️ Missing quality gates | Only runs `cargo test`, missing fmt/clippy checks |
| Publish cockpitctl-types | ✅ Dependency order | ✅ | First in dependency chain |
| Wait for crates.io index update | ⚠️ Reliability | ✅ | 30-second wait for index propagation |
| Publish cockpitctl-conform | ✅ Dependency order | ✅ | Depends on types |
| Publish cockpitctl-domain | ✅ Dependency order | ✅ | Depends on conform |
| Wait for crates.io index update | ⚠️ Reliability | ✅ | 30-second wait |
| Publish cockpitctl-render | ✅ Dependency order | ✅ | Depends on domain |
| Publish cockpitctl-ingest | ✅ Dependency order | ✅ | Depends on render |
| Wait for crates.io index update | ⚠️ Reliability | ✅ | 30-second wait |
| Publish cockpitctl-io | ✅ Dependency order | ✅ | Depends on ingest |
| Wait for crates.io index update | ⚠️ Reliability | ✅ | 30-second wait |
| Publish cockpitctl-core | ✅ Dependency order | ✅ | Depends on io |
| Wait for crates.io index update | ⚠️ Reliability | ✅ | 30-second wait |
| Publish cockpitctl | ✅ Dependency order | ✅ | Main package, depends on core |
| Publish conformctl | ✅ Dependency order | ✅ | Secondary binary |

### 2. Build Binaries Job (`build-binaries`)

| Step | Release Guarantee | Status | Notes |
|------|-------------------|--------|-------|
| Checkout code | ✅ Source code available | ✅ | Uses `actions/checkout@v4` |
| Install Rust toolchain | ✅ Cross-compilation support | ✅ | Installs target-specific toolchains |
| Cache cargo registry | ⚠️ Build performance | ✅ | Includes target directory cache |
| Build release binary | ✅ Optimized binaries | ✅ | Uses `--release` flag |
| Rename binary (Unix) | ✅ Consistent naming | ✅ | Unix-specific naming |
| Rename binary (Windows) | ✅ Consistent naming | ✅ | Windows-specific naming |
| Upload binary artifact | ✅ Artifact preservation | ✅ | Uses `actions/upload-artifact@v4` |

**Matrix Targets:**
- ✅ Linux x64 (cockpitctl & conformctl)
- ✅ macOS x64 (cockpitctl & conformctl)
- ✅ macOS ARM64 (cockpitctl & conformctl)
- ✅ Windows x64 (cockpitctl & conformctl)

### 3. GitHub Release Job (`github-release`)

| Step | Release Guarantee | Status | Notes |
|------|-------------------|--------|-------|
| Checkout code | ✅ Source code available | ✅ | Uses `actions/checkout@v4` |
| Download all artifacts | ✅ Binary retrieval | ✅ | Downloads from build-binaries |
| Prepare release assets | ✅ Asset organization | ✅ | Moves to release-assets directory |
| Create GitHub Release | ✅ Release creation | ✅ | Uses `softprops/action-gh-release@v1` |

## Release-Ready Questions and Answers

### Does the tag trigger the build matrix correctly?
✅ **YES** - The workflow triggers on `push: tags: - 'v*'` and the build-binaries job uses a comprehensive matrix covering all supported platforms.

### Are GitHub Release assets attached with consistent naming?
✅ **YES** - Assets are renamed consistently:
- `cockpitctl-linux-x64`
- `cockpitctl-darwin-x64`
- `cockpitctl-darwin-arm64`
- `cockpitctl-windows-x64.exe`
- Same pattern for `conformctl`

### Are checksums generated, uploaded, downloaded, and verified?
❌ **NO** - The workflow does not generate or verify checksums for the release binaries. This is a security gap.

### Can the Action install a pinned version and run end-to-end ingest on all supported runners?
❌ **NO** - There is no end-to-end test that installs the released binaries and verifies they work correctly on all platforms.

### Does `cargo publish --dry-run` succeed for every intended crate?
❌ **NO** - The workflow publishes directly without running `--dry-run` first to catch potential issues before actual publishing.

### Are all quality gates (fmt, clippy, test) run before publishing?
⚠️ **PARTIALLY** - Only `cargo test` is run. Missing:
- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`

## Verified Fixes

### Malformed JSON Handling Fix
✅ **CONFIRMED** - The fix is present in [`crates/cockpitctl-io/src/lib.rs`](crates/cockpitctl-io/src/lib.rs:307-319):
```rust
// First, parse the JSON. If parsing fails, return Invalid with the parse error.
// This ensures ingest survivability: malformed JSON produces a finding, not a runtime abort.
let value: serde_json::Value = match serde_json::from_slice(bytes) {
    Ok(v) => v,
    Err(e) => {
        // Malformed JSON: return Invalid with the parse error message
        return Ok(SchemaValidationResult::Invalid(vec![format!(
            "malformed JSON: {}",
            e
        )]));
    }
};
```

### Determinism Documentation Fix
✅ **CONFIRMED** - The documentation in [`docs/reference/determinism.md`](docs/reference/determinism.md) is complete and accurate, covering:
- Sensor discovery order (lexical)
- Findings sort order (severity desc → sensor_id → path → line → code → message)
- Highlights sort order (severity desc → blocking desc → sensor_id → path → line → code)
- JSON formatting rules
- Testing approach

## Gaps and Recommendations

### Critical Gaps
1. **Missing Quality Gates**: No `cargo fmt` or `cargo clippy` checks
2. **No Checksums**: Release binaries lack SHA256 checksums
3. **No E2E Testing**: Released binaries aren't tested after build
4. **No Dry Run**: Publishing without validation

### Recommended Improvements

#### 1. Add Quality Gates (High Priority)
Add steps before publishing:
```yaml
- name: Check formatting
  run: cargo fmt --all -- --check

- name: Run clippy
  run: cargo clippy --workspace --all-targets -- -D warnings
```

#### 2. Add Checksum Generation (High Priority)
After building binaries:
```yaml
- name: Generate checksums
  run: |
    cd release-assets
    sha256sum * > checksums.txt
```

#### 3. Add E2E Test (Medium Priority)
Create a new job that:
- Downloads the released binaries
- Tests `cockpitctl ingest` on sample artifacts
- Verifies exit codes and output

#### 4. Add Dry Run Publishing (Medium Priority)
Before each publish step:
```yaml
- name: Dry run publish
  run: cargo publish -p cockpitctl-types --dry-run --token ${{ secrets.CRATES_IO_TOKEN }}
```

#### 5. Add Release Notes Validation (Low Priority)
Verify CHANGELOG.md has entries for the version being released.

## Current Release Readiness Status

| Category | Status | Confidence |
|----------|--------|------------|
| Core Publishing | ✅ Working | High |
| Binary Building | ✅ Working | High |
| GitHub Release | ✅ Working | High |
| Quality Assurance | ⚠️ Partial | Medium |
| Security | ⚠️ Gaps | Medium |
| End-to-End Validation | ❌ Missing | Low |

## Conclusion

The release pipeline has a solid foundation with correct dependency ordering and cross-platform builds. However, it's missing critical quality gates and security features that would make it fully production-ready. The malformed JSON handling and determinism documentation fixes are properly in place.

**Priority Actions:**
1. Add `cargo fmt` and `cargo clippy` checks before publishing
2. Implement checksum generation and verification
3. Add end-to-end testing of released binaries
4. Consider adding `--dry-run` validation before actual publishing

With these improvements, the release pipeline would meet enterprise-grade standards for reliability and security.