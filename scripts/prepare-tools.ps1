[CmdletBinding()]
param(
    [string]$TargetDirectory = (Join-Path $PSScriptRoot '..\src-tauri\resources\tools'),
    [switch]$SkipFfmpeg
)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
$target = [System.IO.Path]::GetFullPath($TargetDirectory)
$tempRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("streamforge-tools-" + [guid]::NewGuid().ToString('N'))

function Get-GitHubLatestRelease([string]$Repository) {
    Invoke-RestMethod -Headers @{ 'User-Agent' = 'StreamForge-build' } -Uri "https://api.github.com/repos/$Repository/releases/latest"
}

function Get-ExpectedHash([string]$ChecksumFile, [string]$FileName) {
    $matching = Get-Content -LiteralPath $ChecksumFile | Where-Object { $_ -match ([regex]::Escape($FileName) + '$') -and $_ -match '(?i)[a-f0-9]{64}' } | Select-Object -First 1
    if (-not $matching) {
        $matching = Get-Content -LiteralPath $ChecksumFile -Raw
    }
    if ($matching -notmatch '(?i)([a-f0-9]{64})') {
        throw "Checksum for $FileName was not found."
    }
    $Matches[1].ToLowerInvariant()
}

function Save-VerifiedFile([string]$Url, [string]$ChecksumUrl, [string]$Destination, [string]$ChecksumName) {
    $checksumFile = "$Destination.sha256"
    Invoke-WebRequest -UseBasicParsing -Uri $Url -OutFile $Destination
    Invoke-WebRequest -UseBasicParsing -Uri $ChecksumUrl -OutFile $checksumFile
    $expected = Get-ExpectedHash $checksumFile $ChecksumName
    $actual = (Get-FileHash -LiteralPath $Destination -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actual -ne $expected) {
        throw "SHA-256 mismatch for $ChecksumName. Expected $expected, got $actual."
    }
}

