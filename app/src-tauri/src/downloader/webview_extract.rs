//! WebView 代取通道：用应用内嵌 WebView（真实浏览器 TLS 指纹）加载
//! 指纹墙站点的页面（如蓝奏云 fn 页），由页面自身 JS 完成直链生成，
//! 轮询执行 js 表达式读取结果。桌面端专属。
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tauri::WebviewUrl;
use tauri::WebviewWindowBuilder;

use super::extract::WebviewExtractPlan;

/// 加载 plan.url，每 500ms 在页面上下文执行 plan.js，
/// 返回首个非空结果（JSON 字符串由调用方 js 负责序列化）。
pub async fn extract(
    app: tauri::AppHandle,
    plan: &WebviewExtractPlan,
) -> Result<String, String> {
    let label = format!("webview-extract-{}", uuid::Uuid::new_v4());
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

    let result: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let deadline = std::time::Instant::now() + Duration::from_millis(plan.timeout_ms);

    loop {
        if std::time::Instant::now() >= deadline {
            let _ = window.close();
            return Err("WebView 代取超时".into());
        }
        tokio::time::sleep(Duration::from_millis(500)).await;

        let (tx, rx) = tokio::sync::oneshot::channel::<String>();
        let js = format!(
            "window.__feiliuSend || (window.__feiliuSend = 1); ({})",
            plan.js
        );
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
        // Tauri 回调返回的是 JS 表达式结果的字符串形式（已 JSON.stringify → 带引号）
        let trimmed = value.trim().trim_matches('"').to_string();
        if !trimmed.is_empty() && trimmed != "null" && trimmed != "undefined" {
            let _ = window.close();
            let mut slot = result.lock().unwrap();
            *slot = Some(trimmed.clone());
            return Ok(trimmed);
        }
        // 页面还在生成直链（fn 页 ajax 未完成），继续轮询
        let _ = result;
        if std::time::Instant::now() >= deadline {
            let _ = window.close();
            return Err("WebView 代取超时（页面未产出结果）".into());
        }
    }
}
