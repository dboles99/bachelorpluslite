<#
.SYNOPSIS
    Build wiki/ from docs/, and optionally push it to the GitHub wiki.

.DESCRIPTION
    The third generator ADR-0075 describes. A GitHub wiki is its own git
    repository -- `<repo>.wiki.git` -- with a flat page namespace and its own
    link syntax, so it cannot simply mirror `docs/`.

    **`wiki/` is committed even though it is generated**, for one reason: it
    is what the push mirrors, so a diff of it is the review of what strangers
    will read. The staleness check is what makes committing it safe.

    **Building and pushing are separate**, and deliberately so. Generating is
    a local act with a `-Check` behind it; pushing writes to something the
    public reads, which is the kind of act this project keeps for an explicit
    gesture rather than a side effect.

.PARAMETER Check
    Generate into a temporary directory and compare rather than writing.

.PARAMETER Push
    Clone the wiki repository, replace its contents with `wiki/`, and push.
    Requires `gh` or git credentials for the remote.

.PARAMETER Remote
    The wiki repository. Defaults to this repository's own, with `.wiki.git`.
#>
[CmdletBinding()]
param(
    [switch]$Check,
    [switch]$Push,
    [string]$Remote = 'https://github.com/dboles99/bachelorpluslite.wiki.git'
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$Root = Split-Path -Parent $PSScriptRoot
. (Join-Path $PSScriptRoot 'DocsCommon.ps1')

$Out = if ($Check) {
    Join-Path ([System.IO.Path]::GetTempPath()) ("bpad-wiki-" + [guid]::NewGuid())
}
else {
    Join-Path $Root 'wiki'
}

# A GitHub wiki has one flat namespace, and a page's title *is* its filename.
# So the mapping from `docs/` is written out rather than derived: a wiki page
# called `01-installation` would be a worse page than one called
# `Installation`, and the numeric prefixes that order `docs/` on disk are not
# ordering anything here.
$pages = @(
    @{ Wiki = 'Home'; Source = $null }
    @{ Wiki = 'Installation'; Source = 'docs/user/01-installation.md' }
    @{ Wiki = 'Your-First-Note'; Source = 'docs/tutorials/01-first-note.md' }
    @{ Wiki = 'Documents'; Source = 'docs/user/02-documents.md' }
    @{ Wiki = 'Editing'; Source = 'docs/user/03-editing.md' }
    @{ Wiki = 'Tabs-and-Windows'; Source = 'docs/user/04-tabs.md' }
    @{ Wiki = 'Find-and-Replace'; Source = 'docs/user/05-find-and-replace.md' }
    @{ Wiki = 'Searching-Across-Files'; Source = 'docs/tutorials/03-search-across-files.md' }
    @{ Wiki = 'Undo-and-Recovery'; Source = 'docs/user/06-undo-and-recovery.md' }
    @{ Wiki = 'Encoding-and-Formats'; Source = 'docs/user/07-encoding-and-formats.md' }
    @{ Wiki = 'Settings'; Source = 'docs/user/08-settings.md' }
    @{ Wiki = 'Notes-and-Organizing'; Source = 'docs/user/09-notes.md' }
    @{ Wiki = 'Default-Editor'; Source = 'docs/tutorials/02-default-editor.md' }
    @{ Wiki = 'Windows-and-Linux'; Source = 'docs/user/10-platform-differences.md' }
    @{ Wiki = 'Privacy-and-Security'; Source = 'docs/user/11-privacy.md' }
    @{ Wiki = 'Troubleshooting'; Source = 'docs/user/12-troubleshooting.md' }
    @{ Wiki = 'Keyboard-Shortcuts'; Source = 'docs/generated/reference/shortcuts.md' }
    @{ Wiki = 'Command-Line'; Source = 'docs/generated/reference/cli.md' }
    @{ Wiki = 'Menus'; Source = 'docs/generated/reference/menus.md' }
    @{ Wiki = 'Building-From-Source'; Source = 'docs/developer/01-building.md' }
    @{ Wiki = 'Testing-and-the-Gate'; Source = 'docs/developer/02-testing.md' }
    @{ Wiki = 'Cutting-a-Release'; Source = 'docs/developer/03-releasing.md' }
    @{ Wiki = 'Architecture-and-Decisions'; Source = 'docs/developer/04-architecture-and-decisions.md' }
)

$linkMap = @{}
foreach ($page in $pages) {
    if ($page.Source) { $linkMap[[System.IO.Path]::GetFileName($page.Source)] = $page.Wiki }
}

$repo = 'https://github.com/dboles99/bachelorpluslite'

foreach ($page in $pages) {
    if (-not $page.Source) { continue }

    $source = Join-Path $Root $page.Source
    if (-not (Test-Path $source)) { throw "$($page.Source) is in the wiki index and does not exist" }

    $text = (Get-Content -Raw -Path $source) -replace "`r`n", "`n"

    # Relative links become wiki page names; anything not in the wiki becomes
    # a link into the repository on GitHub, where it does exist. An ADR
    # reference in a user page is the common case and there are hundreds of
    # them -- turning those into dead links would be the single largest
    # defect this generator could ship.
    $text = [regex]::Replace($text, '\]\((?<target>[^)#][^)]*?)(?<frag>#[^)]*)?\)', {
            param($m)
            $target = $m.Groups['target'].Value
            $frag = $m.Groups['frag'].Value
            if ($target -match '^(https?:|mailto:)') { return $m.Value }
            $leaf = [System.IO.Path]::GetFileName($target)
            if ($linkMap.ContainsKey($leaf)) { return "]($($linkMap[$leaf])$frag)" }
            $clean = Resolve-RepoRelative -From $page.Source -Target $target
            return "]($repo/blob/main/$clean$frag)"
        })

    $text = $text.TrimEnd() + @"


---

*This page is generated from [``$($page.Source)``]($repo/blob/main/$($page.Source)) and
edits made here will be overwritten. Change the source and open a pull request.*
"@

    Write-Generated -Path (Join-Path $Out "$($page.Wiki).md") -Content $text
}

# Home, and the sidebar GitHub shows on every page.
# `$homePage`, not `$home`: PowerShell reserves `$HOME` and refuses the
# assignment, which fails the script after it has already written every
# other page -- a half-generated tree with a confusing error over it.
$homePage = @"
# BachelorPad+ Lite

**Notepad when you want it. More when you need it.**

A lightweight text editor for **Windows 10, Windows 11 and Linux**, in Rust.
Free software under **GPL-3.0-only**.

[Downloads]($repo/releases) &middot; [bpad.prompt-forge.dev](https://bpad.prompt-forge.dev) &middot; [Source]($repo)

> **This wiki is generated from ``docs/`` in the repository (ADR-0075).**
> Edits made here are overwritten by the next sync. To change a page, change
> its source and open a pull request.

## Using it

- [Installation](Installation) -- download, verify, unpack, first launch
- [Your first note](Your-First-Note) -- ten minutes, start to finish
- [Documents](Documents) -- new, open, save, rename
- [Editing](Editing) -- and the two editor surfaces
- [Tabs and windows](Tabs-and-Windows)
- [Find and replace](Find-and-Replace)
- [Searching across files](Searching-Across-Files)
- [Undo and recovery](Undo-and-Recovery)
- [Encoding and formats](Encoding-and-Formats)
- [Settings](Settings)
- [Notes and organising](Notes-and-Organizing) -- the part that is not Notepad
- [Default editor](Default-Editor)

## Reference

- [Keyboard shortcuts](Keyboard-Shortcuts)
- [Command line](Command-Line)
- [Menus](Menus)

## When something is wrong

- [Troubleshooting](Troubleshooting)
- [Privacy and security](Privacy-and-Security)
- [Windows and Linux differences](Windows-and-Linux)

## Contributing

- [Building from source](Building-From-Source)
- [Testing and the gate](Testing-and-the-Gate)
- [Cutting a release](Cutting-a-Release)
- [Architecture and decisions](Architecture-and-Decisions)
- [CONTRIBUTING.md]($repo/blob/main/CONTRIBUTING.md)
- [SECURITY.md]($repo/blob/main/SECURITY.md)
"@

Write-Generated -Path (Join-Path $Out 'Home.md') -Content $homePage

$sidebar = @"
**[BachelorPad+ Lite](Home)**

**Using it**
- [Installation](Installation)
- [Your first note](Your-First-Note)
- [Documents](Documents)
- [Editing](Editing)
- [Tabs and windows](Tabs-and-Windows)
- [Find and replace](Find-and-Replace)
- [Across files](Searching-Across-Files)
- [Undo and recovery](Undo-and-Recovery)
- [Encoding](Encoding-and-Formats)
- [Settings](Settings)
- [Notes](Notes-and-Organizing)
- [Default editor](Default-Editor)

**Reference**
- [Shortcuts](Keyboard-Shortcuts)
- [Command line](Command-Line)
- [Menus](Menus)

**Problems**
- [Troubleshooting](Troubleshooting)
- [Privacy](Privacy-and-Security)
- [Windows / Linux](Windows-and-Linux)

**Contributing**
- [Building](Building-From-Source)
- [Testing](Testing-and-the-Gate)
- [Releasing](Cutting-a-Release)
- [Architecture](Architecture-and-Decisions)

*Generated from ``docs/``.*
"@

Write-Generated -Path (Join-Path $Out '_Sidebar.md') -Content $sidebar

if ($Check) {
    $ok = Compare-GeneratedTree -Expected $Out -Actual (Join-Path $Root 'wiki') -Name 'wiki'
    Remove-Item $Out -Recurse -Force -ErrorAction SilentlyContinue
    if (-not $ok) { exit 1 }
    exit 0
}

Write-Host "wrote wiki ($($pages.Count) pages, plus the sidebar)"

if (-not $Push) {
    Write-Host ''
    Write-Host 'Not pushed. `-Push` mirrors wiki/ to the GitHub wiki.' -ForegroundColor Yellow
    return
}

# --- pushing --------------------------------------------------------------
#
# A wiki repository has no branch protection and no review, so this replaces
# its contents wholesale and says exactly what it did. It is separated from
# generation for that reason: a generator that pushed as a side effect would
# publish on every run of the gate.

$clone = Join-Path ([System.IO.Path]::GetTempPath()) ("bpad-wiki-push-" + [guid]::NewGuid())
Write-Host "==> cloning $Remote"
git clone --quiet $Remote $clone
if ($LASTEXITCODE -ne 0) {
    throw "could not clone $Remote -- the wiki may not be enabled on the repository yet (Settings > Features > Wikis, then create the first page in the browser)"
}

try {
    Get-ChildItem -Path $clone -File -Filter '*.md' | Remove-Item -Force
    Copy-Item -Path (Join-Path $Out '*.md') -Destination $clone -Force

    Push-Location $clone
    try {
        git add -A
        $staged = @(git diff --cached --name-only)
        if ($staged.Count -eq 0) {
            Write-Host 'the wiki is already current; nothing to push'
            return
        }
        Write-Host "==> $($staged.Count) page(s) changed:"
        $staged | ForEach-Object { Write-Host "    $_" }
        git commit --quiet -m "docs: sync the wiki from docs/ (generated -- see ADR-0075)"
        git push --quiet
        if ($LASTEXITCODE -ne 0) { throw 'the push failed -- its output is above' }
        Write-Host 'pushed.' -ForegroundColor Green
    }
    finally { Pop-Location }
}
finally {
    Remove-Item $clone -Recurse -Force -ErrorAction SilentlyContinue
}
