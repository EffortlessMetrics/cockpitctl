<#
.SYNOPSIS
  Verify publishable crates ship no junk and have required metadata.
#>
[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
$fail = $false

$crates = @(
  'cockpitctl'
  'cockpitctl-types'
  'cockpitctl-domain'
  'cockpitctl-domain-buildfix'
  'cockpitctl-domain-signing'
  'cockpitctl-domain-trend'
  'cockpitctl-feature-grid'
  'cockpitctl-feature-state'
  'cockpitctl-ingest'
  'cockpitctl-io'
  'cockpitctl-io-buildfix'
  'cockpitctl-io-hooks'
  'cockpitctl-io-policy-signing'
  'cockpitctl-io-schema'
  'cockpitctl-render'
  'cockpitctl-sarif'
  'cockpitctl-conform'
  'cockpitctl-core'
  'conformctl'
)

Write-Host "=== Checking cargo package --list for junk files ==="
foreach ($crate in $crates) {
    $prevEAP = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    $output = & cargo package --list -p $crate 2>&1 | Out-String
    $ErrorActionPreference = $prevEAP
    if ($LASTEXITCODE -ne 0) {
        Write-Host "FAIL: cargo package --list -p $crate failed"
        $fail = $true
        continue
    }
    $junk = $output -split "`n" | Where-Object {
        $_ -match '(fixtures/|docs/|\.snap$|\.snap\.new$)'
    }
    if ($junk) {
        Write-Host "FAIL: $crate ships junk files:"
        $junk | ForEach-Object { Write-Host "  $_" }
        $fail = $true
    } else {
        Write-Host "  OK: $crate"
    }
}

Write-Host ""
Write-Host "=== Checking crate metadata ==="
$metaJson = cargo metadata --format-version 1 --no-deps 2>$null | ConvertFrom-Json
foreach ($crate in $crates) {
    $pkg = $metaJson.packages | Where-Object { $_.name -eq $crate }
    if (-not $pkg) {
        Write-Host "FAIL: $crate not found in workspace"
        $fail = $true
        continue
    }
    $missing = @()
    foreach ($field in @('name', 'version', 'description', 'license', 'repository')) {
        if (-not $pkg.$field) { $missing += $field }
    }
    if ($missing.Count -gt 0) {
        Write-Host "FAIL: $crate metadata missing: $($missing -join ', ')"
        $fail = $true
    } else {
        Write-Host "  OK: $crate metadata"
    }
}

if ($fail) {
    Write-Host ""
    Write-Host "FAILED: packaging hygiene checks found issues"
    exit 1
}

Write-Host ""
Write-Host "All crates clean"
