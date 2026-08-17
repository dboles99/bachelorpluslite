<#
.SYNOPSIS
    BachelorPad+ local CI gate.

.DESCRIPTION
    This repository has no CI service and no git remote. Local CI is not a
    convenience wrapper around the real gate -- it *is* the gate, so it has to
    be as strict as a hosted pipeline would be and it has to be run.
    `Install-GitHooks.ps1` wires it into pre-commit and pre-push so that
    happens without anyone remembering to.

    Every stage runs even after one fails, so a single run reports every
    problem rather than making you fix them one at a time. The exit code is
    non-zero if any stage failed.

.PARAMETER Quick
    Skip the stages that need a clean dependency resolve. Used by the
    pre-commit hook, where the full gate would be too slow to tolerate.

.PARAMETER Linux
    Also run the gate inside WSL. BP-ADR-0001 makes Linux a first-class
    target, and nothing else in this setup would catch a Windows-only
    regression. Requires a Rust toolchain inside the distro.

.PARAMETER IncludeSpikes
    Also check the standalone spike workspaces under spikes/. They are
    excluded from the main workspace on purpose (they carry heavy GUI
    dependencies), so the normal gate does not see them.

.PARAMETER Distro
    WSL distribution to use for -Linux.

.PARAMETER RecordEvidence
    Write a JSON run record under artifacts/test-evidence/, per the
    Definition of Done's provenance requirement.

.EXAMPLE
    ./scripts/Invoke-LocalCI.ps1
    Run the full gate on Windows.

.EXAMPLE
    ./scripts/Invoke-LocalCI.ps1 -Linux -IncludeSpikes
    Everything, both platforms. What to run before calling a phase done.
