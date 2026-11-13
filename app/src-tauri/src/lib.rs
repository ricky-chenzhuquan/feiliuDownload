//! 飞流下载 — 应用入口
mod commands;
mod downloader;

use downloader::engine::{set_app_handle, TaskManager};
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(TaskManager::new())
        .invoke_handler(tauri::generate_handler![
            commands::probe_url,
            commands::add_task,
            commands::pause_task,
            commands::resume_task,
            commands::delete_task,
            commands::list_tasks,
            commands::get_settings,
            commands::set_settings,
            commands::pick_folder,
            commands::is_mobile,
            commands::reveal_path,
        ])
        .setup(|app| {
            set_app_handle(app.handle().clone());
            let mgr = app.state::<TaskManager>();
            mgr.inner.set_app(app.handle().clone());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
