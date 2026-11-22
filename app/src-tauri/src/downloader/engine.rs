//! 多连接下载引擎：动态分片队列 + N 连接并发 + 暂停/续传/取消。
//!
//! 核心思路（沿用 scripts/pdl.py 的验证过的方案）：
//! - 8MiB 分片塞进共享队列，每个连接取一段下一段，快连接自然多拿
//! - 分片文件大小 = 进度，断点续传零额外状态
//! - 完成后按序合并并校验总大小
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use std::time::Duration;

use futures::StreamExt;
use reqwest::header::{CONTENT_LENGTH, CONTENT_RANGE, RANGE};
use reqwest::StatusCode;
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::AsyncWriteExt;

use crate::downloader::probe::{self, sanitize_filename};
use crate::downloader::state::{load_task_list, save_task_list, Sidecar, SidecarMeta, CHUNK_SIZE};
use crate::downloader::types::{
    BatchSummary, MirrorProbe, PersistedTask, ProbeResult, Settings, StatsSnapshot, TaskEvent,
    TaskKind, TaskSnapshot, TaskStatus,
};

const MAX_CHUNK_RETRIES: u32 = 10;
const MAX_SINGLE_RETRIES: u32 = 8;
const METER_INTERVAL: Duration = Duration::from_millis(500);

pub struct TaskShared {
    pub id: String,
    pub meta: RwLock<TaskMeta>,
    pub status: RwLock<TaskStatus>,
    pub error: RwLock<Option<String>>,
    pub downloaded: AtomicU64,
    pub speed_bps: RwLock<f64>,
    pub eta_secs: RwLock<Option<u64>>,
    pub active_conns: AtomicU32,
    pub pause_flag: AtomicBool,
    pub cancel_flag: AtomicBool,
    pub error_flag: AtomicBool,
    /// 任务已从列表删除：meter 等后台循环据此停止推送
    pub removed: AtomicBool,
    /// 直链过期救援已用过（仅一次，防重解析后仍 404 的死循环）
    pub re_rescued: AtomicBool,
    /// 暂停/删除时唤醒停滞中的连接（瞬时中断）
    pub interrupt: tokio::sync::Notify,
}

impl TaskShared {
    /// 暂停/删除信号等待：标记已设置时立即返回，否则等待唤醒
    pub async fn wait_interrupt(&self) {
        loop {
            if self.pause_flag.load(Ordering::Relaxed)
                || self.cancel_flag.load(Ordering::Relaxed)
            {
                return;
            }
            self.interrupt.notified().await;
        }
    }

    pub fn snapshot(&self) -> TaskSnapshot {
        let meta = self.meta.read().unwrap();
        TaskSnapshot {
            id: self.id.clone(),
            url: meta.url.clone(),
            source_url: meta.source_url.clone(),
            filename: meta.filename.clone(),
            save_path: meta.save_path.clone(),
            total_size: meta.total_size,
            downloaded: self.downloaded.load(Ordering::Relaxed),
            speed_bps: *self.speed_bps.read().unwrap(),
            eta_seconds: *self.eta_secs.read().unwrap(),
            connections: meta.connections,
            sources: meta.mirrors.len() + 1,
            kind: meta.kind,
            status: *self.status.read().unwrap(),
            error: self.error.read().unwrap().clone(),
            created_at: meta.created_at,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TaskMeta {
    url: String,
    kind: TaskKind,
    /// BT 信息哈希（40 位 hex），用于删除时在会话中定位任务
    info_hash: Option<String>,
    /// 镜像地址（同一文件的其它线路/CDN），分片在其与主地址间轮询
    mirrors: Vec<String>,
    /// 站点要求的 Referer（如 B 站 CDN）
    referer: Option<String>,
    /// WebView 代取带出的会话 cookie（GoFile 等 CDN 校验会话级凭据的站点；
    /// 不持久化——会话作用域，重启后靠 source_url 重解析重取）
    cookies: Option<String>,
    /// 站点要求的 User-Agent（百度网盘 dlink 强制 netdisk UA；静态站点
    /// 属性，随任务持久化，重解析不变）
    user_agent: Option<String>,
    /// 解析类任务的原始分享页 URL（直链有时效——82dj 分钟级、B 站数小时——
    /// 下载 404/403 时据此自动重新解析拿新直链）
    source_url: Option<String>,
    /// 站点规范身份键（跨 URL 形态去重：b23.tv 短链 ≡ www 直链 ≡ 带分享参数）
    canonical: Option<String>,
    filename: String,
    save_path: String,
    total_size: u64,
    accept_ranges: bool,
    etag: Option<String>,
    last_modified: Option<String>,
    connections: u32,
    created_at: i64,
}

/// worker 的退出原因
#[derive(Debug)]
enum Flow {
    Ok,
    Interrupted, // 暂停/取消，静默退出
    Fatal(String),
}

pub struct Inner {
    app: OnceLock<AppHandle>,
    tasks: Mutex<HashMap<String, Arc<TaskShared>>>,
    order: Mutex<Vec<String>>,
    running: AtomicU32,
    settings: RwLock<Settings>,
    tasks_json: OnceLock<PathBuf>,
    settings_json: OnceLock<PathBuf>,
    stats_json: OnceLock<PathBuf>,
    lifetime_downloaded: AtomicU64,
    lifetime_tasks: AtomicU64,
    torrent_session: tokio::sync::OnceCell<Arc<librqbit::Session>>,
}

/// 供 Tauri manage 的句柄；内部 Arc 以便 spawn 'static 任务
pub struct TaskManager {
    pub inner: Arc<Inner>,
}

impl TaskManager {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Inner {
                app: OnceLock::new(),
                tasks: Mutex::new(HashMap::new()),
                order: Mutex::new(Vec::new()),
                running: AtomicU32::new(0),
                settings: RwLock::new(Settings::default()),
                tasks_json: OnceLock::new(),
                settings_json: OnceLock::new(),
                stats_json: OnceLock::new(),
                lifetime_downloaded: AtomicU64::new(0),
                lifetime_tasks: AtomicU64::new(0),
                torrent_session: tokio::sync::OnceCell::new(),
            }),
        }
    }
}

impl Inner {
    // ---------- 初始化 ----------

    pub fn set_app(self: &Arc<Self>, app: AppHandle) {
        let _ = self.app.set(app.clone());
        let data_dir = app
            .path()
            .app_data_dir()
            .unwrap_or_else(|_| PathBuf::from("."));
        let _ = self.tasks_json.set(data_dir.join("tasks.json"));
        let _ = self.stats_json.set(data_dir.join("stats.json"));
        self.load_stats();
        let cfg_dir = app
            .path()
            .app_config_dir()
            .unwrap_or_else(|_| data_dir.clone());
        let _ = self.settings_json.set(cfg_dir.join("settings.json"));

        let mut settings = self
            .settings_json
            .get()
            .and_then(|p| std::fs::read(p).ok())
            .and_then(|d| serde_json::from_slice::<Settings>(&d).ok())
            .unwrap_or_default();
        if settings.download_dir.is_empty() {
            settings.download_dir = default_download_dir(&app);
        }
        *self.settings.write().unwrap() = settings;
        self.persist_settings();

        self.load_persisted();
        self.emit_list();

        // 启动自动恢复：把上次中断（Paused）的任务重新排队
        if self.settings.read().unwrap().auto_resume_on_launch {
            let ids: Vec<String> = {
                let tasks = self.tasks.lock().unwrap();
                tasks
                    .values()
                    .filter(|s| *s.status.read().unwrap() == TaskStatus::Paused)
                    .map(|s| s.id.clone())
                    .collect()
            };
            if !ids.is_empty() {
                for id in ids {
                    if let Some(s) = self.tasks.lock().unwrap().get(&id) {
                        *s.status.write().unwrap() = TaskStatus::Queued;
                    }
                }
                self.persist();
                self.emit_list();
                self.dispatch();
            }
        }
    }

    pub fn persist_settings(&self) {
        if let Some(p) = self.settings_json.get() {
            if let Some(parent) = p.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Ok(json) = serde_json::to_vec_pretty(&*self.settings.read().unwrap()) {
                let _ = std::fs::write(p, json);
            }
        }
    }

    /// 应用启动：读取 tasks.json；被中断的任务标记为可续传的 Paused
    fn load_persisted(&self) {
        let Some(path) = self.tasks_json.get() else { return };
        let list = load_task_list(path);
        if list.is_empty() {
            return;
        }
        let mut tasks = self.tasks.lock().unwrap();
        let mut order = self.order.lock().unwrap();
        for p in list {
            let status = match p.status {
                TaskStatus::Downloading | TaskStatus::Queued | TaskStatus::Merging => {
                    TaskStatus::Paused
                }
                s => s,
            };
            let shared = Arc::new(TaskShared {
                id: p.id.clone(),
                meta: RwLock::new(TaskMeta {
                    url: p.url,
                    kind: p.kind,
                    info_hash: p.info_hash,
                    mirrors: p.mirrors,
                    referer: p.referer,
                    // 会话 cookie 不持久化：恢复后直链若因凭据失效被拒，
                    // source_url 重解析会重新走 WebView 代取取回
                    cookies: None,
                    user_agent: p.user_agent,
                    source_url: p.source_url,
                    canonical: p.canonical,
                    filename: p.filename,
                    save_path: p.save_path,
                    total_size: p.total_size,
                    accept_ranges: p.accept_ranges,
                    etag: p.etag,
                    last_modified: p.last_modified,
                    connections: p.connections,
                    created_at: p.created_at,
                }),
                status: RwLock::new(status),
                error: RwLock::new(p.error),
                downloaded: AtomicU64::new(0),
                speed_bps: RwLock::new(0.0),
                eta_secs: RwLock::new(None),
                active_conns: AtomicU32::new(0),
                pause_flag: AtomicBool::new(false),
                cancel_flag: AtomicBool::new(false),
                error_flag: AtomicBool::new(false),
                removed: AtomicBool::new(false),
                re_rescued: AtomicBool::new(false),
                interrupt: tokio::sync::Notify::new(),
            });
            let sidecar = Sidecar::from_save_path(Path::new(&shared.meta.read().unwrap().save_path));
            shared
                .downloaded
                .store(sidecar.sum_parts(), Ordering::Relaxed);
            order.push(shared.id.clone());
            tasks.insert(shared.id.clone(), shared);
        }
        drop(tasks);
        drop(order);
        self.persist();
    }

    fn build_task_list(&self) -> Vec<PersistedTask> {
        let tasks = self.tasks.lock().unwrap();
        let order = self.order.lock().unwrap();
        order
            .iter()
            .filter_map(|id| tasks.get(id))
                .map(|s| {
                    let meta = s.meta.read().unwrap();
                    PersistedTask {
                        id: s.id.clone(),
                        url: meta.url.clone(),
                        mirrors: meta.mirrors.clone(),
                        referer: meta.referer.clone(),
                        user_agent: meta.user_agent.clone(),
                        source_url: meta.source_url.clone(),
                        canonical: meta.canonical.clone(),
                        kind: meta.kind,
                        info_hash: meta.info_hash.clone(),
                        filename: meta.filename.clone(),
                    save_path: meta.save_path.clone(),
                    total_size: meta.total_size,
                    connections: meta.connections,
                    accept_ranges: meta.accept_ranges,
                    etag: meta.etag.clone(),
                    last_modified: meta.last_modified.clone(),
                    status: *s.status.read().unwrap(),
                    error: s.error.read().unwrap().clone(),
                    created_at: meta.created_at,
                }
            })
            .collect()
    }

    /// 同步写盘。任务清单很小（几 KB），任何线程直接写都足够快，
    /// 这样也避免了 .setup() 主线程没有 tokio 运行时上下文的问题。
    fn persist(&self) {
        let Some(path) = self.tasks_json.get() else { return };
        save_task_list(path, &self.build_task_list());
    }

    // ---------- 生命周期统计 ----------

    /// 任务完成：累计下载字节（跨「清除已完成」持久）
    fn record_completion(&self, bytes: u64) {
        self.lifetime_downloaded.fetch_add(bytes, Ordering::Relaxed);
        self.save_stats();
    }

    /// 任务创建：累计任务数
    fn record_task_added(&self) {
        self.lifetime_tasks.fetch_add(1, Ordering::Relaxed);
        self.save_stats();
    }

