//! 云盘登录会话共享核心（r6 轮6 收敛）——百度/夸克/123云盘/天翼四站登录收割
//! 的同构段提炼：会话落盘（0600）+ 可见 WebView 收割流程按 `SiteLoginSpec`
//! 参数化。各站模块（*_login.rs）保留原公开 API（类型+函数名），内部委托本模块。
//!
//! 登录收割的关键约定（2026-11-22 真机实锤）：不用 `cookies_for_url`——wry 按
//! `cookie.domain() == url.domain()` 严格相等过滤，核心凭据挂在前导点域
//! （如 `.baidu.com`）与登录页域永不相等，轮询恒空。必须走 `cookies()`
//! 全量 cookie store + 自行按家族域过滤。
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Serialize;
use tauri::Manager;
use tauri::WebviewUrl;
use tauri::WebviewWindowBuilder;

/// 站点登录规格：一份常量描述一个站的登录收割全流程差异点
pub struct SiteLoginSpec {
    /// 窗口标题前缀（如 "百度网盘"）
    pub title: &'static str,
    /// 日志标签（如 "baidu" → "[feiliu-baidu-login]"）
    pub log_tag: &'static str,
    /// 轮询日志里的家族叫法（如 "baidu"/"123pan"）
    pub family_label: &'static str,
    /// 登录页入口
    pub login_url: &'static str,
    /// 登录窗口标签（幂等重开判定）
    pub window_label: &'static str,
    /// 登录等待上限（秒）
    pub timeout_secs: u64,
    /// 登录成功标志 cookie 名（如 "BDUSS"/"__pus"/"sso-token"/"COOKIE_SESSION"）
    pub login_cookie_mark: &'static str,
    /// 家族域名（含子域判别，如 "baidu.com"）
    pub family_domain: &'static str,
    /// 会话落盘文件名（`<config_dir>/` 下）
    pub session_file: &'static str,
    /// 测试/诊断的会话路径环境变量覆盖名
    pub env_session_file: &'static str,
}

/// 会话文件路径：`<app_config_dir>/<session_file>`（config_dir 取不到回落 data_dir）
pub fn session_path(app: &tauri::AppHandle, spec: &SiteLoginSpec) -> PathBuf {
    let data_dir = app
        .path()
        .app_data_dir()
        .unwrap_or_else(|_| PathBuf::from("."));
    let cfg_dir = app
        .path()
        .app_config_dir()
        .unwrap_or_else(|_| data_dir.clone());
    cfg_dir.join(spec.session_file)
}

/// 判断 cookie domain 是否属于家族域（".baidu.com"/"pan.baidu.com" 等）
pub fn is_family_domain(domain: &str, family: &str) -> bool {
    let d = domain.trim_start_matches('.');
    d == family || d.ends_with(&format!(".{family}"))
}

/// 从路径读取会话 JSON（无文件/损坏 → None）
pub fn load_json_at<S: serde::de::DeserializeOwned>(p: &Path) -> Option<S> {
    let raw = std::fs::read_to_string(p).ok()?;
    serde_json::from_str(&raw).ok()
}

/// 会话落盘（unix 0600：本机敏感凭据最小权限；不进任何仓库/日志）
pub fn save_json_at<S: serde::Serialize>(p: &Path, s: &S) -> Result<(), String> {
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("创建配置目录失败: {e}"))?;
    }
    let body = serde_json::to_string(s).map_err(|e| format!("序列化会话失败: {e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(p)
            .map_err(|e| format!("写入会话文件失败: {e}"))?;
        f.write_all(body.as_bytes())
            .map_err(|e| format!("写入会话文件失败: {e}"))
    }
    #[cfg(not(unix))]
    {
        std::fs::write(p, body).map_err(|e| format!("写入会话文件失败: {e}"))
    }
}

/// 删除会话文件（不存在视为已退出，幂等）
pub fn clear_at(p: &Path) {
    let _ = std::fs::remove_file(p);
}

