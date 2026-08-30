<#
.SYNOPSIS
    Has the full gate already run over exactly this content?

.DESCRIPTION
    The pre-push hook used to re-run the whole Windows gate on every push. For
    a session that follows the documented rule -- run
    `Invoke-LocalCI.ps1 -Linux` before calling an item done -- that was the
    *third* execution of the same work for one change: the explicit full run,
    then pre-commit, then pre-push. Roughly seven minutes of it, per commit.

    The fix is not to make pre-push weaker. It is to let pre-push *check*
    rather than *repeat*: `Invoke-LocalCI.ps1` records a JSON run record after
    a successful full run, and this script decides whether one of those
    records still describes the tree in front of it.

    **What makes a record valid, and why each condition is there:**

    - `succeeded` and `mode = full` -- a quick or failed run is evidence of
      nothing a push could rely on;
    - `linux = true` -- ADR-0001 makes Linux a first-class target, and a
      record whose Linux leg skipped (no distro, no toolchain) has checked
      half of what this project promises;
    - `source_at` is not older than the newest write time of any tracked
      file -- **this is the whole mechanism.** It dates the *content* rather
      than the commit, because the gate runs on a working tree and the commit
      that tree becomes does not exist yet. Edit anything afterwards and the
      record is stale by construction.

    **It fails closed.** No record, an unreadable one, a stale one, or a git
    command that will not answer -- every one of those returns "no", and the
    hook runs the full gate. A verification that fails open is a verification
    that quietly disables the thing it was meant to protect.

    This does *not* prove the pushed commit's tree byte-for-byte matches what
    was gated; it proves nothing tracked has been written since. That is a
    weaker claim, and it is stated here rather than left to be assumed.

.PARAMETER Quiet
    Print nothing; the exit code is the answer.

.OUTPUTS
    Exit code 0 when a valid record exists, 1 when the full gate must run.
#>
[CmdletBinding()]
param(
    [switch]$Quiet
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$Root = Split-Path -Parent $PSScriptRoot

function Write-Reason {
    param([string]$Message)
    if (-not $Quiet) { Write-Host "  $Message" -ForegroundColor DarkGray }
}

$dir = Join-Path $Root 'artifacts/test-evidence'
if (-not (Test-Path $dir)) {
    Write-Reason 'no evidence directory'
    exit 1
}

# The newest tracked write, computed the same way Invoke-LocalCI records it.
# Duplicated deliberately rather than shared: this script must be runnable on
# its own, and a helper hoisted into a third file for two callers is a file
# nobody remembers exists.
$files = & git -C $Root ls-files 2>$null
if ($LASTEXITCODE -ne 0 -or -not $files) {
    Write-Reason 'git will not list tracked files'
    exit 1
}

$newest = [datetime]::MinValue
foreach ($relative in $files) {
    # A guard, not a fix, for the reason Invoke-LocalCI's copy gives at
    # length: these records are ignored today, so this never fires -- but
    # un-ignoring them would make every record stale against itself, and
    # would do it silently.
    if ($relative -like 'artifacts/test-evidence/*') { continue }

    $item = Get-Item -LiteralPath (Join-Path $Root $relative) -ErrorAction SilentlyContinue
    if ($item -and $item.LastWriteTime -gt $newest) { $newest = $item.LastWriteTime }
}

$records = Get-ChildItem -Path $dir -Filter 'local-ci-*.json' -File -ErrorAction SilentlyContinue |
    Sort-Object LastWriteTime -Descending

foreach ($record in $records) {
    try {
        $run = Get-Content -Raw -LiteralPath $record.FullName | ConvertFrom-Json
    }
    catch {
        continue
    }

    # `source_at` is absent from every record written before this mechanism
    # existed, and an old record cannot be dated against a tree it never saw.
    $hasFields = $run.PSObject.Properties.Name -contains 'source_at' -and
                 $run.PSObject.Properties.Name -contains 'linux'
    if (-not $hasFields) { continue }

    if (-not $run.succeeded) { continue }
    if ($run.mode -ne 'full') { continue }
    if (-not $run.linux) { continue }

    if ([datetime]$run.source_at -lt $newest) {
        # The newest valid-looking record is stale, and so is every older one.
        Write-Reason ("tracked files changed since the last full gate ({0})" -f $record.Name)
        exit 1
    }

    Write-Reason ("full gate already passed over this content ({0})" -f $record.Name)
    exit 0
}

Write-Reason 'no passing full-gate record with a Linux leg'
exit 1
