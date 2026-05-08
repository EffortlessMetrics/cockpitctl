#!/usr/bin/env bash
# Compatibility wrapper: implementation lives in Rust xtask.
set -euo pipefail
cargo run -p xtask -- check-packaging "$@"
