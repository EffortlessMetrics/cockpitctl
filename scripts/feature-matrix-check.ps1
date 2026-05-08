<#
.SYNOPSIS
  Compatibility wrapper for the Rust xtask feature matrix.
#>
[CmdletBinding()]
param(
    [switch]$Quick
)
$ErrorActionPreference = 'Stop'
$args = @('run', '-p', 'xtask', '--', 'feature-matrix')
if ($Quick) { $args += '--quick' }
cargo @args
exit $LASTEXITCODE