/// 登录状态摘要（前端设置页展示；四站同形，JSON 字段 logged_in/saved_at）
#[derive(Debug, Serialize)]
pub struct CloudLoginStatus {
    pub logged_in: bool,
    pub saved_at: Option<i64>,
}

/// 由「是否有会话（含收割时间）」构造状态摘要
pub fn status_of(saved_at: Option<i64>) -> CloudLoginStatus {
    CloudLoginStatus {
        logged_in: saved_at.is_some(),
        saved_at,
    }
}

/// 打开可见登录窗口，轮询原生全量 cookie store 等待登录标记 cookie 出现。
/// 用户关窗=取消；超时（规格给定）报错；成功后窗口自动关闭。
/// 返回（家族 cookie 拼成的请求头形态, 收割时间 ms）。
pub async fn collect(
    app: tauri::AppHandle,
    spec: &'static SiteLoginSpec,
) -> Result<(String, i64), String> {
    // 上次未关的同名窗口先销毁（幂等重开）
    if let Some(w) = app.get_webview_window(spec.window_label) {
        let _ = w.destroy();
    }
    let url = spec
        .login_url
        .parse()
        .map_err(|e| format!("URL 解析失败: {e}"))?;
    let window = WebviewWindowBuilder::new(&app, spec.window_label, WebviewUrl::External(url))
        .title(format!("登录{} · 登录成功后本窗口自动关闭", spec.title))
        .inner_size(420.0, 560.0)
        .build()
        .map_err(|e| format!("创建登录窗口失败: {e}"))?;
    let _ = window.show();
    let _ = window.set_focus();

    let deadline = std::time::Instant::now() + Duration::from_secs(spec.timeout_secs);
    let mut polls = 0u32;
    loop {
        tokio::time::sleep(Duration::from_secs(1)).await;
        // 窗口已被用户关闭（get 不到）= 取消
        if app.get_webview_window(spec.window_label).is_none() {
            return Err("已取消登录".into());
        }
        if std::time::Instant::now() >= deadline {
            let _ = window.destroy();
            return Err(format!(
                "登录超时（{} 分钟），请重试",
                spec.timeout_secs / 60
            ));
        }
        polls += 1;
        // 导航中/尚未登录时可能报错——静默继续下一轮
        let cookies = match window.cookies() {
            Ok(c) => c,
            Err(e) => {
                eprintln!(
                    "[feiliu-{}-login] 轮询 {polls}: cookie store 读取失败 {e}",
                    spec.log_tag
                );
                continue;
            }
        };
        let family_cookies: Vec<_> = cookies
            .iter()
            .filter(|c| {
                c.domain()
                    .map(|d| is_family_domain(d, spec.family_domain))
                    .unwrap_or(false)
            })
            .collect();
        if polls % 5 == 0 {
            eprintln!(
                "[feiliu-{}-login] 轮询 {polls}: 全量 {} 条，{} 家族 {} 条",
                spec.log_tag,
                cookies.len(),
                spec.family_label,
                family_cookies.len()
            );
        }
        if family_cookies.iter().all(|c| c.name() != spec.login_cookie_mark) {
            continue;
        }
        // 登录成功：家族 cookie 拼成请求头形态
        let header = family_cookies
            .iter()
            .map(|c| format!("{}={}", c.name(), c.value()))
            .collect::<Vec<_>>()
            .join("; ");
        eprintln!(
            "[feiliu-{}-login] 登录成功（轮询 {polls} 次），收割 {} 条 cookie",
            spec.log_tag,
            family_cookies.len()
        );
        let _ = window.destroy();
        let saved_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        return Ok((header, saved_at));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 家族域判别：裸域/前导点/子域=是；他域/前缀陷阱=否
    #[test]
    fn test_family_domain() {
        assert!(is_family_domain("baidu.com", "baidu.com"));
        assert!(is_family_domain(".baidu.com", "baidu.com"));
        assert!(is_family_domain("pan.baidu.com", "baidu.com"));
        assert!(!is_family_domain("notbaidu.com", "baidu.com"));
        assert!(!is_family_domain("example.com", "baidu.com"));
    }
}
