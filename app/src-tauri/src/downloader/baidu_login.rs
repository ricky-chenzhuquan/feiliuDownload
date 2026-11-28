//! 百度网盘登录态代取：应用内可见 WebView 打开 pan.baidu.com，用户完成
//! 登录（扫码/账密）后用 tauri 原生 cookie API 收割会话。
//!
//! r6 轮6 收敛：同构实现提炼进 `cloud_login.rs`（落盘 0600 + 收割流程），
//! 本模块只保留站点规格与原公开 API。核心凭据 BDUSS 是 HttpOnly cookie，
//! 必须走原生 cookie store（见 `cloud_login::collect` 模块注释）。
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::cloud_login::{self, SiteLoginSpec};

/// 收割到的百度会话（cookie 头形态 "BDUSS=...; STOKEN=...; ..."）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaiduSession {
    pub cookies: String,
    /// 收割时间（ms）
    pub saved_at: i64,
}

/// 百度站登录规格
pub const SPEC: SiteLoginSpec = SiteLoginSpec {
    title: "百度网盘",
    log_tag: "baidu",
    family_label: "baidu",
    login_url: "https://pan.baidu.com/",
    window_label: "feiliu-baidu-login",
    timeout_secs: 300,
    login_cookie_mark: "BDUSS",
    family_domain: "baidu.com",
    session_file: "baidu_session.json",
    env_session_file: "FEILIU_BAIDU_SESSION_FILE",
};

/// 会话文件路径：应用启动时由 lib.rs setup 调 init 注入
/// （解析器分发点无 AppHandle，走静态路径）
static SESSION_PATH: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

pub fn init(app: &tauri::AppHandle) {
    let _ = SESSION_PATH.set(cloud_login::session_path(app, &SPEC));
}

/// 读取已存会话（无初始化/无文件/损坏 → None）
/// 测试/诊断可用 FEILIU_BAIDU_SESSION_FILE 覆盖路径
pub fn load_session() -> Option<BaiduSession> {
    let p = std::env::var(SPEC.env_session_file)
        .ok()
        .map(PathBuf::from)
        .or_else(|| SESSION_PATH.get().cloned())?;
    cloud_login::load_json_at(&p)
}

/// 收割到的百度会话落盘（login 命令成功后调用）
pub fn save_session(session: &BaiduSession) -> Result<(), String> {
    let path = SESSION_PATH.get().ok_or("会话路径未初始化")?;
    cloud_login::save_json_at(path, session)
}

/// 退出登录：删除会话文件（不存在视为已退出，幂等）
pub fn clear_session() {
    if let Some(p) = SESSION_PATH.get() {
        cloud_login::clear_at(p);
    }
}

/// 登录状态摘要（前端设置页展示）
#[derive(Debug, Serialize)]
pub struct BaiduLoginStatus {
    pub logged_in: bool,
    pub saved_at: Option<i64>,
}

pub fn login_status() -> BaiduLoginStatus {
    match load_session() {
        Some(s) => BaiduLoginStatus {
            logged_in: true,
            saved_at: Some(s.saved_at),
        },
        None => BaiduLoginStatus {
            logged_in: false,
            saved_at: None,
        },
    }
}

/// 打开可见登录窗口并收割会话（共享流程见 `cloud_login::collect`）
pub async fn login_collect(app: tauri::AppHandle) -> Result<BaiduSession, String> {
    cloud_login::collect(app, &SPEC).await.map(|(cookies, saved_at)| BaiduSession {
        cookies,
        saved_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 会话序列化往返（不落盘——路径依赖应用环境）
    #[test]
    fn test_session_roundtrip() {
        let s = BaiduSession {
            cookies: "BDUSS=abc; STOKEN=def".into(),
            saved_at: 1,
        };
        let raw = serde_json::to_string(&s).unwrap();
        let back: BaiduSession = serde_json::from_str(&raw).unwrap();
        assert_eq!(back.cookies, s.cookies);
    }

    /// 家族域判别（规格绑定百度）
    #[test]
    fn test_is_baidu_domain() {
        let f = |d| cloud_login::is_family_domain(d, SPEC.family_domain);
        assert!(f(".baidu.com"));
        assert!(f("pan.baidu.com"));
        assert!(!f("example.com"));
    }
}
