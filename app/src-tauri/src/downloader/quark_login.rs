//! 夸克网盘登录态代取：应用内可见 WebView 打开 pan.quark.cn，用户完成
//! 登录（扫码/账密）后收割 quark.cn 家族会话 cookie（核心凭据 __pus）。
//!
//! 与 baidu_login 同构（全量 cookie store + 家族域过滤——wry 按
//! `cookie.domain == url.domain` 严格相等过滤，`.quark.cn` 前导点域
//! 与 `pan.quark.cn` 永不相等的坑，见 baidu_login 注释）。
//!
//! 会话落盘 `<config_dir>/quark_session.json`（unix 0600）。注意 __puus
//! 直链凭据服务器侧 1h 过期，由解析层按需经 auth/pc/flush 刷新（见
//! r5 轮2 设计定稿），会话文件只保 __pus 等长时会话票。
//!
//! 另发现（r5 轮4 实测）：uop.quark.cn 二维码 CAS 链（getTokenForQrcodeLogin
//! →getServiceTicketByQrcodeToken）匿名 token 端点 200 可达，但轮询端
//! 对新签 token 立即返回 50004002 Token Not Found（token 形态 sta…≠
//! pan-qk.http 示例 UUID），绑定机制未破——纯 API 二维码路线暂缓，
//! WebView 收割为现行路线。
use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::Manager;
use tauri::WebviewUrl;
use tauri::WebviewWindowBuilder;

/// 收割到的夸克会话（cookie 头形态 "__pus=...; __puus=...; ..."）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuarkSession {
    pub cookies: String,
    /// 收割时间（ms）
    pub saved_at: i64,
}

const LOGIN_URL: &str = "https://pan.quark.cn/";
const LOGIN_WINDOW_LABEL: &str = "feiliu-quark-login";
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
    cfg_dir.join("quark_session.json")
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
/// 测试/诊断可用 FEILIU_QUARK_SESSION_FILE 覆盖路径
pub fn load_session() -> Option<QuarkSession> {
    let p = std::env::var("FEILIU_QUARK_SESSION_FILE")
        .ok()
        .map(PathBuf::from)
        .or_else(|| path().cloned())?;
    let raw = std::fs::read_to_string(p).ok()?;
    serde_json::from_str(&raw).ok()
}

/// 收割到的夸克会话落盘（login 命令成功后调用）
pub fn save_session(session: &QuarkSession) -> Result<(), String> {
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
pub struct QuarkLoginStatus {
    pub logged_in: bool,
    pub saved_at: Option<i64>,
}

pub fn login_status() -> QuarkLoginStatus {
    match load_session() {
        Some(s) => QuarkLoginStatus {
            logged_in: true,
            saved_at: Some(s.saved_at),
        },
        None => QuarkLoginStatus {
            logged_in: false,
            saved_at: None,
        },
    }
}

/// 判断 cookie domain 是否属于 quark.cn 家族（".quark.cn"/"pan.quark.cn" 等）
fn is_quark_domain(domain: &str) -> bool {
    let d = domain.trim_start_matches('.');
    d == "quark.cn" || d.ends_with(".quark.cn")
}

/// 打开可见登录窗口，轮询原生 cookie 等待 __pus 出现（登录成功标志）。
/// 用户关窗=取消；5 分钟超时。成功后窗口自动关闭并返回会话。
pub async fn login_collect(app: tauri::AppHandle) -> Result<QuarkSession, String> {
    // 上次未关的同名窗口先销毁（幂等重开）
    if let Some(w) = app.get_webview_window(LOGIN_WINDOW_LABEL) {
        let _ = w.destroy();
    }
    let url = LOGIN_URL
        .parse()
        .map_err(|e| format!("URL 解析失败: {e}"))?;
    let window = WebviewWindowBuilder::new(&app, LOGIN_WINDOW_LABEL, WebviewUrl::External(url))
        .title("登录夸克网盘 · 登录成功后本窗口自动关闭")
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
                eprintln!("[feiliu-quark-login] 轮询 {polls}: cookie store 读取失败 {e}");
                continue;
            }
        };
        let quark_cookies: Vec<_> = cookies
            .iter()
            .filter(|c| c.domain().map(is_quark_domain).unwrap_or(false))
            .collect();
        if polls % 5 == 0 {
            eprintln!(
                "[feiliu-quark-login] 轮询 {polls}: 全量 {} 条，quark 家族 {} 条",
                cookies.len(),
                quark_cookies.len()
            );
        }
        let Some(_) = quark_cookies.iter().find(|c| c.name() == "__pus") else {
            continue;
        };
        // 登录成功：quark 家族 cookie 拼成请求头形态
        let header = quark_cookies
            .iter()
            .map(|c| format!("{}={}", c.name(), c.value()))
            .collect::<Vec<_>>()
            .join("; ");
        eprintln!(
            "[feiliu-quark-login] 登录成功（轮询 {polls} 次），收割 {} 条 cookie",
            quark_cookies.len()
        );
        let _ = window.destroy();
        return Ok(QuarkSession {
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
        assert!(is_quark_domain(".quark.cn"));
        assert!(is_quark_domain("quark.cn"));
        assert!(is_quark_domain("pan.quark.cn"));
        assert!(is_quark_domain("drive-pc.quark.cn"));
        assert!(!is_quark_domain("quark.com"));
        assert!(!is_quark_domain("pan.baidu.com"));
        assert!(!is_quark_domain("uc.cn"));
    }
}
