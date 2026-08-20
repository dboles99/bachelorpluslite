<#
.SYNOPSIS
    Deprecated. Use Invoke-LocalCI.ps1.

.DESCRIPTION
    This was the original repository smoke test. It has been superseded by
    scripts/Invoke-LocalCI.ps1, which runs a strictly larger set of checks
    (it adds clippy, the test suite, a locked dependency resolve, and opt-in
    Linux and spike passes) and is what the git hooks invoke.

    Kept as a shim so existing muscle memory and any external references keep
    working. It forwards every argument through.
#>
[CmdletBinding()]
param(
    [Parameter(ValueFromRemainingArguments = $true)]
    $Forwarded
)

$ErrorActionPreference = 'Stop'
$target = Join-Path $PSScriptRoot 'Invoke-LocalCI.ps1'

Write-Host 'Test-Repo.ps1 is deprecated; forwarding to Invoke-LocalCI.ps1' -ForegroundColor Yellow

if ($Forwarded) {
    & $target @Forwarded
}
else {
    & $target
}
exit $LASTEXITCODE
