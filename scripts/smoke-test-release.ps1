# Smoke test script for cockpitctl release validation (PowerShell)
# This script validates a release using only published artifacts (no vendoring required)
#
# Usage: .\scripts\smoke-test-release.ps1 -Tag v0.2.0
# Example: .\scripts\smoke-test-release.ps1 -Tag v0.2.0

param(
    [Parameter(Mandatory=$true)]
    [string]$Tag
)

# Ensure tag starts with 'v'
if (-not $Tag.StartsWith('v')) {
    $Tag = "v$Tag"
}

Write-Host "Starting smoke test for cockpitctl release: $Tag" -ForegroundColor Green

# Detect platform
$Platform = $env:PROCESSOR_ARCHITECTURE
$BinaryExt = ".exe"

Write-Host "Detected platform: $Platform" -ForegroundColor Green

# Create temporary directory
$TempDir = Join-Path $env:TEMP "cockpitctl-smoke-test-$(Get-Random)"
New-Item -ItemType Directory -Path $TempDir -Force | Out-Null

try {
    # Download conformctl
    $ConformctlUrl = "https://github.com/EffortlessMetrics/cockpitctl/releases/download/$Tag/conformctl-windows-x64.exe"
    $ConformctlPath = Join-Path $TempDir "conformctl.exe"
    
    Write-Host "Downloading conformctl from $ConformctlUrl" -ForegroundColor Green
    Invoke-WebRequest -Uri $ConformctlUrl -OutFile $ConformctlPath -UseBasicParsing
    Write-Host "Downloaded to: $ConformctlPath" -ForegroundColor Green
    
    # Test conformctl version
    Write-Host "Testing conformctl..." -ForegroundColor Green
    & $ConformctlPath --version
    if ($LASTEXITCODE -ne 0) {
        throw "conformctl --version failed"
    }
    
    # Create test receipt
    $TestReceiptDir = Join-Path $TempDir "test-receipt"
    New-Item -ItemType Directory -Path $TestReceiptDir -Force | Out-Null
    
    $TestReceiptContent = @"
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
"@

    $TestReceiptPath = Join-Path $TestReceiptDir "report.json"
    $TestReceiptContent | Out-File -FilePath $TestReceiptPath -Encoding utf8

    # Test conformctl check
    & $ConformctlPath check --report $TestReceiptPath --sensor-id test-sensor
    if ($LASTEXITCODE -ne 0) {
        throw "conformctl check failed"
    }
    Write-Host "conformctl tests passed" -ForegroundColor Green
    
    # Download cockpitctl
    $CockpitctlUrl = "https://github.com/EffortlessMetrics/cockpitctl/releases/download/$Tag/cockpitctl-windows-x64.exe"
    $CockpitctlPath = Join-Path $TempDir "cockpitctl.exe"
    
    Write-Host "Downloading cockpitctl from $CockpitctlUrl" -ForegroundColor Green
    Invoke-WebRequest -Uri $CockpitctlUrl -OutFile $CockpitctlPath -UseBasicParsing
    Write-Host "Downloaded to: $CockpitctlPath" -ForegroundColor Green
    
    # Test cockpitctl version
    Write-Host "Testing cockpitctl..." -ForegroundColor Green
    & $CockpitctlPath --version
    if ($LASTEXITCODE -ne 0) {
        throw "cockpitctl --version failed"
    }
    
    # Create test artifacts directory
    $TestArtifactsDir = Join-Path $TempDir "test-artifacts"
    New-Item -ItemType Directory -Path $TestArtifactsDir -Force | Out-Null
    
    # Create config
    $ConfigContent = @"
[sensor.builddiag]
required = true

[sensor.diffguard]
required = true
"@
    
    $ConfigPath = Join-Path $TestArtifactsDir "cockpit.toml"
    $ConfigContent | Out-File -FilePath $ConfigPath -Encoding utf8
    
    # Create sensor artifacts directories
    $BuilddiagDir = Join-Path $TestArtifactsDir "artifacts\builddiag"
    $DiffguardDir = Join-Path $TestArtifactsDir "artifacts\diffguard"
    New-Item -ItemType Directory -Path $BuilddiagDir -Force | Out-Null
    New-Item -ItemType Directory -Path $DiffguardDir -Force | Out-Null
    
    # Create sensor reports
    $BuilddiagReportContent = @"
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
"@

    $BuilddiagReportPath = Join-Path $BuilddiagDir "report.json"
    $BuilddiagReportContent | Out-File -FilePath $BuilddiagReportPath -Encoding utf8

    $DiffguardReportContent = @"
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
"@

    $DiffguardReportPath = Join-Path $DiffguardDir "report.json"
    $DiffguardReportContent | Out-File -FilePath $DiffguardReportPath -Encoding utf8
    
    # Run cockpitctl ingest
    Push-Location $TestArtifactsDir
    & $CockpitctlPath ingest --artifacts artifacts --config cockpit.toml
    $ExitCode = $LASTEXITCODE
    Pop-Location
    
    if ($ExitCode -ne 0) {
        throw "cockpitctl ingest failed with exit code $ExitCode"
    }
    
    # Verify outputs
    $ReportPath = Join-Path $TestArtifactsDir "artifacts\cockpit\report.json"
    $CommentPath = Join-Path $TestArtifactsDir "artifacts\cockpit\comment.md"
    
    if (-not (Test-Path $ReportPath)) {
        throw "cockpitctl did not create report.json"
    }
    
    if (-not (Test-Path $CommentPath)) {
        throw "cockpitctl did not create comment.md"
    }
    
    # Validate report structure
    $ReportContent = Get-Content $ReportPath -Raw | ConvertFrom-Json
    if ($ReportContent.verdict.status -ne "pass") {
        throw "Expected verdict 'pass', got '$($ReportContent.verdict.status)'"
    }
    
    Write-Host "cockpitctl tests passed" -ForegroundColor Green
    
    # Summary
    Write-Host "==========================================" -ForegroundColor Green
    Write-Host "All smoke tests passed!" -ForegroundColor Green
    Write-Host "==========================================" -ForegroundColor Green
    Write-Host "Binaries tested:" -ForegroundColor Green
    Write-Host "  - conformctl: $ConformctlPath" -ForegroundColor Green
    Write-Host "  - cockpitctl: $CockpitctlPath" -ForegroundColor Green
    Write-Host ""
    Write-Host "Release $Tag is ready for announcement." -ForegroundColor Green
    
} finally {
    # Cleanup
    Remove-Item -Path $TempDir -Recurse -Force -ErrorAction SilentlyContinue
}
