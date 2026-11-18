//! 通用兜底：og:video / `<video src>`（任意含视频的网页，64KB 范围嗅探）。
use super::helpers::{clean_filename, MOBILE_UA};

fn find_meta<'a>(html: &'a str, prop: &str) -> Option<&'a str> {
    // <meta property="og:video" content="...">
    let needle = format!("{prop}\" content=\"");
    let idx = html.find(&needle)?;
    let rest = &html[idx + needle.len()..];
    let end = rest.find('"')?;
    Some(&rest[..end])
}

pub async fn resolve(url: &str) -> Option<super::Extracted> {
    let http = reqwest::Client::builder()
        .user_agent(MOBILE_UA)
        .build()
        .ok()?;
    // 范围嗅探：只取 64KB 判断类型并解析 meta（直链媒体文件在这里直接返回 None）
    let resp = http
        .get(url)
        .header(reqwest::header::RANGE, "bytes=0-65535")
        .send()
        .await
        .ok()?;
    let ctype = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    if !ctype.contains("text/html") {
        return None; // 已是媒体/二进制 → 引擎直接按直链处理
    }
    let html = resp.text().await.ok()?;
    for prop in ["og:video:secure_url", "og:video:url", "og:video"] {
        if let Some(u) = find_meta(&html, prop) {
            if u.starts_with("http") {
                let title = find_meta(&html, "og:title").unwrap_or("video");
                return Some(super::Extracted {
                    direct_url: u.to_string(),
                    filename: clean_filename(title, "mp4"),
                    referer: None,
                    notice: None,
            webview_plan: None,
                });
            }
        }
    }
    // <video ... src="...">
    if let Some(pos) = html.find("<video") {
        let seg = &html[pos..(pos + 2000).min(html.len())];
        if let Some(i) = seg.find("src=\"") {
            let rest = &seg[i + "src=\"".len()..];
            if let Some(end) = rest.find('"') {
                let u = &rest[..end];
                if u.starts_with("http") {
                    return Some(super::Extracted {
                        direct_url: u.to_string(),
                        filename: clean_filename("page_video", "mp4"),
                        referer: None,
                        notice: None,
            webview_plan: None,
                    });
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_og_video_meta() {
        let html = r#"<meta property="og:video:secure_url" content="https://cdn.example.com/video.mp4" /><meta property="og:title" content="通用页面视频" />"#;
        assert_eq!(
            super::find_meta(html, "og:video:secure_url"),
            Some("https://cdn.example.com/video.mp4")
        );
        assert_eq!(super::find_meta(html, "og:title"), Some("通用页面视频"));
    }
}
