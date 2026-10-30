[CmdletBinding()]
param(
    [switch]$SkipToolDownload,
    [switch]$NsisOnly
)

$ErrorActionPreference = 'Stop'
$projectRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
Set-Location -LiteralPath $projectRoot

if (-not $SkipToolDownload) {
    & (Join-Path $PSScriptRoot 'prepare-tools.ps1')
}

if (-not (Test-Path -LiteralPath (Join-Path $projectRoot 'node_modules'))) {
    npm install
    if ($LASTEXITCODE -ne 0) { throw 'Frontend dependency installation failed.' }
}

npm run icon
if ($LASTEXITCODE -ne 0) { throw 'Application icon generation failed.' }

if ($NsisOnly) {
    npm run tauri:build -- --bundles nsis
} else {
    npm run tauri:build
}
if ($LASTEXITCODE -ne 0) { throw 'The Windows release build failed.' }

Write-Host 'Installers are in src-tauri\target\release\bundle\.'
