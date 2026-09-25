<#
.SYNOPSIS
    Check the generated website: every local link resolves, no placeholder
    survived, and nothing is loaded from a third party.

.DESCRIPTION
    **One home for these three checks**, because the first version had them
    written twice -- once in `.github/workflows/site.yml` and once in the gate
    -- and both copies carried the same two bugs. Two gates that disagree are
    worse than one, and two copies of a check are two places to fix a false
    positive.

    The two bugs, kept as comments where they were made:

      * `site/_templates/` and `site/_locales/` are *sources*, not pages. The
        templates are full of `{{placeholders}}`, so scanning them reported
        thirty broken links that do not exist.
      * A `<link rel="canonical">` or `hreflang` alternate is **metadata, not
        a subresource**. The browser fetches nothing for either. Flagging them
        as third-party requests would have failed every run forever, and the
        fix is not to loosen the check but to ask what a request actually is.

.PARAMETER Root
    Repository root. Defaults to the parent of this script's directory.
#>
[CmdletBinding()]
param([string]$Root)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

if (-not $Root) { $Root = Split-Path -Parent $PSScriptRoot }
$SiteRoot = Join-Path $Root 'site'

# Only what is actually served. `_templates` and `_locales` are inputs to
# Build-Site.ps1; `api` is server-side code that never reaches a browser.
function Get-ServedFiles {
    param([string[]]$Include)
    Get-ChildItem $SiteRoot -Include $Include -Recurse -File |
        Where-Object {
            $rel = $_.FullName.Substring($SiteRoot.Length).Replace([char]92, [char]47)
            $rel -notmatch '^/_' -and $rel -notmatch '^/api/'
        }
}

$failures = 0

# --- 1. every local link points at a file that exists ---------------------
#
# A link that 404s is the commonest defect a static site ships, and the one
# nobody notices, because the page it is on still renders.
$broken = @()
foreach ($page in (Get-ServedFiles -Include '*.html')) {
    $html = Get-Content -Raw $page.FullName
    foreach ($m in [regex]::Matches($html, '(?:href|src)="(?<t>[^"]+)"')) {
        $target = $m.Groups['t'].Value
        if ($target -match '^(https?:|mailto:|#|data:)') { continue }
        # `/docs` is staged into the site by the deploy workflow.
        if ($target -eq '/docs' -or $target -eq '/') { continue }
        $relative = ($target -replace '#.*$', '').TrimStart('/')
        if (-not $relative) { continue }
        if (-not (Test-Path (Join-Path $SiteRoot $relative))) {
            $broken += "  {0} -> {1}" -f $page.Name, $target
        }
    }
}
if ($broken) {
    Write-Host 'These links point at files that do not exist:' -ForegroundColor Red
    $broken | Select-Object -Unique | ForEach-Object { Write-Host $_ }
    $failures++
}
else { Write-Host 'Every local link resolves.' }

# --- 2. no placeholder survived generation --------------------------------
#
# `Expand-Template` throws on an unfilled placeholder, so this should never
# fire. It exists because "should never fire" is exactly the claim worth
# having a reader for, and because a `{{key}}` rendered to a visitor is the
# most embarrassing possible defect.
$left = Get-ServedFiles -Include '*.html', '*.txt', '*.xml' |
    Select-String -Pattern '\{\{[A-Za-z0-9_]+\}\}'
if ($left) {
    Write-Host 'Unfilled placeholders reached the output:' -ForegroundColor Red
    $left | ForEach-Object { Write-Host "  $($_.Filename):$($_.LineNumber): $($_.Line.Trim())" }
    $failures++
}
else { Write-Host 'No placeholder survived generation.' }

# --- 3. nothing is loaded from a third party ------------------------------
#
# ADR-0076's central claim. `staticwebapp.config.json` carries a CSP that
# makes the browser enforce it; this makes the build enforce it, so a pasted
# tracker fails here rather than being silently blocked in a console.
#
# What counts as a *request* is the whole difficulty:
#   * `<script src>`, `<img src>`, `<link rel=stylesheet href>`, `@import`   yes
#   * `<a href>`                                          no, it is a link
#   * `<link rel=canonical|alternate href>`               no, it is metadata
$offenders = @()
foreach ($f in (Get-ServedFiles -Include '*.html', '*.css', '*.js')) {
    foreach ($hit in (Select-String -Path $f.FullName -Pattern 'https?://')) {
        $line = $hit.Line
        if ($line -match '<a\s') { continue }
        if ($line -match 'rel="(canonical|alternate)"') { continue }
        if ($line -match '<meta\s') { continue }
        if ($line -match '^\s*(\*|//|#|<!--)') { continue }
        if ($line -match '(?:src|href)\s*=\s*"https?://' -or
            $line -match '@import\s+url\(\s*["'']?https?://' -or
            $line -match 'googletagmanager|google-analytics|plausible\.io|matomo|hotjar|fonts\.googleapis') {
            $offenders += "  {0}:{1}: {2}" -f $f.Name, $hit.LineNumber, $line.Trim()
        }
    }
}
if ($offenders) {
    Write-Host 'ADR-0076 says this site makes no third-party requests. These would:' -ForegroundColor Red
    $offenders | ForEach-Object { Write-Host $_ }
    $failures++
}
else { Write-Host 'No third-party subresources. (A link and a canonical are not requests.)' }

if ($failures -gt 0) { exit 1 }
Write-Host 'site/ passes all three checks.' -ForegroundColor Green
# Explicit, because a script that falls off its end leaves `$LASTEXITCODE` as
# the last native command set it -- and the gate reads it. When the `site`
# stage before this one failed, this printed "passes all three checks" and
# was reported as failed, which turns one red into two.
exit 0
