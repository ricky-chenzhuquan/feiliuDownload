//! 夸克网盘登录态代取：应用内可见 WebView 打开 pan.quark.cn，用户完成
//! 登录（扫码/账密）后收割 quark.cn 家族会话 cookie（核心凭据 __pus）。
//!
//! r6 轮6 收敛：同构实现提炼进 `cloud_login.rs`，本模块只保留站点规格
//! 与原公开 API。__puus 直链凭据服务器侧 1h 过期，由解析层按需经
//! auth/pc/flush 刷新（r5 轮2 设计定稿），会话文件只保 __pus 等长时会话票。
//!
//! 另发现（r5 轮4 实测）：uop.quark.cn 二维码 CAS 链匿名 token 端点 200
//! 可达，但轮询端对新签 token 立即返回 50004002 Token Not Found，绑定
//! 机制未破——纯 API 二维码路线暂缓，WebView 收割为现行路线。
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::cloud_login::{self, SiteLoginSpec};

/// 收割到的夸克会话（cookie 头形态 "__pus=...; __puus=...; ..."）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuarkSession {
    pub cookies: String,
    /// 收割时间（ms）
    pub saved_at: i64,
}

/// 夸克站登录规格
pub const SPEC: SiteLoginSpec = SiteLoginSpec {
    title: "夸克网盘",
    log_tag: "quark",
    family_label: "quark",
    login_url: "https://pan.quark.cn/",
    window_label: "feiliu-quark-login",
    timeout_secs: 300,
    login_cookie_mark: "__pus",
    family_domain: "quark.cn",
    session_file: "quark_session.json",
    env_session_file: "FEILIU_QUARK_SESSION_FILE",
};

/// 会话文件路径：应用启动时由 lib.rs setup 调 init 注入
static SESSION_PATH: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

pub fn init(app: &tauri::AppHandle) {
    let _ = SESSION_PATH.set(cloud_login::session_path(app, &SPEC));
}

/// 读取已存会话（无初始化/无文件/损坏 → None）
/// 测试/诊断可用 FEILIU_QUARK_SESSION_FILE 覆盖路径
pub fn load_session() -> Option<QuarkSession> {
    let p = std::env::var(SPEC.env_session_file)
        .ok()
        .map(PathBuf::from)
        .or_else(|| SESSION_PATH.get().cloned())?;
    cloud_login::load_json_at(&p)
}

/// 收割到的夸克会话落盘（login 命令成功后调用）
pub fn save_session(session: &QuarkSession) -> Result<(), String> {
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
pub async fn login_collect(app: tauri::AppHandle) -> Result<QuarkSession, String> {
    cloud_login::collect(app, &SPEC).await.map(|(cookies, saved_at)| QuarkSession {
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
        let s = QuarkSession {
            cookies: "__pus=abc; __puus=def".into(),
            saved_at: 1,
        };
        let raw = serde_json::to_string(&s).unwrap();
        let back: QuarkSession = serde_json::from_str(&raw).unwrap();
        assert_eq!(back.cookies, s.cookies);
    }

    /// quark 家族域判别：前导点域/子域命中，相近域不误判
    #[test]
    fn test_is_quark_domain() {
        let f = |d| cloud_login::is_family_domain(d, SPEC.family_domain);
        assert!(f(".quark.cn"));
        assert!(f("quark.cn"));
        assert!(f("pan.quark.cn"));
        assert!(f("drive-pc.quark.cn"));
        assert!(!f("quark.com"));
        assert!(!f("pan.baidu.com"));
        assert!(!f("uc.cn"));
    }
}