    fn load_stats(&self) {
        let Some(path) = self.stats_json.get() else { return };
        if let Ok(d) = std::fs::read(path) {
            if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&d) {
                let dl = v.get("lifetime_downloaded").and_then(|x| x.as_u64()).unwrap_or(0);
                let tk = v.get("lifetime_tasks").and_then(|x| x.as_u64()).unwrap_or(0);
                self.lifetime_downloaded.store(dl, Ordering::Relaxed);
                self.lifetime_tasks.store(tk, Ordering::Relaxed);
            }
        }
    }

    fn save_stats(&self) {
        let Some(path) = self.stats_json.get() else { return };
        let v = serde_json::json!({
            "lifetime_downloaded": self.lifetime_downloaded.load(Ordering::Relaxed),
            "lifetime_tasks": self.lifetime_tasks.load(Ordering::Relaxed),
        });
        let _ = std::fs::write(path, v.to_string());
    }

    pub fn stats(&self) -> StatsSnapshot {
        StatsSnapshot {
            lifetime_downloaded: self.lifetime_downloaded.load(Ordering::Relaxed),
            lifetime_tasks: self.lifetime_tasks.load(Ordering::Relaxed),
        }
    }

    fn emit_list(&self) {
        if let Some(app) = self.app.get() {
            let _ = app.emit("task://events", TaskEvent::List { tasks: self.snapshots() });
        }
    }

    fn emit_progress(&self, id: &str) {
        if let Some(app) = self.app.get() {
            if let Some(s) = self.tasks.lock().unwrap().get(id) {
                let _ = app.emit("task://events", TaskEvent::Progress { task: s.snapshot() });
            }
        }
    }

    // ---------- 查询 ----------

    pub fn snapshots(&self) -> Vec<TaskSnapshot> {
        let order = self.order.lock().unwrap();
        let tasks = self.tasks.lock().unwrap();
        order
            .iter()
            .filter_map(|id| tasks.get(id))
            .map(|s| s.snapshot())
            .collect()
    }

    pub fn settings(&self) -> Settings {
        self.settings.read().unwrap().clone()
    }

    pub fn update_settings(&self, s: Settings) -> Settings {
        *self.settings.write().unwrap() = s;
        self.persist_settings();
        self.settings()
    }

    /// 懒初始化 BT 会话（DHT + tracker，默认输出目录为应用下载目录）
    pub async fn torrent_session(&self) -> Result<Arc<librqbit::Session>, String> {
        self.torrent_session
            .get_or_try_init(|| async {
                let folder = self.settings.read().unwrap().download_dir.clone();
                librqbit::Session::new(std::path::PathBuf::from(folder))
                    .await
                    .map_err(|e| format!("初始化 BT 会话失败: {e}"))
            })
            .await
            .cloned()
    }

    /// 新建任务时对镜像逐个测速（256KB 采样），供 UI 展示与排序
    pub async fn probe_mirrors(&self, urls: Vec<String>) -> Vec<MirrorProbe> {
        let client = probe::client();
        let measures =
            futures::future::join_all(urls.iter().map(|u| measure_speed(&client, u))).await;
        urls.into_iter()
            .zip(measures.into_iter())
            .map(|(url, m)| MirrorProbe {
                url,
                ok: m.is_some(),
                speed_bps: m.map(|(s, _)| s).unwrap_or(0.0),
            })
            .collect()
    }

    // ---------- 任务操作 ----------

    pub async fn probe_url(&self, url: &str) -> Result<ProbeResult, String> {
        let client = probe::client();
        let info = probe::probe(&client, url).await?;
        let notice = info.notice.clone();
        // 网易云歌单 / B 站合集支持一键批量（batch_kind 告诉前端调哪个命令）
        let lower = url.to_lowercase();
        // GoFile：单文件可普通添加、多文件可批量——批量按钮无条件给出
        // （清单数只有代取时知道；单文件分享点批量=添加 1 个任务，无害）
        let batch_kind = if lower.contains("gofile.io") && lower.contains("/d/") {
            Some("gofile_share".to_string())
        } else if lower.contains("uc.cn") && lower.contains("/s/") {
            // UC 网盘：单文件可普通添加、多文件/文件夹可批量（序号 URL 逐个建任务）
            Some("uc_share".to_string())
        } else if lower.contains("pan.baidu.com") && lower.contains("/s/1") {
            // 百度网盘：登录态下批量直链；未登录时点批量给诚实引导（命令内拦截）
            Some("baidu_share".to_string())
        } else if lower.contains("pan.quark.cn") && lower.contains("/s/") {
            // 夸克网盘：登录态批量转存直链；未登录点批量给诚实引导（命令内拦截）
            Some("quark_share".to_string())
        } else if lower.contains("123pan.com")
            || lower.contains("123684.com")
            || lower.contains("123865.com")
        {
            // 123 云盘：登录态用月度流量直链批量；未登录点批量给诚实引导（命令内拦截）
            Some("pan123_share".to_string())
        } else if lower.contains("ctfile.com") && (lower.contains("/f/") || lower.contains("/d/")) {
            // 城通网盘：/f/ 单文件批量=1 任务无害；/d/ 文件夹批量（轮27 UA 伪装复活）
            Some("ct_share".to_string())
        } else if notice.is_some()
            && (lower.contains("music.163.com/playlist") || lower.contains("163cn.tv"))
        {
            Some("netease_playlist".to_string())
        } else if notice.is_some()
            && lower.contains("space.bilibili.com")
            && (lower.contains("collectiondetail") || lower.contains("seriesdetail"))
        {
            Some("bilibili_collection".to_string())
        } else if notice.is_some()
            && (lower.contains("lanzou") || lower.contains("lanzn.com") || lower.contains("lanpv.com"))
            && lower.contains("/s/")
        {
            Some("lanzou_folder".to_string())
        } else if notice.is_some() && lower.contains("bandcamp.com") && lower.contains("/album/") {
            Some("bandcamp_album".to_string())
        } else if notice.is_some() && lower.contains("bilibili.com/bangumi/play/ss") {
            Some("bilibili_bangumi".to_string())
        } else {
            None
        };
        Ok(ProbeResult {
            url: info.resolved_url.clone().unwrap_or_else(|| url.to_string()),
            filename: info.filename.unwrap_or_else(|| fallback_filename(url)),
            total_size: info.total_size.unwrap_or(0),
            accept_ranges: info.accept_ranges,
            notice,
            batch_kind,
            webview_plan: info.webview_plan.clone(),
        })
    }

    /// 网易云歌单一键批量：逐首创建下载任务（VIP 歌自动跳过）
    pub async fn add_netease_playlist(
        self: &Arc<Self>,
        url: String,
        dir: Option<String>,
        connections: Option<u32>,
    ) -> Result<BatchSummary, String> {
        let (name, urls) = crate::downloader::extract::netease::playlist_song_urls(&url)
            .await
            .ok_or("歌单解析失败")?;
        let cap = (self.settings().batch_limit as usize).max(1).min(urls.len()); // 设置上限，上限 30 首，避免 CDN 直链过期与请求风暴
        let mut added = 0u32;
        let mut skipped = 0u32;
        for u in &urls[..cap] {
            match self
                .add_task(u.clone(), vec![], None, dir.clone(), connections)
                .await
            {
                Ok(_) => added += 1,
                Err(_) => skipped += 1, // VIP 歌或探测失败
            }
        }
        Ok(BatchSummary {
            playlist: name,
            total: urls.len() as u32,
            added,
            skipped,
            truncated: urls.len() > cap,
        })
    }

    /// B 站合集一键批量：逐个解析单视频直链建任务（上限 30 个防直链过期）
    pub async fn add_bilibili_collection(
        self: &Arc<Self>,
        url: String,
        dir: Option<String>,
        connections: Option<u32>,
    ) -> Result<BatchSummary, String> {
        let (name, urls) = crate::downloader::extract::bilibili::collection_video_urls(&url)
            .await
            .ok_or("合集解析失败")?;
        let cap = (self.settings().batch_limit as usize).max(1).min(urls.len()); // 设置上限，上限 30 个，B 站 CDN 直链有时效
        let mut added = 0u32;
        let mut skipped = 0u32;
        for u in &urls[..cap] {
            match self
                .add_task(u.clone(), vec![], None, dir.clone(), connections)
                .await
            {
                Ok(_) => added += 1,
                Err(_) => skipped += 1, // 下架/风控/重复
            }
        }
        Ok(BatchSummary {
            playlist: name,
            total: urls.len() as u32,
            added,
            skipped,
            truncated: urls.len() > cap,
        })
    }

    /// B 站番剧整季一键批量：逐集解析直链建任务（上限 30 集防直链过期）
    pub async fn add_bilibili_bangumi(
        self: &Arc<Self>,
        url: String,
        dir: Option<String>,
        connections: Option<u32>,
    ) -> Result<BatchSummary, String> {
        let (name, urls) = crate::downloader::extract::bangumi::season_ep_urls(&url)
            .await
            .ok_or("番剧解析失败（可能全部剧集为会员专享，或链接已失效）")?;
        let cap = (self.settings().batch_limit as usize).max(1).min(urls.len()); // 设置上限，上限 30 集，直链有时效
        let mut added = 0u32;
        let mut skipped = 0u32;
        for u in &urls[..cap] {
            match self
                .add_task(u.clone(), vec![], None, dir.clone(), connections)
                .await
            {
                Ok(_) => added += 1,
                Err(_) => skipped += 1, // 无直链/重复
            }
        }
        Ok(BatchSummary {
            playlist: name,
            total: urls.len() as u32,
            added,
            skipped,
            truncated: urls.len() > cap,
        })
    }

    /// Bandcamp 专辑一键批量：逐首走单曲解析建任务（流地址有时效，上限 30 首）
    pub async fn add_bandcamp_album(
        self: &Arc<Self>,
        url: String,
        dir: Option<String>,
        connections: Option<u32>,
    ) -> Result<BatchSummary, String> {
        let (name, urls) = crate::downloader::extract::bandcamp::album_track_urls(&url)
            .await
            .ok_or("专辑解析失败")?;
        let cap = (self.settings().batch_limit as usize).max(1).min(urls.len()); // 设置上限，上限 30 首，流地址带 ts 签名有时效
        let mut added = 0u32;
        let mut skipped = 0u32;
        for u in &urls[..cap] {
            match self
                .add_task(u.clone(), vec![], None, dir.clone(), connections)
                .await
            {
                Ok(_) => added += 1,
                Err(_) => skipped += 1, // 无流/重复/失败
            }
        }
        Ok(BatchSummary {
            playlist: name,
            total: urls.len() as u32,
            added,
            skipped,
            truncated: urls.len() > cap,
        })
    }

    /// 蓝奏云文件夹一键批量：逐个单文件页 WebView 代取直链建任务（上限 30 个）
    pub async fn add_lanzou_folder(
        self: &Arc<Self>,
        url: String,
        dir: Option<String>,
        connections: Option<u32>,
    ) -> Result<BatchSummary, String> {
        let (name, urls) = crate::downloader::extract::lanzou::folder_file_urls(&url)
            .await
            .ok_or("文件夹解析失败")?;
        let cap = (self.settings().batch_limit as usize).max(1).min(urls.len()); // 设置上限，上限 30 个，每个文件一次 WebView 代取
        let mut added = 0u32;
        let mut skipped = 0u32;
        for u in &urls[..cap] {
            match self
                .add_task(u.clone(), vec![], None, dir.clone(), connections)
                .await
            {
                Ok(_) => added += 1,
                Err(_) => skipped += 1, // 代取超时/风控/重复
            }
        }
        Ok(BatchSummary {
            playlist: name,
            total: urls.len() as u32,
            added,
            skipped,
            truncated: urls.len() > cap,
        })
    }

    /// UC 网盘分享一键批量：原生扁平清单（根文件+子文件夹一层）→ 逐个 feiliu_i 序号建任务
    /// （每个文件一次独立解析链拿直链+会话 cookie，同蓝奏云/gofile 批量模式）
    pub async fn add_uc_batch(
        self: &Arc<Self>,
        url: String,
        dir: Option<String>,
        connections: Option<u32>,
    ) -> Result<BatchSummary, String> {
        let (name, urls) = crate::downloader::extract::uc::folder_file_urls(&url)
            .await
            .ok_or("UC 分享清单解析失败")?;
        let cap = (self.settings().batch_limit as usize).max(1).min(urls.len());
        let mut added = 0u32;
        let mut skipped = 0u32;
        for u in &urls[..cap] {
            match self
                .add_task(u.clone(), vec![], None, dir.clone(), connections)
                .await
            {
                Ok(_) => added += 1,
                Err(_) => skipped += 1, // 限流/风控/重复
            }
        }
        Ok(BatchSummary {
            playlist: name,
            total: urls.len() as u32,
            added,
            skipped,
            truncated: urls.len() > cap,
        })
    }

    /// 百度网盘分享一键批量（登录态）：原生扁平清单 → 逐个 feiliu_i 序号建任务
    /// （dlink 直链 + 会话 cookie + netdisk UA；会话必须已登录）
    pub async fn add_baidu_batch(
        self: &Arc<Self>,
        url: String,
        dir: Option<String>,
        connections: Option<u32>,
    ) -> Result<BatchSummary, String> {
        if crate::downloader::baidu_login::load_session().is_none() {
            return Err("百度网盘批量下载需要登录：请先在「设置 → 百度网盘」登录自己的账号".into());
        }
        let (name, urls) = crate::downloader::extract::baidu::folder_file_urls(&url)
            .await
            .ok_or("百度网盘分享清单解析失败")?;
        let cap = (self.settings().batch_limit as usize).max(1).min(urls.len());
        let mut added = 0u32;
        let mut skipped = 0u32;
        for u in &urls[..cap] {
            match self
                .add_task(u.clone(), vec![], None, dir.clone(), connections)
                .await
            {
                Ok(_) => added += 1,
                Err(_) => skipped += 1, // 限流/风控/重复
            }
        }
        Ok(BatchSummary {
            playlist: name,
            total: urls.len() as u32,
            added,
            skipped,
            truncated: urls.len() > cap,
        })
    }

    /// 城通网盘文件夹一键批量：WebView 代取单会话状态机（收割 t- 链→逐文件
    /// 捕获直链）→ files 数组直链直接建任务（免重解析，downurl 短时效即建即下）
    pub async fn add_ct_batch(
        self: &Arc<Self>,
        url: String,
        dir: Option<String>,
        connections: Option<u32>,
    ) -> Result<BatchSummary, String> {
        let app = self.app.get().ok_or("应用尚未初始化完成")?.clone();
        let info = probe::probe(&probe::client(), &url).await.map_err(|e| e.to_string())?;
        let plan = info
            .webview_plan
            .clone()
            .ok_or("城通解析需 WebView 代取")?;
        let wv = crate::downloader::webview_extract::extract(app, &plan)
            .await
            .map_err(|e| format!("文件夹清单代取失败: {e}"))?;
        if wv.files.is_empty() {
            return Err("文件夹内未解析出文件".into());
        }
        let cap = (self.settings().batch_limit as usize).max(1).min(wv.files.len());
        let mut added = 0u32;
        let mut skipped = 0u32;
        // wv.files 元组序 = (name, link)（webview_extract 约定 [{name,link}]）
        for (name, link) in &wv.files[..cap] {
            match self
                .add_task(link.clone(), vec![], Some(name.clone()), dir.clone(), connections)
                .await
            {
                Ok(_) => added += 1,
                Err(_) => skipped += 1, // 直链过期/重复
            }
        }
        Ok(BatchSummary {
            playlist: wv.name.unwrap_or_else(|| "城通文件夹".into()),
            total: wv.files.len() as u32,
            added,
            skipped,
            truncated: wv.files.len() > cap,
        })
    }

    /// GoFile 分享一键批量：WebView 代取列文件清单 → 逐个 feiliu_i 序号建任务
    /// （每个文件一次 WebView 代取，同蓝奏云文件夹模式；序号入 canonical 键防撞查重）
    /// 123 云盘分享一键批量（登录态）：walk 清单 → 逐个 feiliu_i 序号建任务
    /// （登录态用 10GB/月流量直链；未登录匿名全拦 5112，命令内诚实拦截）
    pub async fn add_pan123_batch(
        self: &Arc<Self>,
        url: String,
        dir: Option<String>,
        connections: Option<u32>,
    ) -> Result<BatchSummary, String> {
        if crate::downloader::pan123_login::load_session().is_none() {
            return Err("123 云盘批量下载需要登录：请先在「设置 → 123 云盘」登录自己的账号".into());
        }
        let base = url
            .split(['?', '&'])
            .next()
            .unwrap_or(&url)
            .to_string();
        let files = crate::downloader::extract::pan123::list_files(&base)
            .await
            .map_err(|e| format!("123 云盘分享清单解析失败: {e}"))?;
        if files.is_empty() {
            return Err("分享内未解析出文件".into());
        }
        let name = files
            .first()
            .map(|(n, _)| n.rsplit('/').next().unwrap_or(n).to_string())
            .unwrap_or_else(|| "123 云盘分享".into());
        let cap = (self.settings().batch_limit as usize).max(1).min(files.len());
        let mut added = 0u32;
        let mut skipped = 0u32;
        for i in 0..cap {
            let task_url = if i == 0 {
                base.clone()
            } else {
                format!("{base}?feiliu_i={i}")
            };
            match self
                .add_task(task_url, vec![], None, dir.clone(), connections)
                .await
            {
                Ok(_) => added += 1,
                Err(_) => skipped += 1, // 流量尽/限流/重复
            }
        }
        Ok(BatchSummary {
            playlist: name,
            total: files.len() as u32,
            added,
            skipped,
            truncated: files.len() > cap,
        })
    }

    /// 夸克网盘分享一键批量（登录态）：walk 清单 → 逐个 feiliu_i 序号建任务
    /// （登录态逐文件转存直链；匿名访客仅 ≤10MiB 可下，批量主路径=登录转存）
    pub async fn add_quark_batch(
        self: &Arc<Self>,
        url: String,
        dir: Option<String>,
        connections: Option<u32>,
    ) -> Result<BatchSummary, String> {
        if crate::downloader::quark_login::load_session().is_none() {
            return Err("夸克网盘批量下载需要登录：请先在「设置 → 夸克网盘」登录自己的账号".into());
        }
        let base = url
            .split(['?', '&'])
            .next()
            .unwrap_or(&url)
            .to_string();
        let files = crate::downloader::extract::quark::list_files(&base)
            .await
            .map_err(|e| format!("夸克分享清单解析失败: {e}"))?;
        if files.is_empty() {
            return Err("分享内未解析出文件".into());
        }
        let name = files
            .first()
            .map(|(n, _)| n.rsplit('/').next().unwrap_or(n).to_string())
            .unwrap_or_else(|| "夸克分享".into());
        let cap = (self.settings().batch_limit as usize).max(1).min(files.len());
        let mut added = 0u32;
        let mut skipped = 0u32;
        for i in 0..cap {
            let task_url = if i == 0 {
                base.clone()
            } else {
                format!("{base}?feiliu_i={i}")
            };
            match self
                .add_task(task_url, vec![], None, dir.clone(), connections)
                .await
            {
                Ok(_) => added += 1,
                Err(_) => skipped += 1, // 转存失败/限流/重复
            }
        }
        Ok(BatchSummary {
            playlist: name,
            total: files.len() as u32,
            added,
            skipped,
            truncated: files.len() > cap,
        })
    }

    pub async fn add_gofile_batch(
        self: &Arc<Self>,
        url: String,
        dir: Option<String>,
        connections: Option<u32>,
    ) -> Result<BatchSummary, String> {
        let app = self.app.get().ok_or("应用尚未初始化完成")?.clone();
        let base = url
            .split(['?', '&'])
            .next()
            .unwrap_or(&url)
            .to_string();
        let info = probe::probe(&probe::client(), &base).await.map_err(|e| e.to_string())?;
        let plan = info
            .webview_plan
            .clone()
            .ok_or("gofile 解析需 WebView 代取")?;
        let wv = crate::downloader::webview_extract::extract(app, &plan)
            .await
            .map_err(|e| format!("文件清单代取失败: {e}"))?;
        if wv.files.is_empty() {
            return Err("分享内未解析出文件".into());
        }
        let cap = (self.settings().batch_limit as usize).max(1).min(wv.files.len());
        let mut added = 0u32;
        let mut skipped = 0u32;
        for i in 0..cap {
            let task_url = if i == 0 {
                base.clone()
            } else {
                format!("{base}?feiliu_i={i}")
            };
            match self
                .add_task(task_url, vec![], None, dir.clone(), connections)
                .await
            {
                Ok(_) => added += 1,
                Err(_) => skipped += 1, // 代取超时/风控/重复
            }
        }
        Ok(BatchSummary {
            playlist: wv.name.unwrap_or_else(|| "gofile 分享".into()),
            total: wv.files.len() as u32,
            added,
            skipped,
            truncated: wv.files.len() > cap,
        })
    }

    pub async fn add_task(
        self: &Arc<Self>,
        url: String,
        mirrors: Vec<String>,
        filename: Option<String>,
        dir: Option<String>,
        connections: Option<u32>,
    ) -> Result<TaskSnapshot, String> {
        let settings = self.settings();
        if url.trim_start().starts_with("magnet:") || url.trim_end().ends_with(".torrent") {
            return self.add_torrent_task(url, dir, settings.download_dir.clone()).await;
        }
        let client = probe::client();
        // 防重复：同 URL（含 WebView 代取后的直链对比）已在列表中 → 拒绝
        {
            let tasks = self.tasks.lock().unwrap();
            let dup = tasks
                .values()
                .any(|t| t.meta.read().unwrap().url == url);
            if dup {
                return Err("该链接已在任务列表中，请勿重复添加".into());
            }
        }
        // 跨 URL 形态查重：b23.tv 短链 ≡ www 链接 ≡ 带分享参数（规范身份键）
        let canonical_key = crate::downloader::extract::canonical_key(&url).await;
        if let Some(key) = &canonical_key {
            let tasks = self.tasks.lock().unwrap();
            let dup = tasks.values().any(|t| {
                t.meta.read().unwrap().canonical.as_deref() == Some(key.as_str())
            });
            if dup {
                return Err(
                    "该内容已在任务列表中（链接形态不同但指向同一视频），请勿重复添加".into(),
                );
            }
        }
        let mut info = probe::probe(&client, &url).await?;
        // 站点限制（如阿里云盘需登录转存）→ 不创建任务
        if let Some(notice) = &info.notice {
            return Err(format!("{}：{}", info.filename.as_deref().unwrap_or("分享内容"), notice));
        }
        // WebView 代取直链（蓝奏云等 TLS 指纹墙站点，桌面端）
        let mut dl_url = info.resolved_url.clone().unwrap_or_else(|| url.clone());
        // JSON 形态代取结果带出的会话 cookie / 真实文件名
        let mut wv_cookie: Option<String> = None;
        let mut wv_name: Option<String> = None;
        if dl_url.is_empty() || info.webview_plan.is_some() {
            if let Some(plan) = &info.webview_plan {
                #[cfg(any(target_os = "ios", target_os = "android"))]
                {
                    let _ = plan;
                    return Err("该站点的直链代取仅桌面版支持".into());
                }
                #[cfg(not(any(target_os = "ios", target_os = "android")))]
                {
                    if let Some(app) = self.app.get() {
                        let wv = crate::downloader::webview_extract::extract(app.clone(), plan)
                            .await
                            .map_err(|e| format!("WebView 代取直链失败: {e}"))?;
                        dl_url = wv.url;
                        // 防御清洗：直链不可能以反斜杠/引号结尾（多层 stringify
                        // 转义偶发残留，实测 82dj 直链被污染 → CDN 404）
                        dl_url = dl_url
                            .trim_end_matches(['\\', '"'])
                            .trim_matches('"')
                            .trim_end_matches(['\\', '"'])
                            .to_string();
                        // JSON 形态结果带出的会话 cookie / 真实文件名
                        wv_cookie = wv.cookie;
                        if let Some(n) = wv.name.filter(|s| !s.trim().is_empty()) {
                            wv_name = Some(n);
                        }
                    } else {
                        return Err("应用尚未初始化完成".into());
                    }
                }
            }
        }
        // 原生解析器透传的会话 cookie（UC 网盘等：API 流程下发的 __pugs 必须随下载）
        if wv_cookie.is_none() {
            wv_cookie = info.cookies.clone();
        }
        if dl_url.is_empty() {
            return Err("未解析出可下载的直链".into());
        }
        // WebView 代取站点：对真实直链带 Referer 二次探测——首次探测的是
        // 分享页（HTML），大小/断点支持都不可用；82dj 等直链无 Referer
        // 还会 404。best-effort：探测失败不阻塞建任务（下载仍可行）
        if info.webview_plan.is_some() {
            if let Some(p2) =
                // WebView JSON 结果带出的会话 cookie 一并探测（GoFile 无 cookie 302 拿不到大小）
                probe::probe_direct_ref(
                    &client,
                    &dl_url,
                    info.referer.as_deref(),
                    wv_cookie.as_deref(),
                    info.user_agent.as_deref(),
                )
                .await
            {
                info.total_size = p2.total_size;
                info.accept_ranges = p2.accept_ranges;
                info.etag = p2.etag;
                info.last_modified = p2.last_modified;
            }
        }

        let name = sanitize_name(
            filename
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                // WebView 代取带出的真实文件名（GoFile 等解析时点未知文件名的站点）
                .or(wv_name.clone())
                .or(info.filename.clone()),
        )
        .unwrap_or_else(|| fallback_filename(&url));

        let dir_path = PathBuf::from(
            dir.filter(|d| !d.trim().is_empty()).unwrap_or(settings.download_dir),
        );
        std::fs::create_dir_all(&dir_path).map_err(|e| format!("创建目录失败: {e}"))?;
        let save_path = unique_save_path(&dir_path, &name);

        let sidecar = Sidecar::from_save_path(&save_path);
        sidecar.create().map_err(|e| e.to_string())?;
        sidecar
            .write_meta(&SidecarMeta {
                version: 1,
                url: dl_url.clone(),
                mirrors: mirrors.clone(),
                referer: info.referer.clone(),
                save_path: save_path.to_string_lossy().into_owned(),
                filename: name.clone(),
                total_size: info.total_size.unwrap_or(0),
                accept_ranges: info.accept_ranges,
                etag: info.etag.clone(),
                last_modified: info.last_modified.clone(),
                chunk_size: CHUNK_SIZE,
            })
            .map_err(|e| e.to_string())?;

        let id = uuid::Uuid::new_v4().to_string();
        let conns = connections
            .unwrap_or(settings.max_connections)
            .clamp(1, 64);
        let shared = Arc::new(TaskShared {
            id: id.clone(),
            meta: RwLock::new(TaskMeta {
                url: dl_url.clone(),
                kind: TaskKind::Http,
                info_hash: None,
                mirrors: mirrors.clone(),
                referer: info.referer.clone(),
                // WebView 代取站点带出的会话 cookie（GoFile 等）
                cookies: wv_cookie.clone(),
                // 站点要求的 UA（百度网盘 dlink 强制 netdisk）
                user_agent: info.user_agent.clone(),
                // 直链 ≠ 分享页（解析/代取类站点）时记录原始页，供直链过期重解析
                source_url: if dl_url != url { Some(url.clone()) } else { None },
                canonical: canonical_key,
                filename: name,
                save_path: save_path.to_string_lossy().into_owned(),
                total_size: info.total_size.unwrap_or(0),
                accept_ranges: info.accept_ranges,
                etag: info.etag,
                last_modified: info.last_modified,
                connections: conns,
                created_at: now_ms(),
            }),
            status: RwLock::new(TaskStatus::Queued),
            error: RwLock::new(None),
            downloaded: AtomicU64::new(0),
            speed_bps: RwLock::new(0.0),
            eta_secs: RwLock::new(None),
            active_conns: AtomicU32::new(0),
            pause_flag: AtomicBool::new(false),
            cancel_flag: AtomicBool::new(false),
            error_flag: AtomicBool::new(false),
            removed: AtomicBool::new(false),
            re_rescued: AtomicBool::new(false),
            interrupt: tokio::sync::Notify::new(),
        });

        {
            let mut tasks = self.tasks.lock().unwrap();
            let mut order = self.order.lock().unwrap();
            tasks.insert(id.clone(), shared);
            order.push(id.clone());
        }
        self.record_task_added();
        self.persist();
        self.emit_list();
        self.dispatch();
        Ok(self
            .tasks
            .lock()
            .unwrap()
            .get(&id)
            .map(|s| s.snapshot())
            .expect("task just added"))
    }

    /// 创建 BT 任务：magnet 链接或本地 .torrent 文件路径
    async fn add_torrent_task(
        self: &Arc<Self>,
        url: String,
        dir: Option<String>,
        default_dir: String,
    ) -> Result<TaskSnapshot, String> {
        let dir_path = PathBuf::from(
            dir.filter(|d| !d.trim().is_empty()).unwrap_or(default_dir),
        );
        std::fs::create_dir_all(&dir_path).map_err(|e| format!("创建目录失败: {e}"))?;

        let id = uuid::Uuid::new_v4().to_string();
        let info_hash = parse_magnet_btih(&url);
        let shared = Arc::new(TaskShared {
            id: id.clone(),
            meta: RwLock::new(TaskMeta {
                url: url.clone(),
                kind: TaskKind::Torrent,
                info_hash,
                mirrors: Vec::new(),
                referer: None,
                cookies: None,
                user_agent: None,
                source_url: None,
                canonical: None,
                filename: "磁力任务（等待元数据…）".into(),
                save_path: dir_path.to_string_lossy().into_owned(),
                total_size: 0,
                accept_ranges: false,
                etag: None,
                last_modified: None,
                connections: 0,
                created_at: now_ms(),
            }),
            status: RwLock::new(TaskStatus::Queued),
            error: RwLock::new(None),
            downloaded: AtomicU64::new(0),
            speed_bps: RwLock::new(0.0),
            eta_secs: RwLock::new(None),
            active_conns: AtomicU32::new(0),
            pause_flag: AtomicBool::new(false),
            cancel_flag: AtomicBool::new(false),
            error_flag: AtomicBool::new(false),
            removed: AtomicBool::new(false),
            re_rescued: AtomicBool::new(false),
            interrupt: tokio::sync::Notify::new(),
        });

        {
            let mut tasks = self.tasks.lock().unwrap();
            let mut order = self.order.lock().unwrap();
            tasks.insert(id.clone(), shared);
            order.push(id.clone());
        }
        self.persist();
        self.emit_list();
        self.dispatch();
        Ok(self
            .tasks
            .lock()
            .unwrap()
            .get(&id)
            .map(|s| s.snapshot())
            .expect("task just added"))
    }

    /// 有空位就启动排队中的任务
    pub fn dispatch(self: &Arc<Self>) {
        let max = self.settings.read().unwrap().max_concurrent_tasks.max(1);
        let running = self.running.load(Ordering::Relaxed);
        if running >= max {
            return;
        }
        let to_start: Vec<String> = {
            let order = self.order.lock().unwrap();
            let tasks = self.tasks.lock().unwrap();
            order
                .iter()
                .filter(|id| {
                    tasks
                        .get(*id)
                        .map(|s| *s.status.read().unwrap() == TaskStatus::Queued)
                        .unwrap_or(false)
                })
                .take((max - running) as usize)
                .cloned()
                .collect()
        };
        for id in to_start {
            self.start_task(id);
        }
    }

    fn start_task(self: &Arc<Self>, id: String) {
        let Some(shared) = self.tasks.lock().unwrap().get(&id).cloned() else {
            return;
        };
        *shared.status.write().unwrap() = TaskStatus::Downloading;
        shared.error_flag.store(false, Ordering::Relaxed);
        shared.pause_flag.store(false, Ordering::Relaxed);
        shared.error.write().unwrap().take();
        *shared.speed_bps.write().unwrap() = 0.0;
        self.running.fetch_add(1, Ordering::Relaxed);
        self.emit_progress(&id);
        self.persist();

        let inner = self.clone();
        tauri::async_runtime::spawn(async move {
            run_task(inner, id, shared).await;
        });
    }

    pub async fn pause_task(&self, id: &str) -> Result<(), String> {
        let Some(shared) = self.tasks.lock().unwrap().get(id).cloned() else {
            return Err("任务不存在".into());
        };
        match *shared.status.read().unwrap() {
            TaskStatus::Queued => {
                *shared.status.write().unwrap() = TaskStatus::Paused;
                self.emit_progress(id);
                self.persist();
            }
            TaskStatus::Downloading => {
                shared.pause_flag.store(true, Ordering::Relaxed);
                shared.interrupt.notify_waiters(); // 唤醒停滞中的连接，瞬时暂停
            }
            _ => {}
        }
        Ok(())
    }

    pub async fn resume_task(self: &Arc<Self>, id: &str) -> Result<(), String> {
        let Some(shared) = self.tasks.lock().unwrap().get(id).cloned() else {
            return Err("任务不存在".into());
        };
        match *shared.status.read().unwrap() {
            TaskStatus::Paused | TaskStatus::Failed => {
                *shared.status.write().unwrap() = TaskStatus::Queued;
                shared.error.write().unwrap().take();
                shared.error_flag.store(false, Ordering::Relaxed);
                shared.pause_flag.store(false, Ordering::Relaxed);
                self.emit_progress(id);
                self.persist();
                self.dispatch();
                Ok(())
            }
            _ => Err("任务状态不支持继续".into()),
        }
    }

    /// 清除全部已完成任务（保留磁盘文件，仅从列表移除）
    pub async fn clear_completed(&self) -> Result<u32, String> {
        let ids: Vec<String> = {
            let tasks = self.tasks.lock().unwrap();
            tasks
                .values()
                .filter(|s| *s.status.read().unwrap() == TaskStatus::Completed)
                .map(|s| s.id.clone())
                .collect()
        };
        let count = ids.len() as u32;
        for id in &ids {
            let _ = self.delete_task(id, false).await; // 保留文件
        }
        self.emit_list();
        Ok(count)
    }

    /// 暂停全部运行中/排队中的任务
    pub async fn pause_all(&self) {
        let ids: Vec<String> = {
            let tasks = self.tasks.lock().unwrap();
            tasks
                .values()
                .filter(|s| {
                    matches!(
                        *s.status.read().unwrap(),
                        TaskStatus::Downloading | TaskStatus::Queued
                    )
                })
                .map(|s| s.id.clone())
                .collect()
        };
        for id in ids {
            let _ = self.pause_task(&id).await;
        }
    }

    /// 恢复全部已暂停/失败的任务
    pub async fn resume_all(self: &Arc<Self>) {
        let ids: Vec<String> = {
            let tasks = self.tasks.lock().unwrap();
            tasks
                .values()
                .filter(|s| {
                    matches!(
                        *s.status.read().unwrap(),
                        TaskStatus::Paused | TaskStatus::Failed
                    )
                })
                .map(|s| s.id.clone())
                .collect()
        };
        for id in ids {
            let _ = self.resume_task(&id).await;
        }
    }

    pub async fn delete_task(&self, id: &str, delete_file: bool) -> Result<(), String> {
        let (shared, save_path, kind, status) = {
            let mut tasks = self.tasks.lock().unwrap();
            let shared = tasks.remove(id).ok_or("任务不存在")?;
            {
                let mut order = self.order.lock().unwrap();
                order.retain(|x| x != id);
            }
            let save_path = shared.meta.read().unwrap().save_path.clone();
            let kind = shared.meta.read().unwrap().kind;
            let status = *shared.status.read().unwrap();
            (shared, save_path, kind, status)
        };
        shared.cancel_flag.store(true, Ordering::Relaxed);
        shared.removed.store(true, Ordering::Relaxed);
        shared.interrupt.notify_waiters();

        // BT 任务：通过会话删除（可同时删文件），不走 HTTP sidecar
        if kind == TaskKind::Torrent {
            if let Ok(session) = self.torrent_session().await {
                let hex = shared.meta.read().unwrap().info_hash.clone();
                if let Some(hex) = hex {
                    if let Ok(hash) = hex.parse::<librqbit_core::Id20>() {
                        let _ = session
                            .delete(
                                librqbit::api::TorrentIdOrHash::Hash(hash),
                                delete_file,
                            )
                            .await;
                    }
                }
            }
            self.persist();
            self.emit_list();
            return Ok(());
        }

        if status == TaskStatus::Downloading || status == TaskStatus::Merging {
            // 运行中的任务由 run_task 在退出后清理 sidecar
            if delete_file && status == TaskStatus::Merging {
                let _ = std::fs::remove_file(&save_path);
            }
        } else {
            Sidecar::from_save_path(Path::new(&save_path)).remove();
            if delete_file && status == TaskStatus::Completed {
                let _ = std::fs::remove_file(&save_path);
            }
        }
        self.persist();
        self.emit_list();
        Ok(())
    }
}

