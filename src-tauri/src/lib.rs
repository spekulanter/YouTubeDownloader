use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    process::Stdio,
    sync::Mutex,
};
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use url::Url;

const PROGRESS_PREFIX: &str = "__SF_PROGRESS__";
const FILE_PREFIX: &str = "__SF_FILE__";

fn hidden_command(program: impl AsRef<std::ffi::OsStr>) -> Command {
    let mut command = Command::new(program);
    #[cfg(target_os = "windows")]
    command.creation_flags(0x08000000);
    command
}

#[derive(Default)]
struct DownloadState(Mutex<HashMap<String, ActiveJob>>);

#[derive(Clone, Copy)]
struct ActiveJob {
    pid: u32,
    cancelled: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DownloadRequest {
    id: String,
    url: String,
    output_dir: String,
    mode: String,
    quality: String,
    container: String,
    audio_format: String,
    codec: String,
    subtitles: bool,
    auto_subtitles: bool,
    subtitle_languages: String,
    thumbnail: bool,
    cookies_from_browser: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct VideoMetadata {
    id: String,
    title: String,
    uploader: Option<String>,
    thumbnail: Option<String>,
    duration: Option<f64>,
    webpage_url: String,
    is_live: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProgressPayload {
    id: String,
    status: String,
    percent: Option<f64>,
    speed: Option<String>,
    eta: Option<String>,
    message: Option<String>,
    file_path: Option<String>,
    error_code: Option<String>,
    error_detail: Option<String>,
}

impl ProgressPayload {
    fn status(id: &str, status: &str, message: impl Into<String>) -> Self {
        Self {
            id: id.to_owned(),
            status: status.to_owned(),
            percent: None,
            speed: None,
            eta: None,
            message: Some(message.into()),
            file_path: None,
            error_code: None,
            error_detail: None,
        }
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ToolInfo {
    available: bool,
    version: Option<String>,
    path: Option<String>,
    source: Option<String>,
    error: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ToolStatus {
    yt_dlp: ToolInfo,
    ffmpeg: ToolInfo,
    deno: ToolInfo,
}

fn validate_youtube_url(value: &str) -> Result<String, String> {
    let parsed = Url::parse(value.trim()).map_err(|_| "Zadaj platnú HTTPS YouTube URL.".to_string())?;
    if parsed.scheme() != "https" {
        return Err("Z bezpečnostných dôvodov je povolená iba HTTPS URL.".into());
    }
    let host = parsed.host_str().unwrap_or_default().to_ascii_lowercase();
    let allowed = host == "youtu.be"
        || host == "youtube.com"
        || host.ends_with(".youtube.com")
        || host == "youtube-nocookie.com"
        || host.ends_with(".youtube-nocookie.com");
    if !allowed {
        return Err("Aplikácia prijíma iba odkazy z YouTube.".into());
    }
    Ok(parsed.to_string())
}

fn validate_choice(value: &str, allowed: &[&str], label: &str) -> Result<(), String> {
    if allowed.contains(&value) {
        Ok(())
    } else {
        Err(format!("Neplatná hodnota pre {label}."))
    }
}

fn validate_request(request: &DownloadRequest) -> Result<PathBuf, String> {
    if request.id.len() < 8 || request.id.len() > 80 || !request.id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err("Neplatné ID úlohy.".into());
    }
    validate_youtube_url(&request.url)?;
    validate_choice(&request.mode, &["video", "audio"], "režim")?;
    validate_choice(&request.quality, &["auto", "best", "4320", "2160", "1440", "1080", "720"], "kvalitu")?;
    validate_choice(&request.container, &["mp4", "mkv", "webm"], "kontajner")?;
    validate_choice(&request.audio_format, &["mp3", "m4a", "opus"], "audio formát")?;
    validate_choice(&request.codec, &["auto", "av1", "vp9", "h264"], "kodek")?;
    if request.mode == "video" && request.container == "webm" && request.codec == "h264" {
        return Err("WebM nepodporuje H.264. Vyber Auto, AV1 alebo VP9, prípadne MP4/MKV.".into());
    }
    validate_choice(&request.cookies_from_browser, &["none", "edge", "chrome", "firefox"], "prehliadač")?;
    if request.subtitle_languages.len() > 120
        || !request.subtitle_languages.chars().all(|c| c.is_ascii_alphanumeric() || ".,*_-".contains(c))
    {
        return Err("Neplatný zoznam jazykov titulkov.".into());
    }
    let output = PathBuf::from(&request.output_dir);
    if !output.is_dir() {
        return Err("Cieľový priečinok neexistuje alebo nie je dostupný.".into());
    }
    Ok(output)
}

fn managed_tools_dir(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_local_data_dir().ok().map(|p| p.join("tools"))
}

fn seed_bundled_tools(app: &AppHandle) {
    let Some(target) = managed_tools_dir(app) else { return };
    let Ok(resources) = app.path().resource_dir() else { return };
    let direct = resources.join("tools");
    let nested = resources.join("resources").join("tools");
    let source = if direct.is_dir() { direct } else { nested };
    if !source.is_dir() || fs::create_dir_all(&target).is_err() {
        return;
    }
    for name in ["yt-dlp.exe", "deno.exe", "ffmpeg.exe", "ffprobe.exe"] {
        let from = source.join(name);
        let to = target.join(name);
        let dependency_changed = name != "yt-dlp.exe"
            && from.metadata().ok().map(|m| m.len()) != to.metadata().ok().map(|m| m.len());
        if from.is_file() && (!to.exists() || dependency_changed) {
            let _ = fs::copy(from, to);
        }
    }
}

fn tool_candidates(app: &AppHandle, env_name: &str, file_name: &str, command_name: &str) -> Vec<(PathBuf, &'static str)> {
    let mut result = Vec::new();
    if let Some(value) = std::env::var_os(env_name) {
        result.push((PathBuf::from(value), "environment"));
    }
    if let Some(dir) = managed_tools_dir(app) {
        result.push((dir.join(file_name), "managed"));
    }
    if let Ok(dir) = app.path().resource_dir() {
        result.push((dir.join("tools").join(file_name), "bundled"));
        result.push((dir.join("resources").join("tools").join(file_name), "bundled"));
    }
    result.push((PathBuf::from(command_name), "PATH"));
    result
}

async fn inspect_tool(app: &AppHandle, env_name: &str, file_name: &str, command_name: &str, version_arg: &str) -> ToolInfo {
    let mut launch_error = None;
    for (candidate, source) in tool_candidates(app, env_name, file_name, command_name) {
        if candidate.components().count() > 1 && !candidate.is_file() {
            continue;
        }
        let output = hidden_command(&candidate)
            .arg(version_arg)
            .stdin(Stdio::null())
            .output()
            .await;
        if let Ok(output) = output {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);
                let version = stdout.lines().chain(stderr.lines()).find(|l| !l.trim().is_empty()).map(|s| s.trim().to_string());
                return ToolInfo {
                    available: true,
                    version,
                    path: Some(candidate.to_string_lossy().into_owned()),
                    source: Some(source.to_string()),
                    error: None,
                };
            }
        } else if let Err(error) = output {
            if error.raw_os_error() == Some(1260) || error.raw_os_error() == Some(577) {
                launch_error = Some(format!("Windows politika zablokovala spustenie {command_name}: {error}"));
            }
        }
    }
    ToolInfo { available: false, version: None, path: None, source: None, error: launch_error }
}

async fn tool_status(app: &AppHandle) -> ToolStatus {
    let (yt_dlp, ffmpeg, deno) = tokio::join!(
        inspect_tool(app, "STREAMFORGE_YTDLP", "yt-dlp.exe", "yt-dlp", "--version"),
        inspect_tool(app, "STREAMFORGE_FFMPEG", "ffmpeg.exe", "ffmpeg", "-version"),
        inspect_tool(app, "STREAMFORGE_DENO", "deno.exe", "deno", "--version")
    );
    ToolStatus { yt_dlp, ffmpeg, deno }
}

fn add_runtime_args(args: &mut Vec<String>, status: &ToolStatus) {
    if let Some(path) = &status.deno.path {
        args.push("--js-runtimes".into());
        args.push(format!("deno:{path}"));
    }
    if let Some(path) = &status.ffmpeg.path {
        let ffmpeg = Path::new(path);
        if ffmpeg.components().count() > 1 {
            if let Some(parent) = ffmpeg.parent() {
                args.push("--ffmpeg-location".into());
                args.push(parent.to_string_lossy().into_owned());
            }
        }
    }
}

fn add_cookie_args(args: &mut Vec<String>, browser: &str) {
    if browser != "none" {
        args.push("--cookies-from-browser".into());
        args.push(browser.to_owned());
    }
}

#[tauri::command]
async fn get_tool_status(app: AppHandle) -> Result<ToolStatus, String> {
    Ok(tool_status(&app).await)
}

#[tauri::command]
async fn get_metadata(app: AppHandle, url: String, cookies_from_browser: String) -> Result<VideoMetadata, String> {
    let safe_url = validate_youtube_url(&url)?;
    validate_choice(&cookies_from_browser, &["none", "edge", "chrome", "firefox"], "prehliadač")?;
    let status = tool_status(&app).await;
    let yt_dlp = status.yt_dlp.path.clone().ok_or_else(|| "yt-dlp sa nenašiel. Spusti scripts/prepare-tools.ps1 alebo ho pridaj do PATH.".to_string())?;
    let mut args = vec![
        "--no-config".into(),
        "--encoding".into(),
        "utf-8".into(),
        "--no-playlist".into(),
        "--skip-download".into(),
        "--dump-single-json".into(),
        "--no-warnings".into(),
    ];
    add_runtime_args(&mut args, &status);
    add_cookie_args(&mut args, &cookies_from_browser);
    args.push(safe_url);
    let output = hidden_command(yt_dlp).args(args).stdin(Stdio::null()).kill_on_drop(true).output().await.map_err(|e| format!("Nepodarilo sa spustiť yt-dlp: {e}"))?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let code = classify_error(&detail);
        return Err(format!("{}: {}", friendly_error(code), last_meaningful_line(&detail)));
    }
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).map_err(|e| format!("Neplatná odpoveď yt-dlp: {e}"))?;
    Ok(VideoMetadata {
        id: value.get("id").and_then(|v| v.as_str()).unwrap_or_default().to_string(),
        title: value.get("title").and_then(|v| v.as_str()).unwrap_or("Video bez názvu").to_string(),
        uploader: value.get("uploader").and_then(|v| v.as_str()).map(str::to_string),
        thumbnail: value.get("thumbnail").and_then(|v| v.as_str()).map(str::to_string),
        duration: value.get("duration").and_then(|v| v.as_f64()),
        webpage_url: value.get("webpage_url").and_then(|v| v.as_str()).unwrap_or(&url).to_string(),
        is_live: value.get("is_live").and_then(|v| v.as_bool()).unwrap_or(false),
    })
}

fn format_selector(request: &DownloadRequest) -> String {
    let height = match request.quality.as_str() {
        "4320" | "2160" | "1440" | "1080" | "720" => Some(request.quality.as_str()),
        _ => None,
    };
    let codec = match request.codec.as_str() {
        "av1" => Some("av01"),
        "vp9" => Some("vp9"),
        "h264" => Some("avc1"),
        _ => None,
    };
    let mut filters = format!(
        "{}{}",
        height.map(|h| format!("[height<={h}]")).unwrap_or_default(),
        codec.map(|c| format!("[vcodec^={c}]")).unwrap_or_default()
    );
    let audio_selector = if request.container == "webm" {
        if codec.is_none() { filters.push_str("[vcodec~='^(vp9|vp09|av01)']"); }
        "ba[acodec^=opus]/ba[acodec^=vorbis]"
    } else { "ba" };
    if filters.is_empty() {
        if request.quality == "best" { "bv+ba/b".into() } else { "bv*+ba/b".into() }
    } else {
        format!("bv*{filters}+({audio_selector})/b{filters}")
    }
}

fn download_args(request: &DownloadRequest, output: &Path, status: &ToolStatus) -> Vec<String> {
    let mut args = vec![
        "--no-config".into(),
        "--encoding".into(),
        "utf-8".into(),
        "--no-playlist".into(),
        "--newline".into(),
        "--progress".into(),
        "--no-quiet".into(),
        "--no-simulate".into(),
        "--windows-filenames".into(),
        "--trim-filenames".into(),
        "180".into(),
        "--progress-template".into(),
        format!("download:{PROGRESS_PREFIX}%(progress._percent_str)s|%(progress._speed_str)s|%(progress._eta_str)s"),
        "--print".into(),
        format!("after_move:{FILE_PREFIX}%(filepath)s"),
        "-P".into(),
        output.to_string_lossy().into_owned(),
        "-o".into(),
        "%(title).180B [%(id)s].%(ext)s".into(),
    ];
    add_runtime_args(&mut args, status);
    add_cookie_args(&mut args, &request.cookies_from_browser);
    if request.mode == "audio" {
        args.extend(["-f".into(), "ba/b".into(), "-x".into(), "--audio-format".into(), request.audio_format.clone(), "--audio-quality".into(), "0".into()]);
    } else {
        args.extend([
            "-f".into(), format_selector(request),
            "--merge-output-format".into(), request.container.clone(),
            "--remux-video".into(), request.container.clone(),
        ]);
    }
    if request.subtitles && request.mode == "video" {
        let subtitle_format = if request.container == "webm" { "vtt" } else { "srt" };
        args.extend(["--write-subs".into(), "--sub-langs".into(), request.subtitle_languages.clone(), "--convert-subs".into(), subtitle_format.into(), "--embed-subs".into()]);
        if request.auto_subtitles {
            args.push("--write-auto-subs".into());
        }
    }
    if request.thumbnail {
        args.push("--write-thumbnail".into());
        if (request.mode == "video" && request.container != "webm")
            || (request.mode == "audio" && request.audio_format != "opus") {
            args.push("--embed-thumbnail".into());
        }
    }
    args.push(request.url.clone());
    args
}

// Keep draining subprocess pipes even if a tool emits an unexpected non-UTF-8 byte.
// yt-dlp itself is launched with --encoding utf-8 for lossless titles and paths.
async fn read_output_line<R: tokio::io::AsyncBufRead + Unpin>(reader: &mut R) -> std::io::Result<Option<String>> {
    let mut bytes = Vec::new();
    if reader.read_until(b'\n', &mut bytes).await? == 0 {
        return Ok(None);
    }
    while matches!(bytes.last(), Some(b'\n' | b'\r')) {
        bytes.pop();
    }
    Ok(Some(String::from_utf8_lossy(&bytes).into_owned()))
}

fn parse_progress(id: &str, line: &str) -> Option<ProgressPayload> {
    let raw = line.strip_prefix(PROGRESS_PREFIX)?;
    let mut fields = raw.splitn(3, '|');
    let percent = fields.next().and_then(|v| v.trim().trim_end_matches('%').trim().parse::<f64>().ok());
    let speed = fields.next().map(str::trim).filter(|v| !v.is_empty() && *v != "NA").map(str::to_string);
    let eta = fields.next().map(str::trim).filter(|v| !v.is_empty() && *v != "NA").map(str::to_string);
    Some(ProgressPayload {
        id: id.to_owned(), status: "downloading".into(), percent, speed, eta,
        message: None, file_path: None, error_code: None, error_detail: None,
    })
}

#[tauri::command]
async fn start_download(app: AppHandle, state: State<'_, DownloadState>, request: DownloadRequest) -> Result<(), String> {
    let output = validate_request(&request)?;
    let status = tool_status(&app).await;
    let Some(yt_dlp) = status.yt_dlp.path.clone() else {
        return Err("yt-dlp sa nenašiel. Spusti scripts/prepare-tools.ps1 alebo ho pridaj do PATH.".into());
    };
    if !status.ffmpeg.available {
        return Err(status.ffmpeg.error.unwrap_or_else(|| "FFmpeg sa nenašiel. Je potrebný na spojenie a konverziu streamov.".into()));
    }
    let args = download_args(&request, &output, &status);
    app.emit("download-progress", ProgressPayload::status(&request.id, "starting", "Pripravujem yt-dlp…")).ok();
    let mut child = hidden_command(yt_dlp)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("Nepodarilo sa spustiť yt-dlp: {e}"))?;
    let pid = child.id().ok_or_else(|| "Proces yt-dlp nemá platné PID.".to_string())?;
    state.0.lock().map_err(|_| "Interný stav je nedostupný.".to_string())?.insert(request.id.clone(), ActiveJob { pid, cancelled: false });

