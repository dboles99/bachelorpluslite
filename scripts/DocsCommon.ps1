<#
    Shared by the three documentation generators (ADR-0075).

    Dot-sourced rather than duplicated: `Build-Docs.ps1`, `Build-AppHelp.ps1`
    and `Sync-GitHubWiki.ps1` all write a tree and all need to compare one,
    and three copies of a comparison is three chances for `-Check` to be
    wrong in a way that reports success.

    Everything here is a function. Nothing runs on dot-source.
#>

Set-StrictMode -Version Latest

# --- writing --------------------------------------------------------------

<#
    One line ending and one trailing newline, everywhere.

    Not tidiness: `-Check` compares text, and a generator that emitted CRLF on
    Windows and LF on Linux would report drift on every run of the other
    platform's CI -- a check that fails for a reason unrelated to the thing it
    is checking is a check people learn to ignore.
#>
function Write-Generated {
    param(
        [Parameter(Mandatory)][string]$Path,
        [Parameter(Mandatory)][string]$Content
    )
    $dir = Split-Path -Parent $Path
    if (-not (Test-Path $dir)) { New-Item -ItemType Directory -Force -Path $dir | Out-Null }
    $normalised = ($Content -replace "`r`n", "`n").TrimEnd() + "`n"
    [System.IO.File]::WriteAllText($Path, $normalised, [System.Text.UTF8Encoding]::new($false))
}

<#
    Compare a freshly generated tree against the committed one.

    Reports **what** differs rather than only that something does. A staleness
    check whose whole output is "stale" sends somebody to `git diff` to find
    out what it meant, which is a step it could have taken for them.
