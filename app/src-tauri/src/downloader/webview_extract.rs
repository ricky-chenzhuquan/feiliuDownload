//! WebView 代取通道：用应用内嵌 WebView（真实浏览器 TLS 指纹）加载
//! 指纹墙站点的页面（如蓝奏云 fn 页），由页面自身 JS 完成直链生成，
//! 轮询执行 js 表达式读取结果。桌面端专属。
//!
//! 媒体静音（2026-11-20 强化，用户报障「打电话时放歌」）：目标页面
//! （82dj play 页）audio autoplay，且页面 JS 会反复重启播放；音频设备
//! 切换（来电/通话）时 WKWebView 还会恢复已暂停的媒体。三层防线：
//! ① hook HTMLMediaElement.prototype.play——任何播放调用强制静音；
//! ② MutationObserver——动态新增媒体元素立即静音；③ 500ms 轮询兜底。
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tauri::WebviewUrl;
use tauri::WebviewWindowBuilder;

use super::extract::WebviewExtractPlan;

/// 三层静音防线（hook 只装一次，静音部分每轮执行）
const MUTE_JS: &str = r##"(()=>{
document.querySelectorAll("audio,video").forEach(m=>{try{m.muted=true;m.volume=0;if(m.pause)m.pause()}catch(e){}});
try{
  if(!window.__feiliu_hooked){
    window.__feiliu_hooked=true;
    const orig=HTMLMediaElement.prototype.play;
    HTMLMediaElement.prototype.play=function(){try{this.muted=true;this.volume=0}catch(e){};return orig.call(this)};
  }
}catch(e){};
try{
  if(!window.__feiliu_mo){
    window.__feiliu_mo=new MutationObserver(()=>{
      document.querySelectorAll("audio,video").forEach(m=>{try{if(!m.muted){m.muted=true;m.volume=0;m.pause()}}catch(e){}});
    });
    window.__feiliu_mo.observe(document,{childList:true,subtree:true});
  }
}catch(e){};
})()"##;

/// WebView 代取结果：直链 + 可选会话凭据/真实文件名。
/// cookie/name 由 plan.js 以 JSON {"url","cookie","name"} 形态返回（GoFile
/// 等 CDN 校验会话级凭据的站点需要把 cookie 透传给下载通道；82dj/蓝奏云
/// 老形态裸 URL 不受影响）。批量清单站点可附带 files/total（GoFile 列表形态）。
pub struct WebviewResult {
    pub url: String,
    pub cookie: Option<String>,
    pub name: Option<String>,
    /// 分享内全部文件（name, link）——批量命令用（如 gofile 多文件）
    pub files: Vec<(String, String)>,
}

/// 解析轮询结果：JSON 对象（{"url","cookie","name","files"}，url 非 http/magnet
/// 时整体拒绝；files 为可选 [{name,link}] 数组）或裸 URL/magnet
fn parse_webview_result(raw: &str) -> Option<WebviewResult> {
    if raw.starts_with('{') {
        let v: serde_json::Value = serde_json::from_str(raw).ok()?;
        let url = v.get("url")?.as_str()?.to_string();
        if !url.starts_with("http") && !url.starts_with("magnet:") {
            return None;
        }
        let non_empty = |x: Option<&str>| x.filter(|s| !s.is_empty()).map(|s| s.to_string());
        let files = v
            .get("files")
            .and_then(|f| f.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|it| {
                        let name = it.get("name")?.as_str()?.to_string();
                        let link = it.get("link")?.as_str()?.to_string();
                        Some((name, link))
                    })
                    .collect()
            })
            .unwrap_or_default();
        Some(WebviewResult {
            url,
            cookie: non_empty(v.get("cookie").and_then(|c| c.as_str())),
            name: non_empty(v.get("name").and_then(|c| c.as_str())),
            files,
        })
    } else if raw.starts_with("http") || raw.starts_with("magnet:") {
        Some(WebviewResult {
            url: raw.to_string(),
            cookie: None,
            name: None,
            files: Vec::new(),
        })
    } else {
        None
    }
}

