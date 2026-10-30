export type Quality = "auto" | "best" | "4320" | "2160" | "1440" | "1080" | "720";
export type VideoContainer = "mp4" | "mkv" | "webm";
export type AudioFormat = "mp3" | "m4a" | "opus";
export type Codec = "auto" | "av1" | "vp9" | "h264";
export type CookieBrowser = "none" | "edge" | "chrome" | "firefox";

export interface VideoMetadata {
  id: string;
  title: string;
  uploader?: string;
  thumbnail?: string;
  duration?: number;
  webpageUrl: string;
  isLive: boolean;
}

export interface ToolStatus {
  ytDlp: ToolInfo;
  ffmpeg: ToolInfo;
  deno: ToolInfo;
}

export interface ToolInfo {
  available: boolean;
  version?: string;
  path?: string;
  source?: string;
  error?: string;
}

export interface DownloadRequest {
  id: string;
  url: string;
  outputDir: string;
  mode: "video" | "audio";
  quality: Quality;
  container: VideoContainer;
  audioFormat: AudioFormat;
  codec: Codec;
  subtitles: boolean;
  autoSubtitles: boolean;
  subtitleLanguages: string;
  thumbnail: boolean;
  cookiesFromBrowser: CookieBrowser;
}

export interface DownloadProgress {
  id: string;
  status: "starting" | "downloading" | "processing" | "completed" | "cancelled" | "failed";
  percent?: number;
  speed?: string;
  eta?: string;
  message?: string;
  filePath?: string;
  errorCode?: string;
  errorDetail?: string;
}

export interface HistoryItem {
  id: string;
  title: string;
  thumbnail?: string;
  outputDir: string;
  status: DownloadProgress["status"];
  createdAt: string;
  filePath?: string;
  error?: string;
}
