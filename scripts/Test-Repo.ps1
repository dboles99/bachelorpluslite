[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$Root = Split-Path -Parent $PSScriptRoot
Push-Location $Root
try {
    Write-Host "BachelorPad+ repository smoke test" -ForegroundColor Cyan
    rustc --version
    cargo --version
    cargo metadata --no-deps --format-version 1 | Out-Null
    cargo fmt --all -- --check
    cargo check --workspace
    Write-Host "PASS" -ForegroundColor Green
}
finally {
    Pop-Location
}
