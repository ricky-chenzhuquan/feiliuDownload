//! 123 云盘（123pan，含轮换域 123684/123865）登录态代取：应用内可见 WebView
//! 打开 www.123pan.com，用户完成登录后收割 123pan 家族会话。
//!
//! 凭据形态（qaiu 123.py 实锤，r5 轮12）：登录态 = Bearer JWT——
//! `localStorage.authorToken` / 请求头 `authorization` / **Cookie `sso-token`**
//! 三形态同源。收割锚 = cookie store 出现 `sso-token`（原生 cookies() 全量
//! store + 家族域过滤，wry 严格相等过滤坑同 baidu/quark 注释）；使用层
//! 直接把 cookie 值塞 `Authorization:` 头（**不带 Bearer 前缀**）或手动
//! Cookie 头（解析层双试，r5 轮10：匿名链 100% 通至 5112 流量包墙，登录态
//! 是唯一缺口）。
//!
//! 会话落盘 `<config_dir>/pan123_session.json`（unix 0600）。
use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::Manager;
use tauri::WebviewUrl;
use tauri::WebviewWindowBuilder;

/// 收割到的 123pan 会话（cookie 头形态 "sso-token=...; ..."）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PanSession {
    pub cookies: String,
    /// 收割时间（ms）
    pub saved_at: i64,
}

const LOGIN_URL: &str = "https://www.123pan.com/";
const LOGIN_WINDOW_LABEL: &str = "feiliu-pan123-login";
/// 登录等待上限：扫码/账密通常 <1 分钟，留足余量
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
    cfg_dir.join("pan123_session.json")
}

/// 会话文件路径：应用启动时由 lib.rs setup 调 init 注入
static SESSION_PATH: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

pub fn init(app: &tauri::AppHandle) {
    let _ = SESSION_PATH.set(session_path(app));
}

fn path() -> Option<&'static PathBuf> {
    SESSION_PATH.get()
}

/// 读取已存会话（无初始化/无文件/损坏 → None）
/// 测试/诊断可用 FEILIU_PAN123_SESSION_FILE 覆盖路径
pub fn load_session() -> Option<PanSession> {
    let p = std::env::var("FEILIU_PAN123_SESSION_FILE")
        .ok()
        .map(PathBuf::from)
        .or_else(|| path().cloned())?;
    let raw = std::fs::read_to_string(p).ok()?;
    serde_json::from_str(&raw).ok()
}

/// 收割到的 123pan 会话落盘（login 命令成功后调用）
pub fn save_session(session: &PanSession) -> Result<(), String> {
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
pub struct PanLoginStatus {
    pub logged_in: bool,
    pub saved_at: Option<i64>,
}

pub fn login_status() -> PanLoginStatus {
    match load_session() {
        Some(s) => PanLoginStatus {
            logged_in: true,
            saved_at: Some(s.saved_at),
        },
        None => PanLoginStatus {
            logged_in: false,
            saved_at: None,
        },
    }
}

/// 判断 cookie domain 是否属于 123pan 家族（".123pan.com"/"www.123pan.com" 等；
/// 轮换域 123684/123865 是独立站点域，cookie 不跨——登录窗口固定主域收割，
/// 解析层手动带 Cookie 头跨轮换域调用）
fn is_pan123_domain(domain: &str) -> bool {
    let d = domain.trim_start_matches('.');
    d == "123pan.com" || d.ends_with(".123pan.com")
}

/// 打开可见登录窗口，轮询原生 cookie 等待 `sso-token` 出现（登录成功标志）。
/// 用户关窗=取消；5 分钟超时。成功后窗口自动关闭并返回会话。
pub async fn login_collect(app: tauri::AppHandle) -> Result<PanSession, String> {
    // 上次未关的同名窗口先销毁（幂等重开）
    if let Some(w) = app.get_webview_window(LOGIN_WINDOW_LABEL) {
        let _ = w.destroy();
    }
    let url = LOGIN_URL
        .parse()
        .map_err(|e| format!("URL 解析失败: {e}"))?;
    let window = WebviewWindowBuilder::new(&app, LOGIN_WINDOW_LABEL, WebviewUrl::External(url))
        .title("登录 123 云盘 · 登录成功后本窗口自动关闭")
        .inner_size(420.0, 560.0)
        .build()
        .map_err(|e| format!("创建登录窗口失败: {e}"))?;
    let _ = window.show();
    let _ = window.set_focus();

    let deadline = std::time::Instant::now() + LOGIN_TIMEOUT;
    let mut polls = 0u32;
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
        polls += 1;
        // 导航中/尚未登录时可能报错——静默继续下一轮
        let cookies = match window.cookies() {
            Ok(c) => c,
            Err(e) => {
                eprintln!("[feiliu-pan123-login] 轮询 {polls}: cookie store 读取失败 {e}");
                continue;
            }
        };
        let pan_cookies: Vec<_> = cookies
            .iter()
            .filter(|c| c.domain().map(is_pan123_domain).unwrap_or(false))
            .collect();
        if polls % 5 == 0 {
            eprintln!(
                "[feiliu-pan123-login] 轮询 {polls}: 全量 {} 条，123pan 家族 {} 条",
                cookies.len(),
                pan_cookies.len()
            );
        }
        let Some(sso) = pan_cookies.iter().find(|c| c.name() == "sso-token") else {
            continue;
        };
        // 登录成功：123pan 家族 cookie 拼成请求头形态
        let header = pan_cookies
            .iter()
            .map(|c| format!("{}={}", c.name(), c.value()))
            .collect::<Vec<_>>()
            .join("; ");
        eprintln!(
            "[feiliu-pan123-login] 登录成功（轮询 {polls} 次），收割 {} 条 cookie（sso-token 长度 {}）",
            pan_cookies.len(),
            sso.value().len()
        );
        let _ = window.destroy();
        return Ok(PanSession {
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
        assert!(is_pan123_domain(".123pan.com"));
        assert!(is_pan123_domain("123pan.com"));
        assert!(is_pan123_domain("www.123pan.com"));
        // 轮换域是独立站点域（cookie 不跨），不算主域家族
        assert!(!is_pan123_domain("123684.com"));
        assert!(!is_pan123_domain("www.123865.com"));
        assert!(!is_pan123_domain("pan.baidu.com"));
    }
}
