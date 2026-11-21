//! 通用兜底：og:video / `<video src>`（任意含视频的网页，64KB 范围嗅探）。
use super::helpers::{clean_filename, MOBILE_UA};

/// 扫描页面全部 ld+json 块，找 @type=VideoObject 的 contentUrl
/// 提取首个 <audio src="http...">
fn find_audio_src(html: &str) -> Option<String> {
    let pos = html.find("<audio")?;
    let seg = &html[pos..(pos + 2000).min(html.len())];
    let i = seg.find("src=\"")?;
    let rest = &seg[i + "src=\"".len()..];
    let end = rest.find('"')?;
    let u = &rest[..end];
    u.starts_with("http").then(|| u.to_string())
}

fn find_jsonld_video(html: &str) -> Option<String> {
    let mut from = 0;
    while let Some(rel) = html[from..].find("application/ld+json") {
        let block_start = from + rel;
        let rest = &html[block_start..];
        let brace = rest.find('{')?;
        let bytes = rest[brace..].as_bytes();
        let mut depth = 0usize;
        let mut in_str = false;
        let mut esc = false;
        let mut end = None;
        for (i, &b) in bytes.iter().enumerate() {
            if in_str {
                if esc { esc = false; }
                else if b == b'\\' { esc = true; }
                else if b == b'"' { in_str = false; }
                continue;
            }
            match b {
                b'"' => in_str = true,
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 { end = Some(i); break; }
                }
                _ => {}
            }
        }
        let json_text = &rest[brace..brace + end? + 1];
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(json_text) {
            if v.get("@type").and_then(|t| t.as_str()) == Some("VideoObject") {
                if let Some(u) = v.get("contentUrl").and_then(|c| c.as_str()) {
                    if u.starts_with("http") {
                        return Some(u.to_string());
                    }
                }
            }
        }
        from = block_start + 20;
    }
    None
}

/// JSON-LD 区域内提取字段
fn find_jsonld_field<'a>(html: &'a str, key: &str) -> Option<&'a str> {
    let idx = html.find("application/ld+json")?;
    let region = &html[idx..(idx + 100_000).min(html.len())];
    let needle = format!("\"{}\":\"", key);
    region.find(&needle).and_then(|i| {
        let rest = &region[i + needle.len()..];
        let end = rest.find('"')?;
        Some(&rest[..end])
    })
}

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
            cookies: None,
            webview_plan: None,
                });
            }
        }
    }
    // JSON-LD VideoObject（schema.org 标准：新闻/博客站广泛使用）
    if let Some(u) = find_jsonld_video(&html) {
        let title = find_jsonld_field(&html, "name").unwrap_or("video");
        return Some(super::Extracted {
            direct_url: u,
            filename: clean_filename(title, "mp4"),
            referer: None,
            notice: None,
            cookies: None,
            webview_plan: None,
        });
    }
    // <audio ... src="...">（音乐博客等，audio 标签同样常见）
    if let Some(u) = find_audio_src(&html) {
        return Some(super::Extracted {
            direct_url: u,
            filename: clean_filename("page_audio", "mp3"),
            referer: None,
            notice: None,
            cookies: None,
            webview_plan: None,
        });
    }
    // <video ... src="...">
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
            cookies: None,
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
    fn test_jsonld_video_object() {
        let html = r#"<script type="application/ld+json">{"@context":"https://schema.org","@type":"VideoObject","name":"新闻视频","contentUrl":"https://cdn.news.com/video.mp4","thumbnailUrl":"x"}</script>"#;
        assert_eq!(
            super::find_jsonld_video(html),
            Some("https://cdn.news.com/video.mp4".into())
        );
        assert_eq!(super::find_jsonld_field(html, "name"), Some("新闻视频"));
    }

    #[test]
    fn test_jsonld_video_object_missing_type() {
        let html = r#"<script type="application/ld+json">{"@type":"Article","name":"不是视频"}</script>"#;
        assert_eq!(super::find_jsonld_video(html), None);
    }

    #[test]
    fn test_audio_tag() {
        let html = r#"<audio controls src="https://cdn.blog.com/song.mp3"></audio>"#;
        assert_eq!(
            super::find_audio_src(html),
            Some("https://cdn.blog.com/song.mp3".into())
        );
    }

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
