<#
.SYNOPSIS
    Build the localised website from one template and one strings file per
    language, or check that what is committed still matches.

.DESCRIPTION
    Six locales times two pages is twelve HTML files, and three facts have to
    agree across all twelve: the `hreflang` alternates, the canonical URLs,
    and the language switcher. **That is exactly the fact that goes stale** --
    a locale added by hand reaches eleven of the twelve, and the one it misses
    is invisible until a search engine reports it.

    So the pages are generated, the same way `docs/generated/` is
    (ADR-0075), and `-Check` is what notices.

    **Per-language sitemaps**, indexed by `sitemap.xml`. One sitemap per
    locale is what lets a language be submitted, diagnosed and re-crawled on
    its own rather than as a slice of one large file.

    **No third-party requests are introduced** (ADR-0076). Everything here is
    markup: `hreflang`, canonicals, JSON-LD, Open Graph. Structured data is
    inline, so the CSP of `self` still holds.

.PARAMETER Check
    Generate into a temporary directory and compare rather than writing.
#>
[CmdletBinding()]
param([switch]$Check)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$Root = Split-Path -Parent $PSScriptRoot
. (Join-Path $PSScriptRoot 'DocsCommon.ps1')

$SiteSource = Join-Path $Root 'site'
$Out = if ($Check) {
    Join-Path ([System.IO.Path]::GetTempPath()) ("bpad-site-" + [guid]::NewGuid())
}
else { $SiteSource }

$Origin = 'https://bpad.prompt-forge.dev'
$Repo = 'https://github.com/dboles99/bachelorpluslite'

# The donation links, which af-site already established.
#
# `paypal.me/DBoles648` with a preset amount, exactly as
# af-site/donate.html does it, so the two sites on this domain behave the
# same way. Preset amounts because "how much?" is a question that loses
# donations; three buttons answer it.
#
# **Links, never PayPal's button script.** Their SDK is a third-party script
# and this site makes no third-party requests (ADR-0076); the CSP of `self`
# would block it. A plain anchor costs nothing until somebody clicks it.
$PayPalBase = 'https://www.paypal.com/paypalme/DBoles648'
$PayPalAmounts = @('5USD', '10USD', '25USD')

# The locale table. `Dir` empty means the root, which is en-GB: a visitor with
# no language preference gets British English, and `x-default` points there.
$locales = @(
    # `Dir` matches af-site's shape on the same domain: en-GB at the root,
    # then us/, fr/, de/, ja/. `Slug` names the sitemap, also as af-site does.
    @{ Code = 'en-GB'; Dir = ''; Slug = 'en' }
    @{ Code = 'en-US'; Dir = 'us'; Slug = 'us' }
    @{ Code = 'fr-FR'; Dir = 'fr'; Slug = 'fr' }
    @{ Code = 'de-DE'; Dir = 'de'; Slug = 'de' }
    @{ Code = 'ja-JP'; Dir = 'ja'; Slug = 'ja' }
    @{ Code = 'zh-TW'; Dir = 'zh-tw'; Slug = 'zh-tw' }
)

$strings = @{}
foreach ($l in $locales) {
    $path = Join-Path $SiteSource "_locales/$($l.Code).json"
    if (-not (Test-Path $path)) { throw "$path is missing -- the locale table names it" }
    $strings[$l.Code] = Get-Content -Raw -Path $path | ConvertFrom-Json
}

function Get-Url {
    param([string]$Dir, [string]$Page)
    $prefix = if ($Dir) { "$Origin/$Dir" } else { $Origin }
    if ($Page -eq 'index') { return "$prefix/" }
    return "$prefix/$Page.html"
}

# One `hreflang` block, built once per page and shared by every locale of it.
# Reciprocal and complete by construction: a locale that is in the table is in
# every page's alternates, and one that is not is in none of them.
function Get-Alternates {
    param([string]$Page)
    $lines = foreach ($l in $locales) {
        '<link rel="alternate" hreflang="{0}" href="{1}">' -f $l.Code, (Get-Url -Dir $l.Dir -Page $Page)
    }
    $lines += '<link rel="alternate" hreflang="x-default" href="{0}">' -f (Get-Url -Dir '' -Page $Page)
    return ($lines -join "`n")
}

function Get-Switcher {
    param([string]$Page, [string]$Current)
    $items = foreach ($l in $locales) {
        $url = Get-Url -Dir $l.Dir -Page $Page
        $name = $strings[$l.Code].nativeName
        if ($l.Code -eq $Current) {
            '<span class="lang-current" aria-current="true">{0}</span>' -f $name
        }
        else {
            '<a href="{0}" hreflang="{1}" lang="{1}">{2}</a>' -f $url, $l.Code, $name
        }
    }
    return ($items -join "`n      ")
}

