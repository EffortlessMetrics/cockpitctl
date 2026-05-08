#!/usr/bin/env bash
# release-dry-run.sh — compatibility wrapper for the Rust xtask implementation.
set -euo pipefail
cargo run -p xtask -- release-dry-run
