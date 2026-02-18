# Toolpack Manifest

The `toolpack.json` manifest provides a declarative way to specify tools and their binary assets for installation. This is used by the GitHub Action and other tooling to download and verify binaries.

## Schema

```json
{
  "version": "0.3.0",
  "train": "nightly",
  "tools": {
    "<tool-name>": {
      "version": "0.3.0",
      "assets": {
        "<platform>": {
          "url": "https://github.com/<org>/<repo>/releases/download/vX.Y.Z/<asset>",
          "sha256": "<sha256-hash>"
        }
      }
    }
  },
  "toolpacks": {
    "<pack-name>": ["<tool-name>", ...]
  }
}
```

## Important: Use Versioned URLs

**Always pin URLs to specific release tags** instead of using `latest`. This ensures reproducibility and prevents unexpected updates.

### ❌ Incorrect (uses `latest`)
```json
{
  "url": "https://github.com/EffortlessMetrics/cockpitctl/releases/latest/download/cockpitctl-linux-x64"
}
```

### ✅ Correct (uses version tag)
```json
{
  "url": "https://github.com/EffortlessMetrics/cockpitctl/releases/download/v0.3.0/cockpitctl-linux-x64"
}
```

## SHA256 Checksums

The `sha256` field must be filled at release time with the actual checksum of the binary asset. This provides integrity verification during installation.

### How to Calculate SHA256

```bash
# Linux/macOS
sha256sum cockpitctl-linux-x64

# macOS (alternative)
shasum -a 256 cockpitctl-linux-x64

# PowerShell
Get-FileHash cockpitctl-linux-x64 -Algorithm SHA256
```

### Release Workflow

When preparing a release:

1. Build and upload binaries to GitHub Releases
2. Calculate SHA256 for each binary asset
3. Update `toolpack.json` with the correct version tag URLs and SHA256 values
4. Commit the updated `toolpack.json` to the repository

## Platform Identifiers

Supported platform identifiers:

- `linux-x64` - Linux x86_64
- `darwin-x64` - macOS x86_64 (Intel)
- `darwin-arm64` - macOS ARM64 (Apple Silicon)
- `windows-x64` - Windows x86_64

## Toolpacks

The `toolpacks` section defines named collections of tools that can be installed together:

```json
{
  "toolpacks": {
    "standard": ["cockpitctl", "conformctl"],
    "minimal": ["cockpitctl"]
  }
}
```

## Example

See [`toolpack.json`](../../toolpack.json) for a complete example.
