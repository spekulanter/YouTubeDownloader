# YouTube Downloader architecture

```text
React / TypeScript UI
        │ typed Tauri commands and progress events
        ▼
Rust command layer
  URL and option validation ─ cancellation ─ error classification
        │ argument array; no shell and no arbitrary CLI flags
        ▼
managed yt-dlp.exe ── FFmpeg/ffprobe ── Deno runtime
```

## Boundaries

- The React layer owns transient form state and a small, persistent download history in WebView local storage.
- Rust is the trust boundary. It accepts only YouTube HTTPS hosts and enumerated choices, checks the target directory, constructs a fixed output template, and starts `yt-dlp` directly without a command shell.
- `yt-dlp` remains the only extractor. YouTube Downloader does not implement YouTube protocols, decrypt content, or synthesize authentication/PO tokens.
- FFmpeg merges or remuxes selected streams and performs audio extraction. Deno supplies the restricted JavaScript runtime required by current YouTube challenge handling.

## Tool lifecycle

Release builds stage verified tools in `src-tauri/resources/tools`. On first launch they are copied to the per-user Tauri app-data folder. The `yt-dlp` executable in that writable folder can use the official `yt-dlp --update-to stable` flow without replacing the application or writing into Program Files. A newer app release refreshes changed Deno/FFmpeg payloads but deliberately does not overwrite a newer self-updated `yt-dlp`.

Tool lookup order is: explicit `STREAMFORGE_*` environment override, managed app-data copy, bundled resource, then `PATH`. This permits development with system tools and production with self-contained installers.

## Security properties

- No shell interpolation and no user-supplied extra arguments.
- HTTPS and a fixed YouTube host allow-list.
- Tool downloads in the build script are verified with publisher-provided SHA-256 files.
- Browser-cookie support is opt-in and restricted to known browser identifiers. Cookies are passed directly to yt-dlp and never returned to or stored by the UI.
- PO Token failures are explained, not bypassed. A supported provider can be added later at the yt-dlp layer without changing the UI architecture.