    let stdout = child.stdout.take().ok_or_else(|| "Nepodarilo sa čítať výstup yt-dlp.".to_string())?;
    let stderr = child.stderr.take().ok_or_else(|| "Nepodarilo sa čítať chyby yt-dlp.".to_string())?;
    let app_out = app.clone();
    let id_out = request.id.clone();
    let stdout_task = tokio::spawn(async move {
        let mut reader = BufReader::new(stdout);
        let mut final_file = None;
        while let Ok(Some(line)) = read_output_line(&mut reader).await {
            if let Some(payload) = parse_progress(&id_out, &line) {
                app_out.emit("download-progress", payload).ok();
            } else if let Some(path) = line.strip_prefix(FILE_PREFIX) {
                final_file = Some(path.trim().to_string());
            } else if is_processing_line(&line) {
                app_out.emit("download-progress", ProgressPayload::status(&id_out, "processing", line.trim())).ok();
            }
        }
        final_file
    });
    let app_err = app.clone();
    let id_err = request.id.clone();
    let stderr_task = tokio::spawn(async move {
        let mut reader = BufReader::new(stderr);
        let mut captured = Vec::new();
        while let Ok(Some(line)) = read_output_line(&mut reader).await {
            if let Some(payload) = parse_progress(&id_err, &line) {
                app_err.emit("download-progress", payload).ok();
            } else if is_processing_line(&line) {
                app_err.emit("download-progress", ProgressPayload::status(&id_err, "processing", line.trim())).ok();
            }
            if !line.trim().is_empty() {
                captured.push(line);
                if captured.len() > 80 { captured.remove(0); }
            }
        }
        captured.join("\n")
    });

