//! Pixiv（pixiv.net）作品页解析。
//!
//! 实测结论（2026-11-20 r3 轮 2）：
//! - 匿名链路全通（无需登录）：
//!   ① 单图 /ajax/illust/{id} → body.urls.original
//!   ② 多图 /ajax/illust/{id}/pages → body[].urls.original（p0..pN）
//!   ③ 动图 /ajax/illust/{id}/ugoira_meta → body.originalSrc（帧 zip）
//!   ④ 日榜 ranking.php?content=ugoira&format=json 匿名可抓（真实 ID 测试素材源）
//! - 图片 CDN i.pximg.net 强 Referer 墙：无 Referer 403，
//!   带 `Referer: https://www.pixiv.net/` 即 200（引擎 per-task referer 已支持）
//! - 档位策略：动图取 originalSrc 帧 zip；多图取第 1 页 + 引导 notice（多页批量后续项）
use serde_json::Value;

use super::helpers::clean_filename;

const PIXIV_REFERER: &str = "https://www.pixiv.net/";
const UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0 Safari/537.36";

fn client() -> Option<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent(UA)
        .build()
        .ok()
}

/// 识别 pixiv 作品页 URL，提取作品 ID（支持 /artworks/{id} 与 /en|/zh-CN 等语言前缀）
fn parse_illust_id(url: &str) -> Option<String> {
    let lower = url.to_lowercase();
    if !lower.contains("pixiv.net") {
        return None;
    }
    let idx = lower.find("/artworks/")?;
    let rest = &url[idx + "/artworks/".len()..];
    let id: String = rest
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    if id.is_empty() {
        None
    } else {
        Some(id)
    }
}

async fn get_json(http: &reqwest::Client, path: &str) -> Option<Value> {
    let resp = http
        .get(format!("https://www.pixiv.net{path}"))
        .header("Referer", PIXIV_REFERER)
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let v: Value = resp.json().await.ok()?;
    if v.get("error")?.as_bool()? {
        return None;
    }
    Some(v)
}

/// 从直链 URL 后缀取扩展名（.jpg/.png/.gif/.zip）
fn ext_of(direct: &str) -> &'static str {
    let lower = direct.to_lowercase();
    if lower.ends_with(".png") {
        "png"
    } else if lower.ends_with(".gif") {
        "gif"
    } else if lower.ends_with(".zip") {
        "zip"
    } else {
        "jpg"
    }
}

pub async fn resolve(url: &str) -> Option<super::Extracted> {
    let id = parse_illust_id(url)?;
    let http = client()?;

    // ① 作品元信息（标题 + 类型 + 单图直链）
    let v = get_json(&http, &format!("/ajax/illust/{id}")).await?;
    let b = v.get("body")?;
    let title = b
        .get("title")
        .and_then(|t| t.as_str())
        .unwrap_or(&format!("pixiv_{id}"))
        .to_string();
    let illust_type = b.get("illustType").and_then(|t| t.as_i64()).unwrap_or(0);

    // ③ 动图：帧 zip（originalSrc），文件名带「动图」标记
    if illust_type == 2 {
        let meta = get_json(&http, &format!("/ajax/illust/{id}/ugoira_meta")).await?;
        let src = meta
            .get("body")
            .and_then(|b| b.get("originalSrc"))
            .and_then(|s| s.as_str())?;
        let frames = meta
            .get("body")
            .and_then(|b| b.get("frames"))
            .and_then(|f| f.as_array())
            .map(|a| a.len())
            .unwrap_or(0);
        return Some(super::Extracted {
            direct_url: src.to_string(),
            filename: clean_filename(&format!("{title}（pixiv动图{frames}帧）"), "zip"),
            referer: Some(PIXIV_REFERER.into()),
            notice: Some("pixiv 动图为帧序列 zip（非视频），解压后可拼接".into()),
            cookies: None,
            user_agent: None,
            webview_plan: None,
        });
    }

    // ② 多图作品：取第 1 页 + 引导 notice
    if b.get("pageCount").and_then(|c| c.as_i64()).unwrap_or(1) > 1 {
        let pages = get_json(&http, &format!("/ajax/illust/{id}/pages")).await?;
        let first = pages
            .get("body")
            .and_then(|p| p.as_array())
            .and_then(|a| a.first())
            .and_then(|p| p.get("urls"))
            .and_then(|u| u.get("original"))
            .and_then(|s| s.as_str())?;
        let count = pages
            .get("body")
            .and_then(|p| p.as_array())
            .map(|a| a.len())
            .unwrap_or(0);
        return Some(super::Extracted {
            direct_url: first.to_string(),
            filename: clean_filename(&title, ext_of(first)),
            referer: Some(PIXIV_REFERER.into()),
            notice: Some(format!("pixiv 多图作品共 {count} 页：本次取第 1 页，其余页面请在官网查看")),
            cookies: None,
            user_agent: None,
            webview_plan: None,
        });
    }

    // ① 单图
    let original = b
        .get("urls")
        .and_then(|u| u.get("original"))
        .and_then(|s| s.as_str())?;
    if original.is_empty() {
        return None;
    }
    Some(super::Extracted {
        direct_url: original.to_string(),
        filename: clean_filename(&title, ext_of(original)),
        referer: Some(PIXIV_REFERER.into()),
        notice: None,
        cookies: None,
        user_agent: None,
        webview_plan: None,
    })
}

