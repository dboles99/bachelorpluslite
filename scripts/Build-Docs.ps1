<#
.SYNOPSIS
    Build docs/generated/ from docs/, or check that what is committed still
    matches.

.DESCRIPTION
    The first of the three generators ADR-0075 describes. `docs/` is the one
    source; this produces the reference pages that have a home in the *code*
    rather than in prose, and assembles everything into a single manual.

    **Three pages here are generated from something that is not Markdown**,
    and that is the point of the design rather than an optimisation:

      * `reference/cli.md`       -- from the binary's own `--help`
      * `reference/shortcuts.md` -- from `bp_ui::dispatch::SHORTCUTS`
      * `reference/menus.md`     -- from docs/product/MENU_MAP.md

    A shortcut table typed into `docs/reference/` would be a second spelling
    of a fact the product already holds, and it would be wrong within a
    session. The question that decides it is trap 3 pointed at documentation:
    if this page and the code disagreed, what would fail? For a page about
    what a tab is *for*, nothing, and prose is the right home. For a page
    listing the flags, nothing either -- which is exactly why it cannot be
    prose.

.PARAMETER Check
    Generate into a temporary directory and compare, rather than writing.
    Exits non-zero if what is committed has drifted. This is what the gate
    and .github/workflows/docs.yml run.

.PARAMETER SkipBinary
    Read the flag reference from source instead of asking the binary. For a
    machine with no toolchain; the binary is the better answer and the
    default.
