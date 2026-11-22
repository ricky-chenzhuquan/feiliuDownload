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
            commands::add_bilibili_collection,
            commands::add_lanzou_folder,
            commands::add_gofile_batch,
            commands::add_uc_batch,
            commands::add_ct_batch,
            commands::add_bandcamp_album,
            commands::add_bilibili_bangumi,
            commands::add_task,
            commands::pause_task,
            commands::resume_task,
            commands::pause_all_tasks,
            commands::clear_completed_tasks,
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
            commands::get_stats,
        ])
        .setup(|app| {
            set_app_handle(app.handle().clone());
            let mgr = app.state::<TaskManager>();
            mgr.inner.set_app(app.handle().clone());

            // dev 构建：启动 6 秒后自动执行一次 WebView 代取自检（82dj），
            // 用于端到端定位「卡添加中」——结果写入日志 [feiliu-selftest]
            #[cfg(debug_assertions)]
            if std::env::var("FEILIU_SELFTEST").is_ok() {
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_secs(6)).await;
                    eprintln!("[feiliu-selftest] 开始 WebView 代取自检（82dj/play/31534）");
                    let plan = downloader::extract::WebviewExtractPlan {
                        url: "https://www.82dj.com/play/31534".into(),
                        visible: false,
                        js: r##"(()=>{const a=document.querySelector("audio");return a?JSON.stringify(a.currentSrc||a.src||""):"";})()"##.into(),
                        timeout_ms: 15_000,
                    };
                    let started = std::time::Instant::now();
                    match downloader::webview_extract::extract(handle, &plan).await {
                        Ok(u) => eprintln!(
                            "[feiliu-selftest] 成功 {:.1}s 直链长度={} 尾10={:?} 尾部污染={} cookie={} name={}",
                            started.elapsed().as_secs_f64(),
                            u.url.len(),
                            &u.url[u.url.len().saturating_sub(10)..],
                            u.url.ends_with('\\') || u.url.ends_with('"'),
                            u.cookie.as_deref().map(|c| c.len()).unwrap_or(0),
                            u.name.as_deref().unwrap_or("-"),
                        ),
                        Err(e) => eprintln!(
                            "[feiliu-selftest] 失败 {:.1}s err={}",
                            started.elapsed().as_secs_f64(),
                            e
                        ),
                    }
                });
            }

                // 自检 2：新站点解析 → 真实引擎 probe_url 链路（不下载，只探测）
                // 覆盖：archive.org 视频/音频、Bandcamp 流、番剧免费集、番剧会员集提示
                let probe_handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_secs(7)).await;
                    let mgr = probe_handle.state::<TaskManager>();
                    let cases: [(&str, &str, &str); 4] = [
                        ("archive-video", "https://archive.org/details/BigBuckBunny_124", "archive.org/download"),
                        ("bandcamp-track", "https://iksre.bandcamp.com/track/lake-shore", "bcbits.com"),
                        ("bangumi-free", "https://www.bilibili.com/bangumi/play/ep70866", "bilivideo.com"),
                        ("bangumi-paid", "https://www.bilibili.com/bangumi/play/ep72067", "会员"),
                    ];
                    for (label, url, expect) in cases {
                        let started = std::time::Instant::now();
                        match mgr.inner.probe_url(url).await {
                            Ok(p) => {
                                let hit = if expect == "会员" {
                                    p.notice.as_deref().map_or(false, |n| n.contains("会员"))
                                } else {
                                    p.url.contains(expect) || p.filename.contains(expect)
                                };
                                eprintln!(
                                    "[feiliu-selftest] {label}: {} {:.1}s 大小={} 文件名={:?} 提示={:?} 期望命中={}",
                                    if hit { "成功" } else { "未命中期望" },
                                    started.elapsed().as_secs_f64(),
                                    p.total_size,
                                    p.filename,
                                    p.notice.as_deref().unwrap_or("-"),
                                    hit
                                );
                            }
                            Err(e) => eprintln!(
                                "[feiliu-selftest] {label}: 失败 {:.1}s err={e}",
                                started.elapsed().as_secs_f64()
                            ),
                        }
                    }
                });

                // 自检 3：GoFile JSON 通道端到端——WebView 代取（JSON{url,cookie,name}）
                // → 引擎式 reqwest 下载裁决（带/不带会话 cookie 各一次）。
                // 结论写日志 [feiliu-selftest]，决定 gofile 提取器接入与 blob 兜底取舍。
                #[cfg(debug_assertions)]
                {
                    let gof_handle = app.handle().clone();
                    tauri::async_runtime::spawn(async move {
                        tokio::time::sleep(std::time::Duration::from_secs(9)).await;
                        eprintln!("[feiliu-selftest] 开始 GoFile JSON 通道自检（d1AQzWks，生产 resolve 链路）");
                        // 走 gofile::resolve 生产提取器（列表形态 plan.js），非内联旧版
                        let plan = downloader::extract::gofile::resolve("https://gofile.io/d/d1AQzWks")
                            .await
                            .and_then(|e| e.webview_plan)
                            .ok_or("gofile resolve 未产出代取计划")
                            .expect("gofile resolve 失败");
                        let started = std::time::Instant::now();
                        let wv = match downloader::webview_extract::extract(gof_handle, &plan).await {
                            Ok(w) => w,
                            Err(e) => {
                                eprintln!("[feiliu-selftest] gofile-extract: 失败 {:.1}s err={e}", started.elapsed().as_secs_f64());
                                return;
                            }
                        };
                        eprintln!(
                            "[feiliu-selftest] gofile-extract: 成功 {:.1}s url={} cookie_len={} name={} 清单文件数={}",
                            started.elapsed().as_secs_f64(),
                            &wv.url[..60.min(wv.url.len())],
                            wv.cookie.as_deref().map(|c| c.len()).unwrap_or(0),
                            wv.name.as_deref().unwrap_or("-"),
                            wv.files.len(),
                        );
                        // 引擎式下载裁决：带/不带 cookie 各取 1KB
                        let ua = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 Chrome/126.0 Safari/537.36";
                        let client = match reqwest::Client::builder().user_agent(ua).redirect(reqwest::redirect::Policy::none()).build() {
                            Ok(c) => c,
                            Err(e) => { eprintln!("[feiliu-selftest] gofile-client: {e}"); return; }
                        };
                        for (label, cookie) in [
                            ("带cookie", wv.cookie.clone()),
                            ("裸链", None::<String>),
                        ] {
                            let mut req = client.get(&wv.url).header("Range", "bytes=0-1023").header("Referer", "https://gofile.io/");
                            if let Some(c) = &cookie { req = req.header("Cookie", c); }
                            match req.send().await {
                                Ok(r) => eprintln!(
                                    "[feiliu-selftest] gofile-download({label}): HTTP {} location={:?} type={:?}",
                                    r.status(),
                                    r.headers().get("location").and_then(|v| v.to_str().ok()).map(|s| s[..60.min(s.len())].to_string()),
                                    r.headers().get("content-type").and_then(|v| v.to_str().ok()),
                                ),
                                Err(e) => eprintln!("[feiliu-selftest] gofile-download({label}): err={e}"),
                            }
                        }
                    });
                }

                // 自检 3b：城通 WebView 代取端到端（ALTCHA 自动验证 + api2 新链路）
                #[cfg(debug_assertions)]
                {
                    let ct_handle = app.handle().clone();
                    tauri::async_runtime::spawn(async move {
                        tokio::time::sleep(std::time::Duration::from_secs(10)).await;
                        eprintln!("[feiliu-selftest] 开始城通 WebView 自检（/f/9603111-996209741-b3814a 单文件链路）");
                        let plan = downloader::extract::ct::resolve("https://url11.ctfile.com/f/9603111-996209741-b3814a?p=5425")
                            .await
                            .and_then(|e| e.webview_plan)
                            .ok_or("ct resolve 未产出代取计划")
                            .expect("ct resolve 失败");
                        let started = std::time::Instant::now();
                        let wv = match downloader::webview_extract::extract(ct_handle, &plan).await {
                            Ok(w) => w,
                            Err(e) => {
                                eprintln!("[feiliu-selftest] ct-extract: 失败 {:.1}s err={e}", started.elapsed().as_secs_f64());
                                return;
                            }
                        };
                        eprintln!(
                            "[feiliu-selftest] ct-extract: 成功 {:.1}s url={} name={}",
                            started.elapsed().as_secs_f64(),
                            &wv.url[..90.min(wv.url.len())],
                            wv.name.as_deref().unwrap_or("-"),
                        );
                        let client = match reqwest::Client::builder().user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36").build() {
                            Ok(c) => c,
                            Err(e) => { eprintln!("[feiliu-selftest] ct-client: {e}"); return; }
                        };
                        match client.get(&wv.url).header("Range", "bytes=0-1023").header("Referer", "https://url11.ctfile.com/").send().await {
                            Ok(r) => {
                                let status = r.status();
                                let ctype = r.headers().get("content-type").and_then(|v| v.to_str().ok()).map(String::from);
                                let n = r.bytes().await.map(|b| b.len()).unwrap_or(0);
                                eprintln!("[feiliu-selftest] ct-download: HTTP {status} type={ctype:?} bytes={n}");
                            }
                            Err(e) => eprintln!("[feiliu-selftest] ct-download: err={e}"),
                        }
                    });
                }

                // 自检 4：微博 WebView 代取真机全链（游客墙→statuses/show→CDN 裁决）
                #[cfg(debug_assertions)]
                {
                    let wb_handle = app.handle().clone();
                    tauri::async_runtime::spawn(async move {
                        tokio::time::sleep(std::time::Duration::from_secs(11)).await;
                        eprintln!("[feiliu-selftest] 开始微博 WebView 代取自检（Rse19cSin，生产 resolve 链路）");
                        let plan = downloader::extract::weibo::resolve("https://m.weibo.cn/status/Rse19cSin")
                            .await
                            .and_then(|e| e.webview_plan)
                            .ok_or("weibo resolve 未产出代取计划")
                            .expect("weibo resolve 失败");
                        let started = std::time::Instant::now();
                        let wv = match downloader::webview_extract::extract(wb_handle, &plan).await {
                            Ok(w) => w,
                            Err(e) => {
                                eprintln!("[feiliu-selftest] weibo-extract: 失败 {:.1}s err={e}", started.elapsed().as_secs_f64());
                                return;
                            }
                        };
                        eprintln!(
                            "[feiliu-selftest] weibo-extract: 成功 {:.1}s url={} name={}",
                            started.elapsed().as_secs_f64(),
                            &wv.url[..60.min(wv.url.len())],
                            wv.name.as_deref().unwrap_or("-"),
                        );
                        // 引擎式下载裁决：Referer + Range 1KB
                        let client = match reqwest::Client::builder().user_agent("Mozilla/5.0 (iPhone; CPU iPhone OS 16_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/16.0 Mobile/15E148 Safari/604.1").redirect(reqwest::redirect::Policy::limited(5)).build() {
                            Ok(c) => c,
                            Err(e) => { eprintln!("[feiliu-selftest] weibo-client: {e}"); return; }
                        };
                        match client.get(&wv.url)
                            .header("Referer", "https://m.weibo.cn/")
                            .header("Range", "bytes=0-1023")
                            .send().await
                        {
                            Ok(r) => eprintln!(
                                "[feiliu-selftest] weibo-download: HTTP {} type={:?}",
                                r.status(),
                                r.headers().get("content-type").and_then(|v| v.to_str().ok()),
                            ),
                            Err(e) => eprintln!("[feiliu-selftest] weibo-download: err={e}"),
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