// ---------- 运行循环 ----------

/// BT 任务：librqbit 负责下载，本循环轮询统计并映射到任务状态
async fn run_torrent(inner: &Arc<Inner>, id: String, shared: Arc<TaskShared>) {
    let meta = shared.meta.read().unwrap().clone();
    let session = match inner.torrent_session().await {
        Ok(s) => s,
        Err(e) => {
            *shared.status.write().unwrap() = TaskStatus::Failed;
            *shared.error.write().unwrap() = Some(e);
            inner.running.fetch_sub(1, Ordering::Relaxed);
            inner.emit_progress(&id);
            inner.persist();
            inner.dispatch();
            return;
        }
    };
    let resp = session
        .add_torrent(
            librqbit::AddTorrent::from_url(meta.url.clone()),
            Some(librqbit::AddTorrentOptions {
                output_folder: Some(meta.save_path.clone()),
                ..Default::default()
            }),
        )
        .await;
    let handle = match resp {
        Ok(r) => match r.into_handle() {
            Some(h) => h,
            None => {
                *shared.status.write().unwrap() = TaskStatus::Failed;
                *shared.error.write().unwrap() = Some("任务已在会话中但句柄不可用".into());
                inner.running.fetch_sub(1, Ordering::Relaxed);
                inner.emit_progress(&id);
                inner.persist();
                inner.dispatch();
                return;
            }
        },
        Err(e) => {
            *shared.status.write().unwrap() = TaskStatus::Failed;
            *shared.error.write().unwrap() = Some(format!("添加种子失败: {e}"));
            inner.running.fetch_sub(1, Ordering::Relaxed);
            inner.emit_progress(&id);
            inner.persist();
            inner.dispatch();
            return;
        }
    };
    // 记录 info_hash（删除时在会话中定位）
    {
        let hash_hex: String = handle
            .info_hash()
            .0
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        shared.meta.write().unwrap().info_hash = Some(hash_hex);
    }
    // 恢复场景：任务可能处于暂停态
    let _ = session.unpause(&handle).await;
    shared.active_conns.store(1, Ordering::Relaxed);
    inner.emit_progress(&id);

    loop {
        tokio::time::sleep(METER_INTERVAL).await;
        if shared.pause_flag.load(Ordering::Relaxed)
            || shared.cancel_flag.load(Ordering::Relaxed)
        {
            break;
        }
        let st = handle.stats();
        if let Some(err) = &st.error {
            *shared.status.write().unwrap() = TaskStatus::Failed;
            *shared.error.write().unwrap() = Some(err.clone());
            break;
        }
        if st.total_bytes > 0 {
            let mut meta = shared.meta.write().unwrap();
            if meta.total_size != st.total_bytes {
                meta.total_size = st.total_bytes;
            }
            if !meta.filename.starts_with("磁力任务") {
                if let Some(n) = handle.name() {
                    if !n.is_empty() && n != meta.filename {
                        meta.filename = n;
                    }
                }
            }
            drop(meta);
            shared.downloaded.store(st.progress_bytes, Ordering::Relaxed);
        }
        let sp = st
            .live
            .as_ref()
            .map(|l| l.download_speed.mbps * 1024.0 * 1024.0)
            .unwrap_or(0.0);
        *shared.speed_bps.write().unwrap() = sp;
        let total = shared.meta.read().unwrap().total_size;
        let cur = shared.downloaded.load(Ordering::Relaxed);
        *shared.eta_secs.write().unwrap() = if total > 0 && sp > 1.0 {
            Some(((total - cur.min(total)) as f64 / sp) as u64)
        } else {
            None
        };
        if st.finished {
            break;
        }
        inner.emit_progress(&id);
    }

    let paused = shared.pause_flag.load(Ordering::Relaxed);
    let cancelled = shared.cancel_flag.load(Ordering::Relaxed);
    if paused && !cancelled {
        let _ = session.pause(&handle).await;
        *shared.status.write().unwrap() = TaskStatus::Paused;
    } else if handle.stats().finished && !cancelled {
        *shared.status.write().unwrap() = TaskStatus::Completed;
        let st = handle.stats();
        shared.downloaded.store(st.progress_bytes, Ordering::Relaxed);
        inner.record_completion(st.progress_bytes);
    }
    if cancelled {
        // 交由 delete_task 走 session.delete；此处仅暂停避免继续下载
        let _ = session.pause(&handle).await;
    }
    shared.speed_bps.write().unwrap().clone_from(&0.0);
    shared.active_conns.store(0, Ordering::Relaxed);
    inner.running.fetch_sub(1, Ordering::Relaxed);
    inner.emit_progress(&id);
    inner.persist();
    inner.dispatch();
}

