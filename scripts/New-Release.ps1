<#
.SYNOPSIS
    Build, stage and archive a release of BachelorPad+ Lite, for Windows and Linux.

.DESCRIPTION
    Phase 20's artefact. Nothing in this repository produced a shippable thing
    before it -- `cargo build --release` produces a binary, and a binary on its
    own is not a release: it carries no licence text, no checksum, and nothing
    saying which commit it came from.

    **The archives are unsigned, deliberately** ([ADR-0055](../docs/decisions/ADR-0055.md)).
    D15 was answered "defer": a certificate is not bought and self-signing is
    refused outright, because a self-signed Authenticode certificate is only
    satisfied once the user installs a root certificate they have no reason to
    trust. The signing step is one command and one function away -- see
    `Add-Signature` at the bottom, which exists to be filled in rather than
    written from scratch. Every run says the artefacts are unsigned, in the
    same words, because a release that stays quiet about it is asking the user
    to discover SmartScreen themselves.

    **The version comes from the binary, not from a manifest.** `--version` is
    asked of the thing being shipped (ADR-0054), so the name on the archive is
    what the executable inside it will tell a user. A manifest can be edited
    without a rebuild; the binary cannot.

.PARAMETER Linux
    Also build, stage and archive the Linux target, through WSL. ADR-0001
    makes Linux an equal target, so a release without it is half a release --
    this is a switch rather than the default only because it needs a Rust
    toolchain inside the distro, exactly as the gate's Linux leg does.

.PARAMETER Distro
    WSL distribution to use for -Linux.

.PARAMETER Out
    Where the archives and SHA256SUMS.txt are written.

.PARAMETER AllowDirty
    Build anyway with uncommitted changes. Off by default: an archive built
    from a tree nobody can check out again is not a release, it is a copy, and
    the difference only becomes visible when somebody asks what is in it.

.EXAMPLE
    ./scripts/New-Release.ps1 -Linux
    Both targets, into artifacts/releases.
