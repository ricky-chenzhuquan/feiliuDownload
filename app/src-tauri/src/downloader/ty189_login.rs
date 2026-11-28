//! 天翼云盘（cloud.189.cn）登录态代取：应用内可见 WebView 打开 cloud.189.cn，
//! 用户完成登录（手机号/短信）后收割 189 家族会话 cookie。
//!
//! r6 轮6 收敛：同构实现提炼进 `cloud_login.rs`，本模块只保留站点规格
//! 与原公开 API。登录态用途（r5 轮28/29 端点实锤）：列表页访问码提交后
//! 页内可达，但**下载=登录墙**（「您未登录，请登录后进行操作」页面级拦截）
//! ——登录会话供解析层页内触发下载按钮直链捕获或转存链
//! （getFileDownloadUrl.action 家族，2022 otokaze 端点形态仍现行）。
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::cloud_login::{self, SiteLoginSpec};

/// 收割到的 189 会话（cookie 头形态 "COOKIE_SESSION=...; ..."）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ty189Session {
    pub cookies: String,
    /// 收割时间（ms）
    pub saved_at: i64,
}

/// 天翼 189 站登录规格（短信登录链路较长，超时 7 分钟）
pub const SPEC: SiteLoginSpec = SiteLoginSpec {
    title: "天翼云盘",
    log_tag: "ty189",
    family_label: "189",
    login_url: "https://cloud.189.cn/",
    window_label: "feiliu-ty189-login",
    timeout_secs: 420,
    login_cookie_mark: "COOKIE_SESSION",
    family_domain: "189.cn",
    session_file: "ty189_session.json",
    env_session_file: "FEILIU_TY189_SESSION_FILE",
};

/// 会话文件路径：应用启动时由 lib.rs setup 调 init 注入
static SESSION_PATH: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

pub fn init(app: &tauri::AppHandle) {
    let _ = SESSION_PATH.set(cloud_login::session_path(app, &SPEC));
}

/// 读取已存会话（无初始化/无文件/损坏 → None）
/// 测试/诊断可用 FEILIU_TY189_SESSION_FILE 覆盖路径
pub fn load_session() -> Option<Ty189Session> {
    let p = std::env::var(SPEC.env_session_file)
        .ok()
        .map(PathBuf::from)
        .or_else(|| SESSION_PATH.get().cloned())?;
    cloud_login::load_json_at(&p)
}

/// 收割到的 189 会话落盘（login 命令成功后调用）
pub fn save_session(session: &Ty189Session) -> Result<(), String> {
    let path = SESSION_PATH.get().ok_or("会话路径未初始化")?;
    cloud_login::save_json_at(path, session)
}

/// 退出登录：删除会话文件（不存在视为已退出，幂等）
pub fn clear_session() {
    if let Some(p) = SESSION_PATH.get() {
        cloud_login::clear_at(p);
    }
}

/// 登录状态摘要（前端设置页展示；四站统一 cloud_login::CloudLoginStatus）
pub fn login_status() -> cloud_login::CloudLoginStatus {
    cloud_login::status_of(load_session().map(|s| s.saved_at))
}

/// 打开可见登录窗口并收割会话（共享流程见 `cloud_login::collect`）
pub async fn login_collect(app: tauri::AppHandle) -> Result<Ty189Session, String> {
    cloud_login::collect(app, &SPEC).await.map(|(cookies, saved_at)| Ty189Session {
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
        let s = Ty189Session {
            cookies: "COOKIE_SESSION=abc; SESSION=def".into(),
            saved_at: 1,
        };
        let raw = serde_json::to_string(&s).unwrap();
        let back: Ty189Session = serde_json::from_str(&raw).unwrap();
        assert_eq!(back.cookies, s.cookies);
    }

    /// 189 家族域判别
    #[test]
    fn test_is_ty189_domain() {
        let f = |d| cloud_login::is_family_domain(d, SPEC.family_domain);
        assert!(f(".189.cn"));
        assert!(f("189.cn"));
        assert!(f("cloud.189.cn"));
        assert!(!f("189.com"));
        assert!(!f("pan.baidu.com"));
        assert!(!f("e.189store.com"));
    }
}
