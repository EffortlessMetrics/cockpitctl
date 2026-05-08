<# Compatibility wrapper: implementation lives in Rust xtask. #>
[CmdletBinding()]
param(
    [switch]$Quick,
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$RemainingArgs
)
$ErrorActionPreference = 'Stop'
$argsForXtask = @()
if ($Quick) { $argsForXtask += '--quick' }
$argsForXtask += $RemainingArgs
& cargo run -p xtask -- feature-matrix-check @argsForXtask
exit $LASTEXITCODE
