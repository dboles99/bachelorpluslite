[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Project,
    [Parameter(Mandatory)][string]$Task,
    [Parameter(Mandatory)][string]$ArtifactPath,
    [string]$Step,
    [string]$Rosetta,
    [string]$Status = "created"
)

$ErrorActionPreference = 'Stop'
$Root = Split-Path -Parent $PSScriptRoot
$resolved = Resolve-Path $ArtifactPath
$hash = (Get-FileHash -Algorithm SHA256 $resolved).Hash.ToLowerInvariant()
$id = [guid]::NewGuid().ToString()

$record = [ordered]@{
    artifact_id = $id
    project = $Project
    task = $Task
    step = $Step
    rosetta = $Rosetta
    created_at = (Get-Date).ToString("o")
    path = $resolved.Path
    sha256 = $hash
    status = $Status
    notes = $null
}

$out = Join-Path $Root "metadata\$id.json"
$record | ConvertTo-Json -Depth 10 | Set-Content -Encoding utf8 $out
Write-Host $out
