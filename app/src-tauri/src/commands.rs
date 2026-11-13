//! Tauri 命令层：前端通过 invoke 调用。
use std::path::Path;

use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;

use crate::downloader::engine::TaskManager;
use crate::downloader::types::{ProbeResult, Settings, TaskSnapshot};

#[tauri::command]
pub async fn probe_url(state: State<'_, TaskManager>, url: String) -> Result<ProbeResult, String> {
    state.inner.probe_url(&url).await
}

#[tauri::command]
pub async fn add_task(
    state: State<'_, TaskManager>,
    url: String,
    filename: Option<String>,
    dir: Option<String>,
    connections: Option<u32>,
) -> Result<TaskSnapshot, String> {
    state.inner.add_task(url, filename, dir, connections).await
}

#[tauri::command]
pub async fn pause_task(state: State<'_, TaskManager>, id: String) -> Result<(), String> {
    state.inner.pause_task(&id).await
}

#[tauri::command]
pub async fn resume_task(state: State<'_, TaskManager>, id: String) -> Result<(), String> {
    state.inner.resume_task(&id).await
}

#[tauri::command]
pub async fn delete_task(
    state: State<'_, TaskManager>,
    id: String,
    delete_file: bool,
) -> Result<(), String> {
    state.inner.delete_task(&id, delete_file).await
}

#[tauri::command]
pub fn list_tasks(state: State<'_, TaskManager>) -> Vec<TaskSnapshot> {
    state.inner.snapshots()
}

#[tauri::command]
pub fn get_settings(state: State<'_, TaskManager>) -> Settings {
    state.inner.settings()
}

#[tauri::command]
pub async fn set_settings(
    state: State<'_, TaskManager>,
    settings: Settings,
) -> Result<Settings, String> {
    Ok(state.inner.update_settings(settings))
}

/// 弹出系统文件夹选择框（仅桌面端；移动端无此能力，返回 None）
#[tauri::command]
pub async fn pick_folder(app: AppHandle) -> Result<Option<String>, String> {
    #[cfg(any(target_os = "ios", target_os = "android"))]
    {
        let _ = app;
        Ok(None)
    }
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    {
        let (tx, rx) = tokio::sync::oneshot::channel::<Option<String>>();
        app.dialog().file().pick_folder(move |p| {
            let s = p.map(|f| match f {
                tauri_plugin_dialog::FilePath::Path(pb) => pb.display().to_string(),
                tauri_plugin_dialog::FilePath::Url(u) => u.to_string(),
            });
            let _ = tx.send(s);
        });
        rx.await.map_err(|e| e.to_string())
    }
}

/// 前端判断是否移动端（用于隐藏桌面专属 UI）
#[tauri::command]
pub fn is_mobile() -> bool {
    cfg!(any(target_os = "ios", target_os = "android"))
}

/// 在系统文件管理器中显示文件（下载完成后「打开所在文件夹」）
#[tauri::command]
pub async fn reveal_path(_app: AppHandle, path: String) -> Result<(), String> {
    tauri_plugin_opener::reveal_item_in_dir(Path::new(&path)).map_err(|e| e.to_string())
}
