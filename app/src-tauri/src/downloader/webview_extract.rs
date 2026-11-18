//! WebView 代取通道：用应用内嵌 WebView（真实浏览器 TLS 指纹）加载
//! 指纹墙站点的页面（如蓝奏云 fn 页），由页面自身 JS 完成直链生成，
//! 轮询执行 js 表达式读取结果。桌面端专属。
//!
//! 媒体静音：目标页面（如 82dj play 页）会自动播放音频——隐藏窗口里
//! 每次轮询都强制静音/暂停全部 audio/video，避免「突然放歌」。
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tauri::WebviewUrl;
use tauri::WebviewWindowBuilder;

use super::extract::WebviewExtractPlan;

/// 静音并暂停页面内全部媒体（含后续动态创建的，靠轮询重复执行）
const MUTE_JS: &str = r##"(()=>{document.querySelectorAll("audio,video").forEach(m=>{try{m.muted=true;m.volume=0;if(m.pause)m.pause()}catch(e){}});})()"##;

/// 加载 plan.url，每 500ms 静音媒体并执行 plan.js，
/// 返回首个非空结果（JSON 字符串由调用方 js 负责序列化）。
pub async fn extract(
    app: tauri::AppHandle,
    plan: &WebviewExtractPlan,
) -> Result<String, String> {
    let label = format!("webview-extract-{}", uuid::Uuid::new_v4());
    eprintln!("[feiliu-webview] 创建代取窗口 url={}", plan.url);
    let window = WebviewWindowBuilder::new(
        &app,
        &label,
        WebviewUrl::External(plan.url.parse().map_err(|e| format!("URL 解析失败: {e}"))?),
    )
    .title("feiliu-extract")
    .visible(false)
    .skip_taskbar(true)
    .inner_size(420.0, 320.0)
    .build()
    .map_err(|e| format!("创建 WebView 窗口失败: {e}"))?;

    // 窗口建好后立即静音（页面 load 前的 eval 可能无效，轮询里会重复执行）
    let _ = window.eval(MUTE_JS);

    let result: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let deadline = std::time::Instant::now() + Duration::from_millis(plan.timeout_ms);
    let mut attempts = 0u32;

    loop {
        if std::time::Instant::now() >= deadline {
            eprintln!(
                "[feiliu-webview] 超时（{:.1}s，{} 次轮询无结果）url={}",
                plan.timeout_ms as f64 / 1000.0,
                attempts,
                plan.url
            );
            let _ = window.close();
            return Err("WebView 代取超时".into());
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
        attempts += 1;

        // 每轮先静音（autoplay 页面会反复起播）
        let _ = window.eval(MUTE_JS);

        let (tx, rx) = tokio::sync::oneshot::channel::<String>();
        let js = format!("({})", plan.js);
        let tx_mutex = Arc::new(Mutex::new(Some(tx)));
        let tx_clone = tx_mutex.clone();
        let res = window.eval_with_callback(js, move |value| {
            if let Some(tx) = tx_clone.lock().unwrap().take() {
                let _ = tx.send(value);
            }
        });
        if res.is_err() {
            continue; // 页面尚未就绪，下轮重试
        }
        let Ok(value) = rx.await else {
            continue;
        };
        // 回调值是多层 stringify 的产物：去掉转义斜杠（\/）与首尾引号
        // 回调值是多层 stringify 的产物：字符级 unescape（\/ → /、\" → "），
        // 避免字面量转义歧义
        let raw = value.trim().trim_matches('"');
        let chars: Vec<char> = raw.chars().collect();
        let mut out = String::new();
        let mut i = 0usize;
        while i < chars.len() {
            if chars[i] == '\\' && i + 1 < chars.len() {
                match chars[i + 1] {
                    '/' => { out.push('/'); i += 2; }
                    '"' => { out.push('"'); i += 2; }
                    _ => { out.push(chars[i]); i += 1; }
                }
            } else {
                out.push(chars[i]);
                i += 1;
            }
        }
        // unescape 后（\" → "）还剩内层包裹引号，再去一次
        let cleaned = out.trim().trim_matches('"').to_string();
        eprintln!(
            "[feiliu-webview] 第 {} 次轮询结果: {}",
            attempts,
            &cleaned[..80.min(cleaned.len())]
        );
        if cleaned.starts_with("http") || cleaned.starts_with("magnet:") {
            let _ = window.close();
            let mut slot = result.lock().unwrap();
            *slot = Some(cleaned.clone());
            return Ok(cleaned);
        }
        // 页面还在生成直链（fn 页 ajax 未完成 / audio src 未设置），继续轮询
        let _ = result;
    }
}
