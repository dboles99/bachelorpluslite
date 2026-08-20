<#
.SYNOPSIS
    Install BachelorPad+'s git hooks into this clone.

.DESCRIPTION
    Copies scripts/hooks/* into .git/hooks/. Hooks cannot be tracked by git
    directly, so the tracked copies under scripts/hooks/ are the source of
    truth and this script installs them.

    A note on this machine's setup: `core.hooksPath` is set globally (to
    enforce commit attribution), which normally makes git ignore every
    repo-local .git/hooks script. The global hooks directory handles that with
    pass-through stubs that exec the repo's own hook of the same name if it is
    executable -- so installing here works, and the global commit-msg hook
    keeps working too. This script deliberately does *not* set a repo-local
    core.hooksPath, which would override and disable that global hook.

.PARAMETER Force
    Overwrite existing hooks. Without it, hooks that already exist and differ
    are left alone and reported.

.EXAMPLE
    ./scripts/Install-GitHooks.ps1
#>
[CmdletBinding()]
param(
    [switch]$Force
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$Root = Split-Path -Parent $PSScriptRoot
$Source = Join-Path $Root 'scripts/hooks'

$gitDir = & git -C $Root rev-parse --git-dir 2>$null
if ($LASTEXITCODE -ne 0) { throw "not a git repository: $Root" }
if (-not [System.IO.Path]::IsPathRooted($gitDir)) {
    $gitDir = Join-Path $Root $gitDir
}
$Target = Join-Path $gitDir 'hooks'

New-Item -ItemType Directory -Force -Path $Target | Out-Null

$installed = 0
$skipped = 0

foreach ($hook in Get-ChildItem -Path $Source -File) {
    $dest = Join-Path $Target $hook.Name

    if ((Test-Path $dest) -and -not $Force) {
        $same = (Get-FileHash $dest).Hash -eq (Get-FileHash $hook.FullName).Hash
        if ($same) {
            Write-Host "  = $($hook.Name) (already current)" -ForegroundColor DarkGray
            $installed++
            continue
        }
        Write-Host "  ! $($hook.Name) exists and differs -- re-run with -Force to replace" -ForegroundColor Yellow
        $skipped++
        continue
    }

    Copy-Item -Path $hook.FullName -Destination $dest -Force
    Write-Host "  + $($hook.Name)" -ForegroundColor Green
    $installed++
}

Write-Host ''
Write-Host "Installed $installed hook(s) into $Target"
if ($skipped -gt 0) {
    Write-Host "$skipped hook(s) skipped." -ForegroundColor Yellow
}

# The global pass-through stubs test `-x` before exec'ing the repo hook, so a
# hook without the executable bit fails silently -- exactly the failure mode
# that is hardest to notice. Verify rather than assume.
# Git's *bundled* bash, not whatever `bash` is on PATH. On a machine with WSL
# installed, `bash` resolves to C:\Windows\System32\bash.exe, which cannot see
# a `G:/...` path at all and reports every hook as non-executable. Git runs
# hooks with its own bash, so that is the interpreter whose opinion counts.
$bash = $null
$gitExe = (Get-Command git -ErrorAction SilentlyContinue).Source
if ($gitExe) {
    $candidate = Join-Path (Split-Path (Split-Path $gitExe)) 'bin/bash.exe'
    if (Test-Path $candidate) { $bash = $candidate }
}

if ($bash) {
    $allExec = $true
    foreach ($hook in Get-ChildItem -Path $Target -File | Where-Object { $_.Name -notlike '*.sample' }) {
        # Forward slashes, not the native path: inside `bash -c` a backslash
        # is an escape character, so a Windows path silently becomes an
        # unusable string and every hook looks non-executable. A verification
        # that always fails is worse than none -- it teaches you to ignore it.
        $posix = $hook.FullName -replace '\\', '/'
        $isExec = & $bash -c "test -x '$posix' && echo yes || echo no"
        if ($isExec -ne 'yes') {
            $allExec = $false
            Write-Host "WARNING: $($hook.Name) is not executable; it will be silently skipped." -ForegroundColor Red
            Write-Host "         Fix with: bash -c `"chmod +x '$posix'`"" -ForegroundColor Red
        }
    }
    if ($allExec) {
        Write-Host 'Verified: all hooks are executable.' -ForegroundColor DarkGray
    }
}
else {
    Write-Host "Could not verify the executable bit (Git's bundled bash not found)." -ForegroundColor DarkGray
}

Write-Host ''
Write-Host "pre-commit runs: Invoke-LocalCI.ps1 -Quick"
Write-Host "pre-push runs:   Invoke-LocalCI.ps1"
Write-Host "Bypass either with: BPAD_SKIP_CI=1 git ..."