async fn run_task(inner: Arc<Inner>, id: String, shared: Arc<TaskShared>) {
    if shared.meta.read().unwrap().kind == TaskKind::Torrent {
        run_torrent(&inner, id, shared).await;
        return;
    }
    let mut meta = shared.meta.read().unwrap().clone();
    let sidecar = Sidecar::from_save_path(Path::new(&meta.save_path));

    // 校验/恢复：重探测确认远端文件未变化
    if meta.accept_ranges {
        let client = probe::client();
        if let Some(info) = probe::range_probe_detail(
            &client,
            &meta.url,
            meta.referer.as_deref(),
            meta.cookies.as_deref(),
            meta.user_agent.as_deref(),
        )
        .await
        {
            let changed = info.total_size.is_some_and(|t| t != meta.total_size)
                || both_present_diff(&info.etag, &meta.etag);
            if changed {
                // 远端文件已变化：重置分片并更新元数据
                sidecar.remove();
                let _ = sidecar.create();
                meta.total_size = info.total_size.unwrap_or(0);
                meta.accept_ranges = info.accept_ranges;
                meta.etag = info.etag.clone();
                meta.last_modified = info.last_modified.clone();
                *shared.meta.write().unwrap() = meta.clone();
                let _ = sidecar.write_meta(&SidecarMeta {
                    version: 1,
                    url: meta.url.clone(),
                    mirrors: meta.mirrors.clone(),
                    referer: meta.referer.clone(),
                    save_path: meta.save_path.clone(),
                    filename: meta.filename.clone(),
                    total_size: meta.total_size,
                    accept_ranges: meta.accept_ranges,
                    etag: meta.etag.clone(),
                    last_modified: meta.last_modified.clone(),
                    chunk_size: CHUNK_SIZE,
                });
            }
        }
    }

    // 镜像校验 + 自动测速选线：主地址与各镜像采样 256KB，
    // 剔除大小不符/超时的源，按速度加权构建源池（快源占更多分片）
    let pool: Arc<Vec<String>>;
    if meta.accept_ranges {
        let client = probe::client();
        let mut urls = vec![meta.url.clone()];
        for m in meta.mirrors.iter().take(8) {
            if m != &meta.url && !urls.contains(m) {
                urls.push(m.clone());
            }
        }
        let measures = futures::future::join_all(
            urls.iter().map(|u| measure_speed(&client, u)),
        )
        .await;
        let main_speed = measures.first().cloned().flatten().map(|m| m.0);
        let mut valid: Vec<(String, f64)> = Vec::new();
        for (u, m) in urls.iter().skip(1).zip(measures.iter().skip(1)) {
            if let Some((sp, total)) = m {
                if total.map_or(true, |t| t == meta.total_size) {
                    valid.push((u.clone(), *sp));
                }
            }
        }
        if valid.len() != meta.mirrors.len() {
            meta.mirrors = valid.iter().map(|(u, _)| u.clone()).collect();
            *shared.meta.write().unwrap() = meta.clone();
            let _ = sidecar.write_meta(&SidecarMeta {
                version: 1,
                url: meta.url.clone(),
                mirrors: meta.mirrors.clone(),
                referer: meta.referer.clone(),
                save_path: meta.save_path.clone(),
                filename: meta.filename.clone(),
                total_size: meta.total_size,
                accept_ranges: meta.accept_ranges,
                etag: meta.etag.clone(),
                last_modified: meta.last_modified.clone(),
                chunk_size: CHUNK_SIZE,
            });
        }
        // 构建加权源池：主地址永远在场；速度按 3 倍归一取 1~4 份
        let mut entries: Vec<(String, f64)> = vec![(meta.url.clone(), main_speed.unwrap_or(0.0))];
        entries.extend(valid);
        if entries.len() > 1 {
            let max = entries
                .iter()
                .map(|(_, s)| *s)
                .fold(0.0f64, f64::max)
                .max(1.0);
            let mut built: Vec<String> = Vec::new();
            for (u, sp) in &entries {
                let w = (((3.0 * sp / max).round() as usize).clamp(1, 4)).min(4);
                for _ in 0..w {
                    built.push(u.clone());
                }
            }
            pool = Arc::new(built);
        } else {
            pool = Arc::new(vec![meta.url.clone()]);
        }
    } else {
        pool = Arc::new(vec![meta.url.clone()]);
    }

    shared
        .downloaded
        .store(sidecar.sum_parts(), Ordering::Relaxed);

    let meter_id = id.clone();
    let meter_shared = shared.clone();
    let meter = tauri::async_runtime::spawn(async move {
        run_meter(meter_id, meter_shared).await;
    });

    let result = if !meta.accept_ranges || meta.total_size == 0 {
        run_single(&shared, &sidecar, &meta).await
    } else {
        run_parallel(&shared, &sidecar, &meta, pool).await
    };

    shared.active_conns.store(0, Ordering::Relaxed);
    meter.abort();

    let cancel = shared.cancel_flag.load(Ordering::Relaxed);
    if cancel {
        // 已被 delete_task 移出 map：清理 sidecar 后直接退出
        sidecar.remove();
        inner.running.fetch_sub(1, Ordering::Relaxed);
        inner.dispatch();
        return;
    }

    match result {
        Flow::Ok => {
            *shared.status.write().unwrap() = TaskStatus::Merging;
            inner.emit_progress(&id);

            let save_path = meta.save_path.clone();
            let parts_dir = sidecar.parts_dir.clone();
            let total = meta.total_size;
            let single = !meta.accept_ranges;
            let merge = tokio::task::spawn_blocking(move || {
                merge_parts(&parts_dir, &save_path, total, single)
            });
            match merge.await {
                Ok(Ok(())) => {
                    sidecar.remove();
                    *shared.status.write().unwrap() = TaskStatus::Completed;
                    shared.eta_secs.write().unwrap().take();
                    *shared.speed_bps.write().unwrap() = 0.0;
                    if meta.total_size > 0 {
                        shared.downloaded.store(meta.total_size, Ordering::Relaxed);
                    }
                    inner.record_completion(meta.total_size);
                }
                Ok(Err(e)) => {
                    *shared.status.write().unwrap() = TaskStatus::Failed;
                    *shared.error.write().unwrap() = Some(format!("合并失败: {e}"));
                }
                Err(e) => {
                    *shared.status.write().unwrap() = TaskStatus::Failed;
                    *shared.error.write().unwrap() = Some(e.to_string());
                }
            }
        }
        Flow::Interrupted => {
            *shared.status.write().unwrap() = TaskStatus::Paused;
        }
        Flow::Fatal(e) => {
            // 直链过期救援：解析类任务（记录了分享页 source_url）遇 404/403
            // 时重新解析拿新直链（82dj 实测直链分钟级失效——重装/重启后自动
            // 恢复的旧任务必 404）。分片保留（同文件 Range 内容一致），
            // 回队列后由正常收尾的 dispatch 重新调度
            let expired = e.contains("404") || e.contains("403");
            let rescued = expired
                && !shared.re_rescued.swap(true, Ordering::Relaxed)
                && re_rescue_link(&inner, &shared).await;
            if rescued {
                let (url, mirrors, referer, save_path, filename, total, ranges) = {
                    let m = shared.meta.read().unwrap();
                    (
                        m.url.clone(),
                        m.mirrors.clone(),
                        m.referer.clone(),
                        m.save_path.clone(),
                        m.filename.clone(),
                        m.total_size,
                        m.accept_ranges,
                    )
                };
                let _ = sidecar.write_meta(&SidecarMeta {
                    version: 1,
                    url,
                    mirrors,
                    referer,
                    save_path,
                    filename,
                    total_size: total,
                    accept_ranges: ranges,
                    // etag 置空：新直链的校验值与旧分片不同属预期，勿触发重置
                    etag: None,
                    last_modified: None,
                    chunk_size: CHUNK_SIZE,
                });
                *shared.status.write().unwrap() = TaskStatus::Queued;
                *shared.error.write().unwrap() = None;
            } else {
                let msg = if expired {
                    format!("{e}；直链已过期且自动重解析失败，请删除任务后重新添加（分享页链接）")
                } else {
                    e
                };
                *shared.status.write().unwrap() = TaskStatus::Failed;
                *shared.error.write().unwrap() = Some(msg);
            }
        }
    }

    *shared.speed_bps.write().unwrap() = 0.0;
    inner.running.fetch_sub(1, Ordering::Relaxed);
    inner.emit_progress(&id);
    inner.persist();
    inner.dispatch();
}

