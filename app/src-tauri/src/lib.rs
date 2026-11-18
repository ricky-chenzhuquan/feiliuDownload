//! 飞流下载 — 应用入口
mod commands;
mod downloader;

use downloader::engine::{set_app_handle, TaskManager};

#[cfg(desktop)]
use tauri::{
    menu::{Menu, MenuItem},
    tray::{TrayIconBuilder, TrayIconEvent},
};
use tauri::{Manager, WindowEvent};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(TaskManager::new())
        .on_window_event(|window, event| {
            // 桌面端：关闭窗口时最小化到托盘（可在设置中关闭），下载继续
            if let WindowEvent::CloseRequested { api, .. } = event {
                #[cfg(desktop)]
                {
                    let app = window.app_handle();
                    let keep = app
                        .state::<TaskManager>()
                        .inner
                        .settings()
                        .close_to_tray;
                    if keep {
                        api.prevent_close();
                        let _ = window.hide();
                    }
                }
                #[cfg(not(desktop))]
                {
                    let _ = api;
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::probe_url,
            commands::probe_mirrors,
            commands::add_netease_playlist,
            commands::add_task,
            commands::pause_task,
            commands::resume_task,
            commands::pause_all_tasks,
            commands::resume_all_tasks,
            commands::delete_task,
            commands::list_tasks,
            commands::get_settings,
            commands::set_settings,
            commands::pick_folder,
            commands::pick_torrent_file,
            commands::is_mobile,
            commands::reveal_path,
            commands::check_for_update,
            commands::install_update,
            commands::probe_mirrors,
        ])
        .setup(|app| {
            set_app_handle(app.handle().clone());
            let mgr = app.state::<TaskManager>();
            mgr.inner.set_app(app.handle().clone());

            // dev 构建：启动 6 秒后自动执行一次 WebView 代取自检（82dj），
            // 用于端到端定位「卡添加中」——结果写入日志 [feiliu-selftest]
            #[cfg(debug_assertions)]
            {
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_secs(6)).await;
                    eprintln!("[feiliu-selftest] 开始 WebView 代取自检（82dj/play/31534）");
                    let plan = downloader::extract::WebviewExtractPlan {
                        url: "https://www.82dj.com/play/31534".into(),
                        js: r##"(()=>{const a=document.querySelector("audio");return a?JSON.stringify(a.currentSrc||a.src||""):"";})()"##.into(),
                        timeout_ms: 15_000,
                    };
                    let started = std::time::Instant::now();
                    match downloader::webview_extract::extract(handle, &plan).await {
                        Ok(u) => eprintln!(
                            "[feiliu-selftest] 成功 {:.1}s 直链长度={}",
                            started.elapsed().as_secs_f64(),
                            u.len()
                        ),
                        Err(e) => eprintln!(
                            "[feiliu-selftest] 失败 {:.1}s err={}",
                            started.elapsed().as_secs_f64(),
                            e
                        ),
                    }
                });
            }

            // 桌面端：托盘常驻（左键点击显示窗口，菜单可退出）
            #[cfg(desktop)]
            {
                let show = MenuItem::with_id(app, "show", "显示主窗口", true, None::<&str>)?;
                let quit = MenuItem::with_id(app, "quit", "退出飞流下载", true, None::<&str>)?;
                let menu = Menu::with_items(app, &[&show, &quit])?;
                TrayIconBuilder::with_id("feiliu-tray")
                    .icon(app.default_window_icon().unwrap().clone())
                    .tooltip("飞流下载 FeiLiu")
                    .menu(&menu)
                    .show_menu_on_left_click(false)
                    .on_tray_icon_event(|tray, event| {
                        if let TrayIconEvent::Click {
                            button: tauri::tray::MouseButton::Left,
                            button_state: tauri::tray::MouseButtonState::Up,
                            ..
                        } = event
                        {
                            let app = tray.app_handle();
                            if let Some(w) = app.get_webview_window("main") {
                                let _ = w.show();
                                let _ = w.set_focus();
                            }
                        }
                    })
                    .on_menu_event(|app, event| match event.id.as_ref() {
                        "show" => {
                            if let Some(w) = app.get_webview_window("main") {
                                let _ = w.show();
                                let _ = w.set_focus();
                            }
                        }
                        "quit" => app.exit(0),
                        _ => {}
                    })
                    .build(app)?;
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