# --- the templates -------------------------------------------------------
#
# Placeholders are `{{key}}` and substituted by regex, never by `-f`: the
# JSON-LD below is full of literal braces and the format operator reads every
# one of them as a placeholder.

$indexTemplate = Get-Content -Raw -Path (Join-Path $SiteSource '_templates/index.html')
$waitlistTemplate = Get-Content -Raw -Path (Join-Path $SiteSource '_templates/waitlist.html')
$privacyTemplate = Get-Content -Raw -Path (Join-Path $SiteSource '_templates/privacy.html')

function Expand-Template {
    param(
        [Parameter(Mandatory)][string]$Template,
        [Parameter(Mandatory)][hashtable]$Values
    )
    $out = $Template
    # Longest keys first, so `{{wf1H}}` is not eaten by a shorter prefix.
    foreach ($key in ($Values.Keys | Sort-Object -Property Length -Descending)) {
        $out = $out.Replace("{{$key}}", [string]$Values[$key])
    }
    $left = [regex]::Matches($out, '\{\{(?<k>[A-Za-z0-9_]+)\}\}') | ForEach-Object { $_.Groups['k'].Value } | Sort-Object -Unique
    if ($left) { throw "unfilled placeholders: $($left -join ', ')" }
    return $out
}

$pages = @()
$generated = New-Object System.Collections.Generic.List[string]

foreach ($l in $locales) {
    $s = $strings[$l.Code]
    $dir = $l.Dir
    $homePath = if ($dir) { "/$dir/" } else { '/' }
    $assets = '/'   # styles, icons and the script are shared from the root

    foreach ($page in 'index', 'waitlist', 'privacy') {
        $values = @{}
        foreach ($p in $s.PSObject.Properties) { $values[$p.Name] = $p.Value }

        $values['home'] = $homePath
        $values['assets'] = $assets
        $values['repo'] = $Repo
        $values['paypal5'] = "$PayPalBase/$($PayPalAmounts[0])"
        $values['paypal10'] = "$PayPalBase/$($PayPalAmounts[1])"
        $values['paypal25'] = "$PayPalBase/$($PayPalAmounts[2])"
        $values['origin'] = $Origin
        $values['canonical'] = Get-Url -Dir $dir -Page $page
        $values['alternates'] = Get-Alternates -Page $page
        $values['switcher'] = Get-Switcher -Page $page -Current $l.Code
        $values['otherPage'] = if ($page -eq 'index') { "$homePath" + 'waitlist.html' } else { $homePath }
        $values['year'] = (Get-Date).Year

        # An attribute-safe copy of every string, as `{{keyAttr}}`.
        #
        # The messages the script shows are HTML fragments -- `jsFallback`
        # carries an `<a href="...">` -- and they are handed to the page
        # through `data-` attributes. A double quote inside one of those ends
        # the attribute, and the rest of the string is then rendered as
        # visible text: the waitlist page displayed
        # `github.com/.../issues, or try again later.">` above the form.
        #
        # Only the quote is escaped. These values already contain entities
        # like `&amp;`, so escaping `&` again would double-encode them, and
        # the attribute is double-quoted so a bare `<` is harmless.
        foreach ($key in @($values.Keys)) {
            $v = $values[$key]
            if ($v -is [string]) { $values["${key}Attr"] = $v.Replace('"', '&quot;') }
        }

        # `{{wFooter}}` itself contains `{{home}}`, so it is expanded twice.
        if ($values.ContainsKey('wFooter')) {
            $values['wFooter'] = $values['wFooter'].Replace('{{home}}', $homePath)
        }

        $template = switch ($page) {
            'index' { $indexTemplate }
            'waitlist' { $waitlistTemplate }
            'privacy' { $privacyTemplate }
        }

        $html = Expand-Template -Template $template -Values $values

        $relative = if ($dir) { "$dir/$page.html" } else { "$page.html" }
        Write-Generated -Path (Join-Path $Out $relative) -Content $html
        $generated.Add($relative)

        $pages += [pscustomobject]@{
            Locale = $l.Code
            Dir    = $dir
            Page   = $page
            Url    = $values['canonical']
        }
    }
}

# --- llms.txt, one per language ------------------------------------------

