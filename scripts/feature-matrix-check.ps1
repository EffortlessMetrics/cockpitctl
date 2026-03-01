#!/usr/bin/env pwsh
# Feature compilation matrix: verify every feature combination builds.
#
# Usage:
#   pwsh scripts/feature-matrix-check.ps1          # check build for all combos
#   pwsh scripts/feature-matrix-check.ps1 -Quick   # only single-feature isolation
#
# Exit 0 on success, 1 on first failure.

param(
    [switch]$Quick
)

$ErrorActionPreference = 'Stop'
$features = @('feature-hooks', 'feature-buildfix', 'feature-policy-signing', 'feature-schema')
$failed = 0

function Test-Build {
    param([string]$Label, [string[]]$CargoArgs)

    Write-Host "  [$Label] cargo build $($CargoArgs -join ' ')" -ForegroundColor Cyan
    & cargo build -p cockpitctl @CargoArgs 2>&1 | Out-Null
    if ($LASTEXITCODE -ne 0) {
        Write-Host "  FAIL: $Label" -ForegroundColor Red
        $script:failed++
        return $false
    }
    Write-Host "  OK: $Label" -ForegroundColor Green
    return $true
}

Write-Host "`n=== Feature compilation matrix ===" -ForegroundColor Yellow

# 1) No features
Test-Build 'no-features' @('--no-default-features')

# 2) Each feature in isolation
foreach ($f in $features) {
    Test-Build "only-$f" @('--no-default-features', '--features', $f)
}

# 3) All features (default)
Test-Build 'all-defaults' @()

if (-not $Quick) {
    # 4) Pairwise combinations (6 pairs)
    for ($i = 0; $i -lt $features.Count; $i++) {
        for ($j = $i + 1; $j -lt $features.Count; $j++) {
            $pair = "$($features[$i]),$($features[$j])"
            Test-Build "pair-$pair" @('--no-default-features', '--features', $pair)
        }
    }

    # 5) Three-feature combinations (4 triples)
    for ($i = 0; $i -lt $features.Count; $i++) {
        $triple = ($features | Where-Object { $_ -ne $features[$i] }) -join ','
        Test-Build "triple-without-$($features[$i])" @('--no-default-features', '--features', $triple)
    }
}

Write-Host "`n=== Results ===" -ForegroundColor Yellow
if ($failed -gt 0) {
    Write-Host "$failed combination(s) failed." -ForegroundColor Red
    exit 1
} else {
    $total = if ($Quick) { $features.Count + 2 } else { $features.Count + 2 + 6 + 4 }
    Write-Host "All $total combinations passed." -ForegroundColor Green
    exit 0
}