/// 直链过期救援：用分享页 source_url 重新解析（含 WebView 代取）拿新直链。
/// 成功则更新 shared.meta（url/大小/断点；etag 置空防「文件已变化」误判
/// 重置分片）并返回 true。失败返回 false（任务照常标 Failed）。
async fn re_rescue_link(inner: &Inner, shared: &Arc<TaskShared>) -> bool {
    let Some(source) = shared.meta.read().unwrap().source_url.clone() else {
        return false;
    };
    let client = probe::client();
    let info = match probe::probe(&client, &source).await {
        Ok(i) if i.notice.is_none() => i,
        _ => return false,
    };
    let mut new_url = info.resolved_url.clone().unwrap_or_default();
    let mut new_cookie: Option<String> = None;
    if new_url.is_empty() || info.webview_plan.is_some() {
        let (Some(plan), Some(app)) = (&info.webview_plan, inner.app.get()) else {
            return false;
        };
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        {
            match crate::downloader::webview_extract::extract(app.clone(), plan).await {
                Ok(wv) if !wv.url.is_empty() => {
                    new_url = wv.url;
                    new_cookie = wv.cookie;
                }
                _ => return false,
            }
        }
        #[cfg(any(target_os = "ios", target_os = "android"))]
        {
            let _ = app;
            return false;
        }
    }
    if new_url.is_empty() {
        return false;
    }
    // 锁内先定好新 url/referer/cookie（guard 不可跨 await），探测在锁外做
    let (url_new, referer_new) = {
        let mut m = shared.meta.write().unwrap();
        if new_url == m.url {
            return false; // 直链未变，重试无意义
        }
        let referer_new = info.referer.clone().or_else(|| m.referer.clone());
        m.url = new_url;
        m.etag = None;
        m.last_modified = None;
        if info.referer.is_some() {
            m.referer = info.referer.clone();
        }
        m.cookies = new_cookie.clone().or_else(|| info.cookies.clone());
        if info.user_agent.is_some() {
            m.user_agent = info.user_agent.clone();
        }
        (m.url.clone(), referer_new)
    };
    // 新直链二次探测拿真实大小/断点（best-effort，失败沿用旧值）
    if let Some(p2) =
        probe::probe_direct_ref(
            &client,
            &url_new,
            referer_new.as_deref(),
            new_cookie.as_deref(),
            info.user_agent.as_deref(),
        )
        .await
    {
        let mut m = shared.meta.write().unwrap();
        if let Some(t) = p2.total_size {
            m.total_size = t;
        }
        m.accept_ranges = p2.accept_ranges;
    }
    true
}

/// 速度计量 + 进度事件推送
async fn run_meter(id: String, shared: Arc<TaskShared>) {
    let mut prev = shared.downloaded.load(Ordering::Relaxed);
    let mut ema = 0.0f64;
    let mut first = true;
    loop {
        tokio::time::sleep(METER_INTERVAL).await;
        let status = *shared.status.read().unwrap();
        if status != TaskStatus::Downloading || shared.removed.load(Ordering::Relaxed) {
            break;
        }
        let cur = shared.downloaded.load(Ordering::Relaxed);
        let delta = cur.saturating_sub(prev);
        prev = cur;
        let raw = delta as f64 / METER_INTERVAL.as_secs_f64();
        ema = if first { raw } else { ema * 0.65 + raw * 0.35 };
        first = false;
        *shared.speed_bps.write().unwrap() = ema;
        *shared.eta_secs.write().unwrap() = {
            let total = shared.meta.read().unwrap().total_size;
            if total > 0 && ema > 1.0 {
                Some(((total - cur.min(total)) as f64 / ema) as u64)
            } else {
                None
            }
        };
        if let Some(app) = APP.get() {
            let _ = app.emit("task://events", TaskEvent::Progress { task: shared.snapshot() });
        }
        let _ = &id; // 保留参数以备日志
    }
}

static APP: OnceLock<AppHandle> = OnceLock::new();
pub fn set_app_handle(app: AppHandle) {
    let _ = APP.set(app);
}

/// 并行多连接下载（urls: 加权源池，分片轮询分发）
async fn run_parallel(
    shared: &Arc<TaskShared>,
    sidecar: &Sidecar,
    meta: &TaskMeta,
    pool: Arc<Vec<String>>,
) -> Flow {
    let total = meta.total_size;
    let chunk_size = CHUNK_SIZE;
    let chunk_count = total.div_ceil(chunk_size) as u32;

    let mut queue = VecDeque::new();
    for idx in 0..chunk_count {
        let expected = chunk_len(idx, chunk_size, total);
        if sidecar.part_size(idx) < expected {
            queue.push_back(idx);
        }
    }
    let queue = Arc::new(Mutex::new(queue));
    let workers = (meta.connections.clamp(1, 64) as usize).min(chunk_count.max(1) as usize);
    shared.active_conns.store(workers as u32, Ordering::Relaxed);

    let client = probe::download_client();
    let url_counter = Arc::new(AtomicU64::new(0));
    let urls = pool;

    let mut joins = Vec::new();
    for _ in 0..workers {
        let queue = queue.clone();
        let shared = shared.clone();
        let parts_dir = sidecar.parts_dir.clone();
        let client = client.clone();
        let urls = urls.clone();
        let url_counter = url_counter.clone();
        joins.push(tauri::async_runtime::spawn(async move {
            loop {
                if shared.pause_flag.load(Ordering::Relaxed)
                    || shared.cancel_flag.load(Ordering::Relaxed)
                    || shared.error_flag.load(Ordering::Relaxed)
                {
                    return;
                }
                let idx = {
                    let mut q = queue.lock().unwrap();
                    match q.pop_front() {
                        Some(i) => i,
                        None => return, // 队列空 → 干完收工
                    }
                };
                let n = url_counter.fetch_add(1, Ordering::Relaxed) as usize;
                let url = urls[n % urls.len()].clone();
                let start = idx as u64 * chunk_size;
                let end = start + chunk_len(idx, chunk_size, total) - 1;
                let part = parts_dir.join(format!("part_{idx:06}"));
                match download_chunk(&client, &url, &part, start, end, &shared).await {
                    Ok(()) => continue,
                    Err(Flow::Ok) => continue, // 不可能出现，补全分支
                    Err(Flow::Interrupted) => return,
                    Err(Flow::Fatal(e)) => {
                        shared.error_flag.store(true, Ordering::Relaxed);
                        *shared.error.write().unwrap() = Some(e);
                        return;
                    }
                }
            }
        }));
    }
    for j in joins {
        let _ = j.await;
    }

    if shared.error_flag.load(Ordering::Relaxed) {
        return Flow::Fatal(shared.error.read().unwrap().clone().unwrap_or_default());
    }
    if shared.cancel_flag.load(Ordering::Relaxed) {
        return Flow::Interrupted;
    }
    Flow::Ok
}