/// 加载 plan.url，每 500ms 静音媒体并执行 plan.js，
/// 返回首个非空结果（裸 URL/magnet 或 JSON {"url","cookie","name"}）。
pub async fn extract(
    app: tauri::AppHandle,
    plan: &WebviewExtractPlan,
) -> Result<WebviewResult, String> {
    let label = format!("webview-extract-{}", uuid::Uuid::new_v4());
    eprintln!("[feiliu-webview] 创建代取窗口 url={}", plan.url);
    let mut builder = WebviewWindowBuilder::new(
        &app,
        &label,
        WebviewUrl::External(plan.url.parse().map_err(|e| format!("URL 解析失败: {e}"))?),
    )
    .title("feiliu-extract")
    .skip_taskbar(true)
    .inner_size(480.0, 400.0)
    // document-start 注入静音 hook：先于页面全部 JS（含 autoplay）执行，
    // 零出声间隙（2026-11-20 用户报障「打电话时放歌」的治本修复）
    .initialization_script(MUTE_JS);
    // ALTCHA 等 PoW 验证码依赖页内计时器，隐藏 WebView 被 WebKit 节流
    // （1Hz）爬行——此类站点（plan.visible=true）用可见窗口全速跑完
    if !plan.visible {
        builder = builder.visible(false);
    }
    let window = builder
        .build()
        .map_err(|e| format!("创建 WebView 窗口失败: {e}"))?;
    if plan.visible {
        let _ = window.show();
        let _ = window.set_focus();
    }

    // 窗口建好后立即静音（页面 load 前的 eval 可能无效，轮询里会重复执行）
    let _ = window.eval(MUTE_JS);

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
        // 回调等待加超时保护：eval 成功但回调永不触发时（部分站点页面 JS
        // 卡死/被拦，gofile d/ 页实测），rx.await 会无限挂起——deadline
        // 检查在 await 之后永远跑不到，自检实测整条代取链路卡死
        let Ok(Ok(value)) = tokio::time::timeout(Duration::from_millis(2500), rx).await else {
            continue;
        };
        // 回调值是多层 stringify 的产物：字符级 unescape（\/ → /、\" → "、
        // \\ → \），避免字面量转义歧义
        let raw = value.trim().trim_matches('"');
        let chars: Vec<char> = raw.chars().collect();
        let mut out = String::new();
        let mut i = 0usize;
        while i < chars.len() {
            if chars[i] == '\\' && i + 1 < chars.len() {
                match chars[i + 1] {
                    '/' => { out.push('/'); i += 2; }
                    '"' => { out.push('"'); i += 2; }
                    // 多层 stringify 的 \\ 转义：必须成对消费，否则下一个
                    // 字符会被误当转义符配对，trim 引号后尾部残留 \（82dj
                    // 直链实测被污染成 xxx.mp4\ → CDN 404）
                    '\\' => { out.push('\\'); i += 2; }
                    _ => { out.push(chars[i]); i += 1; }
                }
            } else {
                out.push(chars[i]);
                i += 1;
            }
        }
        // unescape 后（\" → "）还剩内层包裹引号，再去一次；尾部防御清洗
        // （URL 不可能以反斜杠/引号结尾，残留即污染）
        let cleaned = out
            .trim()
            .trim_end_matches(['\\', '"'])
            .trim_matches('"')
            .trim_end_matches(['\\', '"'])
            .to_string();
        eprintln!(
            "[feiliu-webview] 第 {} 次轮询结果: {}",
            attempts,
            &cleaned[..80.min(cleaned.len())]
        );
        if let Some(wv) = parse_webview_result(&cleaned) {
            let _ = window.close();
            return Ok(wv);
        }
        // 页面还在生成直链（fn 页 ajax 未完成 / audio src 未设置），继续轮询
    }
}

#[cfg(test)]
mod tests {
    use super::parse_webview_result;

