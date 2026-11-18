//! Tauri 命令层：前端通过 invoke 调用。
use std::path::Path;

use tauri::{AppHandle, Emitter, State};
use tauri_plugin_dialog::DialogExt;

use crate::downloader::engine::TaskManager;
use crate::downloader::types::{
    BatchSummary, MirrorProbe, ProbeResult, Settings, TaskSnapshot,
};

#[tauri::command]
pub async fn probe_url(state: State<'_, TaskManager>, url: String) -> Result<ProbeResult, String> {
    state.inner.probe_url(&url).await
}

/// 网易云歌单一键批量：免费歌自动建任务，VIP 跳过
#[tauri::command]
pub async fn add_netease_playlist(
    state: State<'_, TaskManager>,
    url: String,
    dir: Option<String>,
    connections: Option<u32>,
) -> Result<BatchSummary, String> {
    state
        .inner
        .add_netease_playlist(url, dir, connections)
        .await
}

/// 镜像测速：并行采样各镜像 256KB，返回可达性与速度
#[tauri::command]
pub async fn probe_mirrors(
    state: State<'_, TaskManager>,
    urls: Vec<String>,
) -> Result<Vec<MirrorProbe>, String> {
    Ok(state.inner.probe_mirrors(urls).await)
}

#[tauri::command]
pub async fn add_task(
    state: State<'_, TaskManager>,
    url: String,
    mirrors: Option<Vec<String>>,
    filename: Option<String>,
    dir: Option<String>,
    connections: Option<u32>,
) -> Result<TaskSnapshot, String> {
    let mirrors: Vec<String> = mirrors.unwrap_or_default().into_iter().collect();
    // 弹窗探测成功后 add_task 会再探一次（瞬态站点可能此时失败），
    // 失败自动重试一次，与前端重试语义对齐
    match state
        .inner
        .add_task(url.clone(), mirrors.clone(), filename.clone(), dir.clone(), connections)
        .await
    {
        Ok(t) => Ok(t),
        Err(e) if e.contains("连接失败") || e.contains("无法访问") => {
            tokio::time::sleep(std::time::Duration::from_millis(900)).await;
            state
                .inner
                .add_task(url, mirrors, filename, dir, connections)
                .await
        }
        Err(e) => Err(e),
    }
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
pub async fn clear_completed_tasks(
    state: State<'_, TaskManager>,
) -> Result<u32, String> {
    state.inner.clear_completed().await
}

#[tauri::command]
pub async fn pause_all_tasks(state: State<'_, TaskManager>) -> Result<(), String> {
    state.inner.pause_all().await;
    Ok(())
}

#[tauri::command]
pub async fn resume_all_tasks(state: State<'_, TaskManager>) -> Result<(), String> {
    state.inner.resume_all().await;
    Ok(())
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

/// 选择本地 .torrent 种子文件（桌面端）
#[tauri::command]
pub async fn pick_torrent_file(app: AppHandle) -> Result<Option<String>, String> {
    #[cfg(any(target_os = "ios", target_os = "android"))]
    {
        let _ = app;
        Ok(None)
    }
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    {
        let (tx, rx) = tokio::sync::oneshot::channel::<Option<String>>();
        app.dialog()
            .file()
            .add_filter("种子文件", &["torrent"])
            .pick_file(move |p| {
                let s = p.map(|f| match f {
                    tauri_plugin_dialog::FilePath::Path(pb) => pb.display().to_string(),
                    tauri_plugin_dialog::FilePath::Url(u) => u.to_string(),
                });
                let _ = tx.send(s);
            });
        rx.await.map_err(|e| e.to_string())
    }
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

// ---------- 应用自动更新 ----------

#[derive(serde::Serialize)]
pub struct UpdateInfo {
    pub available: bool,
    pub version: Option<String>,
    pub notes: Option<String>,
}

/// 检查更新（GitHub Releases latest.json）
#[tauri::command]
pub async fn check_for_update(app: AppHandle) -> Result<UpdateInfo, String> {
    use tauri_plugin_updater::UpdaterExt;
    let update = app
        .updater_builder()
        .build()
        .map_err(|e| e.to_string())?
        .check()
        .await
        .map_err(|e| e.to_string())?;
    Ok(match update {
        Some(u) => UpdateInfo {
            available: true,
            version: Some(u.version.clone()),
            notes: u.body.clone(),
        },
        None => UpdateInfo {
            available: false,
            version: None,
            notes: None,
        },
    })
}

/// 下载并安装更新，完成后自动重启
#[tauri::command]
pub async fn install_update(app: AppHandle) -> Result<(), String> {
    use tauri_plugin_updater::UpdaterExt;
    let update = app
        .updater_builder()
        .build()
        .map_err(|e| e.to_string())?
        .check()
        .await
        .map_err(|e| e.to_string())?
        .ok_or("当前已是最新版本")?;

    let app2 = app.clone();
    let mut downloaded: u64 = 0;
    update
        .download_and_install(
            move |chunk, total| {
                downloaded += chunk as u64;
                let _ = app2.emit("update://progress", (downloaded, total));
            },
            || {},
        )
        .await
        .map_err(|e| e.to_string())?;
    // 安装成功后重启应用加载新版本
    app.restart();
    #[allow(unreachable_code)]
    Ok(())
}
