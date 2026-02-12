#!/usr/bin/env bash
# Smoke test script for cockpitctl release validation
# This script validates a release using only published artifacts (no vendoring required)
#
# Usage: ./scripts/smoke-test-release.sh <TAG>
# Example: ./scripts/smoke-test-release.sh v0.2.0

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Helper functions
log_info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Detect platform
detect_platform() {
    case "$(uname -s)" in
        Linux*)
            PLATFORM="linux-x64"
            BINARY_EXT=""
            ;;
        Darwin*)
            if [[ "$(uname -m)" == "arm64" ]]; then
                PLATFORM="darwin-arm64"
            else
                PLATFORM="darwin-x64"
            fi
            BINARY_EXT=""
            ;;
        MINGW*|MSYS*|CYGWIN*)
            PLATFORM="windows-x64"
            BINARY_EXT=".exe"
            ;;
        *)
            log_error "Unsupported platform: $(uname -s)"
            exit 1
            ;;
    esac
    log_info "Detected platform: $PLATFORM"
}

# Download and verify binary
download_binary() {
    local binary_name=$1
    local tag=$2
    local output_path=$3

    local asset_name="${binary_name}-${PLATFORM}${BINARY_EXT}"
    local download_url="https://github.com/EffortlessMetrics/cockpitctl/releases/download/${tag}/${asset_name}"

    log_info "Downloading $binary_name from $download_url"
    
    if command -v curl &> /dev/null; then
        curl -fsSL "$download_url" -o "$output_path"
    elif command -v wget &> /dev/null; then
        wget -qO "$output_path" "$download_url"
    else
        log_error "Neither curl nor wget is available"
        exit 1
    fi

    # Make executable on Unix
    if [[ "$OSTYPE" != "msys" && "$OSTYPE" != "win32" ]]; then
        chmod +x "$output_path"
    fi

    log_info "Downloaded to: $output_path"
}

# Test conformctl
test_conformctl() (
    local conformctl_path=$1
    local tag=$2

    log_info "Testing conformctl..."

    # Check version
    "$conformctl_path" --version || {
        log_error "conformctl --version failed"
        return 1
    }

    # Create a minimal test receipt
    local test_receipt_dir=$(mktemp -d)
    trap "rm -rf $test_receipt_dir" EXIT

    cat > "$test_receipt_dir/report.json" << 'EOF'
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

    # Run conformctl check
    "$conformctl_path" check --report "$test_receipt_dir/report.json" --sensor-id test-sensor || {
        log_error "conformctl check failed on test receipt"
        return 1
    }

    log_info "conformctl tests passed"
)

# Test cockpitctl
test_cockpitctl() (
    local cockpitctl_path=$1
    local tag=$2

    log_info "Testing cockpitctl..."

    # Check version
    "$cockpitctl_path" --version || {
        log_error "cockpitctl --version failed"
        return 1
    }

    # Create test artifacts directory
    local test_artifacts_dir=$(mktemp -d)
    trap "rm -rf $test_artifacts_dir" EXIT

    # Create minimal config
    cat > "$test_artifacts_dir/cockpit.toml" << 'EOF'
[sensor.builddiag]
required = true

[sensor.diffguard]
required = true
EOF

    # Create sensor artifacts directories
    mkdir -p "$test_artifacts_dir/artifacts/builddiag"
    mkdir -p "$test_artifacts_dir/artifacts/diffguard"

    # Create minimal sensor reports
    cat > "$test_artifacts_dir/artifacts/builddiag/report.json" << 'EOF'
{
  "schema": "builddiag.report.v1",
  "tool": {
    "name": "builddiag",
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

    cat > "$test_artifacts_dir/artifacts/diffguard/report.json" << 'EOF'
{
  "schema": "diffguard.report.v1",
  "tool": {
    "name": "diffguard",
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

    # Run cockpitctl ingest
    cd "$test_artifacts_dir"
    "$cockpitctl_path" ingest --artifacts artifacts --config cockpit.toml || {
        log_error "cockpitctl ingest failed"
        return 1
    }

    # Verify outputs
    if [[ ! -f "artifacts/cockpit/report.json" ]]; then
        log_error "cockpitctl did not create report.json"
        return 1
    fi

    if [[ ! -f "artifacts/cockpit/comment.md" ]]; then
        log_error "cockpitctl did not create comment.md"
        return 1
    fi

    # Validate report structure
    local verdict=$(python3 -c "import sys,json; print(json.load(open(sys.argv[1]))['verdict']['status'])" artifacts/cockpit/report.json)
    if [[ "$verdict" != "pass" ]]; then
        log_error "Expected verdict 'pass', got '$verdict'"
        return 1
    fi

    log_info "cockpitctl tests passed"
)

# Test composite action (optional, requires gh CLI)
test_composite_action() (
    local tag=$1

    if ! command -v gh &> /dev/null; then
        log_warn "gh CLI not found, skipping composite action test"
        return 0
    fi

    log_info "Testing composite action..."

    # Create a test workflow file
    local test_workflow_dir=$(mktemp -d)
    trap "rm -rf $test_workflow_dir" EXIT

    cat > "$test_workflow_dir/test-workflow.yml" << EOF
name: Smoke Test cockpitctl Action
on: workflow_dispatch
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Test cockpitctl action
        uses: EffortlessMetrics/cockpitctl@${tag}
        with:
          artifacts-path: fixtures/happy_path/artifacts
          config-path: fixtures/happy_path/cockpit.toml
          version: ${tag}
          post-comment: false
          fail-on-error: true
EOF

    log_info "Created test workflow at: $test_workflow_dir/test-workflow.yml"
    log_info "To test the composite action manually:"
    log_info "  1. Create a new GitHub repository or use an existing one"
    log_info "  2. Copy the workflow file to .github/workflows/"
    log_info "  3. Push and run the workflow manually via GitHub UI"
)

# Main
main() {
    if [[ $# -ne 1 ]]; then
        log_error "Usage: $0 <TAG>"
        log_error "Example: $0 v0.2.0"
        exit 1
    fi

    local tag=$1

    # Ensure tag starts with 'v'
    if [[ "$tag" != v* ]]; then
        tag="v${tag}"
    fi

    log_info "Starting smoke test for cockpitctl release: $tag"
    detect_platform

    # Create temporary directory for binaries
    local temp_dir=$(mktemp -d)
    trap "rm -rf $temp_dir" EXIT

    local conformctl_path="$temp_dir/conformctl${BINARY_EXT}"
    local cockpitctl_path="$temp_dir/cockpitctl${BINARY_EXT}"

    # Download binaries
    download_binary "conformctl" "$tag" "$conformctl_path"
    download_binary "cockpitctl" "$tag" "$cockpitctl_path"

    # Run tests
    test_conformctl "$conformctl_path" "$tag"
    test_cockpitctl "$cockpitctl_path" "$tag"
    test_composite_action "$tag"

    log_info "=========================================="
    log_info "All smoke tests passed!"
    log_info "=========================================="
    log_info "Binaries tested:"
    log_info "  - conformctl: $conformctl_path"
    log_info "  - cockpitctl: $cockpitctl_path"
    log_info ""
    log_info "Release $tag is ready for announcement."
}

main "$@"
