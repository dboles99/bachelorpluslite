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

    `cargo-nextest` is used for the test stage when it is installed, which is
    about 2.5x faster on this workspace, and the doctests then need a stage of
    their own because nextest cannot run them. Neither leg requires it; both
    fall back to `cargo test --workspace`, which does both halves more slowly.
    Install it with `cargo install cargo-nextest --locked`, on Windows and
    inside WSL separately.

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

    A *successful full* run records one whether or not this is passed, and
    that is load-bearing rather than tidy: Test-GateEvidence.ps1 reads those
    records so the pre-push hook can tell that the full gate has already run
    over exactly this content, and skip re-running it. Evidence nobody writes
    cannot be checked, and a gate that runs three times per commit is the
    thing that record exists to stop.

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

function Get-NewestTrackedWrite {
    <#
    .SYNOPSIS
        The most recent write time of any file git tracks.

    .DESCRIPTION
        Tracked files only. `target/` alone would make every run look newer
        than itself, and an untracked scratch file is not something the gate
        has an opinion about.

        Returns the epoch when git cannot answer, which reads as "infinitely
        stale" and makes every consumer re-run rather than trust a record it
        could not date. Failing open here would mean a broken git invocation
        silently disabling the gate.
    #>
    $files = & git -C $Root ls-files 2>$null
    if ($LASTEXITCODE -ne 0 -or -not $files) { return [datetime]::MinValue }

    $newest = [datetime]::MinValue
    foreach ($relative in $files) {
        # A guard rather than a fix. These records are ignored today
        # (.gitignore line 26 -- the "deliberately NOT ignored" comment above
        # it is about artifacts/benchmarks/, which is a different rule), so
        # `git ls-files` never lists one and this branch never fires.
        #
        # It is here because un-ignoring them would break this mechanism
        # *silently*: a record is written after the timestamp it carries, so a
        # tracked one is always newer than itself, every record would read as
        # stale, and pre-push would re-run the full gate forever while
        # appearing to work. Nothing would fail -- the optimisation would just
        # never fire, which is the failure mode nobody reports.
        if ($relative -like 'artifacts/test-evidence/*') { continue }

        $full = Join-Path $Root $relative
        $item = Get-Item -LiteralPath $full -ErrorAction SilentlyContinue
        if ($item -and $item.LastWriteTime -gt $newest) {
            $newest = $item.LastWriteTime
        }
    }
    return $newest
}

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

    # `cargo nextest` where it exists, `cargo test` where it does not.
    #
    # Measured on this workspace, warm: 111.8s for `cargo test --workspace`
    # against 35.4s + 9.8s for nextest plus a separate doctest pass. A
    # process per test rather than a thread per test, so it is also stricter
    # about shared state -- and it makes the doctest split mandatory rather
    # than optional, which is the trap:
    #
    # **nextest cannot run doctests at all**, and this workspace has one.
    #
    # It had six when this stage was written, and five of them were the whole
    # argument: `compile_fail` doctests proving a `UserGesture` could not be
    # constructed outside the crate defining it -- ADR-0011 and ADR-0025's
    # "notebook content never auto-runs", enforced by the type system and
    # checked nowhere else. ADR-0057 removed execution, so those five went
    # with the crates that held them.
    #
    # The stage stays, and the reason it stays is now the general one rather
    # than that specific guarantee: **a runner that silently skips a category
    # of test is a runner that retires it.** One doctest is enough for that to
    # be true, and the day somebody writes the seventh is not the day anybody
    # would remember to add the stage back.
    #
    # Not a required tool: a clone without it runs `cargo test --workspace`,
    # which covers both halves in one slower command. The gate must not stop
    # working because an optional accelerator is missing.
    $script:HasNextest = $null -ne (Get-Command cargo-nextest -ErrorAction SilentlyContinue)
    if ($script:HasNextest) {
        Invoke-Stage 'test' { cargo nextest run --workspace }
        Invoke-Stage 'doctests' { cargo test --workspace --doc }
    }
    else {
        Invoke-Stage 'test' { cargo test --workspace }
        Skip-Stage 'doctests' 'covered by test (cargo-nextest not installed)'
    }

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

    # --- fuzz ----------------------------------------------------------
    # `fuzz/` declares its own `[workspace]`, so every `--workspace` command
    # above walks straight past it. Nothing formatted it, linted it or ran it
    # until this stage existed -- harnesses feeding hostile input to shipped
    # crates, outside the gate that validates everything else, while ROADMAP
    # called phase 19 *Started* on the strength of them.
    #
    # Deliberately **not** behind -IncludeSpikes. A spike is a prototype the
    # product does not depend on; these are tests of `bp-files` and
    # `bp-formats` against input designed to break them, which is the one
    # thing a gate is most for.
    #
    # Two harnesses, down from five. `bp-notebook`'s went under ADR-0057,
    # `bp-data`'s YAML one under ADR-0062, and `bp-crypto`'s envelope target
    # -- the highest-value one in the suite -- under ADR-0064, each with the
    # crate it protected.
    #
    # It costs about two and a half minutes, which is most of why it belongs
    # in the full run and not in -Quick. Where a single harness is too
    # expensive to gate on, the answer is the one `fuzz/tests/envelope.rs`
    # already used: `#[ignore]` that test with the reason on it, and leave the
    # rest running.
    $fuzzRoot = Join-Path $Root 'fuzz'
    $hasFuzz = Test-Path (Join-Path $fuzzRoot 'Cargo.toml')

    if (-not $hasFuzz) {
        Skip-Stage 'fuzz' 'no fuzz workspace found'
    }
    elseif ($Quick) {
        Skip-Stage 'fuzz' 'quick mode -- pre-push runs it'
    }
    else {
        Invoke-Stage 'fuzz' {
            Push-Location $fuzzRoot
            try {
                cargo fmt --all -- --check
                if ($LASTEXITCODE -ne 0) { throw "fuzz workspace is not formatted" }
                cargo clippy --all-targets -- -D warnings
                if ($LASTEXITCODE -ne 0) { throw "clippy rejected the fuzz workspace" }
                cargo test
            }
            finally { Pop-Location }
        }
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
            # The same nextest-or-not choice as the Windows leg, decided
            # inside the distro because the two machines install tools
            # separately -- `cargo-nextest` on the host says nothing about
            # what is on the other side of WSL. Written as shell rather than
            # resolved here so the check and the run cannot disagree.
            $linuxTest = "if command -v cargo-nextest >/dev/null 2>&1; then " +
                         "cargo nextest run --workspace && cargo test --workspace --doc; " +
                         "else cargo test --workspace; fi"
            $cmd = $prelude +
                   "export CARGO_TARGET_DIR=`$HOME/.cache/bachelorpadplus-target; " +
                   "cd '$wslRoot' && cargo fmt --all -- --check && " +
                   "cargo clippy --workspace --all-targets -- -D warnings && " +
                   $linuxTest
            Invoke-Stage "linux (wsl: $Distro)" { wsl -d $Distro -- bash -c $cmd }

            # The fuzz workspace on the Linux leg too, and for the reason the
            # leg exists at all: one-leg testing hides defects, and this
            # repository has been caught by that twice -- `PathBuf::join`
            # standing in for `bp_platform::paths::join` (4390593), and a
            # device-name rule whose *judgement* took a platform while its
            # *split* used `std::path`. `fuzz/tests/files.rs` drives
            # `bp-files`, which is where the second one lived.
            #
            # A separate CARGO_TARGET_DIR: two workspaces sharing one target
            # directory evict each other's artefacts, so sharing it would turn
            # every run into a cold build of whichever went second.
            if ($hasFuzz) {
                $fuzzCmd = $prelude +
                           "export CARGO_TARGET_DIR=`$HOME/.cache/bachelorpadplus-fuzz-target; " +
                           "cd '$wslRoot/fuzz' && cargo fmt --all -- --check && " +
                           "cargo clippy --all-targets -- -D warnings && " +
                           "cargo test"
                Invoke-Stage "linux fuzz (wsl: $Distro)" { wsl -d $Distro -- bash -c $fuzzCmd }
            }
        }
    }

    # --- summary -------------------------------------------------------
    Write-Host ''
    $script:Results | Format-Table -AutoSize | Out-String | Write-Host

    # A successful full run always records, because pre-push reads these to
    # decide whether it has to repeat the work -- see Test-GateEvidence.ps1.
    # A quick or failed run records only when asked, since neither is evidence
    # of anything a later step could rely on.
    if ($RecordEvidence -or ((-not $Quick) -and (-not $script:Failed))) {
        $dir = Join-Path $Root 'artifacts/test-evidence'
        New-Item -ItemType Directory -Force -Path $dir | Out-Null
        $stamp = (Get-Date).ToString('yyyyMMdd-HHmmss')
        $file = Join-Path $dir "local-ci-$stamp.json"
        [pscustomobject]@{
            run_at    = (Get-Date).ToString('o')
            host_os   = [System.Environment]::OSVersion.VersionString
            rustc     = (rustc --version)
            mode      = $(if ($Quick) { 'quick' } else { 'full' })
            # Whether the Linux leg actually *ran*, not whether -Linux was
            # passed: it skips itself when the distro or its toolchain is
            # missing, and a record claiming a leg that skipped would let
            # pre-push wave through a push nothing had checked on Linux.
            linux     = [bool](
                $script:Results | Where-Object {
                    $_.Stage -like 'linux*' -and $_.Status -eq 'pass'
                }
            )
            # The newest write time of anything tracked, taken *after* the
            # stages have run. Any later edit makes this record stale, which
            # is the whole mechanism: it dates the content rather than the
            # commit, because the gate runs on a working tree and the commit
            # it becomes does not exist yet.
            source_at = (Get-NewestTrackedWrite).ToString('o')
            head      = (& git -C $Root rev-parse HEAD 2>$null)
            succeeded = (-not $script:Failed)
            stages    = $script:Results
        } | ConvertTo-Json -Depth 5 | Set-Content -Encoding utf8 $file
        Write-Host "evidence: $file" -ForegroundColor DarkGray

        # Keep the ten most recent and delete the rest. A full run now writes
        # one every time rather than only when asked, so without a bound this
        # directory grows by a file per gate run forever. They are ignored, so
        # this is about disk and tidiness rather than repository churn -- and
        # about the reader below, which scans every record it finds.
        Get-ChildItem -Path $dir -Filter 'local-ci-*.json' -File |
            Sort-Object LastWriteTime -Descending |
            Select-Object -Skip 10 |
            Remove-Item -Force -ErrorAction SilentlyContinue
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
