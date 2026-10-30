import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import {
  AlertCircle,
  CheckCircle2,
  ChevronDown,
  Clock3,
  Download,
  ExternalLink,
  FileAudio,
  FileVideo2,
  FolderOpen,
  History,
  Image,
  LoaderCircle,
  Moon,
  RefreshCw,
  Settings2,
  Subtitles,
  Sun,
  X,
} from "lucide-react";
import { backend } from "./api";
import { useTheme } from "./theme";
import { version } from "../package.json";
import type {
  AudioFormat,
  Codec,
  CookieBrowser,
  DownloadProgress,
  DownloadRequest,
  HistoryItem,
  Quality,
  ToolStatus,
  VideoContainer,
  VideoMetadata,
} from "./types";

const HISTORY_KEY = "streamforge.history.v1";

const errorMessages: Record<string, string> = {
  forbidden: "YouTube odmietol požiadavku (403). Skontroluj aktualizáciu yt-dlp, prihlásenie a sieť.",
  age_restricted: "Video je vekovo obmedzené. Použi cookies z vlastného prihláseného prehliadača.",
  login_required: "Video vyžaduje prihlásenie. Zvoľ svoj prehliadač v rozšírených možnostiach.",
  cookies_required: "Relácia alebo cookies nie sú dostupné. Prihlás sa v prehliadači a skús ho vybrať.",
  po_token: "YouTube vyžaduje PO Token. Aplikácia ochranu neobchádza; skús najnovší yt-dlp alebo oficiálne podporovaný provider.",
  missing_ffmpeg: "FFmpeg sa nenašiel. Je potrebný na spájanie videa a zvuku aj konverziu audia.",
  missing_js_runtime: "Chýba kompatibilný JavaScript runtime. Nainštaluj Deno 2.3 alebo novší.",
  unsupported: "Odkaz nie je podporovaný alebo nie je platnou YouTube URL.",
};

