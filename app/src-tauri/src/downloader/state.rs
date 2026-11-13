//! 任务的磁盘状态：分片文件 + 元数据 sidecar + 任务列表持久化。
//!
//! 文件系统本身就是进度状态：每个分片写入独立文件（追加），
//! 文件大小即该分片已完成的字节数；meta.json 记录任务元数据用于校验续传。
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const CHUNK_SIZE: u64 = 8 * 1024 * 1024; // 8 MiB 动态分片
pub const SIDECAR_SUFFIX: &str = ".feiliu";

#[derive(Debug, Serialize, Deserialize)]
pub struct SidecarMeta {
    pub version: u32,
    pub url: String,
    #[serde(default)]
    pub mirrors: Vec<String>,
    pub save_path: String,
    pub filename: String,
    pub total_size: u64,
    pub accept_ranges: bool,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub chunk_size: u64,
}

pub struct Sidecar {
    pub dir: PathBuf,
    pub parts_dir: PathBuf,
    pub meta_path: PathBuf,
}

impl Sidecar {
    /// save_path 旁的 sidecar 目录：`<file>.feiliu/`
    pub fn from_save_path(save_path: &Path) -> Self {
        let mut os = save_path.as_os_str().to_os_string();
        os.push(SIDECAR_SUFFIX);
        let dir = PathBuf::from(os);
        Self {
            parts_dir: dir.join("parts"),
            meta_path: dir.join("meta.json"),
            dir,
        }
    }

    pub fn create(&self) -> Result<(), String> {
        fs::create_dir_all(&self.parts_dir).map_err(|e| e.to_string())
    }

    pub fn write_meta(&self, meta: &SidecarMeta) -> Result<(), String> {
        let tmp = self.meta_path.with_extension("json.tmp");
        let json = serde_json::to_vec_pretty(meta).map_err(|e| e.to_string())?;
        fs::write(&tmp, json).map_err(|e| e.to_string())?;
        fs::rename(&tmp, &self.meta_path).map_err(|e| e.to_string())
    }

    #[allow(dead_code)]
    pub fn read_meta(&self) -> Option<SidecarMeta> {
        let data = fs::read(&self.meta_path).ok()?;
        serde_json::from_slice(&data).ok()
    }

    pub fn part_file(&self, index: u32) -> PathBuf {
        self.parts_dir.join(format!("part_{index:06}"))
    }

    pub fn part_size(&self, index: u32) -> u64 {
        fs::metadata(self.part_file(index)).map(|m| m.len()).unwrap_or(0)
    }

    /// 汇总所有分片已下载字节
    pub fn sum_parts(&self) -> u64 {
        let mut total = 0;
        if let Ok(rd) = fs::read_dir(&self.parts_dir) {
            for e in rd.flatten() {
                if let Ok(m) = e.metadata() {
                    if m.is_file() {
                        total += m.len();
                    }
                }
            }
        }
        total
    }

    pub fn remove(&self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

/// tasks.json：应用重启后恢复任务列表
pub fn load_task_list(path: &Path) -> Vec<crate::downloader::types::PersistedTask> {
    match fs::read(path) {
        Ok(data) => serde_json::from_slice(&data).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

pub fn save_task_list(path: &Path, tasks: &[crate::downloader::types::PersistedTask]) {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let tmp = path.with_extension("json.tmp");
    if let Ok(json) = serde_json::to_vec_pretty(tasks) {
        if fs::write(&tmp, json).is_ok() {
            let _ = fs::rename(&tmp, path);
        }
    }
}