foreach ($l in $locales) {
    $t = $strings[$l.Code]
    $dir = $l.Dir
    $pageUrl = Get-Url -Dir $dir -Page 'index'
    $waitUrl = Get-Url -Dir $dir -Page 'waitlist'

    # Markdown-ish plain prose, because that is what these files are read as.
    # HTML entities would be quoted literally, so they are undone here.
    $plain = {
        param([string]$Text)
        ($Text -replace '<[^>]+>', '') -replace '&amp;', '&' -replace '&mdash;', ', '
    }

    $llms = @"
# BachelorPad+ Lite

> $pageUrl

## What this is

$(& $plain $t.lede)

Free and open source under GPL-3.0-only. Windows 10, Windows 11 and Linux,
64-bit. There is no macOS build. Version 0.9.5.

Source: $Repo
Downloads: $Repo/releases/latest

## What it does

- $(& $plain $t.card1H). $(& $plain $t.card1P)
- $(& $plain $t.card2H). $(& $plain $t.card2P)
- $(& $plain $t.card3H). $(& $plain $t.card3P)
- $(& $plain $t.note1H) $(& $plain $t.note1P)
- $(& $plain $t.note2H). $(& $plain $t.note2P)

## What it refuses to do, stated exactly

- **It executes nothing.** No scripts, macros, plugins or interpreters.
  Opening a document cannot run anything.
- **It makes no network connection.** No telemetry, no update check, no crash
  reporting, no AI, no account. It behaves identically offline.
- **It launches no other program**, not even a browser for its own help.
- **It will not seize a file association** or delete anything from a user
  profile without showing what it would do first.

## What it records on the user's machine

$(& $plain $t.privacyNote)

- $(& $plain $t.profStandard): recovery file $(& $plain $t.journalOn); records $(& $plain $t.recordedFull).
- $(& $plain $t.profPrivate): recovery file $(& $plain $t.journalOn); records $(& $plain $t.recordedPath).
- $(& $plain $t.profConfidential): no recovery file; records nothing.
- $(& $plain $t.profMaximum): no recovery file; records nothing.

Nothing is encrypted in this edition, and the program says so rather than
implying otherwise.

## Downloads, and one thing to tell people

The archives are **unsigned**. Windows SmartScreen warns on first run. A
code-signing certificate has not been bought, and self-signing was refused
because it would ask a user to install a root certificate they have no reason
to trust. Every release ships a SHA-256 checksum file.

There is no installer. Unpack the archive anywhere and run it.

## The paid version

BachelorPad+ is the full version: encrypted notes, runnable notebooks,
structured-data tools, clipboard history and complete privacy controls. It
arrives on 10 October 2026. The price is not set. Lite stays free either way.

Waitlist: $waitUrl

## Licence

GPL-3.0-only, not -or-later. The user interface is built with Slint, whose
grant is to version 3 and no other, so "or later" would offer terms this
project has not been granted.
"@

    $relative = if ($dir) { "$dir/llms.txt" } else { 'llms.txt' }
    Write-Generated -Path (Join-Path $Out $relative) -Content $llms
    $generated.Add($relative)
}

# --- one sitemap per language, plus the index ----------------------------
#
# Asked for explicitly, and it is the right shape: a per-language sitemap can
# be submitted, diagnosed and re-crawled on its own. A single file mixing six
# languages reports one coverage number for all of them.

$today = (Get-Date).ToString('yyyy-MM-dd')
$sitemapNames = @()

foreach ($l in $locales) {
    $name = "sitemap-{0}.xml" -f $l.Slug
    $sitemapNames += $name

    $urls = foreach ($p in ($pages | Where-Object { $_.Locale -eq $l.Code })) {
        $alt = foreach ($a in $locales) {
            '    <xhtml:link rel="alternate" hreflang="{0}" href="{1}"/>' -f $a.Code, (Get-Url -Dir $a.Dir -Page $p.Page)
        }
        $alt += '    <xhtml:link rel="alternate" hreflang="x-default" href="{0}"/>' -f (Get-Url -Dir '' -Page $p.Page)
        $priority = switch ($p.Page) {
            'index' { '1.0' }
            'waitlist' { '0.8' }
            default { '0.3' }
        }
        @"
  <url>
    <loc>$($p.Url)</loc>
    <lastmod>$today</lastmod>
    <changefreq>weekly</changefreq>
    <priority>$priority</priority>
$($alt -join "`n")
  </url>
"@
    }

    $sitemap = @"
<?xml version="1.0" encoding="UTF-8"?>
<!-- $($l.Code). One sitemap per language (see scripts/Build-Site.ps1). -->
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9"
        xmlns:xhtml="http://www.w3.org/1999/xhtml">
$($urls -join "`n")
</urlset>
"@
    Write-Generated -Path (Join-Path $Out $name) -Content $sitemap
    $generated.Add($name)
}

$indexEntries = foreach ($name in $sitemapNames) {
    @"
  <sitemap>
    <loc>$Origin/$name</loc>
    <lastmod>$today</lastmod>
  </sitemap>
"@
}

