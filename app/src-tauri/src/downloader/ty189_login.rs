//! 天翼云盘（cloud.189.cn）登录态代取：应用内可见 WebView 打开 cloud.189.cn，
//! 用户完成登录（手机号/短信）后收割 189 家族会话 cookie。
//!
//! 与 baidu/quark/pan123_login 同构（全量 cookie store + 家族域过滤）。
//! 登录态用途（r5 轮28/29 端点实锤）：列表页访问码提交后页内可达，但
//! **下载=登录墙**（「您未登录，请登录后进行操作」页面级拦截）——登录会话
//! 供解析层页内触发下载按钮直链捕获或转存链（getFileDownloadUrl.action
//! 家族，2022 otokaze 端点形态仍现行）。
//!
//! 会话落盘 `<config_dir>/ty189_session.json`（unix 0600）。
use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::Manager;
use tauri::WebviewUrl;
use tauri::WebviewWindowBuilder;

/// 收割到的 189 会话（cookie 头形态 "COOKIE_SESSION=...; ..."）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ty189Session {
    pub cookies: String,
    /// 收割时间（ms）
    pub saved_at: i64,
}

const LOGIN_URL: &str = "https://cloud.189.cn/";
const LOGIN_WINDOW_LABEL: &str = "feiliu-ty189-login";
/// 登录等待上限：手机号+短信/密码登录通常 <2 分钟，留足余量
const LOGIN_TIMEOUT: Duration = Duration::from_secs(420);

fn session_path(app: &tauri::AppHandle) -> PathBuf {
    let data_dir = app
        .path()
        .app_data_dir()
        .unwrap_or_else(|_| PathBuf::from("."));
    let cfg_dir = app
        .path()
        .app_config_dir()
        .unwrap_or_else(|_| data_dir.clone());
    cfg_dir.join("ty189_session.json")
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
/// 测试/诊断可用 FEILIU_TY189_SESSION_FILE 覆盖路径
pub fn load_session() -> Option<Ty189Session> {
    let p = std::env::var("FEILIU_TY189_SESSION_FILE")
        .ok()
        .map(PathBuf::from)
        .or_else(|| path().cloned())?;
    let raw = std::fs::read_to_string(p).ok()?;
    serde_json::from_str(&raw).ok()
}

/// 收割到的 189 会话落盘（login 命令成功后调用）
pub fn save_session(session: &Ty189Session) -> Result<(), String> {
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
pub struct Ty189LoginStatus {
    pub logged_in: bool,
    pub saved_at: Option<i64>,
}

pub fn login_status() -> Ty189LoginStatus {
    match load_session() {
        Some(s) => Ty189LoginStatus {
            logged_in: true,
            saved_at: Some(s.saved_at),
        },
        None => Ty189LoginStatus {
            logged_in: false,
            saved_at: None,
        },
    }
}

/// 判断 cookie domain 是否属于 189 家族（".189.cn"/"cloud.189.cn" 等）
fn is_ty189_domain(domain: &str) -> bool {
    let d = domain.trim_start_matches('.');
    d == "189.cn" || d.ends_with(".189.cn")
}

/// 登录成功标志 cookie：189 登录后会话凭据在 COOKIE_SESSION（portal 主会话）
const LOGIN_COOKIE_MARK: &str = "COOKIE_SESSION";

/// 打开可见登录窗口，轮询原生 cookie 等待 COOKIE_SESSION 出现（登录成功标志）。
/// 用户关窗=取消；7 分钟超时（短信登录链路较长）。成功后窗口自动关闭并返回会话。
pub async fn login_collect(app: tauri::AppHandle) -> Result<Ty189Session, String> {
    // 上次未关的同名窗口先销毁（幂等重开）
    if let Some(w) = app.get_webview_window(LOGIN_WINDOW_LABEL) {
        let _ = w.destroy();
    }
    let url = LOGIN_URL
        .parse()
        .map_err(|e| format!("URL 解析失败: {e}"))?;
    let window = WebviewWindowBuilder::new(&app, LOGIN_WINDOW_LABEL, WebviewUrl::External(url))
        .title("登录天翼云盘 · 登录成功后本窗口自动关闭")
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
            return Err("登录超时（7 分钟），请重试".into());
        }
        polls += 1;
        // 导航中/尚未登录时可能报错——静默继续下一轮
        let cookies = match window.cookies() {
            Ok(c) => c,
            Err(e) => {
                eprintln!("[feiliu-ty189-login] 轮询 {polls}: cookie store 读取失败 {e}");
                continue;
            }
        };
        let ty_cookies: Vec<_> = cookies
            .iter()
            .filter(|c| c.domain().map(is_ty189_domain).unwrap_or(false))
            .collect();
        if polls % 5 == 0 {
            eprintln!(
                "[feiliu-ty189-login] 轮询 {polls}: 全量 {} 条，189 家族 {} 条",
                cookies.len(),
                ty_cookies.len()
            );
        }
        let Some(_) = ty_cookies.iter().find(|c| c.name() == LOGIN_COOKIE_MARK) else {
            continue;
        };
        // 登录成功：189 家族 cookie 拼成请求头形态
        let header = ty_cookies
            .iter()
            .map(|c| format!("{}={}", c.name(), c.value()))
            .collect::<Vec<_>>()
            .join("; ");
        eprintln!(
            "[feiliu-ty189-login] 登录成功（轮询 {polls} 次），收割 {} 条 cookie",
            ty_cookies.len()
        );
        let _ = window.destroy();
        return Ok(Ty189Session {
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
        assert!(is_ty189_domain(".189.cn"));
        assert!(is_ty189_domain("189.cn"));
        assert!(is_ty189_domain("cloud.189.cn"));
        assert!(!is_ty189_domain("189.com"));
        assert!(!is_ty189_domain("pan.baidu.com"));
        assert!(!is_ty189_domain("e.189store.com"));
    }
}
