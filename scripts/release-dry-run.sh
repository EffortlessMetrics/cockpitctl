#!/usr/bin/env bash
# release-dry-run.sh — simulate a full crates.io publish in tier order.
#
# Packages all 19 publishable crates in the same order as the release
# workflow, verifies no oversized packages, checks embedded schemas,
# and reports package sizes.
set -euo pipefail

# Max package size in bytes (10 MB — crates.io hard limit)
MAX_SIZE=$((10 * 1024 * 1024))
FAIL=0

# Publish order (matches release.yml tiers 1-9)
CRATES=(
  # Tier 1: leaf crates
  cockpitctl-types
  cockpitctl-feature-state
  # Tier 2: depends on types / feature-state
  cockpitctl-conform
  cockpitctl-domain-buildfix
  cockpitctl-domain-signing
  cockpitctl-domain-trend
  cockpitctl-feature-grid
  cockpitctl-io-schema
  # Tier 3: depends on tier 2
  cockpitctl-domain
  cockpitctl-io-buildfix
  cockpitctl-io-hooks
  cockpitctl-io-policy-signing
  # Tier 4: render & ingest
  cockpitctl-render
  cockpitctl-ingest
  # Tier 5: io
  cockpitctl-io
  # Tier 6: sarif
  cockpitctl-sarif
  # Tier 7: core facade
  cockpitctl-core
  # Tier 8: CLI binary
  cockpitctl
  # Tier 9: conformctl binary
  conformctl
)

echo "╔══════════════════════════════════════════════════════════════╗"
echo "║              Release Dry-Run (${#CRATES[@]} crates)                    ║"
echo "╚══════════════════════════════════════════════════════════════╝"
echo ""

# ── Step 1: Package all crates ──
echo "=== Step 1: Packaging all crates (publish order) ==="
for crate in "${CRATES[@]}"; do
  echo -n "  Packaging $crate ... "
  if cargo package -p "$crate" --allow-dirty --no-verify 2>&1 | tail -1; then
    echo "OK"
  else
    echo "FAIL"
    FAIL=1
  fi
done
echo ""

# ── Step 2: Verify no oversized packages ──
echo "=== Step 2: Checking package sizes ==="
printf "  %-40s %10s\n" "Crate" "Size"
printf "  %-40s %10s\n" "────────────────────────────────────────" "──────────"
for crate in "${CRATES[@]}"; do
  # Find the .crate file in target/package/
  crate_file=$(find target/package -name "${crate}-*.crate" -type f 2>/dev/null | head -1)
  if [ -z "$crate_file" ]; then
    printf "  %-40s %10s\n" "$crate" "NOT FOUND"
    FAIL=1
    continue
  fi
  size=$(stat -c%s "$crate_file" 2>/dev/null || stat -f%z "$crate_file" 2>/dev/null)
  human_size=$(numfmt --to=iec-i --suffix=B "$size" 2>/dev/null || echo "${size} bytes")
  printf "  %-40s %10s\n" "$crate" "$human_size"
  if [ "$size" -gt "$MAX_SIZE" ]; then
    echo "    FAIL: exceeds ${MAX_SIZE} byte limit!"
    FAIL=1
  fi
done
echo ""

# ── Step 3: Verify embedded schemas ──
echo "=== Step 3: Verifying embedded schemas in cockpitctl-types ==="
SCHEMAS=(
  sensor.report.v1.json
  cockpit.report.v1.json
  buildfix.plan.v1.json
  cockpit.promote.v1.json
)
FILES=$(cargo package --list -p cockpitctl-types --allow-dirty 2>&1)
for schema in "${SCHEMAS[@]}"; do
  if echo "$FILES" | grep -q "$schema"; then
    echo "  OK: $schema included"
  else
    echo "  FAIL: $schema missing from package"
    FAIL=1
  fi
done
echo ""

# ── Step 4: Content hygiene ──
echo "=== Step 4: Content hygiene (no junk files) ==="
JUNK_PATTERN='(fixtures/|docs/|\.snap$|\.snap\.new$|target/|\.github/)'
for crate in "${CRATES[@]}"; do
  output=$(cargo package --list -p "$crate" --allow-dirty 2>&1)
  if echo "$output" | grep -qE "$JUNK_PATTERN"; then
    echo "  FAIL: $crate ships junk:"
    echo "$output" | grep -E "$JUNK_PATTERN" | sed 's/^/    /'
    FAIL=1
  else
    echo "  OK: $crate"
  fi
done
echo ""

# ── Summary ──
if [ "$FAIL" -ne 0 ]; then
  echo "══════════════════════════════════════════════════════════════"
  echo "FAILED: release dry-run found issues — fix before tagging"
  echo "══════════════════════════════════════════════════════════════"
  exit 1
fi

echo "══════════════════════════════════════════════════════════════"
echo "PASSED: all ${#CRATES[@]} crates ready for release"
echo "══════════════════════════════════════════════════════════════"
