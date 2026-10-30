[CmdletBinding()]
param([string]$OutputDirectory = '')
$ErrorActionPreference = 'Stop'
$project = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$version = (Get-Content -LiteralPath (Join-Path $project 'package.json') -Raw | ConvertFrom-Json).version
$releasePaths = @((Join-Path $project 'src-tauri\target\x86_64-pc-windows-msvc\release'), (Join-Path $project 'src-tauri\target\release'))
if ($env:CARGO_TARGET_DIR) {
    $releasePaths = @((Join-Path $env:CARGO_TARGET_DIR 'x86_64-pc-windows-msvc\release'), (Join-Path $env:CARGO_TARGET_DIR 'release')) + $releasePaths
}
$release = $releasePaths | Where-Object { Test-Path -LiteralPath (Join-Path $_ 'streamforge.exe') } | Select-Object -First 1
if (-not $release) { throw 'A built streamforge.exe was not found.' }
$output = Join-Path $project 'outputs'
if ($OutputDirectory) { $output = [System.IO.Path]::GetFullPath($OutputDirectory) }
New-Item -ItemType Directory -Path $output -Force | Out-Null
$stage = Join-Path $project ('work\release-' + [guid]::NewGuid().ToString('N'))
$portable = Join-Path $stage 'YouTubeDownloader'
New-Item -ItemType Directory -Path $portable -Force | Out-Null
Copy-Item -LiteralPath (Join-Path $release 'streamforge.exe') -Destination (Join-Path $portable 'YouTubeDownloader.exe')
Copy-Item -LiteralPath (Join-Path $project 'src-tauri\resources\tools') -Destination (Join-Path $portable 'tools') -Recurse
Copy-Item -LiteralPath (Join-Path $project 'THIRD_PARTY_NOTICES.md') -Destination $portable
Copy-Item -LiteralPath (Join-Path $output 'AKO_SPUSTIT.txt') -Destination $portable
$portableZip = Join-Path $stage "YouTubeDownloader_${version}_Windows_x64_portable.zip"
Compress-Archive -LiteralPath $portable -DestinationPath $portableZip -Force
Copy-Item -LiteralPath $portableZip -Destination (Join-Path $output (Split-Path $portableZip -Leaf)) -Force

foreach ($installer in Get-ChildItem -LiteralPath (Join-Path $release 'bundle') -Recurse -File -ErrorAction SilentlyContinue | Where-Object { $_.Name -like "YouTube Downloader_${version}_*" -and ($_.Extension -eq '.msi' -or $_.Name -like '*-setup.exe') }) {
    Copy-Item -LiteralPath $installer.FullName -Destination (Join-Path $output $installer.Name) -Force
}

$source = Join-Path $stage 'YouTubeDownloader-source'
New-Item -ItemType Directory -Path (Join-Path $source 'src-tauri') -Force | Out-Null
foreach ($entry in @('src','scripts','assets','.github','package.json','package-lock.json','index.html','tsconfig.json','tsconfig.node.json','vite.config.ts','.gitignore','README.md','ARCHITECTURE.md','THIRD_PARTY_NOTICES.md')) {
    Copy-Item -LiteralPath (Join-Path $project $entry) -Destination $source -Recurse
}
foreach ($entry in @('src','icons','capabilities','nsis','Cargo.toml','Cargo.lock','build.rs','tauri.conf.json')) {
    Copy-Item -LiteralPath (Join-Path $project "src-tauri\$entry") -Destination (Join-Path $source 'src-tauri') -Recurse
}
New-Item -ItemType Directory -Path (Join-Path $source 'src-tauri\resources\tools') -Force | Out-Null
Copy-Item -LiteralPath (Join-Path $project 'src-tauri\resources\tools\README.txt') -Destination (Join-Path $source 'src-tauri\resources\tools')
New-Item -ItemType Directory -Path (Join-Path $source 'outputs') -Force | Out-Null
Copy-Item -LiteralPath (Join-Path $output 'AKO_SPUSTIT.txt') -Destination (Join-Path $source 'outputs')
$sourceZip = Join-Path $stage "YouTubeDownloader_${version}_source.zip"
Compress-Archive -LiteralPath $source -DestinationPath $sourceZip -Force
Copy-Item -LiteralPath $sourceZip -Destination (Join-Path $output (Split-Path $sourceZip -Leaf)) -Force
Get-ChildItem -LiteralPath $output -File | Select-Object Name,Length
