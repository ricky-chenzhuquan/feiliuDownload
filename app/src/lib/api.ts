//! Tauri 后端 API 封装（字段与 Rust serde 序列化一致，snake_case）
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export type TaskStatus =
  | "queued"
  | "downloading"
  | "paused"
  | "merging"
  | "completed"
  | "failed";

export type TaskKind = "http" | "torrent";

export interface TaskSnapshot {
  id: string;
  url: string;
  source_url?: string;
  filename: string;
  save_path: string;
  total_size: number;
  downloaded: number;
  speed_bps: number;
  eta_seconds: number | null;
  connections: number;
  sources: number;
  kind: TaskKind;
  status: TaskStatus;
  error: string | null;
  created_at: number;
}

export interface ProbeResult {
  url: string;
  filename: string;
  total_size: number;
  accept_ranges: boolean;
  notice: string | null;
  batch_kind:
    | "netease_playlist"
    | "bilibili_collection"
    | "lanzou_folder"
    | "gofile_share"
    | "bandcamp_album"
    | "bilibili_bangumi"
    | null;
}

export interface BatchSummary {
  playlist: string;
  total: number;
  added: number;
  skipped: number;
  truncated: boolean;
}

export interface Settings {
  download_dir: string;
  max_connections: number;
  max_concurrent_tasks: number;
  auto_resume_on_launch: boolean;
  clipboard_watch: boolean;
  close_to_tray: boolean;
  batch_limit: number;
}

export interface MirrorProbe {
  url: string;
  ok: boolean;
  speed_bps: number;
}

export type TaskEvent =
  | { type: "progress"; task: TaskSnapshot }
  | { type: "list"; tasks: TaskSnapshot[] };

export function onTaskEvent(handler: (e: TaskEvent) => void) {
  return listen<TaskEvent>("task://events", (evt) => handler(evt.payload));
}

export const probeUrl = (url: string) =>
  invoke<ProbeResult>("probe_url", { url });

export const probeMirrors = (urls: string[]) =>
  invoke<MirrorProbe[]>("probe_mirrors", { urls });

export const addNeteasePlaylist = (
  url: string,
  dir?: string,
  connections?: number,
) => invoke<BatchSummary>("add_netease_playlist", { url, dir, connections });

export const addBilibiliCollection = (
  url: string,
  dir?: string,
  connections?: number,
) =>
  invoke<BatchSummary>("add_bilibili_collection", { url, dir, connections });

export const addLanzouFolder = (
  url: string,
  dir?: string,
  connections?: number,
) => invoke<BatchSummary>("add_lanzou_folder", { url, dir, connections });

export const addGofileBatch = (
  url: string,
  dir?: string,
  connections?: number,
) => invoke<BatchSummary>("add_gofile_batch", { url, dir, connections });

export const addBandcampAlbum = (
  url: string,
  dir?: string,
  connections?: number,
) => invoke<BatchSummary>("add_bandcamp_album", { url, dir, connections });

export const addBilibiliBangumi = (
  url: string,
  dir?: string,
  connections?: number,
) => invoke<BatchSummary>("add_bilibili_bangumi", { url, dir, connections });

export const addTask = (
  url: string,
  mirrors: string[],
  filename?: string,
  dir?: string,
  connections?: number,
) => invoke<TaskSnapshot>("add_task", { url, mirrors, filename, dir, connections });

export const pauseTask = (id: string) => invoke<void>("pause_task", { id });

export const resumeTask = (id: string) => invoke<void>("resume_task", { id });

export const pauseAllTasks = () => invoke<void>("pause_all_tasks");

export interface LifetimeStats {
  lifetime_downloaded: number;
  lifetime_tasks: number;
}

export const getStats = () => invoke<LifetimeStats>("get_stats");

export const clearCompletedTasks = () =>
  invoke<number>("clear_completed_tasks");

export const resumeAllTasks = () => invoke<void>("resume_all_tasks");

export const deleteTask = (id: string, deleteFile: boolean) =>
  invoke<void>("delete_task", { id, deleteFile });

export const listTasks = () => invoke<TaskSnapshot[]>("list_tasks");

export const getSettings = () => invoke<Settings>("get_settings");

export const setSettings = (settings: Settings) =>
  invoke<Settings>("set_settings", { settings });

export const pickFolder = () => invoke<string | null>("pick_folder");

export const pickTorrentFile = () =>
  invoke<string | null>("pick_torrent_file");

export const isMobile = () => invoke<boolean>("is_mobile");

// ---------- 应用更新 ----------

export interface UpdateInfo {
  available: boolean;
  version: string | null;
  notes: string | null;
}

export const checkForUpdate = () => invoke<UpdateInfo>("check_for_update");

export const installUpdate = () => invoke<void>("install_update");

export const onUpdateProgress = (
  handler: (downloaded: number, total: number | null) => void,
) =>
  listen<[number, number | null]>("update://progress", (e) =>
    handler(e.payload[0], e.payload[1]),
  );

// ---------- 系统通知 ----------

export async function notify(title: string, body: string) {
  try {
    const { isPermissionGranted, requestPermission, sendNotification } =
      await import("@tauri-apps/plugin-notification");
    let granted = await isPermissionGranted();
    if (!granted) {
      granted = (await requestPermission()) === "granted";
    }
    if (granted) sendNotification({ title, body });
  } catch {
    /* 通知不可用时静默跳过 */
  }
}

// ---------- 剪贴板 ----------

export async function readClipboard(): Promise<string | null> {
  try {
    const { readText } = await import("@tauri-apps/plugin-clipboard-manager");
    return await readText();
  } catch {
    return null;
  }
}

export const revealPath = (path: string) =>
  invoke<void>("reveal_path", { path });
