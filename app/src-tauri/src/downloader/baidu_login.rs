//! 百度网盘登录态代取：应用内可见 WebView 打开 pan.baidu.com，用户完成
//! 登录（扫码/账密）后用 tauri 原生 cookie API 收割会话。
//!
//! 关键点：百度核心凭据 BDUSS 是 HttpOnly cookie，页面 JS（document.cookie）
//! 读不到——必须走 `WebviewWindow::cookies_for_url` 原生 cookie 存储。
//!
//! 会话落盘 `<config_dir>/baidu_session.json`（unix 0600）：cookie 为本机
//! 敏感凭据，不进任何仓库/日志；退出登录即删除文件。
use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::Manager;
use tauri::WebviewUrl;
use tauri::WebviewWindowBuilder;

/// 收割到的百度会话（cookie 头形态 "BDUSS=...; STOKEN=...; ..."）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaiduSession {
    pub cookies: String,
    /// 收割时间（ms）
    pub saved_at: i64,
}

const LOGIN_URL: &str = "https://pan.baidu.com/";
const LOGIN_WINDOW_LABEL: &str = "feiliu-baidu-login";
/// 登录等待上限：扫码通常 <1 分钟，留足余量
const LOGIN_TIMEOUT: Duration = Duration::from_secs(300);

fn session_path(app: &tauri::AppHandle) -> PathBuf {
    let data_dir = app
        .path()
        .app_data_dir()
        .unwrap_or_else(|_| PathBuf::from("."));
    let cfg_dir = app
        .path()
        .app_config_dir()
        .unwrap_or_else(|_| data_dir.clone());
    cfg_dir.join("baidu_session.json")
}

/// 会话文件路径：应用启动时由 lib.rs setup 调 init 注入
/// （解析器分发点无 AppHandle，走静态路径）
static SESSION_PATH: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

pub fn init(app: &tauri::AppHandle) {
    let _ = SESSION_PATH.set(session_path(app));
}

fn path() -> Option<&'static PathBuf> {
    SESSION_PATH.get()
}

/// 读取已存会话（无初始化/无文件/损坏 → None）
pub fn load_session() -> Option<BaiduSession> {
    let raw = std::fs::read_to_string(path()?).ok()?;
    serde_json::from_str(&raw).ok()
}

/// 收割到的百度会话落盘（login 命令成功后调用）
pub fn save_session(session: &BaiduSession) -> Result<(), String> {
    let path = path().ok_or("会话路径未初始化")?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("创建配置目录失败: {e}"))?;
    }
    let body = serde_json::to_string(session).map_err(|e| format!("序列化会话失败: {e}"))?;
    // 0600：本机凭据文件最小权限
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)
            .map_err(|e| format!("写入会话文件失败: {e}"))?;
        f.write_all(body.as_bytes())
            .map_err(|e| format!("写入会话文件失败: {e}"))
    }
    #[cfg(not(unix))]
    {
        std::fs::write(path, body).map_err(|e| format!("写入会话文件失败: {e}"))
    }
}

/// 退出登录：删除会话文件（不存在视为已退出，幂等）
pub fn clear_session() {
    if let Some(p) = path() {
        let _ = std::fs::remove_file(p);
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

/// 打开可见登录窗口，轮询原生 cookie 等待 BDUSS 出现（登录成功标志）。
/// 用户关窗=取消；5 分钟超时。成功后窗口自动关闭并返回会话。
pub async fn login_collect(app: tauri::AppHandle) -> Result<BaiduSession, String> {
    // 上次未关的同名窗口先销毁（幂等重开）
    if let Some(w) = app.get_webview_window(LOGIN_WINDOW_LABEL) {
        let _ = w.destroy();
    }
    let url = LOGIN_URL
        .parse()
        .map_err(|e| format!("URL 解析失败: {e}"))?;
    let window = WebviewWindowBuilder::new(&app, LOGIN_WINDOW_LABEL, WebviewUrl::External(url))
        .title("登录百度网盘 · 登录成功后本窗口自动关闭")
        .inner_size(420.0, 560.0)
        .build()
        .map_err(|e| format!("创建登录窗口失败: {e}"))?;
    let _ = window.show();
    let _ = window.set_focus();

    let deadline = std::time::Instant::now() + LOGIN_TIMEOUT;
    let probe_url: tauri::Url = LOGIN_URL.parse().map_err(|e| format!("URL 解析失败: {e}"))?;
    loop {
        tokio::time::sleep(Duration::from_secs(1)).await;
        // 窗口已被用户关闭（get 不到）= 取消
        if app.get_webview_window(LOGIN_WINDOW_LABEL).is_none() {
            return Err("已取消登录".into());
        }
        if std::time::Instant::now() >= deadline {
            let _ = window.destroy();
            return Err("登录超时（5 分钟），请重试".into());
        }
        // 导航中/尚未登录时可能报错——静默继续下一轮
        let cookies = match window.cookies_for_url(probe_url.clone()) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let Some(_) = cookies.iter().find(|c| c.name() == "BDUSS") else {
            continue;
        };
        // 登录成功：全部适用 cookie 拼成请求头形态
        let header = cookies
            .iter()
            .map(|c| format!("{}={}", c.name(), c.value()))
            .collect::<Vec<_>>()
            .join("; ");
        let _ = window.destroy();
        return Ok(BaiduSession {
            cookies: header,
            saved_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0),
        });
    }
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
}
