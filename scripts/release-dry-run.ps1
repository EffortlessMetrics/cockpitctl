<#
.SYNOPSIS
  Simulate a full crates.io publish in tier order.

.DESCRIPTION
  Packages all 19 publishable crates in the same order as the release
  workflow, verifies no oversized packages, checks embedded schemas,
  and reports package sizes.
#>
[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'

# Max package size in bytes (10 MB — crates.io hard limit)
$MaxSize = 10 * 1024 * 1024
$fail = $false

# Publish order (matches release.yml tiers 1-9)
$crates = @(
  # Tier 1: leaf crates
  'cockpitctl-types'
  'cockpitctl-feature-state'
  # Tier 2: depends on types / feature-state
  'cockpitctl-conform'
  'cockpitctl-domain-buildfix'
  'cockpitctl-domain-signing'
  'cockpitctl-domain-trend'
  'cockpitctl-feature-grid'
  'cockpitctl-io-schema'
  # Tier 3: depends on tier 2
  'cockpitctl-domain'
  'cockpitctl-io-buildfix'
  'cockpitctl-io-hooks'
  'cockpitctl-io-policy-signing'
  # Tier 4: render & ingest
  'cockpitctl-render'
  'cockpitctl-ingest'
  # Tier 5: io
  'cockpitctl-io'
  # Tier 6: sarif
  'cockpitctl-sarif'
  # Tier 7: core facade
  'cockpitctl-core'
  # Tier 8: CLI binary
  'cockpitctl'
  # Tier 9: conformctl binary
  'conformctl'
)

$count = $crates.Count
Write-Host "================================================================"
Write-Host "              Release Dry-Run ($count crates)"
Write-Host "================================================================"
Write-Host ""

# -- Step 1: Package all crates --
Write-Host "=== Step 1: Packaging all crates (publish order) ==="
foreach ($crate in $crates) {
    Write-Host -NoNewline "  Packaging $crate ... "
    $prevEAP = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    $null = cargo package -p $crate --allow-dirty --no-verify 2>&1
    $ErrorActionPreference = $prevEAP
    if ($LASTEXITCODE -ne 0) {
        Write-Host "FAIL"
        $fail = $true
    } else {
        Write-Host "OK"
    }
}
Write-Host ""

# -- Step 2: Verify no oversized packages --
Write-Host "=== Step 2: Checking package sizes ==="
Write-Host ("  {0,-40} {1,10}" -f "Crate", "Size")
Write-Host ("  {0,-40} {1,10}" -f ("-" * 40), ("-" * 10))
foreach ($crate in $crates) {
    $crateFiles = Get-ChildItem -Path "target\package" -Filter "$crate-*.crate" -ErrorAction SilentlyContinue
    if (-not $crateFiles) {
        Write-Host ("  {0,-40} {1,10}" -f $crate, "NOT FOUND")
        $fail = $true
        continue
    }
    $file = $crateFiles | Select-Object -First 1
    $size = $file.Length
    if ($size -ge 1MB) {
        $humanSize = "{0:N1} MiB" -f ($size / 1MB)
    } elseif ($size -ge 1KB) {
        $humanSize = "{0:N1} KiB" -f ($size / 1KB)
    } else {
        $humanSize = "$size B"
    }
    Write-Host ("  {0,-40} {1,10}" -f $crate, $humanSize)
    if ($size -gt $MaxSize) {
        Write-Host "    FAIL: exceeds $MaxSize byte limit!"
        $fail = $true
    }
}
Write-Host ""

# -- Step 3: Verify embedded schemas --
Write-Host "=== Step 3: Verifying embedded schemas in cockpitctl-types ==="
$schemas = @(
    'sensor.report.v1.json'
    'cockpit.report.v1.json'
    'buildfix.plan.v1.json'
    'cockpit.promote.v1.json'
)
$prevEAP = $ErrorActionPreference
$ErrorActionPreference = 'Continue'
$files = cargo package --list -p cockpitctl-types --allow-dirty 2>&1 | Out-String
$ErrorActionPreference = $prevEAP
foreach ($schema in $schemas) {
    if ($files -match [regex]::Escape($schema)) {
        Write-Host "  OK: $schema included"
    } else {
        Write-Host "  FAIL: $schema missing from package"
        $fail = $true
    }
}
Write-Host ""

# -- Step 4: Content hygiene --
Write-Host "=== Step 4: Content hygiene (no junk files) ==="
foreach ($crate in $crates) {
    $prevEAP = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    $output = cargo package --list -p $crate --allow-dirty 2>&1 | Out-String
    $ErrorActionPreference = $prevEAP
    $junk = $output -split "`n" | Where-Object {
        $_ -match '(fixtures/|docs/|\.snap$|\.snap\.new$|target/|\.github/)'
    }
    if ($junk) {
        Write-Host "  FAIL: $crate ships junk:"
        $junk | ForEach-Object { Write-Host "    $_" }
        $fail = $true
    } else {
        Write-Host "  OK: $crate"
    }
}
Write-Host ""

# -- Summary --
if ($fail) {
    Write-Host "================================================================"
    Write-Host "FAILED: release dry-run found issues - fix before tagging"
    Write-Host "================================================================"
    exit 1
}

Write-Host "================================================================"
Write-Host "PASSED: all $count crates ready for release"
Write-Host "================================================================"