#>
[CmdletBinding()]
param(
    [switch]$Check,
    [switch]$SkipBinary
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$Root = Split-Path -Parent $PSScriptRoot
. (Join-Path $PSScriptRoot 'DocsCommon.ps1')

$Out = if ($Check) { Join-Path ([System.IO.Path]::GetTempPath()) ("bpad-docs-" + [guid]::NewGuid()) }
       else { Join-Path $Root 'docs/generated' }

# --- reference/cli.md ----------------------------------------------------
#
# Asked of the binary, for ADR-0054's reason: a manifest can be edited without
# a rebuild and the binary cannot, so `--help` is the only answer that is
# certainly true of the thing being shipped.
#
# A *debug* build deliberately. A Windows release build is GUI-subsystem and
# has no console of its own, so its stdout goes nowhere when nothing has
# attached one -- and Rust tolerates the invalid handle rather than panicking,
# so the text is simply dropped and the exit code stays 0. A debug build is a
# console program on both platforms and prints.
function Get-HelpText {
    if ($SkipBinary) {
        return Read-FlagsFromSource -Root $Root
    }
    Push-Location $Root
    try {
        # **stderr is captured, not discarded**, and that is a fix. It was
        # `2>$null`, so a compile error produced an empty stdout and this
        # threw "the binary printed no help" -- a message about the wrong
        # thing entirely, which sent somebody looking at the *script* while
        # the actual error, a borrow-checker complaint two crates away, had
        # already been thrown away.
        #
        # The general form: **a diagnostic that discards the only evidence is
        # worse than no diagnostic**, because it replaces "I do not know" with
        # a confident wrong answer.
        $errors = Join-Path ([System.IO.Path]::GetTempPath()) "bpad-help-$PID.err"
        try {
            $text = cargo run -q -p bachelorpad -- --help 2>$errors | Out-String
            if ($LASTEXITCODE -ne 0 -or [string]::IsNullOrWhiteSpace($text)) {
                $detail = if (Test-Path $errors) { (Get-Content -Raw $errors).Trim() } else { '' }
                if ($detail) {
                    Write-Host 'cargo said:' -ForegroundColor Yellow
                    Write-Host $detail
                }
                throw 'the binary printed no help -- cargo output is above; pass -SkipBinary to read the flags from source instead'
            }
        }
        finally { Remove-Item $errors -ErrorAction SilentlyContinue }
        return $text.Replace("`r`n", "`n").TrimEnd()
    }
    finally { Pop-Location }
}

$cli = @'
# Command line

*Generated from the binary's own `--help` by `scripts/Build-Docs.ps1`. Do not
edit.*

The flag list in `bp_config::cli::FLAGS` is the one home for this, and
`--help` is rendered from it rather than written out -- so a flag that exists
without a line here is not possible.

```text
{0}
```

## Notes

**On Windows, a release build prints this to a console only when one is
attached.** It is a GUI-subsystem executable, so running it from a terminal
works and double-clicking it shows nothing. Measured and conceded
deliberately: the alternative costs `unsafe` and a Windows dependency, to
serve somebody double-clicking a text editor in order to read its help
(ADR-0054).

**A flag this product does not accept is reported, not swallowed.** That was
once untrue in one direction only -- the config file told you about an
unknown key and the command line quietly discarded an unknown flag, and
nobody had chosen that.
'@ -f (Get-HelpText)

Write-Generated -Path (Join-Path $Out 'reference/cli.md') -Content $cli

# --- reference/shortcuts.md ----------------------------------------------
#
# Parsed out of the constant Help > Keyboard Shortcuts shows, so the dialog
# and the documentation cannot disagree. They are the same string.
$shortcuts = Read-ShortcutsFromSource -Root $Root

# The constant is laid out in **fixed columns** for a fixed-width dialog, and
# splitting on a run of spaces is wrong for exactly the rows that matter:
# `Ctrl+Z / Ctrl+Y Undo / Redo` fills the first column completely, so one
# space separates the two halves and a greedy split swallowed the description
# into the key. Two of thirteen rows, both of them the compound ones.
#
# So: split at the column, and refuse a line that does not fit the shape
# rather than emitting a row that reads plausibly and is wrong.
$shortcutKeyColumn = 16

$shortcutRows = foreach ($line in ($shortcuts -split "`n")) {
    if ([string]::IsNullOrWhiteSpace($line)) { continue }
    if ($line.Length -le $shortcutKeyColumn) {
        throw "SHORTCUTS line '$line' is shorter than the $shortcutKeyColumn-column layout this parses"
    }
    $keys = $line.Substring(0, $shortcutKeyColumn).TrimEnd()
    $what = $line.Substring($shortcutKeyColumn).Trim()
    if ([string]::IsNullOrWhiteSpace($keys) -or [string]::IsNullOrWhiteSpace($what)) {
        throw "SHORTCUTS line '$line' did not split into a key and a description"
    }
    '| `{0}` | {1} |' -f $keys, $what
}

$shortcutsPage = @'
# Keyboard shortcuts

*Generated from `bp_ui::dispatch::SHORTCUTS` by `scripts/Build-Docs.ps1`. Do
not edit.*

This is the same string Help > Keyboard Shortcuts shows, so the dialog and
this page cannot disagree.

| Keys | Does |
| --- | --- |
{0}

## Two of these need the custom editor surface

`Ctrl+D` and `Alt+Up` / `Alt+Down` have to know where the caret is, and the
toolkit's text widget will not say. Start with `--editor-view` to get them
(ADR-0018).

Everything else works in both surfaces.

## They cannot be rebound

Deliberately absent rather than half-present: rebinding needs a keymap file, a
conflict story and a way to see what a key does now, and none of that has been
designed.
'@ -f ($shortcutRows -join "`n")

Write-Generated -Path (Join-Path $Out 'reference/shortcuts.md') -Content $shortcutsPage

# --- reference/menus.md --------------------------------------------------
#
# docs/product/MENU_MAP.md owns every menu row and its state, and it is
# written for somebody working on the product -- the prose around each table
# is about why rows were removed and which ADR did it. The tables themselves
# are the user-facing half, so those are what is lifted.
$menus = Read-MenuTables -Path (Join-Path $Root 'docs/product/MENU_MAP.md')

$menusPage = @'
# Menus

*Generated from `docs/product/MENU_MAP.md` by `scripts/Build-Docs.ps1`. Do not
edit.*

Every row in every menu either does something or is a **readout** -- a greyed
line that answers a question rather than inviting a click. Nothing here means
"not built yet"; there are no planned rows left (ADR-0048).

A greyed row is one of two things, and telling them apart matters:

- **it exists and cannot act right now**, with the reason in its own label --
  Save for a document with nowhere to go. *You can fix this*;
- **a readout**, which is not a row to click at all.

**Caret** means the row needs the custom editor surface (`--editor-view`),
because it has to know where the caret is (ADR-0018).

{0}
'@ -f $menus

Write-Generated -Path (Join-Path $Out 'reference/menus.md') -Content $menusPage

# --- manual.md and manual.html -------------------------------------------
#
# One file, in the order somebody would read it. The order is written down
# here rather than derived from filenames: it is a decision about the
# document, and a numeric prefix is a weak way to record a decision.
$manualPages = @(
    @{ Title = 'Installation and first launch'; Path = 'docs/user/01-installation.md' }
    @{ Title = 'Your first note, start to finish'; Path = 'docs/tutorials/01-first-note.md' }
    @{ Title = 'Creating, opening and saving documents'; Path = 'docs/user/02-documents.md' }
    @{ Title = 'Editing'; Path = 'docs/user/03-editing.md' }
    @{ Title = 'Tabs and windows'; Path = 'docs/user/04-tabs.md' }
    @{ Title = 'Find and replace'; Path = 'docs/user/05-find-and-replace.md' }
    @{ Title = 'Searching across a folder'; Path = 'docs/tutorials/03-search-across-files.md' }
    @{ Title = 'Undo, history and recovery'; Path = 'docs/user/06-undo-and-recovery.md' }
    @{ Title = 'Encoding, line endings and file formats'; Path = 'docs/user/07-encoding-and-formats.md' }
    @{ Title = 'Settings and customisation'; Path = 'docs/user/08-settings.md' }
    @{ Title = 'The note layer'; Path = 'docs/user/09-notes.md' }
    @{ Title = 'Making it your default editor'; Path = 'docs/tutorials/02-default-editor.md' }
    @{ Title = 'Windows and Linux differences'; Path = 'docs/user/10-platform-differences.md' }
    @{ Title = 'Privacy and security'; Path = 'docs/user/11-privacy.md' }
    @{ Title = 'Troubleshooting'; Path = 'docs/user/12-troubleshooting.md' }
    @{ Title = 'Keyboard shortcuts'; Generated = 'reference/shortcuts.md' }
    @{ Title = 'Command line'; Generated = 'reference/cli.md' }
    @{ Title = 'Menus'; Generated = 'reference/menus.md' }
    @{ Title = 'Building from source'; Path = 'docs/developer/01-building.md' }
    @{ Title = 'Testing and the gate'; Path = 'docs/developer/02-testing.md' }
    @{ Title = 'Cutting a release'; Path = 'docs/developer/03-releasing.md' }
    @{ Title = 'Architecture and decisions'; Path = 'docs/developer/04-architecture-and-decisions.md' }
)

$body = New-Object System.Collections.Generic.List[string]
$toc = New-Object System.Collections.Generic.List[string]

foreach ($page in $manualPages) {
    # The extra parentheses are load-bearing. Inside a method call the comma
    # is an *argument separator*, not an array constructor, so
    # `.Add('...' -f $a, $b)` parses as `.Add(('...' -f $a), $b)` -- the
    # format string gets one argument, `{1}` has nothing to bind, and the
    # error names neither the string nor the call.
    $toc.Add(('- [{0}](#{1})' -f $page.Title, (ConvertTo-Anchor $page.Title)))
    $source = if ($page.ContainsKey('Generated')) { Join-Path $Out $page.Generated }
              else { Join-Path $Root $page.Path }
    if (-not (Test-Path $source)) { throw "$source is in the manual's contents and does not exist" }

    # Demote every heading by one, so each page's `#` becomes an `##` under
    # the manual's single title, and links between pages become links within
    # the document.
    $text = (Get-Content -Raw -Path $source) -replace "`r`n", "`n"
    $text = ($text -split "`n" | ForEach-Object {
            if ($_ -match '^#{1,5} ') { '#' + $_ } else { $_ }
        }) -join "`n"
    $body.Add($text.TrimEnd())
    $body.Add('')
}

$manual = @'
# BachelorPad+ Lite -- the manual

*Generated by `scripts/Build-Docs.ps1` from `docs/`. Do not edit.*

Notepad when you want it. More when you need it.

Windows 10, Windows 11 and Linux. GPL-3.0-only.

## Contents

{0}

{1}
'@ -f ($toc -join "`n"), ($body -join "`n")

Write-Generated -Path (Join-Path $Out 'manual.md') -Content $manual

# The printable form. `ConvertFrom-Markdown` ships with PowerShell 7, so this
# needs no converter of its own -- a documentation pipeline that dragged in a
# toolchain would be a strange thing to hand somebody who wants to fix a typo.
# Print this to PDF from a browser; there is no PDF engine here and adding one
# would be a dependency decision rather than a script change.
$html = (ConvertFrom-Markdown -InputObject $manual).Html
# Concatenated rather than built with `-f`. The stylesheet below is full of
# literal braces and the format operator reads every one of them as a
# placeholder -- which fails with an index error naming neither the string nor
# the brace, and costs longer to diagnose than it does to avoid.
$styled = @'
<meta charset="utf-8">
<title>BachelorPad+ Lite -- the manual</title>
<style>
  body { max-width: 46rem; margin: 3rem auto; padding: 0 1.5rem;
         font: 16px/1.65 Georgia, "Times New Roman", serif; color: #1a1a1a; }
  h1, h2, h3, h4 { font-family: system-ui, sans-serif; line-height: 1.25; }
  h2 { margin-top: 3rem; border-bottom: 1px solid #ddd; padding-bottom: .3rem; }
  code, pre { font-family: "Cascadia Mono", Consolas, monospace; font-size: .9em; }
  pre { background: #f6f6f4; padding: 1rem; overflow-x: auto; border-radius: 4px; }
  table { border-collapse: collapse; width: 100%; margin: 1rem 0; font-size: .95em; }
  th, td { border: 1px solid #ddd; padding: .45rem .6rem; text-align: left; vertical-align: top; }
  th { background: #f6f6f4; }
  blockquote { border-left: 3px solid #c9c9c4; margin-left: 0; padding-left: 1rem; color: #444; }
  @media print { body { max-width: none; margin: 0; font-size: 11pt; }
                 h2 { page-break-before: always; } }
</style>
'@ + "`n" + $html

Write-Generated -Path (Join-Path $Out 'manual.html') -Content $styled

# --- report ---------------------------------------------------------------

if ($Check) {
    $result = Compare-GeneratedTree -Expected $Out -Actual (Join-Path $Root 'docs/generated') -Name 'docs/generated'
    Remove-Item $Out -Recurse -Force -ErrorAction SilentlyContinue
    if (-not $result) { exit 1 }
    exit 0
}

Write-Host "wrote docs/generated ($($manualPages.Count) sections)"
