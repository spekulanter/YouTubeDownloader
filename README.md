# YouTube Downloader

YouTube Downloader is a modern Windows 11 desktop wrapper around `yt-dlp`. It uses Tauri 2, Rust, React, TypeScript and Vite. It does not contain a custom YouTube extractor. This is an independent application, not affiliated with or endorsed by YouTube or Google.

Version 0.1.4 supplies all Slovak Tauri-specific NSIS messages, fixing blank labels and upgrade choices when an earlier version is installed. The translation is configured through `customLanguageFiles` and included in source exports. Version 0.1.3 adds a persistent light/dark theme switch in the header and displays the version next to the application name. On first launch, the theme follows Windows. Version 0.1.2 fixes downloads of videos whose titles contain diacritics or other Unicode characters. All yt-dlp output uses UTF-8, and subprocess readers continue draining pipes if unexpected bytes occur. Version 0.1.1 renamed StreamForge to YouTube Downloader. The internal Tauri identifier, history key, executable crate name and environment overrides intentionally remain unchanged for compatibility with existing user data and tool updates.

## Features

- YouTube URL validation and metadata preview: title, thumbnail, channel and duration
- Auto/Best/8K/4K/1440p/1080p/720p quality caps
- MP4, MKV and WebM output; Auto, AV1, VP9 and H.264 selection
- MP3, M4A and Opus audio extraction
- manual and automatic subtitles with language patterns, plus thumbnail embedding
- optional cookies from the user's own Edge, Chrome or Firefox profile
- progress, speed, ETA, post-processing state and cancellation of the full process tree
- persistent local history and one-click opening of the output folder
- system light/dark theme and Windows 11-oriented layout
- classified guidance for HTTP 403, age/login/cookies, PO Token, FFmpeg and JS runtime errors

The resolution choices are maximum-height caps: for example, 4K chooses the best available stream up to 2160p. A forced codec is strict; if YouTube has no matching stream, yt-dlp reports that no format is available. `Auto` is the safest choice.

## Architecture in one minute

React only sends typed settings. Rust validates every value and starts `yt-dlp` directly with an argument array—never through PowerShell or `cmd.exe`. Progress is returned as Tauri events. FFmpeg and Deno are separate managed tools, so `yt-dlp` can be updated independently from the application. See [ARCHITECTURE.md](ARCHITECTURE.md).

## Current YouTube requirements (2026)

Current yt-dlp YouTube support uses an external JavaScript runtime for challenge solving. Deno is the recommended runtime and current releases require Deno 2.3 or newer. The official PyInstaller `yt-dlp.exe` already bundles the matching `yt-dlp-ejs` scripts, so the release package does not enable remote script downloads.

Some sessions or videos may still require login cookies or a supported PO Token provider. YouTube Downloader can request cookies from the user's own browser profile but does not collect them, generate tokens, bypass access controls, or weaken yt-dlp security. Keep yt-dlp current and follow its official documentation if YouTube changes enforcement.

## Development prerequisites

- Windows 11
- Node.js 22 or later and npm
- Rust stable with the MSVC target
- Visual Studio 2022 Build Tools with **Desktop development with C++**
- WebView2 (included with Windows 11)
- yt-dlp, FFmpeg and Deno either in `PATH` or staged by the script below

```powershell
npm install
.\scripts\prepare-tools.ps1
npm run icon
npm run tauri:dev
```

The tool-preparation script downloads:

- the latest official `yt-dlp.exe` and verifies it against the release `SHA2-256SUMS`;
- the latest official Deno x64 Windows ZIP and its `.sha256sum`;
- the Gyan FFmpeg release essentials ZIP from its official GitHub mirror, checked against the release asset SHA-256 digest.

Downloaded executables are build inputs and are ignored by Git.

For development with tools already installed, skip the download and set optional full-path overrides:

```powershell
$env:STREAMFORGE_YTDLP = 'C:\Tools\yt-dlp.exe'
$env:STREAMFORGE_FFMPEG = 'C:\Tools\ffmpeg.exe'
$env:STREAMFORGE_DENO = 'C:\Tools\deno.exe'
npm run tauri:dev
```

## Build the Windows installers

```powershell
.\scripts\build-release.ps1
```

The result is written to:

- `src-tauri\target\release\bundle\nsis\YouTube Downloader_0.1.4_x64-setup.exe`
- `src-tauri\target\release\bundle\msi\YouTube Downloader_0.1.4_x64_en-US.msi`

Run `scripts\export-release.ps1` to additionally create a portable ZIP and a source ZIP. Extract the entire portable ZIP and launch `YouTubeDownloader.exe`, keeping its `tools` folder alongside it. Local history and managed tools still use the user's application-data area.

MSI creation requires the Windows VBSCRIPT optional feature. If it is disabled, build only the NSIS installer:

```powershell
.\scripts\build-release.ps1 -NsisOnly
```

The GitHub Actions workflow also creates both installers for a tag or a manual run. Production releases should be Authenticode-signed; signing secrets are intentionally not included in this repository.

## Updating yt-dlp

Open advanced settings and choose **Aktualizovať yt-dlp**. The app calls the official stable self-updater on the per-user managed copy. FFmpeg and Deno should be refreshed with a new signed application release (or by rerunning the preparation script during a new build), because their archives are verified at build time.

## Troubleshooting

- **403 / Forbidden:** update yt-dlp first. Retry later or check whether the video needs your authenticated browser session.
- **Age/login restriction:** sign into YouTube in your own browser, close active private windows if necessary, and select that browser under advanced settings.
- **Cookies locked:** close the selected browser before retrying; yt-dlp must be able to read its profile database.
- **PO Token required:** install only a provider supported by current official yt-dlp documentation. YouTube Downloader deliberately does not invent or scrape tokens.
- **No matching formats:** switch codec to Auto, choose MKV, or select a lower quality cap.
- **FFmpeg/Deno missing:** run `scripts\prepare-tools.ps1` and rebuild, or set the corresponding environment override.

## Legal and licenses

Use YouTube Downloader only for content you own or are authorized to download, and respect platform terms and local law. The app itself does not remove DRM or access restrictions.

Tauri, React, yt-dlp, Deno and FFmpeg retain their respective licenses. The Gyan FFmpeg essentials build is GPLv3. A distributor must include all required third-party license notices and comply with the relevant source-offer/license obligations; the exact release binaries and notices should be reviewed before public redistribution.
