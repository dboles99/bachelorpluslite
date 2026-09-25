<#
.SYNOPSIS
    Build app-help/ from docs/, or check that what is committed still matches.

.DESCRIPTION
    The in-app half of ADR-0075's pipeline. `app-help/` ships **inside the
    release archives** and is what Help > User Guide opens.

    **It opens as a document, not in a browser**, and that is the decision
    worth knowing before changing anything here. The obvious in-app help is a
    menu row that opens the documentation website; this product will not do
    that, because it launches no programs -- `bp-platform`'s own header says a
    library that spawns processes on the user's behalf is a library that can
    be talked into spawning a different one, and ADR-0057 removed the last
    thing in this product that ran anything.

    So the help is Markdown, it sits beside the executable, and the editor
    shows it the way it shows any other document. No browser, no process, no
    network -- and it works on a machine with none of those.

    It is also the honest test of the product: help that is unpleasant to read
    in this editor is a fact about the editor worth knowing.

.PARAMETER Check
    Generate into a temporary directory and compare rather than writing.
    Exits non-zero if what is committed has drifted.
#>
[CmdletBinding()]
param([switch]$Check)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$Root = Split-Path -Parent $PSScriptRoot
. (Join-Path $PSScriptRoot 'DocsCommon.ps1')

$Out = if ($Check) {
    Join-Path ([System.IO.Path]::GetTempPath()) ("bpad-apphelp-" + [guid]::NewGuid())
}
else {
    Join-Path $Root 'app-help'
}

# The four sections the in-app help has, and what goes in each.
#
# **A subset of `docs/`, deliberately.** The developer pages are not here:
# somebody reading help inside the editor wants to know how to rename a file,
# not how to run the fuzz harness, and a help index with `Cutting a release`
# in it is a help index somebody stops scanning.
$sections = @(
    @{
        Dir   = 'getting-started'
        Title = 'Getting started'
        Pages = @(
            @{ Name = 'installing.md'; Source = 'docs/user/01-installation.md' }
            @{ Name = 'first-note.md'; Source = 'docs/tutorials/01-first-note.md' }
            @{ Name = 'documents.md'; Source = 'docs/user/02-documents.md' }
        )
    }
    @{
        Dir   = 'commands'
        Title = 'Commands and keys'
        Pages = @(
            @{ Name = 'shortcuts.md'; Source = 'docs/generated/reference/shortcuts.md' }
            @{ Name = 'menus.md'; Source = 'docs/generated/reference/menus.md' }
            @{ Name = 'command-line.md'; Source = 'docs/generated/reference/cli.md' }
            @{ Name = 'settings.md'; Source = 'docs/user/08-settings.md' }
        )
    }
    @{
        Dir   = 'workflows'
        Title = 'Doing things'
        Pages = @(
            @{ Name = 'editing.md'; Source = 'docs/user/03-editing.md' }
            @{ Name = 'tabs.md'; Source = 'docs/user/04-tabs.md' }
            @{ Name = 'find-and-replace.md'; Source = 'docs/user/05-find-and-replace.md' }
            @{ Name = 'search-across-files.md'; Source = 'docs/tutorials/03-search-across-files.md' }
            @{ Name = 'notes.md'; Source = 'docs/user/09-notes.md' }
            @{ Name = 'default-editor.md'; Source = 'docs/tutorials/02-default-editor.md' }
            @{ Name = 'encoding.md'; Source = 'docs/user/07-encoding-and-formats.md' }
        )
    }
    @{
        Dir   = 'troubleshooting'
        Title = 'When something is wrong'
        Pages = @(
            @{ Name = 'troubleshooting.md'; Source = 'docs/user/12-troubleshooting.md' }
            @{ Name = 'recovery.md'; Source = 'docs/user/06-undo-and-recovery.md' }
            @{ Name = 'privacy.md'; Source = 'docs/user/11-privacy.md' }
            @{ Name = 'platforms.md'; Source = 'docs/user/10-platform-differences.md' }
        )
    }
)

