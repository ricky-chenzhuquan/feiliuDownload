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

export interface TaskSnapshot {
  id: string;
  url: string;
  filename: string;
  save_path: string;
  total_size: number;
  downloaded: number;
  speed_bps: number;
  eta_seconds: number | null;
  connections: number;
  sources: number;
  status: TaskStatus;
  error: string | null;
  created_at: number;
}

export interface ProbeResult {
  url: string;
  filename: string;
  total_size: number;
  accept_ranges: boolean;
}

export interface Settings {
  download_dir: string;
  max_connections: number;
  max_concurrent_tasks: number;
  auto_resume_on_launch: boolean;
}

export type TaskEvent =
  | { type: "progress"; task: TaskSnapshot }
  | { type: "list"; tasks: TaskSnapshot[] };

export function onTaskEvent(handler: (e: TaskEvent) => void) {
  return listen<TaskEvent>("task://events", (evt) => handler(evt.payload));
}

export const probeUrl = (url: string) =>
  invoke<ProbeResult>("probe_url", { url });

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

export const resumeAllTasks = () => invoke<void>("resume_all_tasks");

export const deleteTask = (id: string, deleteFile: boolean) =>
  invoke<void>("delete_task", { id, deleteFile });

export const listTasks = () => invoke<TaskSnapshot[]>("list_tasks");

export const getSettings = () => invoke<Settings>("get_settings");

export const setSettings = (settings: Settings) =>
  invoke<Settings>("set_settings", { settings });

export const pickFolder = () => invoke<string | null>("pick_folder");

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

export const revealPath = (path: string) =>
  invoke<void>("reveal_path", { path });