#>
[CmdletBinding()]
param(
    [switch]$Quick,
    [switch]$Linux,
    [switch]$IncludeSpikes,
    [string]$Distro = 'Ubuntu-24.04',
    [switch]$RecordEvidence
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$Root = Split-Path -Parent $PSScriptRoot
$script:Results = [System.Collections.Generic.List[object]]::new()
$script:Failed = $false

function Add-Result {
    param([string]$Stage, [string]$Status, [double]$Seconds, [string]$Detail = '')
    $script:Results.Add([pscustomobject]@{
            Stage   = $Stage
            Status  = $Status
            Seconds = [math]::Round($Seconds, 1)
            Detail  = $Detail
        })
}

function Invoke-Stage {
    <#
      Runs one gate stage. Native tools signal failure through $LASTEXITCODE,
      which PowerShell does not turn into a terminating error on its own, so
      it is checked explicitly.
    #>
    param(
        [Parameter(Mandatory)][string]$Name,
        [Parameter(Mandatory)][scriptblock]$Action
    )

    Write-Host "RUN   $Name" -ForegroundColor Cyan
    $sw = [Diagnostics.Stopwatch]::StartNew()
    try {
        & $Action
        if ($LASTEXITCODE -ne 0) { throw "exited with code $LASTEXITCODE" }
        $sw.Stop()
        Add-Result -Stage $Name -Status 'pass' -Seconds $sw.Elapsed.TotalSeconds
        Write-Host ("PASS  {0}  ({1:N1}s)" -f $Name, $sw.Elapsed.TotalSeconds) -ForegroundColor Green
    }
    catch {
        $sw.Stop()
        $script:Failed = $true
        Add-Result -Stage $Name -Status 'FAIL' -Seconds $sw.Elapsed.TotalSeconds -Detail $_.Exception.Message
        Write-Host ("FAIL  {0}  ({1:N1}s): {2}" -f $Name, $sw.Elapsed.TotalSeconds, $_.Exception.Message) -ForegroundColor Red
    }
}

function Skip-Stage {
    param([string]$Name, [string]$Reason)
    Add-Result -Stage $Name -Status 'skipped' -Seconds 0 -Detail $Reason
    Write-Host "SKIP  $Name -- $Reason" -ForegroundColor DarkGray
}

Push-Location $Root
try {
    Write-Host "BachelorPad+ local CI" -ForegroundColor White
    Write-Host ("  root:     {0}" -f $Root)
    Write-Host ("  rustc:    {0}" -f (rustc --version))
    Write-Host ("  cargo:    {0}" -f (cargo --version))
    Write-Host ("  mode:     {0}" -f $(if ($Quick) { 'quick (pre-commit)' } else { 'full' }))
    Write-Host ''

    # --- workspace -----------------------------------------------------
    Invoke-Stage 'fmt' { cargo fmt --all -- --check }

    if ($Quick) {
        # --locked forces a dependency re-resolve, which is the slowest part
        # of a cold run and cannot regress from an edit that does not touch a
        # manifest. pre-push still checks it.
        Skip-Stage 'check (locked)' 'quick mode'
    }
    else {
        Invoke-Stage 'check (locked)' { cargo check --workspace --all-targets --locked }
    }

    Invoke-Stage 'clippy' { cargo clippy --workspace --all-targets -- -D warnings }
    Invoke-Stage 'test' { cargo test --workspace }

    # Actually load the linked binary. `cargo build` and `cargo test` never do
    # -- a bad link-time feature or side-by-side manifest passes both and then
    # fails before `main` with STATUS_ENTRYPOINT_NOT_FOUND, which is precisely
    # what rfd's `common-controls-v6` feature did here.
    Invoke-Stage 'launch' {
        # Build first, unconditionally. A stale binary predating --self-check
        # does not recognise the flag, falls through to opening the GUI, and
        # then never exits -- the stage hangs instead of failing.
        cargo build --workspace --quiet
        if ($LASTEXITCODE -ne 0) { throw "build failed" }

        $exe = Join-Path $Root 'target/debug/bachelorpad.exe'
        $out = Join-Path ([System.IO.Path]::GetTempPath()) "bpad-selfcheck-$PID.txt"

        $proc = Start-Process -FilePath $exe -ArgumentList '--self-check' `
            -PassThru -NoNewWindow -RedirectStandardOutput $out

        # Bounded, and it has to stay bounded: the hang this guards against is
        # the one described above, where the binary opens a window instead of
        # answering and never exits. Running it synchronously would turn that
        # failure into a gate that never returns, which is worse -- a red run
        # tells you something; a hung one tells you nothing and blocks the
        # commit anyway.
        if (-not $proc.WaitForExit(30000)) {
            $proc.Kill()
            $proc.WaitForExit()
            Remove-Item $out -ErrorAction SilentlyContinue
            throw 'did not exit within 30s'
        }
        # The no-argument overload as well, once the bounded one has confirmed
        # the process is gone. It additionally waits for the redirected output
        # to be flushed and for ExitCode to settle -- without it,
        # -RedirectStandardOutput can leave ExitCode unreadable on a process
        # that exited perfectly cleanly, which is what made this stage report
        # "exit 0x" against a working binary.
        $proc.WaitForExit()

        # The marker is the real assertion: only a binary that loaded and
        # reached `main` can have printed it. The exit code corroborates that
        # where the platform gives us one, so it is checked when known rather
        # than being the thing the stage stands on.
        $ok = (Test-Path $out) -and (Select-String -Path $out -Pattern 'BACHELORPAD_SELF_CHECK_OK' -Quiet)
        $code = $proc.ExitCode
        Remove-Item $out -ErrorAction SilentlyContinue

        if (-not $ok) {
            throw "binary did not print the self-check marker (exit code $code)"
        }
        if ($null -ne $code -and $code -ne 0) {
            throw "binary exited with 0x$('{0:X}' -f $code)"
        }
        $global:LASTEXITCODE = 0
    }

    # --- log hygiene ---------------------------------------------------
    # ADR-0011 and the Definition of Done forbid document content, clipboard
    # data, passphrases and key material from reaching logs at any level.
    # That rule is only as good as its enforcement, and a reviewer noticing is
    # not enforcement. Log *about* a document -- its path, its size -- never
    # what it contains.
    Invoke-Stage 'log hygiene' {
        $sources = Get-ChildItem -Path (Join-Path $Root 'crates'), (Join-Path $Root 'apps') `
            -Recurse -Filter '*.rs' -ErrorAction SilentlyContinue
        # Identifiers that carry document text or secrets. A tracing macro
        # mentioning one of these is the thing to look at.
        $carriers = 'doc_text|active_text|text_of|\btexts\b|\bbuffer\b|contents|passphrase|secret|password|\btoken\b|plaintext|clipboard_text'
        # @() forces an array: a single match comes back as a scalar MatchInfo,
        # and .Count on that throws -- which reports a PowerShell error where a
        # leak report belongs.
        $hits = @($sources | Select-String -Pattern "tracing::(trace|debug|info|warn|error)!.*($carriers)")

        if ($hits.Count -gt 0) {
            foreach ($hit in $hits) {
                Write-Host ("  {0}:{1}: {2}" -f $hit.Filename, $hit.LineNumber, $hit.Line.Trim()) -ForegroundColor Red
            }
            throw "$($hits.Count) log statement(s) may carry document content or secrets"
        }
        $global:LASTEXITCODE = 0
    }

    # --- spikes --------------------------------------------------------
    $spikes = Get-ChildItem -Path (Join-Path $Root 'spikes') -Directory -ErrorAction SilentlyContinue |
        ForEach-Object { Get-ChildItem -Path $_.FullName -Directory } |
        Where-Object { Test-Path (Join-Path $_.FullName 'Cargo.toml') }

    if (-not $IncludeSpikes) {
        Skip-Stage 'spikes' 'pass -IncludeSpikes to check them'
    }
    elseif (-not $spikes) {
        Skip-Stage 'spikes' 'none found'
    }
    else {
        foreach ($spike in $spikes) {
            $path = $spike.FullName
            Invoke-Stage ("spike fmt: {0}" -f $spike.Name) {
                Push-Location $path
                try { cargo fmt --all -- --check } finally { Pop-Location }
            }
            Invoke-Stage ("spike clippy: {0}" -f $spike.Name) {
                Push-Location $path
                try { cargo clippy --all-targets -- -D warnings } finally { Pop-Location }
            }
        }
    }

    # --- linux ---------------------------------------------------------
    if (-not $Linux) {
        Skip-Stage 'linux (wsl)' 'pass -Linux to run the Linux leg'
    }
    else {
        # Translate the path in PowerShell rather than shelling out to
        # `wslpath`: wsl.exe emits UTF-16, which arrives here as mojibake or
        # an empty string, and an empty string reads as "distro unavailable"
        # when the distro is fine. Assumes the default /mnt automount root.
        $drive = $Root.Substring(0, 1).ToLowerInvariant()
        $wslRoot = "/mnt/$drive" + ($Root.Substring(2) -replace '\\', '/')

        # Put the *Linux* toolchain on PATH explicitly. WSL inherits the
        # Windows PATH through interop, which already contains the Windows
        # ~/.cargo/bin, while a rustup install into the distro lands in
        # $HOME/.cargo/bin and does not reach a non-interactive shell. Without
        # this the leg either fails to find cargo or finds the wrong one.
        # Backtick-escaped so PowerShell leaves $HOME for bash to expand.
        $prelude = "export PATH=`"`$HOME/.cargo/bin:`$PATH`"; "

        & wsl -d $Distro -- true 2>$null | Out-Null
        $distroUp = ($LASTEXITCODE -eq 0)

        $hasCargo = $false
        if ($distroUp) {
            & wsl -d $Distro -- bash -c "$prelude command -v cargo" 2>$null | Out-Null
            $hasCargo = ($LASTEXITCODE -eq 0)
        }

        if (-not $distroUp) {
            Skip-Stage 'linux (wsl)' "distro '$Distro' unavailable"
        }
        elseif (-not $hasCargo) {
            # Report the real reason. A skip that blames the wrong thing sends
            # you looking for a WSL problem that is not there.
            Skip-Stage 'linux (wsl)' "no Rust toolchain in '$Distro' (see docs/governance/LOCAL_CI.md)"
        }
        else {
            $cmd = $prelude +
                   "export CARGO_TARGET_DIR=`$HOME/.cache/bachelorpadplus-target; " +
                   "cd '$wslRoot' && cargo fmt --all -- --check && " +
                   "cargo clippy --workspace --all-targets -- -D warnings && " +
                   "cargo test --workspace"
            Invoke-Stage "linux (wsl: $Distro)" { wsl -d $Distro -- bash -c $cmd }
        }
    }

    # --- summary -------------------------------------------------------
    Write-Host ''
    $script:Results | Format-Table -AutoSize | Out-String | Write-Host

    if ($RecordEvidence) {
        $dir = Join-Path $Root 'artifacts/test-evidence'
        New-Item -ItemType Directory -Force -Path $dir | Out-Null
        $stamp = (Get-Date).ToString('yyyyMMdd-HHmmss')
        $file = Join-Path $dir "local-ci-$stamp.json"
        [pscustomobject]@{
            run_at    = (Get-Date).ToString('o')
            host_os   = [System.Environment]::OSVersion.VersionString
            rustc     = (rustc --version)
            mode      = $(if ($Quick) { 'quick' } else { 'full' })
            succeeded = (-not $script:Failed)
            stages    = $script:Results
        } | ConvertTo-Json -Depth 5 | Set-Content -Encoding utf8 $file
        Write-Host "evidence: $file" -ForegroundColor DarkGray
    }

    if ($script:Failed) {
        Write-Host 'LOCAL CI FAILED' -ForegroundColor Red
        exit 1
    }
    Write-Host 'LOCAL CI PASSED' -ForegroundColor Green
    exit 0
}
finally {
    Pop-Location
}