#>
[CmdletBinding()]
param(
    [switch]$Linux,
    [string]$Distro = 'Ubuntu-24.04',
    [string]$Out,
    [switch]$AllowDirty
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$Root = Split-Path -Parent $PSScriptRoot
if (-not $Out) { $Out = Join-Path $Root 'artifacts/releases' }

function Write-Step { param([string]$Text) Write-Host "==> $Text" }
function Write-Note { param([string]$Text) Write-Host "    $Text" }

# --- the tree has to be identifiable -------------------------------------
#
# Recorded rather than merely checked: the commit goes into the manifest
# beside each archive, so "which build is this?" is answerable from the
# artefact rather than from whoever ran the script.
Push-Location $Root
try {
    $commit = (git rev-parse HEAD).Trim()
    $dirty = @(git status --porcelain)
}
finally { Pop-Location }

if ($dirty.Count -gt 0 -and -not $AllowDirty) {
    Write-Host "the working tree has uncommitted changes:" -ForegroundColor Yellow
    $dirty | ForEach-Object { Write-Host "    $_" }
    throw 'refusing to build a release nobody can check out again -- commit, or pass -AllowDirty'
}

# --- Windows -------------------------------------------------------------

Write-Step 'building the Windows target'
Push-Location $Root
try {
    cargo build --release -p bachelorpad
    if ($LASTEXITCODE -ne 0) { throw 'the Windows release build failed' }
}
finally { Pop-Location }

$exe = Join-Path $Root 'target/release/bachelorpad.exe'
if (-not (Test-Path $exe)) { throw "$exe is missing after a build that reported success" }

# Ask the binary what it is. Redirected on purpose: a GUI-subsystem executable
# has no console of its own, so stdout needs a handle handed to it -- see
# ADR-0054's table for the three cases and which of them drops the text.
$versionFile = Join-Path ([System.IO.Path]::GetTempPath()) "bpad-version-$PID.txt"
$proc = Start-Process -FilePath $exe -ArgumentList '--version' `
    -PassThru -NoNewWindow -RedirectStandardOutput $versionFile
$null = $proc.WaitForExit(30000)
$proc.WaitForExit()
$reported = @(Get-Content $versionFile -ErrorAction SilentlyContinue)
Remove-Item $versionFile -ErrorAction SilentlyContinue

if ($reported.Count -lt 1) { throw '--version printed nothing; cannot name an archive' }
$parts = $reported[0] -split '\s+'
if ($parts.Count -lt 2 -or $parts[1] -notmatch '^\d+\.\d+\.\d+') {
    throw "cannot read a version out of '$($reported[0])'"
}
$version = $parts[1]
# The product name and the licence come from the same three lines, for the
# reason the version already did: BUILD.txt used to carry both as literals,
# and the Linux leg's literal said "BachelorPad+" while this one said
# "BachelorPlusLite" -- the naming defect ADR-0054 fixed in `--version`,
# still alive in the half nobody had re-read. A literal cannot disagree with
# the binary if there is no literal.
$displayName = $parts[0]
$declaredLicence = if ($reported.Count -ge 3) { $reported[2].Trim() } else { throw '--version did not report a licence' }
Write-Note "version $version, from the binary"
Write-Note "name $displayName, licence $declaredLicence, from the same three lines"

New-Item -ItemType Directory -Force -Path $Out | Out-Null

# The archive stem follows the public name (ADR-0074), not the repository
# directory. `bachelorpadplus-` named neither the product nor the full
# version it is the Lite edition of.
$stem = "bachelorpad-lite-$version"
$staging = Join-Path $Out "$stem-windows-x86_64"
if (Test-Path $staging) { Remove-Item $staging -Recurse -Force }
New-Item -ItemType Directory -Force -Path $staging | Out-Null

Write-Step 'staging the Windows archive'
Copy-Item $exe (Join-Path $staging 'bachelorpad.exe')
foreach ($doc in 'README.md', 'LICENSE', 'THIRD-PARTY-NOTICES.md') {
    $source = Join-Path $Root $doc
    if (-not (Test-Path $source)) { throw "$doc is missing -- the manifests claim a licence this archive would not carry" }
    Copy-Item $source (Join-Path $staging $doc)
}

# The short spelling specs.md section 19 promises. A `.cmd` shim rather than a
# second copy: the executable is ~21 MB and two of them in one archive would
# be a strange thing to hand somebody. `%~dp0` is the shim's own directory, so
# it works wherever the archive is unpacked.
Set-Content -Path (Join-Path $staging 'bpad.cmd') -Encoding ascii -Value @(
    '@echo off',
    'rem The short spelling. See specs.md section 19.',
    '"%~dp0bachelorpad.exe" %*'
)

# The icon, beside the executable rather than inside it (ADR-0068). Windows
# registration writes `DefaultIcon` pointing here, so an archive without it
# would register every file type to a blank page in Explorer -- which is
# exactly what shipped before the icon existed.
$icon = Join-Path $Root 'assets/bachelorpad.ico'
if (-not (Test-Path $icon)) { throw 'assets/bachelorpad.ico is missing -- registration would point at nothing' }
Copy-Item $icon (Join-Path $staging 'bachelorpad.ico')

# And the PNG, which is what the *window* icon loads. Slint decodes png and
# jpeg only, so it cannot read the .ico Explorer needs -- two files, two
# readers, and shipping only one of them is a silent failure in whichever was
# left out.
$windowIcon = Join-Path $Root 'assets/io.github.dboles99.BachelorPadPlus.png'
if (-not (Test-Path $windowIcon)) { throw 'assets/io.github.dboles99.BachelorPadPlus.png is missing -- the window would show the toolkit default' }
Copy-Item $windowIcon (Join-Path $staging 'io.github.dboles99.BachelorPadPlus.png')

# The in-app help, generated from docs/ (ADR-0075). Help > User Guide opens
# `app-help/index.md` as a document -- not in a browser, because this product
# launches no programs. So the help has to travel in the archive: with no
# installer there is no step that could put it anywhere else, which is the
# same argument the icon already makes.
$help = Join-Path $Root 'app-help'
if (-not (Test-Path (Join-Path $help 'index.md'))) {
    throw 'app-help/index.md is missing -- run ./scripts/Build-AppHelp.ps1; Help would open nothing'
}
Copy-Item $help (Join-Path $staging 'app-help') -Recurse

Set-Content -Path (Join-Path $staging 'BUILD.txt') -Encoding utf8 -Value @(
    "$displayName $version",
    "commit:   $commit",
    "target:   windows-x86_64",
    "signed:   no -- see ADR-0055",
    "licence:  $declaredLicence; the text is in LICENSE, beside this file",
    "notices:  THIRD-PARTY-NOTICES.md lists every dependency and its licence",
    "help:     app-help/index.md, which Help > User Guide opens"
)

$windowsArchive = Join-Path $Out "$stem-windows-x86_64.zip"
if (Test-Path $windowsArchive) { Remove-Item $windowsArchive -Force }
Compress-Archive -Path (Join-Path $staging '*') -DestinationPath $windowsArchive
Remove-Item $staging -Recurse -Force
Write-Note "wrote $([IO.Path]::GetFileName($windowsArchive))"

# --- Linux ---------------------------------------------------------------

$linuxArchive = $null
if (-not $Linux) {
    Write-Note 'skipping Linux -- pass -Linux to build it (ADR-0001 makes it an equal target)'
}
else {
    # Path translation and the PATH prelude are the gate's, for the gate's
    # reasons: wsl.exe emits UTF-16 so `wslpath` arrives as mojibake, and a
    # rustup install inside the distro does not reach a non-interactive shell.
    # Two copies of this would drift; if a third appears, extract it.
    $drive = $Root.Substring(0, 1).ToLowerInvariant()
    $wslRoot = "/mnt/$drive" + ($Root.Substring(2) -replace '\\', '/')
    $prelude = "export PATH=`"`$HOME/.cargo/bin:`$PATH`"; "

    & wsl -d $Distro -- true 2>$null | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "distro '$Distro' is unavailable" }
    & wsl -d $Distro -- bash -c "$prelude command -v cargo" 2>$null | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "no Rust toolchain in '$Distro' (see docs/governance/LOCAL_CI.md)" }

    Write-Step "building and staging the Linux target in $Distro"

    # The leg itself is `scripts/release-linux.sh`, not a string built here,
    # and that is a correction rather than a preference. It *was* a string:
    # PowerShell quoting, inside `bash -c`, around a bash `$(...)` holding an
    # `awk '{print $2}'` is three levels of escaping over one line, and what
    # it produced was a version check comparing an empty string against the
    # real one, reporting "the Linux leg failed" and nothing about why.
    #
    # Running the whole leg on the far side is right for a second reason: on
    # Linux the `bpad` short spelling is a symlink and the binary needs its
    # executable bit, and neither survives being authored from Windows.
    wsl -d $Distro -- bash "$wslRoot/scripts/release-linux.sh" $version $commit "$wslRoot/artifacts/releases"
    if ($LASTEXITCODE -ne 0) { throw 'the Linux leg failed -- its own output is above' }

    $linuxArchive = Join-Path $Out "$stem-linux-x86_64.tar.gz"
    if (-not (Test-Path $linuxArchive)) { throw 'the Linux leg reported success and produced no archive' }
    Write-Note "wrote $([IO.Path]::GetFileName($linuxArchive))"
}

