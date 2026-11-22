//! 82DJ 舞曲网（82dj.com）歌曲解析。
//!
//! 实测（2026-11-19）：play 页的音频 src 由前端 JS 计算（静态 HTML 零痕迹），
//! 走 WebView 代取通道读取 `audio.currentSrc`；直链防盗链需
//! `Referer: https://www.82dj.com/`（带 Referer 实测 206 / 31MB）。
//! CDN 内容为 MP4 容器（直链 .mp4 结尾、ftyp mp42）——文件名用 .mp4 扩展名，
//! .mp3 后缀会被 QuickTime 拒播。直链分钟级时效，过期由引擎 source_url
//! 救援机制自动重解析。下载（/down/{id}）需登录账号，notice 引导。
use super::helpers::clean_filename;

fn page_client() -> Option<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent(super::helpers::MOBILE_UA)
        .redirect(reqwest::redirect::Policy::limited(8))
        .build()
        .ok()
}

pub fn is_82dj(url: &str) -> bool {
    let lower = url.to_lowercase();
    lower.contains("82dj.com/play/") || lower.contains("82dj.com/down/")
}

pub async fn resolve(url: &str) -> Option<super::Extracted> {
    if !is_82dj(url) {
        return None;
    }
    let http = page_client()?;
    let resp = http.get(url).send().await.ok()?;
    let html = resp.text().await.ok()?;

    // 歌名：<title>歌名 在线试听,高音质MP3下载 - 82DJ舞曲网</title>
    let title = html
        .find("<title>")
        .and_then(|i| {
            let start = i + "<title>".len();
            let end = html[start..].find("</title>")? + start;
            Some(html[start..end].trim().to_string())
        })?
        .split(" 在线试听")
        .next()
        .unwrap_or("")
        .trim()
        .to_string();
    if title.is_empty() {
        return None;
    }

    // 音频 src 前端计算 → WebView 代取 play 页读 audio.currentSrc
    let plan = super::WebviewExtractPlan {
        url: url.trim_end_matches('/').to_string(),
        visible: false,
        init_js: None,
        js: r##"(()=>{const a=document.querySelector("audio");return a?JSON.stringify(a.currentSrc||a.src||""):"";})()"##.into(),
        timeout_ms: 15_000,
    };

    Some(super::Extracted {
        direct_url: String::new(), // add_task 时由 WebView 代取填充
        // CDN 内容实测为 MP4 容器（直链 .mp4 结尾），mp3 后缀会被 QuickTime 拒播
        filename: clean_filename(&title, "mp4"),
        referer: Some("https://www.82dj.com/".into()),
        notice: None,
        cookies: None,
        user_agent: None,
        webview_plan: Some(plan),
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_is_82dj() {
        assert!(super::is_82dj("https://www.82dj.com/play/31534"));
        assert!(super::is_82dj("https://www.82dj.com/down/31534"));
        assert!(!super::is_82dj("https://www.82dj.com/list/1"));
    }

    /// 真实歌曲页：解析歌名 + 产出 WebView 代取计划
    #[tokio::test]
    async fn test_82dj_real_song() {
        if std::env::var("FEILIU_NET_TEST").is_err() {
            return;
        }
        let info = super::super::resolve_retry("https://www.82dj.com/play/31534")
            .await
            .expect("82dj 解析失败");
        assert!(
            info.filename.contains("DJ") || info.filename.contains("串烧"),
            "应解析出歌名: {}",
            info.filename
        );
        let plan = info.webview_plan.expect("应产出 WebView 代取计划");
        assert!(plan.url.contains("/play/"), "plan 应指向 play 页");
        assert!(info.referer.is_some(), "应带 Referer（直链防盗链实测必需）");
        println!("82dj OK: {}", info.filename);
    }
}
