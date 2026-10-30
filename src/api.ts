import { invoke } from "@tauri-apps/api/core";
import type { DownloadRequest, ToolStatus, VideoMetadata } from "./types";

export const backend = {
  getToolStatus: () => invoke<ToolStatus>("get_tool_status"),
  getMetadata: (url: string, cookiesFromBrowser: string) =>
    invoke<VideoMetadata>("get_metadata", { url, cookiesFromBrowser }),
  startDownload: (request: DownloadRequest) => invoke<void>("start_download", { request }),
  cancelDownload: (id: string) => invoke<void>("cancel_download", { id }),
  openFolder: (path: string) => invoke<void>("open_folder", { path }),
  updateYtDlp: () => invoke<string>("update_yt_dlp"),
};