# Links between pages have to be rewritten, because the tree is flatter here
# than in `docs/`. Built from the section table rather than written out, so a
# page that moves cannot leave a dead link behind.
$linkMap = @{}
foreach ($section in $sections) {
    foreach ($page in $section.Pages) {
        $linkMap[[System.IO.Path]::GetFileName($page.Source)] = "../$($section.Dir)/$($page.Name)"
    }
}

$index = New-Object System.Collections.Generic.List[string]
$index.Add('# BachelorPad+ Lite -- help')
$index.Add('')
$index.Add('*Generated from `docs/` by `scripts/Build-AppHelp.ps1`. Do not edit.*')
$index.Add('')
$index.Add('Notepad when you want it. More when you need it.')
$index.Add('')
$index.Add('This is the help that ships with the program. It is Markdown, and you are')
$index.Add('reading it in the editor it describes -- there is no browser involved,')
$index.Add('because this product launches no programs (ADR-0057).')
$index.Add('')
$index.Add('The full manual, including how to build and contribute, is at')
$index.Add('<https://bpad.prompt-forge.dev/docs>.')
$index.Add('')

$written = 0

foreach ($section in $sections) {
    $index.Add("## $($section.Title)")
    $index.Add('')

    foreach ($page in $section.Pages) {
        $source = Join-Path $Root $page.Source
        if (-not (Test-Path $source)) { throw "$($page.Source) is in the help index and does not exist" }

        $text = (Get-Content -Raw -Path $source) -replace "`r`n", "`n"

        # Rewrite every relative link to its place in this tree, and turn a
        # link to something that is not here into plain text rather than
        # leaving a path that resolves to nothing. A dead link in help shipped
        # inside an archive cannot be fixed by the person who finds it.
        $text = [regex]::Replace($text, '\]\((?<target>[^)#][^)]*?)(?<frag>#[^)]*)?\)', {
                param($m)
                $target = $m.Groups['target'].Value
                $frag = $m.Groups['frag'].Value
                if ($target -match '^(https?:|mailto:)') { return $m.Value }
                $leaf = [System.IO.Path]::GetFileName($target)
                if ($linkMap.ContainsKey($leaf)) { return "]($($linkMap[$leaf])$frag)" }
                # Not shipped: ADRs, the README, the developer pages. Point at
                # the website, which has all of them.
                return "](https://bpad.prompt-forge.dev/docs$frag)"
            })

        # The title line each page already carries becomes the page's heading
        # here too; only the trailer is added.
        $text = $text.TrimEnd() + "`n`n---`n`n[Back to the help index](../index.md)`n"

        Write-Generated -Path (Join-Path $Out "$($section.Dir)/$($page.Name)") -Content $text
        $written++

        # The first heading, as the link's label -- so the index reads as the
        # pages read, and a retitled page retitles its own index entry.
        $heading = ($text -split "`n" | Where-Object { $_ -match '^# ' } | Select-Object -First 1)
        $label = if ($heading) { $heading -replace '^#\s*', '' } else { $page.Name }
        $index.Add("- [$label]($($section.Dir)/$($page.Name))")
    }
    $index.Add('')
}

$index.Add('---')
$index.Add('')
$index.Add('BachelorPad+ Lite is free software under the GNU General Public License,')
$index.Add('version 3. It comes with NO WARRANTY. The full text is in `LICENSE`, beside')
$index.Add('the program, and `THIRD-PARTY-NOTICES.md` lists every crate it links.')
$index.Add('')
$index.Add('Windows 10, Windows 11 and Linux. macOS is not built (ADR-0072).')

Write-Generated -Path (Join-Path $Out 'index.md') -Content ($index -join "`n")

if ($Check) {
    $ok = Compare-GeneratedTree -Expected $Out -Actual (Join-Path $Root 'app-help') -Name 'app-help'
    Remove-Item $Out -Recurse -Force -ErrorAction SilentlyContinue
    if (-not $ok) { exit 1 }
    exit 0
}

Write-Host "wrote app-help ($written pages in $($sections.Count) sections, plus the index)"
