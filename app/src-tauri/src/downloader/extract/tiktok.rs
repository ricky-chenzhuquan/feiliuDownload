//! TikTok（tiktok.com）视频页解析。
//!
//! 实测结论（2026-11-21 r3 轮 4/14）：
//! - 页面匿名 200（浏览器 UA）：__UNIVERSAL_DATA_FOR_REHYDRATION__ →
//!   webapp.video-detail.itemInfo.itemStruct（statusCode 0）：
//!   desc/author.uniqueId/video.bitrateInfo[] 全齐
//! - 直链取 **aweme 同源跳转票**：bitrateInfo[].PlayAddr.UrlList 中
//!   `www.tiktok.com/aweme/v1/play/` 形态的 URL——引擎请求时 302 →
//!   现场铸造的新签名 CDN URL → **200 完整 MP4（无 cookie、无指纹墙）**。
//!   页面内嵌的 playAddr/downloadAddr（v16/v19-webapp-prime 直链）签名
//!   绑定原会话，reqwest 直取 403——勿用（r3 轮 4 曾误判全 CDN TLS 墙）
//! - profile/tag 列表页是空壳（列表走签名 API 墙），仅视频页可解析
use serde_json::Value;

use super::helpers::clean_filename;

const TT_REFERER: &str = "https://www.tiktok.com/";
const UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0 Safari/537.36";

fn client() -> Option<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent(UA)
        .redirect(reqwest::redirect::Policy::limited(8))
        .build()
        .ok()
}

/// 识别 TikTok 视频页 URL，提取 (作者, 视频 id)
fn parse_video_url(url: &str) -> Option<(String, String)> {
    let lower = url.to_lowercase();
    if !lower.contains("tiktok.com") {
        return None;
    }
    let idx = lower.find("/video/")?;
    let after = &url[idx + "/video/".len()..];
    let id: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
    if id.len() < 10 {
        return None;
    }
    // 作者段：/@xxx/video/ → xxx（找不到作者时给占位）
    let author = lower
        .find("/@")
        .map(|ai| {
            url[ai + "/@".len()..]
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
                .collect::<String>()
        })
        .filter(|a| !a.is_empty())
        .unwrap_or_else(|| "tiktok".into());
    Some((author, id))
}

/// 从页面 HTML 抽 __UNIVERSAL_DATA_FOR_REHYDRATION__ JSON
fn extract_universal(html: &str) -> Option<Value> {
    let marker = r#"id="__UNIVERSAL_DATA_FOR_REHYDRATION__" type="application/json">"#;
    let idx = html.find(marker)?;
    let rest = &html[idx + marker.len()..];
    let end = rest.find("</script>")?;
    serde_json::from_str(&rest[..end]).ok()
}

/// 从 itemStruct.video 选 aweme 同源跳转票（302 铸新签名，reqwest 可直取）
fn pick_aweme_play(v: &Value) -> Option<String> {
    for t in v.get("bitrateInfo")?.as_array()? {
        let pa = t.get("PlayAddr")?;
        for u in pa.get("UrlList")?.as_array()? {
            let s = u.as_str()?;
            if s.contains("www.tiktok.com/aweme/v1/play/") {
                return Some(s.to_string());
            }
        }
    }
    None
}

/// 短链域名（App 分享默认形态）：vt.tiktok.com / vm.tiktok.com
fn is_short_link(url: &str) -> bool {
    let lower = url.to_lowercase();
    lower.contains("vt.tiktok.com") || lower.contains("vm.tiktok.com")
}

/// 短链跟随一次跳转（301/302 → /@user/video/{id}；过期短链落主页 → None）
async fn follow_short_link(url: &str) -> Option<String> {
    let client = reqwest::Client::builder()
        .user_agent(UA)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .ok()?;
    let resp = client.get(url).send().await.ok()?;
    let loc = resp
        .headers()
        .get(reqwest::header::LOCATION)?
        .to_str()
        .ok()?
        .to_string();
    let target = if loc.starts_with("http") {
        loc
    } else {
        format!("https://www.tiktok.com{loc}")
    };
    if target.to_lowercase().contains("/video/") {
        Some(target)
    } else {
        None
    }
}

