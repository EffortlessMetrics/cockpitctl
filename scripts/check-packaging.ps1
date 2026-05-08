<#
.SYNOPSIS
  Compatibility wrapper for the Rust xtask packaging check.
#>
[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
cargo run -p xtask -- check-packaging
exit $LASTEXITCODE
