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

/// probe_url 命令的返回值：前端「新建任务」弹窗展示用
#[derive(Debug, Clone, Serialize)]
pub struct ProbeResult {
    pub url: String,
    pub filename: String,
    pub total_size: u64, // 0 表示未知
    pub accept_ranges: bool,
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