#>
function Compare-GeneratedTree {
    param(
        [Parameter(Mandatory)][string]$Expected,
        [Parameter(Mandatory)][string]$Actual,
        [Parameter(Mandatory)][string]$Name
    )

    if (-not (Test-Path $Actual)) {
        Write-Host "$Name does not exist. Run the generator without -Check." -ForegroundColor Red
        return $false
    }

    $relative = {
        param($root)
        Get-ChildItem -Path $root -Recurse -File |
            ForEach-Object { $_.FullName.Substring($root.Length).TrimStart('\', '/') -replace '\\', '/' } |
            Sort-Object
    }

    $want = @(& $relative $Expected)
    $have = @(& $relative $Actual)

    $problems = New-Object System.Collections.Generic.List[string]

    foreach ($file in $want | Where-Object { $_ -notin $have }) {
        $problems.Add("  missing:  $file")
    }
    foreach ($file in $have | Where-Object { $_ -notin $want }) {
        $problems.Add("  extra:    $file  (the source it came from is gone)")
    }
    foreach ($file in $want | Where-Object { $_ -in $have }) {
        $a = [System.IO.File]::ReadAllText((Join-Path $Expected $file)) -replace "`r`n", "`n"
        $b = [System.IO.File]::ReadAllText((Join-Path $Actual $file)) -replace "`r`n", "`n"
        if ($a -ne $b) { $problems.Add("  stale:    $file") }
    }

    if ($problems.Count -eq 0) {
        Write-Host "$Name is current ($($want.Count) files)."
        return $true
    }

    Write-Host "$Name has drifted from docs/:" -ForegroundColor Red
    $problems | ForEach-Object { Write-Host $_ }
    Write-Host ''
    Write-Host 'docs/ is the source and these are generated from it (ADR-0075).' -ForegroundColor Yellow
    Write-Host 'Regenerate rather than editing them:' -ForegroundColor Yellow
    Write-Host '  ./scripts/Build-Docs.ps1; ./scripts/Build-AppHelp.ps1; ./scripts/Sync-GitHubWiki.ps1'
    return $false
}

# --- reading the facts that live in code ----------------------------------

<#
    The keyboard shortcuts, out of the constant Help > Keyboard Shortcuts
    shows.

    Parsed rather than retyped, so the dialog and the documentation are the
    same string. If the constant moves or changes shape this throws, which is
    the correct outcome -- a generator that silently produced an empty table
    would put an empty table in the manual.
#>
function Read-ShortcutsFromSource {
    param([Parameter(Mandatory)][string]$Root)

    $path = Join-Path $Root 'crates/bp-ui/src/dispatch.rs'
    $source = (Get-Content -Raw -Path $path) -replace "`r`n", "`n"

    # `const SHORTCUTS: &str = "\` then the lines, then `";`. The trailing
    # backslash is Rust's line continuation and is not part of the value.
    if ($source -notmatch '(?s)const SHORTCUTS: &str = "\\\n(?<body>.*?)";') {
        throw "could not find SHORTCUTS in $path -- it has moved or changed shape"
    }
    $body = $Matches.body.Trim()
    if ([string]::IsNullOrWhiteSpace($body)) { throw "SHORTCUTS in $path parsed as empty" }
    return $body
}

<#
    The flag list, out of `bp_config::cli::FLAGS`.

    Only for `-SkipBinary`. Asking the binary is the better answer -- a
    manifest can be edited without a rebuild and a binary cannot -- so this is
    the fallback for a machine with no toolchain rather than the default.
#>
function Read-FlagsFromSource {
    param([Parameter(Mandatory)][string]$Root)

    $path = Join-Path $Root 'crates/bp-config/src/cli.rs'
    $source = (Get-Content -Raw -Path $path) -replace "`r`n", "`n"

    $matched = [regex]::Matches(
        $source,
        '(?s)Flag \{\s*names: &\[(?<names>[^\]]*)\],\s*value: (?<value>None|Some\("(?<val>[^"]*)"\)),\s*summary: "(?<summary>[^"]*)"')
    if ($matched.Count -eq 0) { throw "could not find FLAGS in $path -- it has moved or changed shape" }

    $lines = foreach ($m in $matched) {
        $names = ($m.Groups['names'].Value -split ',' |
            ForEach-Object { $_.Trim().Trim('"') } |
            Where-Object { $_ } |
            ForEach-Object { if ($_.Length -eq 1) { "-$_" } else { "--$_" } }) -join ', '
        $value = if ($m.Groups['value'].Value -eq 'None') { '' } else { ' ' + $m.Groups['val'].Value }
        '  {0,-34} {1}' -f ($names + $value), $m.Groups['summary'].Value
    }
    return ($lines -join "`n")
}

<#
    Every menu and its table, out of docs/product/MENU_MAP.md.

    That file owns every menu row and its state, and it is written for
    somebody working on the product: the prose around each table is about why
    rows were removed and which ADR did it. **Only the tables are lifted**,
    because the tables are the user-facing half and the prose is not.
#>
function Read-MenuTables {
    param([Parameter(Mandatory)][string]$Path)

    $lines = (Get-Content -Raw -Path $Path) -replace "`r`n", "`n" -split "`n"

    $out = New-Object System.Collections.Generic.List[string]
    $section = $null
    $inTable = $false
    $found = 0

    foreach ($line in $lines) {
        if ($line -match '^## (?<name>.+)$') {
            $section = $Matches.name.Trim()
            $inTable = $false
            continue
        }
        if ($null -eq $section) { continue }

        if ($line -match '^\|') {
            if (-not $inTable) {
                $out.Add("## $section")
                $out.Add('')
                $inTable = $true
                $found++
            }
            $out.Add($line.TrimEnd())
            continue
        }
        if ($inTable -and [string]::IsNullOrWhiteSpace($line)) {
            # One table per section. The later ones in a section are examples
            # rather than the menu.
            $out.Add('')
            $section = $null
            $inTable = $false
        }
    }

    if ($found -eq 0) { throw "no menu tables found in $Path -- it has changed shape" }
    return ($out -join "`n").TrimEnd()
}

<#
    Resolve a relative Markdown link against the page it appears in, and
    return the path from the repository root.

    Written because stripping leading `../` is wrong and looks right. A link
    to `../decisions/ADR-0006.md` inside `docs/user/03-editing.md` resolves to
    `docs/decisions/ADR-0006.md`; strip the prefix and you get
    `decisions/ADR-0006.md`, which is a URL that 404s -- and there are
    hundreds of ADR references in these pages, so it would have been the
    single largest defect the wiki generator could ship.
#>
function Resolve-RepoRelative {
    param(
        [Parameter(Mandatory)][string]$From,
        [Parameter(Mandatory)][string]$Target
    )
    $dir = (Split-Path -Parent ($From -replace '\\', '/')) -replace '\\', '/'
    $combined = if ($dir) { "$dir/$Target" } else { $Target }

    $out = New-Object System.Collections.Generic.List[string]
    foreach ($segment in ($combined -split '/')) {
        if ($segment -eq '' -or $segment -eq '.') { continue }
        if ($segment -eq '..') {
            if ($out.Count -gt 0) { $out.RemoveAt($out.Count - 1) }
            continue
        }
        $out.Add($segment)
    }
    return ($out -join '/')
}

<#
    A GitHub-style anchor for a heading, for the manual's table of contents.
#>
function ConvertTo-Anchor {
    param([Parameter(Mandatory)][string]$Text)
    $a = $Text.ToLowerInvariant()
    $a = $a -replace '[^a-z0-9 \-]', ''
    return ($a.Trim() -replace ' +', '-')
}