#[cfg(test)]
mod tests {
    /// 真实单图（日榜作品）：匿名 API → original 直链 + Referer
    #[tokio::test]
    async fn test_pixiv_real_single() {
        if std::env::var("FEILIU_NET_TEST").is_err() {
            return;
        }
        let info = super::super::resolve_retry("https://www.pixiv.net/artworks/128965660")
            .await
            .expect("pixiv 单图解析失败");
        assert!(
            info.direct_url.contains("i.pximg.net/img-original/"),
            "直链应为 pximg 原始图: {}",
            info.direct_url
        );
        assert_eq!(
            info.referer.as_deref(),
            Some(super::PIXIV_REFERER),
            "必须带 pixiv Referer（CDN 防盗链墙）"
        );
        assert!(!info.filename.is_empty(), "文件名取作品标题");
        println!("pixiv single OK: {} → {}", info.filename, info.direct_url);
    }

    /// 真实多图作品（2026-11-20 日榜漫画，7 页）：第 1 页直链 + 页数引导
    #[tokio::test]
    async fn test_pixiv_real_manga() {
        if std::env::var("FEILIU_NET_TEST").is_err() {
            return;
        }
        let info = super::super::resolve_retry("https://www.pixiv.net/artworks/152222503")
            .await
            .expect("pixiv 多图解析失败");
        assert!(
            info.direct_url.contains("_p0."),
            "多图应取第 1 页: {}",
            info.direct_url
        );
        let notice = info.notice.expect("多图应有页数引导");
        assert!(notice.contains("7 页"), "应含真实页数: {notice}");
        println!("pixiv manga OK: {} ({})", info.filename, notice);
    }

    /// 真实动图（2026-11-20 动图日榜第 1）：帧 zip 直链
    #[tokio::test]
    async fn test_pixiv_real_ugoira() {
        if std::env::var("FEILIU_NET_TEST").is_err() {
            return;
        }
        let info = super::super::resolve_retry("https://www.pixiv.net/artworks/152189747")
            .await
            .expect("pixiv 动图解析失败");
        assert!(
            info.direct_url.contains("img-zip-ugoira") && info.direct_url.ends_with(".zip"),
            "动图直链应为帧 zip: {}",
            info.direct_url
        );
        assert!(
            info.filename.contains("pixiv动图"),
            "文件名应带动图标记: {}",
            info.filename
        );
        assert!(info.notice.is_some(), "应有 zip 形态说明");
        println!("pixiv ugoira OK: {} → {}", info.filename, info.direct_url);
    }

    /// URL 形态：语言前缀 /en/artworks/ 同样可解析
    #[test]
    fn test_parse_illust_id() {
        assert_eq!(
            super::parse_illust_id("https://www.pixiv.net/artworks/128965660"),
            Some("128965660".into())
        );
        assert_eq!(
            super::parse_illust_id("https://www.pixiv.net/en/artworks/128965660#manga"),
            Some("128965660".into())
        );
        assert_eq!(super::parse_illust_id("https://www.pixiv.net/users/123"), None);
        assert_eq!(super::parse_illust_id("https://example.com/artworks/1"), None);
    }
}
