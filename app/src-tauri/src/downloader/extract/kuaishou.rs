//! 快手：分享页 `__APOLLO_STATE__` → photoUrl（CDN 源，无水印）+ caption。
use super::helpers::{json_client, clean_filename, extract_json_after, find_str};

pub async fn resolve(url: &str) -> Option<super::Extracted> {
    let http = json_client()?;
    let target = super::helpers::follow_redirect(&http, url)
        .await
        .unwrap_or_else(|| url.to_string());
    if !target.contains("kuaishou.com") && !target.contains("chenzhongtech.com") {
        return None;
    }
    let html = http.get(&target).send().await.ok()?.text().await.ok()?;
    let data = extract_json_after(&html, "__APOLLO_STATE__")?;
    let photo_url = find_str(&data, "photoUrl")?.to_string();
    if !photo_url.starts_with("http") {
        return None;
    }
    let caption = find_str(&data, "caption").unwrap_or("kuaishou_video").to_string();
    Some(super::Extracted {
        direct_url: photo_url,
        filename: clean_filename(&caption, "mp4"),
        referer: Some("https://www.kuaishou.com/".into()),
        notice: None,
    })
}
