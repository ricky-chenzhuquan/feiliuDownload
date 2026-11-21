//! 快手：分享链 → 官方移动落地页（fw/photo/{id} / short-video/{id}）→
//! 页面内嵌 JSON 的 representation 视频直链（kwaicdn/upic，pkey 签名，无水印）。
//!
//! 实测结论（2026-11-21，官网真实数据验证）：
//! - v.kuaishou.com/{码} 与 www.kuaishou.com/f/{码} → 301/302 →
//!   c.kuaishou.com/fw/photo/{photoId} 或 v.m.chenzhongtech.com/fw/photo/{photoId}
//!   （官方移动分享落地页，为无 App 接收者设计，匿名 curl 200）
//! - 落地页**无 __APOLLO_STATE__**（旧解析路径失效）：视频数据在页面内嵌
//!   JSON 的 representation[].backupUrl/url（"https://....mp4?pkey=..."）
//! - CDN **无 Referer 也 200** 完整 mp4，Range 206 可断点（v1.kwaicdn.com）
//! - 首页/热榜内容墙（需登录）不在范围；分享落地页即产品输入形态
use super::helpers::{json_client, clean_filename, extract_json_after, find_str};

// find_str 供 __APOLLO_STATE__ 兜底路径使用（Value 形态）
#[allow(unused_imports)]
use super::helpers::find_str as _find_str;

/// 页面 HTML 里扫第一个完整 mp4 直链（representation/backupUrl JSON 形态；
/// JSON 内可能出现 \/ 转义，需还原）
fn find_video_url(html: &str) -> Option<String> {
    // JSON 转义先整体还原（\/ → /、\u0026 → &），否则正则在转义形态上落空
    // （r3 轮快手真机踩坑：页面数据以 \/ 形态内嵌）
    let flat = html.replace("\\/", "/").replace("\\u0026", "&");
    // 路径含大写（真实 URL 如 upic/2026/.../BMjAy....mp4），query 贪婪吃到引号
    for m in regex::Regex::new(r#"https://[a-z0-9.-]+/[A-Za-z0-9/._%-]+\.mp4\?[^"\\]+"#)
        .ok()?
        .find_iter(&flat)
    {
        let u = m.as_str();
        if u.contains("pkey=") || u.contains("kwaicdn") || u.contains("upic") {
            return Some(u.to_string());
        }
    }
    None
}

pub async fn resolve(url: &str) -> Option<super::Extracted> {
    let http = json_client()?;
    let target = super::helpers::follow_redirect(&http, url)
        .await
        .unwrap_or_else(|| url.to_string());
    if !target.contains("kuaishou.com") && !target.contains("chenzhongtech.com") {
        return None;
    }
    let html = http.get(&target).send().await.ok()?.text().await.ok()?;
    // 新路径优先：representation 内嵌 JSON 的 mp4 直链（实测 2026-11 落地页形态）
    let photo_url = find_video_url(&html)
        // 旧路径兜底：__APOLLO_STATE__ photoUrl（历史页面形态，保留兼容）
        .or_else(|| {
            let v = extract_json_after(&html, "__APOLLO_STATE__")?;
            find_str(&v, "photoUrl").map(|s| s.to_string())
        })?;
    if !photo_url.starts_with("http") {
        return None;
    }
    // caption：页面 JSON 字段（正则捕获含转义形态，还原 \/）
    let caption = regex::Regex::new(r#""caption":"((?:[^"\\]|\\.)*)""#)
        .ok()
        .and_then(|re| re.captures(&html))
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().replace("\\/", "/"))
        .unwrap_or_else(|| "kuaishou_video".into());
    Some(super::Extracted {
        direct_url: photo_url,
        filename: clean_filename(&caption, "mp4"),
        // CDN 实测无 Referer 也 200；referer 不设，避免多余约束
        referer: None,
        notice: None,
        cookies: None,
        webview_plan: None,
    })
}

#[cfg(test)]
mod tests {
    /// 真实存活分享链（2026-11-21 搜索定位，官网跳转验证）：
    /// 短链 → 官方落地页 → representation 直链 → 引擎式下载可达
    #[tokio::test]
    async fn test_kuaishou_real_share() {
        if std::env::var("FEILIU_NET_TEST").is_err() {
            return;
        }
        let info = super::super::resolve_retry("https://v.kuaishou.com/nRnDibqI")
            .await
            .expect("快手分享解析失败");
        assert!(
            info.direct_url.contains(".mp4?"),
            "直链应为页面内嵌 mp4: {}",
            &info.direct_url[..80.min(info.direct_url.len())]
        );
        assert!(info.referer.is_none(), "CDN 无 Referer 即 200，不应设 referer");
        // 引擎式下载验证：匿名 GET 头 1KB
        let client = reqwest::Client::builder()
            .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 Chrome/126.0 Safari/537.36")
            .build()
            .unwrap();
        let r = client
            .get(&info.direct_url)
            .header("Range", "bytes=0-1023")
            .send()
            .await
            .expect("下载请求失败");
        let ct = r.headers().get("content-type").and_then(|v| v.to_str().ok()).unwrap_or("");
        assert!(
            r.status().is_success() && ct.starts_with("video/"),
            "直链应匿名可取: HTTP {} type={ct}",
            r.status()
        );
        println!("kuaishou e2e OK: {} → HTTP {}", info.filename, r.status());
    }

    /// 落地页 HTML 片段提取（真实结构的代表性样本）
    #[test]
    fn test_find_video_url_from_real_structure() {
        let html = r#"{"adaptationSet":[{"duration":15200,"id":1,"representation":[{"hidden":false,"backupUrl":["https:\/\/v1.kwaicdn.com\/upic\/2026\/03\/22\/17\/BMjAyNjAzMjIxNzA3MjdfNTAwOTM2MzM2MF8xOTEwMjYzNjAxMjVfMV8z_b_B6a1f51152420529d4007b8fb616c7944.mp4?pkey=AAXqMbZsAvjB8UGRyBWjLULGKByI2f\u0026tag=hd"],"url":"https:\/\/v1.kwaicdn.com\/upic\/x.mp4?pkey=AAA"}]}]}"#;
        let u = super::find_video_url(html).expect("应提取到直链");
        assert!(u.starts_with("https://v1.kwaicdn.com/upic/"), "反斜杠转义应还原: {u}");
        assert!(u.contains("pkey="), "签名参数应保留");
        assert!(!u.contains('\\'), "不得残留转义");
    }
}
