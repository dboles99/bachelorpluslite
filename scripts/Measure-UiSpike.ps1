<#
.SYNOPSIS
    Measure the UI toolkit spikes against the specs.md section 22 targets.

.DESCRIPTION
    specs.md sets cold startup < 150 ms, warm < 75 ms and idle RAM < 50 MB.
    Those numbers rule out whole categories of toolkit, so the choice between
    candidates should be made against measurements rather than reputation.

    Three things are measured per spike:

    * Wall-clock launch -- process spawn to exit, with the shell quitting as
      soon as it has drawn its first frame. This is the number to compare
      against the startup target, because it includes process creation, which
      an in-process timer cannot see.
    * In-process ready time -- `main()` entry to first frame, reported by the
      shell itself as BPSPIKE_READY_MS. The gap between this and wall clock is
      process and loader overhead.
    * Idle working set -- sampled after the window has been up and quiet.

    The first run of each binary is reported separately as the cold figure:
    later runs benefit from the OS file cache and are the warm figures. A
    genuinely cold measurement needs a reboot, so treat the cold column as an
    upper bound rather than a precise value.

.PARAMETER Runs
    Launches per spike. The first is the cold sample; the rest are warm.

.PARAMETER RssDelayMs
    How long to let a shell sit idle before sampling its working set.

.PARAMETER SkipRss
    Skip the memory pass. The memory pass briefly opens a real window on the
    desktop for each spike, which is disruptive if you are using the machine.

.EXAMPLE
    ./scripts/Measure-UiSpike.ps1
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
$SpikeRoot = Join-Path $Root 'spikes/ui-toolkit'

function Get-Median {
    param([double[]]$Values)
    if ($Values.Count -eq 0) { return $null }
    $sorted = $Values | Sort-Object
    $mid = [math]::Floor($sorted.Count / 2)
    if ($sorted.Count % 2 -eq 1) { return $sorted[$mid] }
    return ($sorted[$mid - 1] + $sorted[$mid]) / 2
}

$spikes = Get-ChildItem -Path $SpikeRoot -Directory -ErrorAction Stop |
    ForEach-Object {
        $exe = Get-ChildItem -Path (Join-Path $_.FullName 'target/release') -Filter '*.exe' -ErrorAction SilentlyContinue |
            Where-Object { $_.Name -notlike '*.d' } |
            Select-Object -First 1
        if ($exe) {
            [pscustomobject]@{ Name = $_.Name; Exe = $exe.FullName; Bytes = $exe.Length }
        }
    } | Where-Object { $_ }

if (-not $spikes) {
    throw "No release binaries found under $SpikeRoot. Build them first: cargo build --release"
}

$report = [System.Collections.Generic.List[object]]::new()
$tempOut = Join-Path ([System.IO.Path]::GetTempPath()) "bpspike-$PID.out"

foreach ($spike in $spikes) {
    Write-Host "Measuring $($spike.Name)" -ForegroundColor Cyan

    $wall = @()
    $ready = @()

    for ($i = 0; $i -lt $Runs; $i++) {
        $sw = [Diagnostics.Stopwatch]::StartNew()
        $proc = Start-Process -FilePath $spike.Exe -ArgumentList '--measure-exit' `
            -PassThru -NoNewWindow -RedirectStandardOutput $tempOut
        $proc.WaitForExit()
        $sw.Stop()

        $wall += $sw.Elapsed.TotalMilliseconds

        $line = Select-String -Path $tempOut -Pattern 'BPSPIKE_READY_MS=([0-9.]+)' |
            Select-Object -First 1
        if ($line) {
            $ready += [double]$line.Matches[0].Groups[1].Value
        }
        Write-Host ("  run {0}: {1:N1} ms wall" -f ($i + 1), $sw.Elapsed.TotalMilliseconds) -ForegroundColor DarkGray
    }

    $rssMb = $null
    $privMb = $null
    if (-not $SkipRss) {
        $proc = Start-Process -FilePath $spike.Exe -PassThru
        try {
            Start-Sleep -Milliseconds $RssDelayMs
            $proc.Refresh()
            $rssMb = [math]::Round($proc.WorkingSet64 / 1MB, 1)
            $privMb = [math]::Round($proc.PrivateMemorySize64 / 1MB, 1)
        }
        finally {
            if (-not $proc.HasExited) { $proc.Kill() }
            $proc.WaitForExit()
        }
        Write-Host ("  idle: {0} MB working set, {1} MB private" -f $rssMb, $privMb) -ForegroundColor DarkGray
    }

    # The first launch pays the file-cache cost; the rest do not.
    $warm = if ($wall.Count -gt 1) { $wall[1..($wall.Count - 1)] } else { @() }

    $report.Add([pscustomobject]@{
            Spike           = $spike.Name
            BinaryMB        = [math]::Round($spike.Bytes / 1MB, 1)
            ColdWallMs      = [math]::Round($wall[0], 1)
            WarmMedianMs    = if ($warm) { [math]::Round((Get-Median $warm), 1) } else { $null }
            WarmMinMs       = if ($warm) { [math]::Round(($warm | Measure-Object -Minimum).Minimum, 1) } else { $null }
            ReadyMedianMs   = if ($ready) { [math]::Round((Get-Median $ready), 1) } else { $null }
            IdleWorkingMB   = $rssMb
            IdlePrivateMB   = $privMb
        })
}

Remove-Item $tempOut -ErrorAction SilentlyContinue

Write-Host ''
$report | Format-Table -AutoSize | Out-String | Write-Host

Write-Host 'specs.md section 22 targets: cold < 150 ms, warm < 75 ms, idle RAM < 50 MB' -ForegroundColor Yellow

$dir = Join-Path $Root 'artifacts/benchmarks'
New-Item -ItemType Directory -Force -Path $dir | Out-Null
$stamp = (Get-Date).ToString('yyyyMMdd-HHmmss')
$file = Join-Path $dir "ui-toolkit-$stamp.json"

[pscustomobject]@{
    measured_at = (Get-Date).ToString('o')
    host_os     = [System.Environment]::OSVersion.VersionString
    rustc       = (rustc --version)
    runs        = $Runs
    note        = 'Cold figures are first-launch upper bounds; a true cold start needs a reboot.'
    results     = $report
} | ConvertTo-Json -Depth 5 | Set-Content -Encoding utf8 $file

Write-Host "Recorded: $file" -ForegroundColor DarkGray