pub async fn resolve(url: &str) -> Option<super::Extracted> {
    if is_short_link(url) {
        let target = follow_short_link(url).await?;
        return Box::pin(resolve(&target)).await;
    }
    let (author, id) = parse_video_url(url)?;
    let http = client()?;
    let resp = http
        .get(format!("https://www.tiktok.com/@{author}/video/{id}"))
        .header("Accept-Language", "en-US,en;q=0.9")
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let html = resp.text().await.ok()?;
    let data = extract_universal(&html)?;
    let vd = data.get("__DEFAULT_SCOPE__")?.get("webapp.video-detail")?;
    // statusCode 非 0（如 10204 内容不存在）→ 拒绝
    if vd.get("statusCode").and_then(|c| c.as_i64()).unwrap_or(-1) != 0 {
        return None;
    }
    let it = vd.get("itemInfo")?.get("itemStruct")?;
    let v = it.get("video")?;
    // aweme 跳转票优先（匿名可下）；页面直链（v16/v19）签名绑会话勿用
    let direct = pick_aweme_play(v)?;
    let desc = it
        .get("desc")
        .and_then(|d| d.as_str())
        .unwrap_or("tiktok_video")
        .to_string();
    let filename = clean_filename(&format!("{author} {desc}"), "mp4");
    Some(super::Extracted {
        direct_url: direct,
        filename,
        referer: Some(TT_REFERER.into()),
        notice: None,
        webview_plan: None,
    })
}

#[cfg(test)]
mod tests {
    /// 真实公开视频（r3 轮 4 浏览器收割的测试夹具）：解析 + 引擎式下载验证
    /// ——直接请求跳转票跟随 302，断言拿到真实 MP4 字节（全链路终审）
    #[tokio::test]
    async fn test_tiktok_real_video_download() {
        if std::env::var("FEILIU_NET_TEST").is_err() {
            return;
        }
        let info = super::super::resolve_retry(
            "https://www.tiktok.com/@deluxeklexasmr/video/7710009012472876302",
        )
        .await
        .expect("tiktok 解析失败");
        assert!(
            info.direct_url.contains("www.tiktok.com/aweme/v1/play/"),
            "直链应为 aweme 跳转票: {}",
            info.direct_url
        );
        assert_eq!(info.referer.as_deref(), Some(super::TT_REFERER));
        assert!(info.filename.contains("deluxeklexasmr"), "文件名应含作者: {}", info.filename);
        // 引擎式下载验证：跟随 302 取头 1KB，断言 MP4 真实可达
        let client = reqwest::Client::builder()
            .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 Chrome/126.0 Safari/537.36")
            .build()
            .unwrap();
        let r = client
            .get(&info.direct_url)
            .header("Range", "bytes=0-1023")
            .header("Referer", super::TT_REFERER)
            .send()
            .await
            .expect("下载请求失败");
        let ct = r.headers().get("content-type").and_then(|v| v.to_str().ok()).unwrap_or("");
        assert!(
            r.status().is_success() && ct.starts_with("video/"),
            "跳转票应可直取 MP4: HTTP {} type={ct}",
            r.status()
        );
        println!("tiktok e2e OK: {} → HTTP {} {}", info.filename, r.status(), ct);
    }

    /// 真实短链（App 分享形态）：跟随跳转 → 完整解析 + 下载可达
    #[tokio::test]
    async fn test_tiktok_real_shortlink() {
        if std::env::var("FEILIU_NET_TEST").is_err() {
            return;
        }
        let info = super::super::resolve_retry("https://vt.tiktok.com/ZSpSrc45f/")
            .await
            .expect("tiktok 短链解析失败");
        assert!(
            info.direct_url.contains("www.tiktok.com/aweme/v1/play/"),
            "短链解析后直链仍应为 aweme 跳转票: {}",
            info.direct_url
        );
        assert!(
            info.filename.contains("silhouettesbelow"),
            "文件名应含作者: {}",
            info.filename
        );
        println!("tiktok shortlink OK: {}", info.filename);
    }

    /// URL 形态：作者段解析与非法形态拒绝
    #[test]
    fn test_parse_video_url() {
        assert_eq!(
            super::parse_video_url("https://www.tiktok.com/@deluxeklexasmr/video/7710009012472876302"),
            Some(("deluxeklexasmr".into(), "7710009012472876302".into()))
        );
        assert_eq!(
            super::parse_video_url("https://www.tiktok.com/@user/video/7710009012472876302?is_from_webapp=1"),
            Some(("user".into(), "7710009012472876302".into()))
        );
        assert_eq!(super::parse_video_url("https://www.tiktok.com/@user"), None);
        assert_eq!(super::parse_video_url("https://example.com/@a/video/123"), None);
    }
}
