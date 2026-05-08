<# Compatibility wrapper: implementation lives in Rust xtask. #>
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true, Position = 0)]
    [string]$Tag,
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$RemainingArgs
)
$ErrorActionPreference = 'Stop'
& cargo run -p xtask -- smoke-test-release $Tag @RemainingArgs
exit $LASTEXITCODE
