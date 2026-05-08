<#
.SYNOPSIS
  Compatibility wrapper for the Rust xtask release dry-run.
#>
[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
cargo run -p xtask -- release-dry-run
exit $LASTEXITCODE
