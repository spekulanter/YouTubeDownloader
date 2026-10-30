Release dependencies are staged here by scripts/prepare-tools.ps1.

Expected flat files for a fully bundled Windows build:
- yt-dlp.exe
- deno.exe
- ffmpeg.exe
- ffprobe.exe

Do not commit downloaded binaries. The application copies bundled tools into its
per-user app-data directory on first launch so yt-dlp can update without requiring
administrator rights or modifying Program Files.
