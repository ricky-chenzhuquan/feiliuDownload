//! Bilibili：视频页（含 b23.tv 短链）→ view API → playurl html5 MP4 直链。
use serde::Deserialize;

use super::helpers::{json_client, clean_filename};

#[derive(Deserialize)]
struct BiliViewResp {
    code: i64,
    #[serde(default)]
    data: Option<BiliViewData>,
}

#[derive(Deserialize)]
struct BiliViewData {
    title: String,
    cid: i64,
}

#[derive(Deserialize)]
struct BiliPlayResp {
    code: i64,
    #[serde(default)]
    data: Option<BiliPlayData>,
}

#[derive(Deserialize)]
struct BiliPlayData {
    #[serde(default)]
    durl: Option<Vec<BiliDurlSeg>>,
}

#[derive(Deserialize)]
struct BiliDurlSeg {
    url: String,
}

/// 从 URL 提取 BV 号（BV + 至少 10 位字母数字）
fn parse_bvid(url: &str) -> Option<String> {
    let idx = url.find("/video/BV")?;
    let rest = &url[idx + "/video/".len()..];
    let bvid: String = rest
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric())
        .collect();
    if bvid.len() >= 10 && bvid.starts_with("BV") {
        Some(bvid)
    } else {
        None
    }
}

pub async fn resolve(url: &str) -> Option<super::Extracted> {
    let http = json_client()?;
    let page = if url.to_lowercase().contains("b23.tv/") {
        super::helpers::follow_redirect(&http, url).await.unwrap_or_else(|| url.to_string())
    } else {
        url.to_string()
    };
    let bvid = parse_bvid(&page)?;

    let view: BiliViewResp = http
        .get(format!(
            "https://api.bilibili.com/x/web-interface/view?bvid={bvid}"
        ))
        .send()
        .await
        .ok()?
        .json()
        .await
        .ok()?;
    if view.code != 0 {
        return None;
    }
    let data = view.data?;
    let title = data.title;
    let cid = data.cid;

    let play: BiliPlayResp = http
        .get(format!(
            "https://api.bilibili.com/x/player/playurl?bvid={bvid}&cid={cid}&qn=64&type=mp4&platform=html5"
        ))
        .send()
        .await
        .ok()?
        .json()
        .await
        .ok()?;
    if play.code != 0 {
        return None;
    }
    let seg = play.data?.durl?.into_iter().next()?;
    let direct_url = seg.url;

    Some(super::Extracted {
        direct_url,
        filename: clean_filename(&title, "mp4"),
        referer: Some("https://www.bilibili.com/".into()),
        notice: None,
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_parse_bvid() {
        assert_eq!(
            super::parse_bvid("https://www.bilibili.com/video/BV1kMWz6CESY/?share_source=copy_web"),
            Some("BV1kMWz6CESY".into())
        );
        assert_eq!(super::parse_bvid("https://example.com/foo"), None);
    }
}
