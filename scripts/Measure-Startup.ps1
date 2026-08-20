<#
.SYNOPSIS
    Measure BachelorPad+ startup and idle memory against specs.md section 22.

.DESCRIPTION
    Measures the real application, not a benchmark fixture. A fixture measures
    the fixture, and drifts from the product the moment either changes.

    Two clocks, because no single one is both meaningful and available under
    every renderer:

    * Time to window -- measured externally, from process spawn until the
      process owns a main window. Works under any renderer, so this is the
      column to compare across rows.
    * Time to first frame -- reported by the app as BPSPIKE_READY_MS when run
      with --measure-exit. More precise, but it needs a rendering notifier,
      which Slint's software renderer does not provide. Blank means
      unavailable, never zero.

    Variants select the renderer through SLINT_BACKEND, which Slint reads at
    startup.

.PARAMETER Runs
    Launches per variant. The first is the cold sample; the rest are warm.

.PARAMETER RssDelayMs
    How long to let the app sit idle before sampling its working set.

.PARAMETER SkipRss
    Skip the window/memory pass, which briefly opens a real window.

.EXAMPLE
    cargo build --release ; ./scripts/Measure-Startup.ps1
#>
[CmdletBinding()]
param(
    [int]$Runs = 7,
    [int]$RssDelayMs = 2500,
    [switch]$SkipRss
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$Root = Split-Path -Parent $PSScriptRoot
$Exe = Join-Path $Root 'target/release/bachelorpad.exe'

if (-not (Test-Path $Exe)) {
    throw "Not built. Run: cargo build --release"
}

$Variants = @(
    @{ Label = 'gpu (femtovg)'; Env = @{} }
    @{ Label = 'software'; Env = @{ SLINT_BACKEND = 'winit-software' } }
)

function Get-Median {
    param([double[]]$Values)
    if ($Values.Count -eq 0) { return $null }
    $sorted = $Values | Sort-Object
    $mid = [math]::Floor($sorted.Count / 2)
    if ($sorted.Count % 2 -eq 1) { return $sorted[$mid] }
    return ($sorted[$mid - 1] + $sorted[$mid]) / 2
}

function Measure-WindowAndMemory {
    param([string]$Path, [int]$SettleMs, [int]$TimeoutMs = 20000)

    $sw = [Diagnostics.Stopwatch]::StartNew()
    $proc = Start-Process -FilePath $Path -PassThru
    $windowMs = $null
    try {
        while ($sw.Elapsed.TotalMilliseconds -lt $TimeoutMs) {
            if ($proc.HasExited) { break }
            $proc.Refresh()
            if ($proc.MainWindowHandle -ne [IntPtr]::Zero) {
                $windowMs = $sw.Elapsed.TotalMilliseconds
                break
            }
            [System.Threading.Thread]::Sleep(2)
        }
        if ($null -eq $windowMs) { return $null }

        Start-Sleep -Milliseconds $SettleMs
        if ($proc.HasExited) { return $null }
        $proc.Refresh()
        return [pscustomobject]@{
            WindowMs  = $windowMs
            WorkingMB = [math]::Round($proc.WorkingSet64 / 1MB, 1)
            PrivateMB = [math]::Round($proc.PrivateMemorySize64 / 1MB, 1)
        }
    }
    finally {
        if (-not $proc.HasExited) { $proc.Kill() }
        $proc.WaitForExit()
    }
}

$report = [System.Collections.Generic.List[object]]::new()
$tempOut = Join-Path ([System.IO.Path]::GetTempPath()) "bpstartup-$PID.out"
$binaryMb = [math]::Round((Get-Item $Exe).Length / 1MB, 1)

foreach ($variant in $Variants) {
    Write-Host "Measuring $($variant.Label)" -ForegroundColor Cyan

    $saved = @{}
    foreach ($key in $variant.Env.Keys) {
        $saved[$key] = [Environment]::GetEnvironmentVariable($key)
        [Environment]::SetEnvironmentVariable($key, $variant.Env[$key])
    }

    try {
        # Probe once: without a first-frame hook the --measure-exit runs would
        # time an immediate exit and produce a fast-looking lie.
        #
        # Launch it exactly like a measurement run. A release build sets
        # windows_subsystem = "windows", so it has no console and PowerShell's
        # `>` redirection captures nothing -- the probe then sees no marker,
        # concludes the hook exists, and times the very lie it exists to catch.
        $probeProc = Start-Process -FilePath $Exe -ArgumentList '--measure-exit' `
            -PassThru -NoNewWindow -RedirectStandardOutput $tempOut
        $probeProc.WaitForExit()
        $hasFrameHook = (Select-String -Path $tempOut -Pattern 'BPSPIKE_READY_MS=' -Quiet) -eq $true

        $wall = @()
        $ready = @()

        if ($hasFrameHook) {
            for ($i = 0; $i -lt $Runs; $i++) {
                $sw = [Diagnostics.Stopwatch]::StartNew()
                $proc = Start-Process -FilePath $Exe -ArgumentList '--measure-exit' `
                    -PassThru -NoNewWindow -RedirectStandardOutput $tempOut
                $proc.WaitForExit()
                $sw.Stop()

                $wall += $sw.Elapsed.TotalMilliseconds
                $line = Select-String -Path $tempOut -Pattern 'BPSPIKE_READY_MS=([0-9.]+)' |
                    Select-Object -First 1
                if ($line) { $ready += [double]$line.Matches[0].Groups[1].Value }
                Write-Host ("  run {0}: {1:N1} ms wall" -f ($i + 1), $sw.Elapsed.TotalMilliseconds) -ForegroundColor DarkGray
            }
        }
        else {
            Write-Host '  no first-frame hook for this renderer; window clock only' -ForegroundColor Yellow
        }

        $windowMs = $null
        $rssMb = $null
        $privMb = $null
        if (-not $SkipRss) {
            $probe = Measure-WindowAndMemory -Path $Exe -SettleMs $RssDelayMs
            if ($probe) {
                $windowMs = [math]::Round($probe.WindowMs, 1)
                $rssMb = $probe.WorkingMB
                $privMb = $probe.PrivateMB
                Write-Host ("  window in {0} ms; idle {1} MB working, {2} MB private" -f $windowMs, $rssMb, $privMb) -ForegroundColor DarkGray
            }
            else {
                Write-Host '  window probe failed (process exited early)' -ForegroundColor Yellow
            }
        }

        $warm = if ($wall.Count -gt 1) { $wall[1..($wall.Count - 1)] } else { @() }

        $report.Add([pscustomobject]@{
                Variant       = $variant.Label
                BinaryMB      = $binaryMb
                WindowMs      = $windowMs
                ColdWallMs    = if ($wall) { [math]::Round($wall[0], 1) } else { $null }
                WarmMedianMs  = if ($warm) { [math]::Round((Get-Median $warm), 1) } else { $null }
                ReadyMedianMs = if ($ready) { [math]::Round((Get-Median $ready), 1) } else { $null }
                IdleWorkingMB = $rssMb
                IdlePrivateMB = $privMb
            })
    }
    finally {
        foreach ($key in $saved.Keys) {
            [Environment]::SetEnvironmentVariable($key, $saved[$key])
        }
    }
}

Remove-Item $tempOut -ErrorAction SilentlyContinue

Write-Host ''
$report | Format-Table -AutoSize | Out-String | Write-Host
Write-Host 'specs.md section 22: time to window < 150 ms cold, idle RAM < 50 MB' -ForegroundColor Yellow

$dir = Join-Path $Root 'artifacts/benchmarks'
New-Item -ItemType Directory -Force -Path $dir | Out-Null
$stamp = (Get-Date).ToString('yyyyMMdd-HHmmss')
$file = Join-Path $dir "startup-$stamp.json"

[pscustomobject]@{
    measured_at = (Get-Date).ToString('o')
    host_os     = [System.Environment]::OSVersion.VersionString
    rustc       = (rustc --version)
    runs        = $Runs
    note        = 'WindowMs is measured externally and is comparable across renderers. ReadyMedianMs needs a rendering notifier; null means the renderer has none.'
    results     = $report
} | ConvertTo-Json -Depth 5 | Set-Content -Encoding utf8 $file

Write-Host "Recorded: $file" -ForegroundColor DarkGray
