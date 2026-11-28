//! 123 云盘（123pan，含轮换域 123684/123865）登录态代取：应用内可见 WebView
//! 打开 www.123pan.com，用户完成登录后收割 123pan 家族会话。
//!
//! r6 轮6 收敛：同构实现提炼进 `cloud_login.rs`，本模块只保留站点规格
//! 与原公开 API。凭据形态（qaiu 123.py 实锤，r5 轮12）：登录态 = Bearer
//! JWT——`localStorage.authorToken` / 请求头 `authorization` / **Cookie
//! `sso-token`** 三形态同源；收割锚 = cookie store 出现 `sso-token`；使用层
//! 直接把 cookie 值塞 `Authorization:` 头（**不带 Bearer 前缀**）或手动
//! Cookie 头（解析层双试）。
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::cloud_login::{self, SiteLoginSpec};

/// 收割到的 123pan 会话（cookie 头形态 "sso-token=...; ..."）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PanSession {
    pub cookies: String,
    /// 收割时间（ms）
    pub saved_at: i64,
}

/// 123pan 站登录规格
pub const SPEC: SiteLoginSpec = SiteLoginSpec {
    title: "123 云盘",
    log_tag: "pan123",
    family_label: "123pan",
    login_url: "https://www.123pan.com/",
    window_label: "feiliu-pan123-login",
    timeout_secs: 300,
    login_cookie_mark: "sso-token",
    family_domain: "123pan.com",
    session_file: "pan123_session.json",
    env_session_file: "FEILIU_PAN123_SESSION_FILE",
};

/// 会话文件路径：应用启动时由 lib.rs setup 调 init 注入
static SESSION_PATH: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

pub fn init(app: &tauri::AppHandle) {
    let _ = SESSION_PATH.set(cloud_login::session_path(app, &SPEC));
}

/// 读取已存会话（无初始化/无文件/损坏 → None）
/// 测试/诊断可用 FEILIU_PAN123_SESSION_FILE 覆盖路径
pub fn load_session() -> Option<PanSession> {
    let p = std::env::var(SPEC.env_session_file)
        .ok()
        .map(PathBuf::from)
        .or_else(|| SESSION_PATH.get().cloned())?;
    cloud_login::load_json_at(&p)
}

/// 收割到的 123pan 会话落盘（login 命令成功后调用）
pub fn save_session(session: &PanSession) -> Result<(), String> {
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
pub async fn login_collect(app: tauri::AppHandle) -> Result<PanSession, String> {
    cloud_login::collect(app, &SPEC).await.map(|(cookies, saved_at)| PanSession {
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
        let s = PanSession {
            cookies: "sso-token=abc; uid=1".into(),
            saved_at: 1,
        };
        let raw = serde_json::to_string(&s).unwrap();
        let back: PanSession = serde_json::from_str(&raw).unwrap();
        assert_eq!(back.cookies, s.cookies);
    }

    /// 123pan 家族域判别：主域/子域命中，轮换域不误判
    #[test]
    fn test_is_pan123_domain() {
        let f = |d| cloud_login::is_family_domain(d, SPEC.family_domain);
        assert!(f(".123pan.com"));
        assert!(f("123pan.com"));
        assert!(f("www.123pan.com"));
        // 轮换域是独立站点域（cookie 不跨），不算主域家族
        assert!(!f("123684.com"));
        assert!(!f("www.123865.com"));
        assert!(!f("pan.baidu.com"));
    }
}