    let exit = child.wait().await.map_err(|e| format!("Čakanie na yt-dlp zlyhalo: {e}"))?;
    let final_file = stdout_task.await.unwrap_or(None);
    let error_text = stderr_task.await.unwrap_or_else(|_| "Výstup chybového kanála nie je dostupný.".into());
    let cancelled = state.0.lock().map_err(|_| "Interný stav je nedostupný.".to_string())?.remove(&request.id).map(|j| j.cancelled).unwrap_or(false);
    if cancelled {
        app.emit("download-progress", ProgressPayload::status(&request.id, "cancelled", "Sťahovanie bolo zrušené.")).ok();
    } else if exit.success() {
        let mut payload = ProgressPayload::status(&request.id, "completed", "Súbor je pripravený.");
        payload.percent = Some(100.0);
        payload.file_path = final_file;
        app.emit("download-progress", payload).ok();
    } else {
        let code = classify_error(&error_text).to_string();
        let detail = last_meaningful_line(&error_text);
        app.emit("download-progress", ProgressPayload {
            id: request.id, status: "failed".into(), percent: None, speed: None, eta: None,
            message: None, file_path: None, error_code: Some(code), error_detail: Some(detail),
        }).ok();
    }
    Ok(())
}

#[tauri::command]
async fn cancel_download(state: State<'_, DownloadState>, id: String) -> Result<(), String> {
    let pid = {
        let mut jobs = state.0.lock().map_err(|_| "Interný stav je nedostupný.".to_string())?;
        let job = jobs.get_mut(&id).ok_or_else(|| "Sťahovanie už nie je aktívne.".to_string())?;
        job.cancelled = true;
        job.pid
    };
    #[cfg(target_os = "windows")]
    {
        let status = hidden_command("taskkill").args(["/PID", &pid.to_string(), "/T", "/F"]).stdin(Stdio::null()).status().await.map_err(|e| format!("Proces sa nepodarilo ukončiť: {e}"))?;
        if !status.success() { return Err("Windows nepotvrdil ukončenie procesu.".into()); }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let status = Command::new("kill").args(["-TERM", &pid.to_string()]).status().await.map_err(|e| format!("Proces sa nepodarilo ukončiť: {e}"))?;
        if !status.success() { return Err("Systém nepotvrdil ukončenie procesu.".into()); }
    }
    Ok(())
}