/// 单连接下载（不支持 Range 或大小未知）
async fn run_single(shared: &Arc<TaskShared>, sidecar: &Sidecar, meta: &TaskMeta) -> Flow {
    // 单连接重建分片结构：先存 meta，remove 旧目录后立刻写回，
    // 否则合并/恢复阶段读不到元数据
    let saved_meta = SidecarMeta {
        version: 1,
        url: meta.url.clone(),
        mirrors: meta.mirrors.clone(),
        referer: meta.referer.clone(),
        save_path: meta.save_path.clone(),
        filename: meta.filename.clone(),
        total_size: meta.total_size,
        accept_ranges: meta.accept_ranges,
        etag: meta.etag.clone(),
        last_modified: meta.last_modified.clone(),
        chunk_size: CHUNK_SIZE,
    };
    sidecar.remove();
    let _ = sidecar.create();
    let _ = sidecar.write_meta(&saved_meta);
    shared.active_conns.store(1, Ordering::Relaxed);

    let client = probe::download_client();
    let part = sidecar.part_file(0);
    let shared2 = shared.clone();
    match download_single(&client, &meta.url, &part, &shared2).await {
        Ok(()) => Flow::Ok,
        Err(e) => e,
    }
}

/// 下载单个分片（追加到 part 文件，重试时从断点继续）。
/// 重试计数只统计「无进展」的失败；只要分片有字节增长就重置计数，
/// 这样即使服务器反复限速断流，也能持续续传直到完成。
async fn download_chunk(
    client: &reqwest::Client,
    url: &str,
    part: &Path,
    start: u64,
    end: u64,
    shared: &TaskShared,
) -> Result<(), Flow> {
    let expected = end - start + 1;
    let mut attempt = 0u32;
    let mut last_written = part_size(part);
    loop {
        if shared.pause_flag.load(Ordering::Relaxed)
            || shared.cancel_flag.load(Ordering::Relaxed)
            || shared.error_flag.load(Ordering::Relaxed)
        {
            return Err(Flow::Interrupted);
        }
        let written = part_size(part);
        if written >= expected {
            return Ok(());
        }
        if written > last_written {
            attempt = 0; // 有进展 → 重置连续失败计数
        }
        last_written = written;
        if attempt >= MAX_CHUNK_RETRIES {
            return Err(Flow::Fatal("连续多次重试均无进展，连接持续失败".into()));
        }

        let range = format!("bytes={}-{}", start + written, end);
        let req = client.get(url).header(RANGE, range);
        let req = {
            let m = shared.meta.read().unwrap();
            let req = match m.referer.clone() {
                Some(r) => req.header(reqwest::header::REFERER, r),
                None => req,
            };
            // 会话 cookie 透传（GoFile 等 CDN 校验会话级凭据的站点）
            let req = match m.cookies.clone() {
                Some(c) => req.header(reqwest::header::COOKIE, c),
                None => req,
            };
            // 站点要求的 UA（百度网盘 dlink 强制 netdisk）
            match m.user_agent.clone() {
                Some(ua) => req.header(reqwest::header::USER_AGENT, ua),
                None => req,
            }
        };
        let resp = req.send().await;

        match resp {
            Ok(r) if r.status() == StatusCode::PARTIAL_CONTENT => {
                let finished = stream_response(r, part, written, expected, shared, None).await?;
                if finished {
                    return Ok(());
                }
                // 流提前结束（服务器断开/超时）：保留断点，退避重试
            }
            Ok(r) if r.status().is_success() => {
                // 服务器忽略 Range 返回 200：只能从头读，按 offset 跳过
                let skip = start + written;
                if written > 0 {
                    let _ = std::fs::remove_file(part);
                }
                let finished = stream_response(r, part, 0, expected, shared, Some(skip)).await?;
                if finished {
                    return Ok(());
                }
            }
            Ok(r) => {
                // 4xx/5xx：退避重试
                if r.status().as_u16() == 416 {
                    // 越界，视为完成
                    return Ok(());
                }
            }
            Err(_) => {}
        }
        attempt += 1;
        backoff(attempt).await;
    }
}

/// 单连接整文件下载（无 Range）
async fn download_single(
    client: &reqwest::Client,
    url: &str,
    part: &Path,
    shared: &TaskShared,
) -> Result<(), Flow> {
    let mut attempt = 0u32;
    loop {
        if shared.pause_flag.load(Ordering::Relaxed)
            || shared.cancel_flag.load(Ordering::Relaxed)
            || shared.error_flag.load(Ordering::Relaxed)
        {
            return Err(Flow::Interrupted);
        }
        let (referer, cookies, user_agent) = {
            let m = shared.meta.read().unwrap();
            (m.referer.clone(), m.cookies.clone(), m.user_agent.clone())
        };
        let req = client.get(url);
        let req = match referer {
            Some(r) => req.header(reqwest::header::REFERER, r),
            None => req,
        };
        // 会话 cookie 透传（GoFile 等 CDN 校验会话级凭据的站点）
        let req = match cookies {
            Some(c) => req.header(reqwest::header::COOKIE, c),
            None => req,
        };
        // 站点要求的 UA（百度网盘 dlink 强制 netdisk）
        let req = match user_agent {
            Some(ua) => req.header(reqwest::header::USER_AGENT, ua),
            None => req,
        };
        match req.send().await {
            Ok(r) if r.status().is_success() => {
                let _ = std::fs::remove_file(part);
                // expected 取任务总大小：stream_response 的完成判定要求
                // expected > 0，传 0 会让无 Range 服务器的单连接下载永远
                // 「未完成」退避重试到 Fatal（r3 轮 9 回归测试实锤，潜伏 bug）
                let expected = shared.meta.read().unwrap().total_size;
                if stream_response(r, part, 0, expected, shared, None).await? {
                    return Ok(());
                }
                // 流提前结束：重试
                backoff(attempt.min(MAX_SINGLE_RETRIES - 1)).await;
                attempt += 1;
                if attempt >= MAX_SINGLE_RETRIES {
                    return Err(Flow::Fatal("连接中断，重试次数已达上限".into()));
                }
            }
            Ok(r) => {
                backoff(attempt.min(MAX_SINGLE_RETRIES - 1)).await;
                attempt += 1;
                if attempt >= MAX_SINGLE_RETRIES {
                    return Err(Flow::Fatal(format!("服务器返回 {}", r.status())));
                }
            }
            Err(e) => {
                backoff(attempt.min(MAX_SINGLE_RETRIES - 1)).await;
                attempt += 1;
                if attempt >= MAX_SINGLE_RETRIES {
                    return Err(Flow::Fatal(format!("连接失败: {e}")));
                }
            }
        }
    }
}

/// 流式写盘。返回 Ok(true) 表示该分片已完成；Ok(false) 表示流提前结束需重试。
/// skip_offset: 服务器忽略 Range 时需跳过的字节数。
async fn stream_response(
    resp: reqwest::Response,
    part: &Path,
    written: u64,
    expected: u64,
    shared: &TaskShared,
    skip_offset: Option<u64>,
) -> Result<bool, Flow> {
    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(part)
        .await
        .map_err(|e| Flow::Fatal(format!("写入文件失败: {e}")))?;
        let mut skip = skip_offset.unwrap_or(0);
        let mut done = written;

        let mut stream = resp.bytes_stream();
        loop {
            // 停滞的连接也能被暂停/删除瞬间唤醒
            let item = tokio::select! {
                item = stream.next() => item,
                _ = shared.wait_interrupt() => {
                    let _ = file.flush().await;
                    return Err(Flow::Interrupted);
                }
            };
            let bytes = match item {
                Some(Ok(b)) => b,
                Some(Err(e)) => {
                    // 读取停滞超时 → 记一次 strike，累计后整体降级 HTTP/1.1
                    if e.is_timeout() {
                        crate::downloader::probe::note_stall();
                    }
                    // 网络中断/超时：保留已写数据，返回 false 让上层从断点重试
                    let _ = file.flush().await;
                    return Ok(false);
                }
                None => break, // 流自然结束，走循环尾部的完成度检查
            };
        let mut data: &[u8] = &bytes;
        if skip > 0 {
            if data.len() as u64 <= skip {
                skip -= data.len() as u64;
                continue;
            }
            data = &data[skip as usize..];
            skip = 0;
        }
        if data.is_empty() {
            continue;
        }
        file.write_all(data)
            .await
            .map_err(|e| Flow::Fatal(format!("写入文件失败: {e}")))?;
        done += data.len() as u64;
        shared.downloaded.fetch_add(data.len() as u64, Ordering::Relaxed);

        if shared.pause_flag.load(Ordering::Relaxed)
            || shared.cancel_flag.load(Ordering::Relaxed)
        {
            let _ = file.flush().await;
            return Err(Flow::Interrupted);
        }
        if expected > 0 && done >= expected {
            let _ = file.flush().await;
            // 多写了几字节（200 全量响应越过 end）时截断
            let overshoot = done - expected;
            if overshoot > 0 {
                truncate_file(part, done - overshoot).await;
                shared.downloaded.fetch_sub(overshoot, Ordering::Relaxed);
            }
            return Ok(true);
        }
    }
    let _ = file.flush().await;
    // expected == 0（未知大小的单连接流）：自然 EOF 即完整；
    // 分片路径恒有 expected > 0，语义不受影响
    if expected == 0 || done >= expected {
        Ok(true)
    } else {
        Ok(false)
    }
}

async fn truncate_file(path: &Path, size: u64) {
    let f = match tokio::fs::OpenOptions::new().write(true).open(path).await {
        Ok(f) => f,
        Err(_) => return,
    };
    let _ = f.set_len(size);
}

async fn backoff(attempt: u32) {
    let secs = std::cmp::min(1u64 << attempt.min(6), 30);
    tokio::time::sleep(Duration::from_secs(secs)).await;
}

fn part_size(path: &Path) -> u64 {
    std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}

/// 分片 idx 的实际长度
fn chunk_len(idx: u32, chunk_size: u64, total: u64) -> u64 {
    let start = idx as u64 * chunk_size;
    total.saturating_sub(start).min(chunk_size)
}

/// 合并分片到最终文件并校验大小。single_unknown: 单连接且大小未知（直接改名）。
fn merge_parts(parts_dir: &Path, save_path: &str, total: u64, single_unknown: bool) -> Result<(), String> {
    let out = Path::new(save_path);
    if single_unknown {
        let only = parts_dir.join("part_000000");
        if std::fs::rename(&only, out).is_ok() {
            return Ok(());
        }
        // 某些平台 rename 跨目录失败则走复制
        std::fs::copy(&only, out).map_err(|e| e.to_string())?;
        return Ok(());
    }

    let tmp = out.with_extension("feiliu-merge.tmp");
    let mut file = std::fs::File::create(&tmp).map_err(|e| e.to_string())?;
    let mut written: u64 = 0;
    let mut idx = 0u32;
    loop {
        let part = parts_dir.join(format!("part_{idx:06}"));
        if !part.exists() {
            break;
        }
        let n = std::io::copy(
            &mut std::fs::File::open(&part).map_err(|e| e.to_string())?,
            &mut file,
        )
        .map_err(|e| e.to_string())?;
        written += n;
        idx += 1;
    }
    if total > 0 && written != total {
        let _ = std::fs::remove_file(&tmp);
        return Err(format!("分片合计 {written} 字节，与服务器报告的 {total} 字节不符"));
    }
    drop(file);
    std::fs::rename(&tmp, out).map_err(|e| e.to_string())?;
    Ok(())
}

// ---------- 命名与工具 ----------

/// 测速 + 总大小探测：Range 采样 256KB，12 秒封顶。返回 (速度 B/s, 总大小)
async fn measure_speed(client: &reqwest::Client, url: &str) -> Option<(f64, Option<u64>)> {
    let fut = async {
        let start = std::time::Instant::now();
        let resp = client.get(url).header(RANGE, "bytes=0-262143").send().await.ok()?;
        let status = resp.status();
        if !(status.is_success() || status == StatusCode::PARTIAL_CONTENT) {
            return None;
        }
        let headers = resp.headers().clone();
        let total = if status == StatusCode::PARTIAL_CONTENT {
            headers
                .get(CONTENT_RANGE)?
                .to_str()
                .ok()?
                .rsplit('/')
                .next()?
                .parse::<u64>()
                .ok()
        } else {
            headers
                .get(CONTENT_LENGTH)?
                .to_str()
                .ok()?
                .parse::<u64>()
                .ok()
        };
        let mut n = 0u64;
        let mut stream = resp.bytes_stream();
        while n < 262_144 {
            match stream.next().await {
                Some(Ok(b)) => n += b.len() as u64,
                _ => break,
            }
        }
        let el = start.elapsed().as_secs_f64();
        if n == 0 || el <= 0.0 {
            return None;
        }
        Some((n as f64 / el, total))
    };
    tokio::time::timeout(std::time::Duration::from_secs(12), fut)
        .await
        .ok()?
}