    /// JSON 形态：url + cookie + name 全收
    #[test]
    fn test_parse_json_result() {
        let r = parse_webview_result(
            r#"{"url":"https://s.gofile.io/download/web/x/feiliu_fixture.txt","cookie":"accountToken=abc; other=1","name":"feiliu_fixture.txt"}"#,
        )
        .expect("JSON 应解析成功");
        assert_eq!(r.url, "https://s.gofile.io/download/web/x/feiliu_fixture.txt");
        assert_eq!(r.cookie.as_deref(), Some("accountToken=abc; other=1"));
        assert_eq!(r.name.as_deref(), Some("feiliu_fixture.txt"));
    }

    /// JSON 形态：空 cookie/name 过滤为 None；url 非法时整体拒绝；files 数组解析
    #[test]
    fn test_parse_json_edge_cases() {
        let r = parse_webview_result(r#"{"url":"https://a.b/c.mp4","cookie":"","name":""}"#).expect("空串应过滤");
        assert_eq!(r.cookie, None);
        assert_eq!(r.name, None);
        assert!(parse_webview_result(r#"{"url":"javascript:x"}"#).is_none(), "非 http url 拒绝");
        assert!(parse_webview_result(r#"{"cookie":"x"}"#).is_none(), "缺 url 拒绝");
        // files 批量清单形态（GoFile 列表）
        let r = parse_webview_result(
            r#"{"url":"https://s.gofile.io/download/web/a/f.txt","cookie":"accountToken=x","name":"f.txt","files":[{"name":"f.txt","link":"https://s.gofile.io/download/web/a/f.txt"},{"name":"g.mp4","link":"https://s.gofile.io/download/web/b/g.mp4"}]}"#,
        )
        .expect("files 形态应解析");
        assert_eq!(r.files.len(), 2);
        assert_eq!(r.files[0], ("f.txt".into(), "https://s.gofile.io/download/web/a/f.txt".into()));
        assert_eq!(r.files[1].0, "g.mp4");
        // 缺 files 时为空
        let r = parse_webview_result(r#"{"url":"https://a.b/c.mp4"}"#).expect("无 files 应解析");
        assert!(r.files.is_empty());
        // files 内缺 link 的项被过滤
        let r = parse_webview_result(r#"{"url":"https://a.b/c.mp4","files":[{"name":"x"}]}"#).expect("坏项应过滤");
        assert!(r.files.is_empty());
    }

    /// 裸 URL/magnet 老形态（蓝奏云/82dj）不受影响
    #[test]
    fn test_parse_plain_url_compat() {
        let r = parse_webview_result("https://example.com/f.mp4").expect("裸 URL 应通过");
        assert_eq!(r.url, "https://example.com/f.mp4");
        assert_eq!(r.cookie, None);
        assert_eq!(r.name, None);
        let r = parse_webview_result("magnet:?xt=urn:btih:abc").expect("magnet 应通过");
        assert!(r.url.starts_with("magnet:"));
        assert!(parse_webview_result("").is_none());
        assert!(parse_webview_result("loading...").is_none());
    }

    /// 多层 stringify 污染回归：`\\` + `"` 序列错序处理后尾部残留 `\`
    /// （82dj 直链实测被污染成 xxx.mp4\ → CDN 404）
    #[test]
    fn test_unescape_no_backslash_residue() {
        // 模拟三层 stringify 后 trim 外层引号的 raw：尾部 \\\" 应还原为空
        let raw = r#"\"https://mp3.82dj.com/x.mp4\\\""#;
        let chars: Vec<char> = raw.chars().collect();
        let mut out = String::new();
        let mut i = 0usize;
        while i < chars.len() {
            if chars[i] == '\\' && i + 1 < chars.len() {
                match chars[i + 1] {
                    '/' => { out.push('/'); i += 2; }
                    '"' => { out.push('"'); i += 2; }
                    '\\' => { out.push('\\'); i += 2; }
                    _ => { out.push(chars[i]); i += 1; }
                }
            } else {
                out.push(chars[i]);
                i += 1;
            }
        }
        let cleaned = out
            .trim()
            .trim_end_matches(['\\', '"'])
            .trim_matches('"')
            .trim_end_matches(['\\', '"']);
        assert_eq!(cleaned, "https://mp3.82dj.com/x.mp4");
        assert!(!cleaned.ends_with('\\'), "尾部不得残留反斜杠");
    }
}