#[tauri::command]
async fn open_folder(path: String) -> Result<(), String> {
    let directory = PathBuf::from(path);
    if !directory.is_dir() {
        return Err("Priečinok už neexistuje alebo nie je dostupný.".into());
    }
    #[cfg(target_os = "windows")]
    Command::new("explorer.exe").arg(&directory).spawn().map_err(|e| format!("Priečinok sa nepodarilo otvoriť: {e}"))?;
    #[cfg(target_os = "macos")]
    Command::new("open").arg(&directory).spawn().map_err(|e| format!("Priečinok sa nepodarilo otvoriť: {e}"))?;
    #[cfg(all(unix, not(target_os = "macos")))]
    Command::new("xdg-open").arg(&directory).spawn().map_err(|e| format!("Priečinok sa nepodarilo otvoriť: {e}"))?;
    Ok(())
}

#[tauri::command]
async fn update_yt_dlp(app: AppHandle) -> Result<String, String> {
    if !app.state::<DownloadState>().0.lock().map_err(|_| "Interný stav je nedostupný.")?.is_empty() {
        return Err("Aktualizácia nie je dostupná počas sťahovania.".into());
    }
    let info = inspect_tool(&app, "STREAMFORGE_YTDLP", "yt-dlp.exe", "yt-dlp", "--version").await;
    let path = info.path.ok_or_else(|| "yt-dlp sa nenašiel.".to_string())?;
    let output = hidden_command(path).args(["--no-config", "--encoding", "utf-8", "--update-to", "stable"]).stdin(Stdio::null()).output().await.map_err(|e| format!("Aktualizácia sa nedá spustiť: {e}"))?;
    let text = format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    if output.status.success() { Ok(text.trim().to_string()) } else { Err(format!("Aktualizácia zlyhala: {}", last_meaningful_line(&text))) }
}

