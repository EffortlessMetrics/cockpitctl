#!/usr/bin/env bash
# check-packaging.sh — compatibility wrapper for the Rust xtask implementation.
set -euo pipefail
cargo run -p xtask -- check-packaging
