<# Compatibility wrapper: implementation lives in Rust xtask. #>
[CmdletBinding()]
param(
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$RemainingArgs
)
$ErrorActionPreference = 'Stop'
& cargo run -p xtask -- check-packaging @RemainingArgs
exit $LASTEXITCODE