fn classify_error(detail: &str) -> &'static str {
    let lower = detail.to_ascii_lowercase();
    if lower.contains("po token") || lower.contains("proof of origin") { "po_token" }
    else if lower.contains("age-restricted") || lower.contains("age restricted") || lower.contains("confirm your age") { "age_restricted" }
    else if lower.contains("sign in") || lower.contains("login required") || lower.contains("members-only") || lower.contains("private video") { "login_required" }
    else if lower.contains("cookie") { "cookies_required" }
    else if lower.contains("403") || lower.contains("forbidden") { "forbidden" }
    else if lower.contains("ffmpeg") && (lower.contains("not found") || lower.contains("not installed")) { "missing_ffmpeg" }
    else if lower.contains("javascript runtime") || lower.contains("no supported javascript") || lower.contains("deno") && lower.contains("not found") { "missing_js_runtime" }
    else if lower.contains("unsupported url") || lower.contains("invalid url") { "unsupported" }
    else { "unknown" }
}

fn is_processing_line(line: &str) -> bool {
    ["[Merger]", "[ExtractAudio]", "[EmbedSubtitle]", "[EmbedThumbnail]", "[VideoRemuxer]", "[SubtitlesConvertor]"].iter().any(|prefix| line.contains(prefix))
}

fn friendly_error(code: &str) -> &'static str {
    match code {
        "po_token" => "YouTube vyžaduje PO Token",
        "age_restricted" => "Video je vekovo obmedzené",
        "login_required" => "Video vyžaduje prihlásenie",
        "cookies_required" => "Cookies nie sú dostupné",
        "forbidden" => "YouTube odmietol požiadavku (403)",
        "missing_ffmpeg" => "FFmpeg sa nenašiel",
        "missing_js_runtime" => "Chýba JavaScript runtime/Deno",
        "unsupported" => "Odkaz nie je podporovaný",
        _ => "yt-dlp vrátil chybu",
    }
}