fn sanitize_name(name: Option<String>) -> Option<String> {
    let name = name?;
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    sanitize_filename(name)
}

fn fallback_filename(url: &str) -> String {
    let path = url.split(['?', '#']).next().unwrap_or(url);
    let seg = path.rsplit('/').next().unwrap_or("");
    if !seg.is_empty() {
        if let Some(f) = sanitize_filename(&percent_decode(seg)) {
            return f;
        }
    }
    format!("download_{}", now_ms())
}

fn percent_decode(s: &str) -> String {
    percent_encoding::percent_decode_str(s)
        .decode_utf8_lossy()
        .into_owned()
}

fn unique_save_path(dir: &Path, filename: &str) -> PathBuf {
    let mut candidate = dir.join(filename);
    if !candidate.exists()
        && !Sidecar::from_save_path(&candidate).dir.exists()
    {
        return candidate;
    }
    let stem = Path::new(filename)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| filename.to_string());
    let ext = Path::new(filename)
        .extension()
        .map(|s| format!(".{}", s.to_string_lossy()))
        .unwrap_or_default();
    for n in 1..10000u32 {
        candidate = dir.join(format!("{stem} ({n}){ext}"));
        if !candidate.exists() && !Sidecar::from_save_path(&candidate).dir.exists() {
            return candidate;
        }
    }
    dir.join(format!("{stem}_{}{ext}", now_ms()))
}

/// 从 magnet 链接解析 btih 信息哈希（40 位 hex；32 位 base32 不支持，交由元数据补全）
fn parse_magnet_btih(url: &str) -> Option<String> {
    let idx = url.find("urn:btih:")?;
    let rest = &url[idx + "urn:btih:".len()..];
    let hex: String = rest
        .chars()
        .take_while(|c| c.is_ascii_hexdigit())
        .collect();
    if hex.len() == 40 {
        Some(hex.to_lowercase())
    } else {
        None
    }
}

