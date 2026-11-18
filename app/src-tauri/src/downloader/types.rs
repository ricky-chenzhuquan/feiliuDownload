use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Queued,
    Downloading,
    Paused,
    Merging,
    Completed,
    Failed,
}

/// 任务类型：HTTP 直链 / BT 种子（磁力链或 .torrent）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TaskKind {
    #[default]
    Http,
    Torrent,
}

/// probe_url 命令的返回值：前端「新建任务」弹窗展示用
#[derive(Debug, Clone, Serialize)]
pub struct ProbeResult {
    pub url: String,
    pub filename: String,
    pub total_size: u64, // 0 表示未知
    pub accept_ranges: bool,
    /// 站点限制提示（如歌单需逐首/网盘需登录）；有值时不允许普通添加
    #[serde(default)]
    pub notice: Option<String>,
    /// 一键批量添加类型（netease_playlist 网易云歌单 / bilibili_collection B 站合集）
    #[serde(default)]
    pub batch_kind: Option<String>,
    /// WebView 代取计划（82dj 等），前端展示「正在代取」状态用
    #[serde(default)]
    pub webview_plan: Option<crate::downloader::extract::WebviewExtractPlan>,
}

/// 歌单批量添加结果
#[derive(Debug, Clone, Serialize)]
pub struct BatchSummary {
    pub playlist: String,
    pub total: u32,
    pub added: u32,
    pub skipped: u32,
    pub truncated: bool,
}

/// probe_mirrors 命令的返回值：单条镜像测速结果
#[derive(Debug, Clone, Serialize)]
pub struct MirrorProbe {
    pub url: String,
    pub ok: bool,
    pub speed_bps: f64,
}

/// 单个任务的完整快照，随事件推送给前端
#[derive(Debug, Clone, Serialize)]
pub struct TaskSnapshot {
    pub id: String,
    pub url: String,
    pub filename: String,
    pub save_path: String,
    pub total_size: u64,
    pub downloaded: u64,
    pub speed_bps: f64,
    pub eta_seconds: Option<u64>,
    pub connections: u32,
    pub sources: usize,
    pub kind: TaskKind,
    pub status: TaskStatus,
    pub error: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub download_dir: String,
    pub max_connections: u32,
    pub max_concurrent_tasks: u32,
    #[serde(default = "default_true")]
    pub auto_resume_on_launch: bool,
    #[serde(default = "default_true")]
    pub clipboard_watch: bool,
    #[serde(default = "default_true")]
    pub close_to_tray: bool,
}

fn default_true() -> bool {
    true
}

impl Default for Settings {
    fn default() -> Self {
        let dir = dirs::download_dir()
            .or_else(dirs::home_dir)
            .unwrap_or_default();
        Self {
            download_dir: dir.to_string_lossy().into_owned(),
            max_connections: 16,
            max_concurrent_tasks: 3,
            auto_resume_on_launch: true,
            clipboard_watch: true,
            close_to_tray: true,
        }
    }
}

/// tasks.json 中的任务元数据（不含瞬时进度）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedTask {
    pub id: String,
    pub url: String,
    #[serde(default)]
    pub mirrors: Vec<String>,
    #[serde(default)]
    pub referer: Option<String>,
    #[serde(default)]
    pub kind: TaskKind,
    #[serde(default)]
    pub info_hash: Option<String>,
    pub filename: String,
    pub save_path: String,
    pub total_size: u64,
    pub connections: u32,
    pub accept_ranges: bool,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub status: TaskStatus,
    pub error: Option<String>,
    pub created_at: i64,
}

/// 推送给前端的事件（单一通道，按 type 区分）
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TaskEvent {
    Progress { task: TaskSnapshot },
    List { tasks: Vec<TaskSnapshot> },
}