fn last_meaningful_line(value: &str) -> String {
    value.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("Neznáma chyba.").trim().chars().take(500).collect()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(DownloadState::default())
        .setup(|app| {
            seed_bundled_tools(app.handle());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_tool_status,
            get_metadata,
            start_download,
            cancel_download,
            open_folder,
            update_yt_dlp
        ])
        .run(tauri::generate_context!())
        .expect("error while running StreamForge");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> DownloadRequest {
        DownloadRequest {
            id: "12345678-abcd".into(),
            url: "https://www.youtube.com/watch?v=dQw4w9WgXcQ".into(),
            output_dir: ".".into(),
            mode: "video".into(),
            quality: "2160".into(),
            container: "mkv".into(),
            audio_format: "mp3".into(),
            codec: "av1".into(),
            subtitles: false,
            auto_subtitles: false,
            subtitle_languages: "sk.*,en.*".into(),
            thumbnail: true,
            cookies_from_browser: "none".into(),
        }
    }

    #[test]
    fn download_uses_utf8_and_preserves_unicode_output_directory() {
        let unavailable = ToolInfo { available: false, version: None, path: None, source: None, error: None };
        let status = ToolStatus { yt_dlp: unavailable.clone(), ffmpeg: unavailable.clone(), deno: unavailable };
        let directory = Path::new("C:\\Stiahnuté\\české videá");
        for mode in ["video", "audio"] {
            let mut options = request();
            options.mode = mode.into();
            let args = download_args(&options, directory, &status);
            assert!(args.windows(2).any(|pair| pair[0] == "--encoding" && pair[1] == "utf-8"));
            assert!(args.windows(2).any(|pair| pair[0] == "-P" && pair[1] == directory.to_string_lossy()));
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn subprocess_reader_keeps_draining_after_non_utf8_text() {
        let bytes: &[u8] = b"Destination: Pro\xe8 je kod\r\n__SF_PROGRESS__ 12.5%|4.2MiB/s|00:42\n";
        let mut reader = BufReader::new(bytes);
        assert!(read_output_line(&mut reader).await.unwrap().unwrap().contains('\u{fffd}'));
        let progress = read_output_line(&mut reader).await.unwrap().unwrap();
        assert_eq!(parse_progress("job", &progress).unwrap().percent, Some(12.5));
        assert!(read_output_line(&mut reader).await.unwrap().is_none());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn subprocess_reader_preserves_utf8_filepath() {
        let line = "__SF_FILE__C:\\Stiahnuté\\Proč je funkční kód.mp4\r\n";
        let mut reader = BufReader::new(line.as_bytes());
        assert_eq!(read_output_line(&mut reader).await.unwrap().as_deref(), Some(line.trim_end()));
    }

    #[test]
    fn only_allows_https_youtube_hosts() {
        assert!(validate_youtube_url("https://youtu.be/abc").is_ok());
        assert!(validate_youtube_url("https://music.youtube.com/watch?v=abc").is_ok());
        assert!(validate_youtube_url("http://youtube.com/watch?v=abc").is_err());
        assert!(validate_youtube_url("https://youtube.com.example.test/watch?v=abc").is_err());
    }

    #[test]
    fn selector_applies_resolution_and_codec_as_strict_filters() {
        assert_eq!(format_selector(&request()), "bv*[height<=2160][vcodec^=av01]+(ba)/b[height<=2160][vcodec^=av01]");
    }

    #[test]
    fn known_failures_are_classified() {
        assert_eq!(classify_error("HTTP Error 403: Forbidden"), "forbidden");
        assert_eq!(classify_error("Sign in to confirm your age"), "age_restricted");
        assert_eq!(classify_error("A PO Token is required"), "po_token");
    }

    #[test]
    fn progress_parses_percent_speed_and_eta() {
        let payload = parse_progress("job", "__SF_PROGRESS__ 12.5%| 4.2MiB/s|00:42").unwrap();
        assert_eq!(payload.percent, Some(12.5));
        assert_eq!(payload.speed.as_deref(), Some("4.2MiB/s"));
        assert_eq!(payload.eta.as_deref(), Some("00:42"));
        assert!(parse_progress("job", "WARNING: unrelated output").is_none());
    }

    #[test]
    fn webm_selects_supported_video_and_audio_codecs() {
        let mut options = request();
        options.container = "webm".into();
        options.codec = "auto".into();
        let selector = format_selector(&options);
        assert!(selector.contains("vcodec~='^(vp9|vp09|av01)'"));
        assert!(selector.contains("ba[acodec^=opus]"));
    }
}