/// 字节数 → 人类可读（KB/MB/GB）
pub fn human_size(n: u64) -> String {
    if n < 1024 {
        return format!("{n} B");
    }
    let units = ["KB", "MB", "GB", "TB"];
    let mut v = n as f64 / 1024.0;
    let mut i = 0;
    while v >= 1024.0 && i < units.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    format!("{v:.1} {}", units[i])
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn both_present_diff(a: &Option<String>, b: &Option<String>) -> bool {
    matches!((a, b), (Some(x), Some(y)) if x != y)
}

fn default_download_dir(app: &AppHandle) -> String {
    if cfg!(any(target_os = "ios", target_os = "android")) {
        if let Ok(p) = app.path().app_data_dir() {
            return p.join("Downloads").to_string_lossy().into_owned();
        }
    }
    dirs::download_dir()
        .or_else(|| dirs::home_dir())
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default()
}

// ---------- 单元测试 ----------

#[cfg(test)]
mod tests {
    use super::*;

    /// GoFile probe_url 批量判定：d/ 分享恒有 gofile_share 批量键（前端按钮依据）
    #[tokio::test]
    async fn test_uc_probe_batch_kind() {
        if std::env::var("FEILIU_NET_TEST").is_err() {
            return;
        }
        let mgr = TaskManager::new();
        let p = mgr
            .inner
            .probe_url("https://drive.uc.cn/s/12450d1694844?public=1")
            .await
            .expect("probe 失败");
        assert_eq!(p.batch_kind.as_deref(), Some("uc_share"));
        let p2 = mgr
            .inner
            .probe_url("https://drive.uc.cn/s/a55f8f6ebd234?public=1&feiliu_i=3")
            .await
            .expect("probe 失败");
        assert_eq!(p2.batch_kind.as_deref(), Some("uc_share"), "序号形态同判");
        assert!(
            p2.url.contains(".pds.uc.cn/"),
            "序号形态应解析出直链（批量入口）: {}",
            p2.url
        );
    }

    #[tokio::test]
    async fn test_gofile_probe_batch_kind() {
        if std::env::var("FEILIU_NET_TEST").is_err() {
            return;
        }
        let mgr = TaskManager::new();
        let p = mgr
            .inner
            .probe_url("https://gofile.io/d/d1AQzWks")
            .await
            .expect("gofile probe 失败");
        assert_eq!(p.batch_kind.as_deref(), Some("gofile_share"));
        assert!(p.webview_plan.is_some(), "gofile 必须带代取计划");
        let p2 = mgr
            .inner
            .probe_url("https://gofile.io/d/d1AQzWks?feiliu_i=2")
            .await
            .expect("带序号 probe 失败");
        assert_eq!(p2.batch_kind.as_deref(), Some("gofile_share"), "序号形态同判");
    }

    /// 防重复：同 URL 二次添加被拒（通过 add_task 完整流程验证）
    #[tokio::test]
    async fn test_add_task_duplicate_url() {
        if std::env::var("FEILIU_NET_TEST").is_err() {
            return;
        }
        let mgr = TaskManager::new();
        // 构造设置指向临时目录
        let dir = std::env::temp_dir().join(format!("feiliu_dup_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        mgr.inner.update_settings(crate::downloader::types::Settings {
            download_dir: dir.to_string_lossy().into_owned(),
            ..Default::default()
        });
        let url = "https://proof.ovh.net/files/10Mb.dat";
        let first = mgr.inner.add_task(url.into(), vec![], None, None, Some(2)).await;
        assert!(first.is_ok(), "首次添加应成功: {:?}", first.err());
        let second = mgr.inner.add_task(url.into(), vec![], None, None, Some(2)).await;
        let err = second.expect_err("二次添加应被拒");
        assert!(err.contains("重复"), "错误应提示重复: {err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_chunk_len() {
        let cs = 8 * 1024 * 1024;
        assert_eq!(chunk_len(0, cs, 100), 100);
        assert_eq!(chunk_len(1, cs, cs * 2 + 5), cs); // idx1 在范围内
        assert_eq!(chunk_len(2, cs, cs * 2 + 5), 5);
        assert_eq!(chunk_len(3, cs, cs * 2 + 5), 0); // 越界为 0（不会入队）
    }

    #[test]
    fn test_unique_save_path() {
        let dir = std::env::temp_dir().join(format!("feiliu_test_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let p1 = unique_save_path(&dir, "a.mp4");
        std::fs::write(&p1, b"x").unwrap();
        let p2 = unique_save_path(&dir, "a.mp4");
        assert_ne!(p1, p2);
        assert!(p2.to_string_lossy().contains("(1)"));
        let p3 = unique_save_path(&dir, "b.mp4");
        assert!(p3.ends_with("b.mp4"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_both_present_diff() {
        assert!(both_present_diff(&Some("a".into()), &Some("b".into())));
        assert!(!both_present_diff(&Some("a".into()), &Some("a".into())));
        assert!(!both_present_diff(&Some("a".into()), &None));
    }

    /// 真实网络下载链路测试（Range 分片下载 → 合并校验）。
    /// 需要设置 FEILIU_NET_TEST=1 才会执行，避免 CI/离线环境失败。
    #[tokio::test]
    async fn test_real_range_download_and_merge() {
        if std::env::var("FEILIU_NET_TEST").is_err() {
            return;
        }
        let dir = std::env::temp_dir().join(format!("feiliu_net_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let client = probe::client();
        // 之前验证过的 CDN 直链（支持 Range，1GB）
        let url = "https://d1.xia12345.com/video/202608/6a6ed872be30072b672b4fe8/720.mp4";
        let shared = Arc::new(TaskShared {
            id: "test".into(),
            meta: RwLock::new(TaskMeta {
                url: url.into(),
                kind: TaskKind::Http,
                info_hash: None,
                mirrors: vec![],
                referer: None,
                cookies: None,
                user_agent: None,
                source_url: None,
                canonical: None,
                filename: "test.mp4".into(),
                save_path: dir.join("out.mp4").to_string_lossy().into_owned(),
                total_size: 1_002_897_303,
                accept_ranges: true,
                etag: None,
                last_modified: None,
                connections: 2,
                created_at: now_ms(),
            }),
            status: RwLock::new(TaskStatus::Downloading),
            error: RwLock::new(None),
            downloaded: AtomicU64::new(0),
            speed_bps: RwLock::new(0.0),
            eta_secs: RwLock::new(None),
            active_conns: AtomicU32::new(0),
            pause_flag: AtomicBool::new(false),
            cancel_flag: AtomicBool::new(false),
            error_flag: AtomicBool::new(false),
            removed: AtomicBool::new(false),
            re_rescued: AtomicBool::new(false),
            interrupt: tokio::sync::Notify::new(),
        });

        let mb = 1024 * 1024u64;
        let p0 = dir.join("part_000000");
        let p1 = dir.join("part_000001");
        download_chunk(&client, url, &p0, 0, mb - 1, &shared).await.unwrap();
        download_chunk(&client, url, &p1, mb, 2 * mb - 1, &shared).await.unwrap();
        assert_eq!(part_size(&p0), mb);
        assert_eq!(part_size(&p1), mb);

        let out = dir.join("out.mp4");
        merge_parts(&dir, out.to_str().unwrap(), 2 * mb, false).unwrap();
        assert_eq!(part_size(&out), 2 * mb);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 会话 cookie 透传回归（r3 轮 6 基座 + GoFile 下载依赖的路径）：
    /// 本地 HTTP 服务器捕获请求头——① meta.cookies 时 Cookie/Referer 进请求，
    /// ② cookies=None 时不得发 Cookie 头（老站点零污染）。
    /// download_single 与 Range 分片路径的头构造同源（r3 轮 6 同步改造）。
    #[tokio::test]
    async fn test_cookie_header_passthrough() {
        if std::env::var("FEILIU_NET_TEST").is_err() {
            return;
        }
        use std::io::{Read as _, Write as _};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let captured: Arc<std::sync::Mutex<Vec<Vec<String>>>> = Arc::new(std::sync::Mutex::new(Vec::new()));
        let cap_clone = captured.clone();
        let server = tokio::task::spawn_blocking(move || {
            for _ in 0..2 {
                let Ok((mut s, _)) = listener.accept() else { break };
                let mut buf = [0u8; 8192];
                let n = s.read(&mut buf).unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]).to_string();
                let headers: Vec<String> = req.lines().take(12).map(str::to_string).collect();
                cap_clone.lock().unwrap().push(headers);
                let _ = s.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\nConnection: close\r\n\r\nhello");
            }
        });

        let dir = std::env::temp_dir().join(format!("feiliu_ck_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let mk = |cookies: Option<String>, _part_name: &str| {
            Arc::new(TaskShared {
                id: "ck-test".into(),
                meta: RwLock::new(TaskMeta {
                    url: format!("http://{addr}/f.txt"),
                    kind: TaskKind::Http,
                    info_hash: None,
                    mirrors: vec![],
                    referer: Some("https://gofile.io/".into()),
                    cookies,
                    user_agent: None,
                    source_url: None,
                    canonical: None,
                    filename: "f.txt".into(),
                    save_path: dir.join("out.txt").to_string_lossy().into_owned(),
                    total_size: 5,
                    accept_ranges: false,
                    etag: None,
                    last_modified: None,
                    connections: 1,
                    created_at: now_ms(),
                }),
                status: RwLock::new(TaskStatus::Downloading),
                error: RwLock::new(None),
                downloaded: AtomicU64::new(0),
                speed_bps: RwLock::new(0.0),
                eta_secs: RwLock::new(None),
                active_conns: AtomicU32::new(0),
                pause_flag: AtomicBool::new(false),
                cancel_flag: AtomicBool::new(false),
                error_flag: AtomicBool::new(false),
                removed: AtomicBool::new(false),
                re_rescued: AtomicBool::new(false),
                interrupt: tokio::sync::Notify::new(),
            })
        };

        let client = reqwest::Client::new();
        let url = format!("http://{addr}/f.txt");
        let p1 = dir.join("part_000000");
        download_single(&client, &url, &p1, &mk(Some("accountToken=abc123; other=1".into()), "part_000000"))
            .await
            .unwrap();
        let p2 = dir.join("part_000001");
        download_single(&client, &url, &p2, &mk(None, "part_000001")).await.unwrap();
        server.await.unwrap();

        let caps = captured.lock().unwrap();
        assert_eq!(caps.len(), 2, "应捕获 2 次请求: {}", caps.len());
        let has = |hs: &[String], name: &str| {
            hs.iter().any(|l| l.to_lowercase().starts_with(&format!("{name}:")))
        };
        assert!(has(&caps[0], "cookie"), "带 cookie 场景应发 Cookie 头: {:?}", caps[0]);
        assert!(
            caps[0].iter().any(|l| l.contains("accountToken=abc123")),
            "Cookie 值应完整透传: {:?}",
            caps[0]
        );
        assert!(has(&caps[0], "referer"), "Referer 应透传");
        assert!(!has(&caps[1], "cookie"), "无 cookie 场景不得发 Cookie 头: {:?}", caps[1]);
        assert_eq!(part_size(&p1), 5, "场景 1 下载内容应完整");
        assert_eq!(part_size(&p2), 5, "场景 2 下载内容应完整");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// User-Agent 透传回归（百度网盘 dlink 强制 netdisk UA，普通 UA 403）：
    /// 本地 HTTP 服务器捕获请求头——meta.user_agent → User-Agent 进请求；
    /// None 时不得发 netdisk（客户端默认 UA，老站点零污染）。
    #[tokio::test]
    async fn test_user_agent_header_passthrough() {
        if std::env::var("FEILIU_NET_TEST").is_err() {
            return;
        }
        use std::io::{Read as _, Write as _};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let captured: Arc<std::sync::Mutex<Vec<Vec<String>>>> = Arc::new(std::sync::Mutex::new(Vec::new()));
        let cap_clone = captured.clone();
        let server = tokio::task::spawn_blocking(move || {
            for _ in 0..2 {
                let Ok((mut s, _)) = listener.accept() else { break };
                let mut buf = [0u8; 8192];
                let n = s.read(&mut buf).unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]).to_string();
                let headers: Vec<String> = req.lines().take(12).map(str::to_string).collect();
                cap_clone.lock().unwrap().push(headers);
                let _ = s.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\nConnection: close\r\n\r\nhello");
            }
        });

        let dir = std::env::temp_dir().join(format!("feiliu_ua_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let mk = |user_agent: Option<String>| {
            Arc::new(TaskShared {
                id: "ua-test".into(),
                meta: RwLock::new(TaskMeta {
                    url: format!("http://{addr}/f.txt"),
                    kind: TaskKind::Http,
                    info_hash: None,
                    mirrors: vec![],
                    referer: None,
                    cookies: None,
                    user_agent,
                    source_url: None,
                    canonical: None,
                    filename: "f.txt".into(),
                    save_path: dir.join("out.txt").to_string_lossy().into_owned(),
                    total_size: 5,
                    accept_ranges: false,
                    etag: None,
                    last_modified: None,
                    connections: 1,
                    created_at: now_ms(),
                }),
                status: RwLock::new(TaskStatus::Downloading),
                error: RwLock::new(None),
                downloaded: AtomicU64::new(0),
                speed_bps: RwLock::new(0.0),
                eta_secs: RwLock::new(None),
                active_conns: AtomicU32::new(0),
                pause_flag: AtomicBool::new(false),
                cancel_flag: AtomicBool::new(false),
                error_flag: AtomicBool::new(false),
                removed: AtomicBool::new(false),
                re_rescued: AtomicBool::new(false),
                interrupt: tokio::sync::Notify::new(),
            })
        };

        let client = reqwest::Client::new();
        let url = format!("http://{addr}/f.txt");
        let p1 = dir.join("part_000000");
        download_single(&client, &url, &p1, &mk(Some("netdisk".into())))
            .await
            .unwrap();
        let p2 = dir.join("part_000001");
        download_single(&client, &url, &p2, &mk(None)).await.unwrap();
        server.await.unwrap();

        let caps = captured.lock().unwrap();
        assert_eq!(caps.len(), 2, "应捕获 2 次请求: {}", caps.len());
        let ua_of = |hs: &[String]| {
            hs.iter()
                .find(|l| l.to_lowercase().starts_with("user-agent:"))
                .map(|l| l.to_lowercase())
        };
        assert!(
            ua_of(&caps[0]).map_or(false, |u| u.contains("netdisk")),
            "netdisk UA 应透传: {:?}",
            caps[0]
        );
        assert!(
            ua_of(&caps[1]).map_or(true, |u| !u.contains("netdisk")),
            "None UA 不得发 netdisk（客户端默认）: {:?}",
            caps[1]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 多源（镜像）验证：同一文件在两个不同主机上各拉一个分片，
    /// 验证大小一致后合并。kernel.org 主站 + 镜像站，路径永久有效。
    #[tokio::test]
    async fn test_multi_source_mirrors() {
        if std::env::var("FEILIU_NET_TEST").is_err() {
            return;
        }
        let client = probe::client();
        let p1 = "https://cdn.kernel.org/pub/linux/kernel/v6.x/linux-6.12.tar.xz";
        let p2 = "https://mirrors.kernel.org/pub/linux/kernel/v6.x/linux-6.12.tar.xz";
        let i1 = probe::range_probe_detail(&client, p1, None, None, None).await.expect("主站探测失败");
        let i2 = probe::range_probe_detail(&client, p2, None, None, None).await.expect("镜像探测失败");
        let t1 = i1.total_size.expect("主站无大小");
        let t2 = i2.total_size.expect("镜像无大小");
        assert_eq!(t1, t2, "两源大小不一致");
        assert!(i1.accept_ranges && i2.accept_ranges, "两源都应支持断点");

        let dir = std::env::temp_dir().join(format!("feiliu_mirror_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let mb = 1024 * 1024u64;
        let shared = Arc::new(TaskShared {
            id: "mirror-test".into(),
            meta: RwLock::new(TaskMeta {
                url: p1.into(),
                mirrors: vec![p2.into()],
                referer: None,
                cookies: None,
                user_agent: None,
                source_url: None,
                canonical: None,
                kind: TaskKind::Http,
                info_hash: None,
                filename: "test.tar.xz".into(),
                save_path: dir.join("out.tar.xz").to_string_lossy().into_owned(),
                total_size: t1,
                accept_ranges: true,
                etag: None,
                last_modified: None,
                connections: 2,
                created_at: now_ms(),
            }),
            status: RwLock::new(TaskStatus::Downloading),
            error: RwLock::new(None),
            downloaded: AtomicU64::new(0),
            speed_bps: RwLock::new(0.0),
            eta_secs: RwLock::new(None),
            active_conns: AtomicU32::new(0),
            pause_flag: AtomicBool::new(false),
            cancel_flag: AtomicBool::new(false),
            error_flag: AtomicBool::new(false),
            removed: AtomicBool::new(false),
            re_rescued: AtomicBool::new(false),
            interrupt: tokio::sync::Notify::new(),
        });
        let p0 = dir.join("part_000000");
        let p1f = dir.join("part_000001");
        download_chunk(&client, p1, &p0, 0, mb - 1, &shared).await.unwrap();
        download_chunk(&client, p2, &p1f, mb, 2 * mb - 1, &shared).await.unwrap();
        let out = dir.join("out.tar.xz");
        merge_parts(&dir, out.to_str().unwrap(), 2 * mb, false).unwrap();
        assert_eq!(part_size(&out), 2 * mb);
        let _ = std::fs::remove_dir_all(&dir);
        println!("multi-source OK: {} bytes ×2 from cdn + mirror", mb);
    }

    /// B 站解析链路：视频页 → view/playurl API → mp4 直链 → 大小探测
    #[tokio::test]
    async fn test_bilibili_extract() {
        if std::env::var("FEILIU_NET_TEST").is_err() {
            return;
        }
        let client = probe::client();
        // 官方 MV（长期稳定；原测试视频 BV1kMWz6CESY 已被平台下架）
        let page = "https://www.bilibili.com/video/BV1GJ411x7h7/";
        let info = probe::probe(&client, page).await.expect("B 站探测失败");
        let direct = info.resolved_url.expect("未解析出直链");
        assert!(direct.contains("bilivideo.com"), "直链应来自 bilivideo CDN");
        assert!(info.filename.as_deref().unwrap_or("").ends_with(".mp4"));
        assert!(info.total_size.unwrap_or(0) > 100_000, "直链应有真实大小");
        assert_eq!(info.referer.as_deref(), Some("https://www.bilibili.com/"));
        println!(
            "bilibili OK: {} → {} ({} bytes)",
            info.filename.as_deref().unwrap_or(""),
            &direct[..60.min(direct.len())],
            info.total_size.unwrap_or(0)
        );
    }

    /// 抖音完整 probe() 路径（前端实际调用）：含提取+直链探测
    #[tokio::test]
    async fn test_douyin_full_probe() {
        if std::env::var("FEILIU_NET_TEST").is_err() {
            return;
        }
        let client = probe::client();
        let url = "https://v.douyin.com/A65MuS9JbrI/";
        match probe::probe(&client, url).await {
            Ok(info) => {
                println!(
                    "full probe OK: filename={:?} size={:?} ct={:?} resolved={}",
                    info.filename,
                    info.total_size,
                    info.content_type.as_deref().unwrap_or(""),
                    info.resolved_url.is_some()
                );
                assert!(info.resolved_url.is_some(), "应解析出直链");
                assert!(
                    info.total_size.unwrap_or(0) > 100_000,
                    "直链应有真实大小（HEAD/Range 探测）"
                );
            }
            Err(e) => panic!("probe 失败: {e}"),
        }
    }

    /// 82dj probe_url 组装：webview_plan 站点不得返回空 URL（防回归：
    /// 曾因 resolved_url=Some("") 导致前端回传空串、add_task builder error）
    #[tokio::test]
    async fn test_82dj_probe_url_non_empty() {
        if std::env::var("FEILIU_NET_TEST").is_err() {
            return;
        }
        let mgr = TaskManager::new();
        let r = mgr.inner.probe_url("https://www.82dj.com/play/31529").await.expect("probe_url 失败");
        assert!(
            !r.url.is_empty() && r.url.starts_with("http"),
            "ProbeResult.url 不得为空串: {:?}",
            r.url
        );
        assert!(r.webview_plan.is_some(), "82dj 应产出 WebView 代取计划");
        println!("82dj probe_url OK: url={}", r.url);
    }

    /// 82dj 完整 probe 全路径复现（用户报错 play/31529）
    #[tokio::test]
    async fn test_82dj_full_probe() {
        if std::env::var("FEILIU_NET_TEST").is_err() {
            return;
        }
        let client = probe::client();
        let url = "https://www.82dj.com/play/31529";
        // 先单测 extract 层
        let ex = crate::downloader::extract::resolve(url).await;
        println!("extract 层: {}", if ex.is_some() { "Some ✓" } else { "None ✗" });
        // 完整 probe
        match probe::probe(&client, url).await {
            Ok(info) => {
                println!(
                    "probe OK: filename={:?} resolved={:?} notice={:?} plan={}",
                    info.filename,
                    info.resolved_url.is_some(),
                    info.notice.is_some(),
                    info.webview_plan.is_some()
                );
            }
            Err(e) => println!("probe Err: {e}"),
        }
        // 探测子步骤独立复现
        println!("range_probe: {:?}", probe::range_probe_detail(&client, url, None, None, None).await.is_some());
        println!("head_probe: {:?}", probe::head_probe_detail(&client, url, None, None, None).await.is_some());
    }

    /// 网页/登录页链接 → 明确报错而非下载垃圾文件
    #[tokio::test]
    async fn test_html_page_friendly_error() {
        if std::env::var("FEILIU_NET_TEST").is_err() {
            return;
        }
        let client = probe::client();
        let login = "https://weibo.com/newlogin?tabtype=weibo&gid=102803&url=https%3A%2F%2Fwww.weibo.com%2F";
        let err = probe::probe(&client, login)
            .await
            .err()
            .expect("登录页应被拒绝而非下载");
        assert!(err.contains("网页"), "应提示网页链接，实际: {err}");
        println!("登录页友好拒绝 OK: {err}");
    }

    /// BT 链路：磁力链 → DHT/Tracker 元数据 → 实际下载进度
    #[tokio::test]
    async fn test_torrent_magnet() {
        if std::env::var("FEILIU_NET_TEST").is_err() {
            return;
        }
        let dir = std::env::temp_dir().join(format!("feiliu_bt_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let magnet = "magnet:?xt=urn:btih:dd8255ecdc7ca55fb0bbf81323d87062db1f6d1c&dn=Big+Buck+Bunny&tr=udp%3A%2F%2Ftracker.opentrackr.org%3A1337%2Fannounce";
        let session = librqbit::Session::new(dir.clone()).await.expect("BT 会话失败");
        let handle = session
            .add_torrent(
                librqbit::AddTorrent::from_url(magnet),
                Some(librqbit::AddTorrentOptions {
                    output_folder: Some(dir.to_string_lossy().into_owned()),
                    ..Default::default()
                }),
            )
            .await
            .expect("添加磁力失败")
            .into_handle()
            .expect("句柄缺失");

        // 等元数据（最多 60 秒）
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        loop {
            let st = handle.stats();
            if st.total_bytes > 0 {
                println!("metadata OK: {} bytes", st.total_bytes);
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "60 秒内未获取到种子元数据"
            );
            tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
        }

        // 等下载产生实际进度（最多 30 秒）
        let dl_deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        loop {
            let st = handle.stats();
            if st.progress_bytes > 0 {
                println!("download OK: {} bytes downloaded", st.progress_bytes);
                break;
            }
            assert!(
                std::time::Instant::now() < dl_deadline,
                "30 秒内未产生下载进度"
            );
            tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 生命周期统计：累计记账 + stats.json 持久化往返（清除已完成不影响）
    #[test]
    fn test_lifetime_stats_persist_roundtrip() {
        let tmp = std::env::temp_dir().join(format!(
            "feiliu-stats-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&tmp).unwrap();
        let path = tmp.join("stats.json");

        let mgr = TaskManager::new();
        mgr.inner.stats_json.set(path.clone()).unwrap();
        mgr.inner.record_task_added();
        mgr.inner.record_task_added();
        mgr.inner.record_completion(1024);
        mgr.inner.record_completion(2048);
        assert_eq!(mgr.inner.stats().lifetime_tasks, 2);
        assert_eq!(mgr.inner.stats().lifetime_downloaded, 3072);

        // 模拟重启：新实例从磁盘加载
        let mgr2 = TaskManager::new();
        mgr2.inner.stats_json.set(path.clone()).unwrap();
        mgr2.inner.load_stats();
        assert_eq!(mgr2.inner.stats().lifetime_tasks, 2, "重启后任务数应还原");
        assert_eq!(
            mgr2.inner.stats().lifetime_downloaded,
            3072,
            "重启后累计字节应还原"
        );

        std::fs::remove_dir_all(&tmp).ok();
    }
}
