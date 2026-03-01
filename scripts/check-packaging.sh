#!/usr/bin/env bash
# check-packaging.sh — verify that publishable crates ship no junk files
# and have required crates.io metadata.
set -euo pipefail

FAIL=0

# All publishable crate package names (xtask and fuzz are publish = false)
CRATES=(
  cockpitctl
  cockpitctl-types
  cockpitctl-domain
  cockpitctl-domain-buildfix
  cockpitctl-domain-signing
  cockpitctl-domain-trend
  cockpitctl-feature-grid
  cockpitctl-feature-state
  cockpitctl-ingest
  cockpitctl-io
  cockpitctl-io-buildfix
  cockpitctl-io-hooks
  cockpitctl-io-policy-signing
  cockpitctl-io-schema
  cockpitctl-render
  cockpitctl-sarif
  cockpitctl-conform
  cockpitctl-core
  conformctl
)

echo "=== Checking cargo package --list for junk files ==="
for crate in "${CRATES[@]}"; do
  output=$(cargo package --list -p "$crate" 2>&1) || {
    echo "FAIL: cargo package --list -p $crate failed"
    FAIL=1
    continue
  }
  if echo "$output" | grep -qE '(fixtures/|docs/|\.snap$|\.snap\.new$)'; then
    echo "FAIL: $crate ships junk files:"
    echo "$output" | grep -E '(fixtures/|docs/|\.snap$|\.snap\.new$)'
    FAIL=1
  else
    echo "  OK: $crate"
  fi
done

echo ""
echo "=== Checking crate metadata ==="
for crate in "${CRATES[@]}"; do
  metadata=$(cargo metadata --format-version 1 --no-deps 2>/dev/null \
    | python3 -c "
import sys, json
m = json.load(sys.stdin)
for p in m['packages']:
    if p['name'] == '$crate':
        missing = []
        for field in ['name', 'version', 'description', 'license', 'repository']:
            if not p.get(field):
                missing.append(field)
        if missing:
            print('MISSING: ' + ', '.join(missing))
        else:
            print('OK')
        break
else:
    print('NOT_FOUND')
" 2>/dev/null) || metadata="ERROR"

  if [[ "$metadata" == "OK" ]]; then
    echo "  OK: $crate metadata"
  elif [[ "$metadata" == "NOT_FOUND" ]]; then
    echo "FAIL: $crate not found in workspace"
    FAIL=1
  elif [[ "$metadata" == ERROR* ]]; then
    echo "FAIL: $crate metadata check errored"
    FAIL=1
  else
    echo "FAIL: $crate metadata $metadata"
    FAIL=1
  fi
done

if [[ "$FAIL" -ne 0 ]]; then
  echo ""
  echo "FAILED: packaging hygiene checks found issues"
  exit 1
fi

echo ""
echo "All crates clean"
