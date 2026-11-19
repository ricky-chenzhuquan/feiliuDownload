//! 抖音：短链/视频页 → ttwid Cookie 两段式 → 分享页内嵌 _ROUTER_DATA → 无水印直链。
//!
//! 2026-11 实测：分享页 SSR 需携带 ttwid Cookie 才内嵌 item_list；
//! play_addr 的 playwm 替换为 play 即无水印原始流。
use std::sync::OnceLock;

use super::helpers::{json_client, clean_filename, extract_json_after, find_key};

/// 抖音 ttwid：注册接口 → 回调 Set-Cookie。会话内缓存（有效期一年）。
async fn douyin_ttwid() -> Option<String> {
    if let Some(t) = DOUYIN_TTWID.get() {
        return Some(t.clone());
    }
    let http = json_client()?;
    let resp = http
        .post("https://ttwid.bytedance.com/ttwid/union/register/")
        .json(&serde_json::json!({
            "region": "cn",
            "aid": 1768,
            "needFid": false,
            "service": "www.ixigua.com",
            "migrate_info": {"ticket": "", "source": "node"},
            "cbUrlProtocol": "https",
            "union": true
        }))
        .send()
        .await
        .ok()?;
    let redirect = resp
        .json::<serde_json::Value>()
        .await
        .ok()?
        .get("redirect_url")?
        .as_str()?
        .to_string();
    let resp2 = http.get(&redirect).send().await.ok()?;
    for v in resp2.headers().get_all(reqwest::header::SET_COOKIE) {
        if let Ok(sv) = v.to_str() {
            if let Some(idx) = sv.find("ttwid=") {
                let val = sv[idx + 6..].split(';').next()?.to_string();
                let _ = DOUYIN_TTWID.set(val.clone());
                return Some(val);
            }
        }
    }
    None
}

static DOUYIN_TTWID: OnceLock<String> = OnceLock::new();

/// 从最终页 URL 提取视频数字 ID（/video/{digits}）
fn parse_aweme_id(page: &str) -> Option<String> {
    let idx = page.find("/video/")?;
    let rest = &page[idx + "/video/".len()..];
    let mut id = String::new();
    for c in rest.chars() {
        if c.is_ascii_digit() {
            id.push(c);
        } else if !id.is_empty() {
            break;
        }
    }
    if id.len() >= 15 {
        Some(id)
    } else {
        None
    }
}

pub async fn resolve(url: &str) -> Option<super::Extracted> {
    let http = json_client()?;
    // v.douyin.com 短链 / iesdouyin 分享页 → 归一化
    let target = if url.to_lowercase().contains("v.douyin.com")
        || url.to_lowercase().contains("iesdouyin.com")
    {
        super::helpers::follow_redirect(&http, url)
            .await
            .unwrap_or_else(|| url.to_string())
    } else {
        url.to_string()
    };
    let id = parse_aweme_id(&target)?;

    // iesdouyin 分享页：需携带 ttwid Cookie 才会在 SSR 中内嵌视频数据
    let ttwid = douyin_ttwid().await;
    let mut req = http.get(format!("https://www.iesdouyin.com/share/video/{id}/"));
    if let Some(t) = &ttwid {
        req = req.header(reqwest::header::COOKIE, format!("ttwid={t}"));
    }
    let html = req.send().await.ok()?.text().await.ok()?;
    let data = extract_json_after(&html, "window._ROUTER_DATA")?;
    let item = find_key(&data, "item_list")?.as_array()?.first()?.clone();

    let desc = item
        .get("desc")
        .and_then(|d| d.as_str())
        .unwrap_or("douyin_video")
        .to_string();

    // play_addr 的 url_list 第一个；playwm→play = 去水印
    let play_addr = find_key(&item, "play_addr")?;
    let raw = play_addr
        .get("url_list")
        .and_then(|l| l.as_array())
        .and_then(|a| a.first())
        .and_then(|u| u.as_str())
        .map(|s| s.to_string())?;
    let mut nowm = raw.replace("/playwm/", "/play/");
    if !nowm.contains('?') {
        nowm.push_str("?ratio=720p&line=0");
    }

    Some(super::Extracted {
        direct_url: nowm,
        filename: clean_filename(&desc, "mp4"),
        referer: None,
        notice: None,
            webview_plan: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_douyin_url_id() {
        let target = "https://www.douyin.com/video/7301234567890123456?x=1";
        assert_eq!(parse_aweme_id(target), Some("7301234567890123456".into()));
    }

    /// 真实短链：v.douyin.com → 重定向 → ttwid Cookie → 无水印直链
    #[tokio::test]
    async fn test_douyin_real_link() {
        if std::env::var("FEILIU_NET_TEST").is_err() {
            return;
        }
        let info = super::super::resolve_retry("https://v.douyin.com/13Rs-NJ7TdY/")
            .await
            .expect("抖音解析失败");
        assert!(
            info.direct_url.contains("/play/"),
            "应为无水印 /play/ 直链: {}",
            info.direct_url
        );
        assert!(
            !info.filename.starts_with("douyin_video"),
            "应取到标题文件名: {}",
            info.filename
        );
        println!("douyin OK: {}", info.filename);
    }
}