function formatDuration(seconds?: number) {
  if (!seconds) return "—";
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  const s = Math.floor(seconds % 60);
  return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}` : `${m}:${String(s).padStart(2, "0")}`;
}

function loadHistory(): HistoryItem[] {
  try {
    return JSON.parse(localStorage.getItem(HISTORY_KEY) ?? "[]") as HistoryItem[];
  } catch {
    return [];
  }
}

function App() {
  const { theme, toggleTheme } = useTheme();
  const [url, setUrl] = useState("");
  const [metadata, setMetadata] = useState<VideoMetadata | null>(null);
  const [loadingMetadata, setLoadingMetadata] = useState(false);
  const [mode, setMode] = useState<"video" | "audio">("video");
  const [quality, setQuality] = useState<Quality>("auto");
  const [container, setContainer] = useState<VideoContainer>("mp4");
  const [audioFormat, setAudioFormat] = useState<AudioFormat>("mp3");
  const [codec, setCodec] = useState<Codec>("auto");
  const [subtitles, setSubtitles] = useState(false);
  const [autoSubtitles, setAutoSubtitles] = useState(false);
  const [subtitleLanguages, setSubtitleLanguages] = useState("sk.*,en.*");
  const [thumbnail, setThumbnail] = useState(true);
  const [cookies, setCookies] = useState<CookieBrowser>("none");
  const [outputDir, setOutputDir] = useState("");
  const [advanced, setAdvanced] = useState(false);
  const [progress, setProgress] = useState<DownloadProgress | null>(null);
  const [history, setHistory] = useState<HistoryItem[]>(loadHistory);
  const [tools, setTools] = useState<ToolStatus | null>(null);
  const [error, setError] = useState("");
  const [updating, setUpdating] = useState(false);
  const metadataGeneration = useRef(0);

  const busy = progress && !["completed", "cancelled", "failed"].includes(progress.status);

  useEffect(() => {
    backend.getToolStatus().then(setTools).catch((e) => setError(String(e)));
    const stop = listen<DownloadProgress>("download-progress", ({ payload }) => {
      setProgress((current) => current?.id === payload.id
        ? {
            ...current,
            ...payload,
            percent: payload.percent ?? current.percent,
            speed: payload.speed ?? current.speed,
            eta: payload.eta ?? current.eta,
          }
        : payload);
      setHistory((current) => {
        const next = current.map((item) =>
          item.id === payload.id
            ? { ...item, status: payload.status, filePath: payload.filePath ?? item.filePath, error: payload.errorDetail }
            : item,
        );
        localStorage.setItem(HISTORY_KEY, JSON.stringify(next.slice(0, 40)));
        return next;
      });
    });
    return () => {
      stop.then((unlisten) => unlisten());
    };
  }, []);

  useEffect(() => {
    const timer = window.setTimeout(() => {
      if (/^https:\/\/(www\.|m\.)?(youtube\.com|youtu\.be)\//i.test(url)) void fetchMetadata();
    }, 650);
    return () => window.clearTimeout(timer);
    // Deliberately react only to URL edits; cookie changes are applied on the next explicit load.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [url]);

  const toolIssues = useMemo(() => {
    if (!tools) return [];
    return [
      !tools.ytDlp.available && "yt-dlp",
      !tools.ffmpeg.available && "FFmpeg",
      !tools.deno.available && "Deno",
    ].filter(Boolean) as string[];
  }, [tools]);

  async function fetchMetadata() {
    if (!url.trim()) return;
    const generation = ++metadataGeneration.current;
    setLoadingMetadata(true);
    setError("");
    setMetadata(null);
    try {
      const result = await backend.getMetadata(url.trim(), cookies);
      if (generation === metadataGeneration.current) setMetadata(result);
    } catch (e) {
      if (generation === metadataGeneration.current) setError(String(e));
    } finally {
      if (generation === metadataGeneration.current) setLoadingMetadata(false);
    }
  }

  async function chooseFolder() {
    try {
      const selected = await open({ directory: true, multiple: false, title: "Vyber cieľový priečinok" });
      if (typeof selected === "string") setOutputDir(selected);
    } catch (e) {
      setError(String(e));
    }
  }

  async function startDownload() {
    if (!metadata || !outputDir || busy) return;
    const id = crypto.randomUUID();
    const request: DownloadRequest = {
      id,
      url: metadata.webpageUrl,
      outputDir,
      mode,
      quality,
      container,
      audioFormat,
      codec,
      subtitles,
      autoSubtitles,
      subtitleLanguages,
      thumbnail,
      cookiesFromBrowser: cookies,
    };
    const item: HistoryItem = {
      id,
      title: metadata.title,
      thumbnail: metadata.thumbnail,
      outputDir,
      status: "starting",
      createdAt: new Date().toISOString(),
    };
    setHistory((current) => {
      const next = [item, ...current].slice(0, 40);
      localStorage.setItem(HISTORY_KEY, JSON.stringify(next));
      return next;
    });
    setProgress({ id, status: "starting", percent: 0, message: "Pripravujem sťahovanie…" });
    setError("");
    try {
      await backend.startDownload(request);
    } catch (e) {
      setError(String(e));
      setProgress({ id, status: "failed", errorDetail: String(e) });
      setHistory((current) => {
        const next = current.map((entry) => entry.id === id ? { ...entry, status: "failed" as const, error: String(e) } : entry);
        localStorage.setItem(HISTORY_KEY, JSON.stringify(next));
        return next;
      });
    }
  }

  async function updateYtDlp() {
    setUpdating(true);
    setError("");
    try {
      await backend.updateYtDlp();
      setTools(await backend.getToolStatus());
    } catch (e) {
      setError(String(e));
    } finally {
      setUpdating(false);
    }
  }

  const statusLabel = progress?.status === "processing"
    ? "Spracúvam súbor"
    : progress?.status === "completed"
      ? "Hotovo"
      : progress?.status === "cancelled"
        ? "Zrušené"
        : progress?.status === "failed"
          ? "Sťahovanie zlyhalo"
          : "Sťahujem";

  return (
    <div className="app-shell">
      <header className="topbar">
        <div className="brand-mark"><Download size={20} /></div>
        <div>
          <div className="brand-title">
            <h1>YouTube Downloader</h1>
            <span className="version-badge" aria-label={`Verzia ${version}`}>v{version}</span>
          </div>
          <p>Video jednoducho. Kvalita bez kompromisov.</p>
        </div>
        <div className="header-actions">
          <button className="icon-button theme-button" onClick={toggleTheme}
            aria-label="Tmavý režim" aria-pressed={theme === "dark"}
            title={theme === "dark" ? "Prepnúť na svetlý režim" : "Prepnúť na tmavý režim"}>
            {theme === "dark" ? <Sun size={19} /> : <Moon size={19} />}
          </button>
          <button className="icon-button" onClick={() => setAdvanced((v) => !v)} title="Rozšírené nastavenia" aria-label="Rozšírené nastavenia" aria-expanded={advanced}>
            <Settings2 size={19} />
          </button>
        </div>
      </header>

      <main>
        {toolIssues.length > 0 && (
          <div className="notice warning">
            <AlertCircle size={18} />
            <span>{tools?.ffmpeg.error || `Nedostupné nástroje: ${toolIssues.join(", ")}. Pozri README alebo spusti prípravu závislostí.`}</span>
          </div>
        )}
        {error && <div className="notice error"><AlertCircle size={18} /><span>{error}</span><button onClick={() => setError("")}><X size={16} /></button></div>}

        <section className="url-card">
          <label htmlFor="video-url">Odkaz na YouTube video</label>
          <div className="url-row">
            <input
              id="video-url"
              value={url}
              onChange={(e) => {
                metadataGeneration.current += 1;
                setMetadata(null);
                setLoadingMetadata(false);
                setUrl(e.target.value);
              }}
              disabled={!!busy}
              placeholder="https://www.youtube.com/watch?v=…"
              spellCheck={false}
            />
            <button className="secondary" onClick={fetchMetadata} disabled={!url.trim() || loadingMetadata}>
              {loadingMetadata ? <LoaderCircle className="spin" size={18} /> : "Načítať"}
            </button>
          </div>
        </section>

        {advanced && (
          <section className="advanced-panel">
            <Select label="Cookies z prehliadača" value={cookies} onChange={(v) => setCookies(v as CookieBrowser)} options={[["none", "Nepoužiť"], ["edge", "Microsoft Edge"], ["chrome", "Google Chrome"], ["firefox", "Firefox"]]} />
            <p>Použi vlastnú prihlásenú reláciu. Cookies spracuje priamo yt-dlp a aplikácia ich neukladá.</p>
            <button className="text-button" onClick={updateYtDlp} disabled={updating || !!busy || !tools?.ytDlp.available}>
              <RefreshCw className={updating ? "spin" : ""} size={15} /> Aktualizovať yt-dlp
            </button>
          </section>
        )}

        {loadingMetadata && (
          <section className="video-card skeleton-card">
            <div className="skeleton thumb" /><div className="skeleton-lines"><i /><i /><i /></div>
          </section>
        )}

        {metadata && !loadingMetadata && (
          <>
            <section className="video-card">
              <div className="thumbnail-wrap">
                {metadata.thumbnail ? <img src={metadata.thumbnail} alt="Náhľad videa" /> : <FileVideo2 size={42} />}
                <span><Clock3 size={13} /> {formatDuration(metadata.duration)}</span>
              </div>
              <div className="video-copy">
                <p className="eyebrow">{metadata.isLive ? "Živé vysielanie" : "Pripravené na stiahnutie"}</p>
                <h2>{metadata.title}</h2>
                <p>{metadata.uploader ?? "YouTube"}</p>
              </div>
            </section>

            <section className="settings-card">
              <div className="mode-switch" role="tablist">
                <button className={mode === "video" ? "active" : ""} onClick={() => setMode("video")}><FileVideo2 size={17} /> Video</button>
                <button className={mode === "audio" ? "active" : ""} onClick={() => setMode("audio")}><FileAudio size={17} /> Iba audio</button>
              </div>

              <div className="settings-grid">
                {mode === "video" ? (
                  <>
                    <Select label="Kvalita" value={quality} onChange={(v) => setQuality(v as Quality)} options={[
                      ["auto", "Auto (odporúčané)"], ["best", "Najlepšia"], ["4320", "8K"], ["2160", "4K"],
                      ["1440", "1440p"], ["1080", "1080p"], ["720", "720p"],
                    ]} />
                    <Select label="Kontajner" value={container} onChange={(v) => setContainer(v as VideoContainer)} options={[["mp4", "MP4"], ["mkv", "MKV"], ["webm", "WebM"]]} />
                    <Select label="Video kodek" value={codec} onChange={(v) => setCodec(v as Codec)} options={[["auto", "Auto"], ["av1", "AV1"], ["vp9", "VP9"], ["h264", "H.264"]]} />
                  </>
                ) : (
                  <Select label="Audio formát" value={audioFormat} onChange={(v) => setAudioFormat(v as AudioFormat)} options={[["mp3", "MP3"], ["m4a", "M4A"], ["opus", "Opus"]]} />
                )}
              </div>

              <div className="option-row">
                {mode === "video" && <Toggle checked={subtitles} onChange={setSubtitles} icon={<Subtitles size={18} />} label="Titulky" description="Stiahnuť a vložiť do súboru" />}
                <Toggle checked={thumbnail} onChange={setThumbnail} icon={<Image size={18} />} label="Thumbnail" description={(mode === "video" && container === "webm") || (mode === "audio" && audioFormat === "opus") ? "Uložiť náhľad vedľa súboru" : "Uložiť a vložiť náhľad"} />
              </div>

              {subtitles && mode === "video" && (
                <div className="sub-options">
                  <label>Jazyky <input value={subtitleLanguages} onChange={(e) => setSubtitleLanguages(e.target.value)} placeholder="sk.*,en.*" /></label>
                  <label className="check"><input type="checkbox" checked={autoSubtitles} onChange={(e) => setAutoSubtitles(e.target.checked)} /> Aj automatické titulky</label>
                </div>
              )}

              <div className="folder-row">
                <div><span>Cieľový priečinok</span><strong>{outputDir || "Zatiaľ nevybraný"}</strong></div>
                <button className="secondary" onClick={chooseFolder} disabled={!!busy}><FolderOpen size={17} /> Vybrať</button>
              </div>

              <button className="primary download-button" onClick={startDownload} disabled={!outputDir || !!busy}>
                <Download size={19} /> Stiahnuť {mode === "video" ? "video" : "audio"}
              </button>
            </section>
          </>
        )}

        {progress && (
          <section className={`progress-card ${progress.status}`}>
            <div className="progress-head">
              <div>{progress.status === "completed" ? <CheckCircle2 size={20} /> : progress.status === "failed" ? <AlertCircle size={20} /> : <LoaderCircle className={busy ? "spin" : ""} size={20} />}<strong>{statusLabel}</strong></div>
              {busy && <button className="cancel" onClick={() => backend.cancelDownload(progress.id).catch((e) => setError(String(e)))}>Zrušiť</button>}
            </div>
            <div className="progress-track"><div style={{ width: `${progress.percent ?? 0}%` }} /></div>
            <div className="progress-meta">
              <b>{Math.round(progress.percent ?? 0)} %</b><span>{progress.speed || progress.message || "—"}</span><span>{progress.eta ? `ETA ${progress.eta}` : ""}</span>
            </div>
            {progress.status === "failed" && <p className="failure-detail">{errorMessages[progress.errorCode ?? ""] ?? progress.errorDetail ?? "Neznáma chyba yt-dlp."}</p>}
            {progress.status === "completed" && <button className="text-button" onClick={() => backend.openFolder(outputDir).catch((e) => setError(String(e)))}><FolderOpen size={16} /> Otvoriť priečinok</button>}
          </section>
        )}

        <section className="history-card">
          <div className="section-title"><div><History size={19} /><h2>História</h2></div>{history.length > 0 && <button onClick={() => { setHistory([]); localStorage.removeItem(HISTORY_KEY); }}>Vymazať</button>}</div>
          {history.length === 0 ? <p className="empty">Stiahnuté videá sa zobrazia tu.</p> : history.slice(0, 8).map((item) => (
            <article className="history-item" key={item.id}>
              {item.thumbnail ? <img src={item.thumbnail} alt="" /> : <div className="history-placeholder"><FileVideo2 size={18} /></div>}
              <div><strong>{item.title}</strong><span>{new Date(item.createdAt).toLocaleString("sk-SK")}</span></div>
              <span className={`status-dot ${item.status}`} title={item.status} />
              <button className="icon-button small" onClick={() => backend.openFolder(item.outputDir).catch((e) => setError(String(e)))} title="Otvoriť priečinok"><ExternalLink size={15} /></button>
            </article>
          ))}
        </section>
      </main>
      <footer>YouTube Downloader je nezávislá aplikácia používajúca yt-dlp a FFmpeg, bez prepojenia na YouTube. Sťahuj iba obsah, na ktorý máš právo.</footer>
    </div>
  );
}

function Select({ label, value, onChange, options }: { label: string; value: string; onChange: (value: string) => void; options: string[][] }) {
  return <label className="select-field"><span>{label}</span><div><select value={value} onChange={(e) => onChange(e.target.value)}>{options.map(([v, text]) => <option key={v} value={v}>{text}</option>)}</select><ChevronDown size={15} /></div></label>;
}

function Toggle({ checked, onChange, icon, label, description }: { checked: boolean; onChange: (v: boolean) => void; icon: ReactNode; label: string; description: string }) {
  return <label className="toggle-card"><span className="option-icon">{icon}</span><span className="option-copy"><strong>{label}</strong><small>{description}</small></span><input type="checkbox" checked={checked} onChange={(e) => onChange(e.target.checked)} /><i /></label>;
}

export default App;