try {
    New-Item -ItemType Directory -Path $target -Force | Out-Null
    New-Item -ItemType Directory -Path $tempRoot -Force | Out-Null

    Write-Host 'Downloading official yt-dlp Windows executable...'
    $ytRelease = Get-GitHubLatestRelease 'yt-dlp/yt-dlp'
    $ytAsset = $ytRelease.assets | Where-Object name -eq 'yt-dlp.exe' | Select-Object -First 1
    $ytChecksums = $ytRelease.assets | Where-Object name -eq 'SHA2-256SUMS' | Select-Object -First 1
    if (-not $ytAsset -or -not $ytChecksums) { throw 'Required yt-dlp release assets were not found.' }
    $ytTemp = Join-Path $tempRoot 'yt-dlp.exe'
    Save-VerifiedFile $ytAsset.browser_download_url $ytChecksums.browser_download_url $ytTemp 'yt-dlp.exe'
    Copy-Item -LiteralPath $ytTemp -Destination (Join-Path $target 'yt-dlp.exe') -Force

    Write-Host 'Downloading official Deno Windows runtime...'
    $denoRelease = Get-GitHubLatestRelease 'denoland/deno'
    $denoZipName = 'deno-x86_64-pc-windows-msvc.zip'
    $denoAsset = $denoRelease.assets | Where-Object name -eq $denoZipName | Select-Object -First 1
    $denoChecksum = $denoRelease.assets | Where-Object name -eq "$denoZipName.sha256sum" | Select-Object -First 1
    if (-not $denoAsset -or -not $denoChecksum) { throw 'Required Deno release assets were not found.' }
    $denoZip = Join-Path $tempRoot $denoZipName
    Save-VerifiedFile $denoAsset.browser_download_url $denoChecksum.browser_download_url $denoZip $denoZipName
    $denoExtract = Join-Path $tempRoot 'deno'
    Expand-Archive -LiteralPath $denoZip -DestinationPath $denoExtract -Force
    Copy-Item -LiteralPath (Join-Path $denoExtract 'deno.exe') -Destination (Join-Path $target 'deno.exe') -Force

    if (-not $SkipFfmpeg) {
        Write-Host 'Downloading the verified Gyan FFmpeg essentials build...'
        $ffRelease = Get-GitHubLatestRelease 'GyanD/codexffmpeg'
        $ffAsset = $ffRelease.assets | Where-Object name -match '^ffmpeg-.+-essentials_build\.zip$' | Select-Object -First 1
        if (-not $ffAsset -or $ffAsset.digest -notmatch '^sha256:([a-f0-9]{64})$') { throw 'Verified FFmpeg release asset was not found.' }
        $ffExpectedHash = $Matches[1]
        $ffmpegZip = Join-Path $tempRoot 'ffmpeg-release-essentials.zip'
        Invoke-WebRequest -UseBasicParsing -Uri $ffAsset.browser_download_url -OutFile $ffmpegZip -TimeoutSec 120
        if ((Get-FileHash -LiteralPath $ffmpegZip -Algorithm SHA256).Hash.ToLowerInvariant() -ne $ffExpectedHash) { throw 'FFmpeg SHA-256 mismatch.' }
        $ffmpegExtract = Join-Path $tempRoot 'ffmpeg'
        Expand-Archive -LiteralPath $ffmpegZip -DestinationPath $ffmpegExtract -Force
        $ffmpegExe = Get-ChildItem -LiteralPath $ffmpegExtract -Recurse -File -Filter 'ffmpeg.exe' | Select-Object -First 1
        $ffprobeExe = Get-ChildItem -LiteralPath $ffmpegExtract -Recurse -File -Filter 'ffprobe.exe' | Select-Object -First 1
        if (-not $ffmpegExe -or -not $ffprobeExe) { throw 'FFmpeg archive did not contain ffmpeg.exe and ffprobe.exe.' }
        Copy-Item -LiteralPath $ffmpegExe.FullName -Destination (Join-Path $target 'ffmpeg.exe') -Force
        Copy-Item -LiteralPath $ffprobeExe.FullName -Destination (Join-Path $target 'ffprobe.exe') -Force
        $ffRoot = Split-Path -Parent (Split-Path -Parent $ffmpegExe.FullName)
        $licenseTarget = Join-Path $target 'licenses\ffmpeg'
        New-Item -ItemType Directory -Path $licenseTarget -Force | Out-Null
        Get-ChildItem -LiteralPath $ffRoot -File | ForEach-Object { Copy-Item -LiteralPath $_.FullName -Destination $licenseTarget -Force }
    }

    $licenses = Join-Path $target 'licenses'
    New-Item -ItemType Directory -Path $licenses -Force | Out-Null
    Invoke-WebRequest -Uri "https://raw.githubusercontent.com/yt-dlp/yt-dlp/$($ytRelease.tag_name)/LICENSE" -OutFile (Join-Path $licenses 'yt-dlp-LICENSE.txt') -TimeoutSec 60
    Invoke-WebRequest -Uri "https://raw.githubusercontent.com/yt-dlp/yt-dlp/$($ytRelease.tag_name)/THIRD_PARTY_LICENSES.txt" -OutFile (Join-Path $licenses 'yt-dlp-THIRD_PARTY_LICENSES.txt') -TimeoutSec 60
    Invoke-WebRequest -Uri "https://raw.githubusercontent.com/denoland/deno/$($denoRelease.tag_name)/LICENSE.md" -OutFile (Join-Path $licenses 'deno-LICENSE.md') -TimeoutSec 60

    Write-Host "Tools prepared in $target"
    & (Join-Path $target 'yt-dlp.exe') --version
    & (Join-Path $target 'deno.exe') --version
    if (-not $SkipFfmpeg) {
        try { & (Join-Path $target 'ffmpeg.exe') -version | Select-Object -First 1 }
        catch { Write-Warning "FFmpeg was downloaded and verified, but Windows refused to launch it: $($_.Exception.Message)" }
    }
}
finally {
    $resolvedTemp = [System.IO.Path]::GetFullPath($tempRoot)
    $systemTemp = [System.IO.Path]::GetFullPath([System.IO.Path]::GetTempPath())
    if ($resolvedTemp.StartsWith($systemTemp, [System.StringComparison]::OrdinalIgnoreCase) -and (Test-Path -LiteralPath $resolvedTemp)) {
        Remove-Item -LiteralPath $resolvedTemp -Recurse -Force
    }
}