# --- checksums -----------------------------------------------------------
#
# The whole of what an unsigned publisher can honestly offer, and it is not
# nothing: it answers tampering in transit, which is the threat a certificate
# answers worst. Written in the format `sha256sum -c` reads, so a Linux user
# can verify without being told how.

Write-Step 'checksums'
$archives = @($windowsArchive)
if ($linuxArchive) { $archives += $linuxArchive }

$lines = foreach ($archive in $archives) {
    $hash = (Get-FileHash -Algorithm SHA256 $archive).Hash.ToLowerInvariant()
    "$hash  $([IO.Path]::GetFileName($archive))"
}
$sums = Join-Path $Out 'SHA256SUMS.txt'
Set-Content -Path $sums -Encoding ascii -Value $lines
$lines | ForEach-Object { Write-Note $_ }

# --- signing, deliberately not done --------------------------------------

function Add-Signature {
    <#
        The one command ADR-0055 deferred, kept as a named function so that
        adding it later is an edit here and nothing else. Deliberately not
        called.

        Windows, once a certificate exists:
            signtool sign /fd SHA256 /tr <timestamp-url> /td SHA256 `
                /f <cert.pfx> /p <password> <path-to-exe>
        and note it signs the *executable*, before it is staged -- so the call
        belongs beside the Copy-Item above, not here.

        Linux needs no authority: a detached Ed25519 signature over the
        tarball answers "is this the file the author published?" rather than
        "who is the author?". That is a different and smaller claim, and
        ADR-0055 forbids describing it as though it were Authenticode.
        `bp-crypto::sign` used to be what would produce it; ADR-0064 removed
        that crate, so whoever takes this up brings a signing tool of their
        own -- `minisign` or `ssh-keygen -Y` -- rather than finding one here.
    #>
    throw 'signing is deferred -- ADR-0055'
}

Write-Host ''
Write-Host "release $version staged in $Out" -ForegroundColor Green
Write-Host 'THESE ARCHIVES ARE UNSIGNED. Windows SmartScreen will warn on download.' -ForegroundColor Yellow
Write-Host 'That is ADR-0055, answered rather than overlooked. Say so wherever they are published.'
if (-not $Linux) {
    Write-Host 'Linux was not built. ADR-0001 makes it an equal target; -Linux builds it.' -ForegroundColor Yellow
}