$sitemapIndex = @"
<?xml version="1.0" encoding="UTF-8"?>
<!-- The index. Submit this one to Google Search Console and Bing Webmaster
     Tools; it names the per-language sitemaps, which is what lets a single
     language be diagnosed without the others in the way. -->
<sitemapindex xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
$($indexEntries -join "`n")
</sitemapindex>
"@
Write-Generated -Path (Join-Path $Out 'sitemap.xml') -Content $sitemapIndex
$generated.Add('sitemap.xml')

$llmsLines = foreach ($l in $locales) {
    $u = if ($l.Dir) { "$Origin/$($l.Dir)/llms.txt" } else { "$Origin/llms.txt" }
    "#   {0,-42} {1}" -f $u, $strings[$l.Code].nativeName
}

$robots = @"
# BachelorPad+ Lite -- $Origin
#
# Everything here is public and free software (GPL-3.0-only). Crawl it. There
# is no admin area, no accounts and nothing behind a paywall, so there is
# nothing to disallow, and blocking crawlers "just in case" is how a site
# becomes invisible.

User-agent: *
Allow: /

# Named explicitly as well. "User-agent: *" already permits these, but several
# look for their own token first, and being explicit says the permission is
# deliberate rather than an oversight.
User-agent: bingbot
Allow: /

User-agent: GPTBot
Allow: /

User-agent: ClaudeBot
Allow: /

User-agent: PerplexityBot
Allow: /

User-agent: Google-Extended
Allow: /

User-agent: Applebot-Extended
Allow: /

# Per-language as well as the index, because Bing has historically been
# happier with explicit sitemaps than with an index alone.
Sitemap: $Origin/sitemap.xml
$(($sitemapNames | ForEach-Object { "Sitemap: $Origin/$_" }) -join "`n")

# Written for machines that read prose rather than markup. One per language,
# because an assistant answering in Japanese quotes what it can extract, and
# translating a fact at quote time is where it goes wrong.
#
$($llmsLines -join "`n")
"@
Write-Generated -Path (Join-Path $Out 'robots.txt') -Content $robots
$generated.Add('robots.txt')

$manifestPath = Join-Path $SiteSource '.generated'

if (-not $Check) {
    # Sweep first, write second, so a file that has been renamed does not
    # survive as a copy of itself under the old name.
    if (Test-Path $manifestPath) {
        $previous = @(Get-Content $manifestPath | Where-Object { $_ -and -not $_.StartsWith('#') })
        $current = @($generated | ForEach-Object { $_ })
        foreach ($stale in ($previous | Where-Object { $_ -notin $current })) {
            $path = Join-Path $SiteSource $stale
            if (Test-Path $path) {
                Remove-Item $path -Force
                Write-Host "    removed stale $stale"
            }
        }
    }
    Set-Content -Path $manifestPath -Encoding utf8 -Value (@(
            '# Written by scripts/Build-Site.ps1. Every file below is generated;',
            '# anything listed here and no longer produced is deleted on the next run.',
            '# Do not edit.'
        ) + $generated)
}

if ($Check) {
    # Only the generated files are compared; the sources beside them are not.
    $expected = Get-ChildItem $Out -Recurse -File
    $problems = @()
    foreach ($f in $expected) {
        $rel = $f.FullName.Substring($Out.Length).TrimStart('\', '/')
        $actual = Join-Path $SiteSource $rel
        if (-not (Test-Path $actual)) { $problems += "  missing:  $rel"; continue }
        $a = ([System.IO.File]::ReadAllText($f.FullName)) -replace "`r`n", "`n"
        $b = ([System.IO.File]::ReadAllText($actual)) -replace "`r`n", "`n"
        if ($a -ne $b) { $problems += "  stale:    $rel" }
    }
    Remove-Item $Out -Recurse -Force -ErrorAction SilentlyContinue
    if (Test-Path $manifestPath) {
        $listed = @(Get-Content $manifestPath | Where-Object { $_ -and -not $_.StartsWith('#') })
        $produced = @($generated | ForEach-Object { $_ })
        foreach ($extra in ($listed | Where-Object { $_ -notin $produced })) {
            $problems += "  stale:    $extra  (generated once, no longer produced)"
        }
    }

    if ($problems) {
        Write-Host 'site/ has drifted from its templates and locale files:' -ForegroundColor Red
        $problems | ForEach-Object { Write-Host $_ }
        Write-Host 'Regenerate with: ./scripts/Build-Site.ps1' -ForegroundColor Yellow
        exit 1
    }
    Write-Host "site/ is current ($($expected.Count) generated files)."
    exit 0
}

Write-Host "wrote site/ ($($pages.Count) pages across $($locales.Count) locales, $($sitemapNames.Count) sitemaps plus the index)"
