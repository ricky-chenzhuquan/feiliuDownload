//! 网易云音乐（music.163.com，含 163cn.tv 短链）解析。
//!
//! 开源参照（yt-dlp 内置提取器同款思路）：
//! - 歌曲详情：`/api/song/detail/?id=&ids=`（匿名）→ 歌名/歌手/专辑
//! - 免登录直链：`/song/media/outer/url?id={id}.mp3` 302 → 音频 CDN；
//!   **VIP 歌曲 302 → /404**（返回 HTML），以此区分并给出提示
//! - 歌单：`/api/playlist/detail?id=` 匿名（前 10 首），汇总 + 引导逐首解析
use super::helpers::{clean_filename, MOBILE_UA};

fn client() -> Option<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent(MOBILE_UA)
        .redirect(reqwest::redirect::Policy::limited(8))
        .build()
        .ok()
}

#[derive(Debug)]
enum NeteaseTarget {
    Song(String),
    Playlist(String),
}

/// 识别链接类型并提取 id（song / playlist）
fn parse_target(url: &str) -> Option<NeteaseTarget> {
    let lower = url.to_lowercase();
    let id_of = |key: &str| -> Option<String> {
        let needle = format!("{key}=");
        let idx = lower.find(&needle)?;
        let rest = &url[idx + needle.len()..];
        let id: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
        if id.is_empty() {
            None
        } else {
            Some(id)
        }
    };
    if lower.contains("/song") {
        id_of("id").map(NeteaseTarget::Song)
    } else if lower.contains("/playlist") {
        id_of("id").map(NeteaseTarget::Playlist)
    } else {
        None
    }
}

/// 163cn.tv 短链跟随重定向
async fn follow_short(http: &reqwest::Client, url: &str) -> String {
    if !lower_contains(url, "163cn.tv") {
        return url.to_string();
    }
    http.get(url).send().await.map(|r| r.url().to_string()).unwrap_or_else(|_| url.to_string())
}

fn lower_contains(s: &str, needle: &str) -> bool {
    s.to_lowercase().contains(needle)
}

/// 歌曲详情：歌名/歌手 → 文件名
async fn song_filename(http: &reqwest::Client, id: &str) -> Option<String> {
    let url = format!(
        "https://music.163.com/api/song/detail/?id={id}&ids=%5B{id}%5D"
    );
    let v: serde_json::Value = http.get(url).send().await.ok()?.json().await.ok()?;
    let song = v.get("songs")?.as_array()?.first()?.clone();
    let name = song.get("name").and_then(|n| n.as_str()).unwrap_or("歌曲");
    let artists: Vec<String> = song
        .get("artists")
        .and_then(|a| a.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|x| x.get("name").and_then(|n| n.as_str()))
                .map(|s| s.to_string())
                .collect()
        })
        .unwrap_or_default();
    let label = if artists.is_empty() {
        name.to_string()
    } else {
        format!("{} - {}", artists.join("/"), name)
    };
    Some(clean_filename(&label, "mp3"))
}

/// 免登录直链：302 → 音频 CDN（免费歌）或 /404（VIP 歌）
async fn song_direct_url(http: &reqwest::Client, id: &str) -> Option<String> {
    let resp = http
        .get(format!(
            "https://music.163.com/song/media/outer/url?id={id}.mp3"
        ))
        .send()
        .await
        .ok()?;
    let final_url = resp.url().to_string();
    let ctype = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    if final_url.contains("/404") || ctype.contains("text/html") {
        return None; // VIP 歌曲无匿名直链
    }
    if ctype.contains("audio") || final_url.contains("music.126.net") {
        Some(final_url)
    } else {
        None
    }
}

async fn resolve_song(id: &str) -> Option<super::Extracted> {
    let http = client()?;
    let filename = song_filename(&http, id).await?;
    match song_direct_url(&http, id).await {
        Some(direct) => Some(super::Extracted {
            direct_url: direct,
            filename,
            referer: None,
            notice: None,
            webview_plan: None,
        }),
        None => Some(super::Extracted {
            direct_url: String::new(),
            filename,
            referer: None,
            notice: Some(
                "网易云音乐：该歌曲为 VIP/付费歌曲，需登录会员账号才能获取音频直链（平台限制）".into(),
            ),
            webview_plan: None,
        }),
    }
}

/// 歌单内全部单曲页链接（匿名 API 返回前 N 首），供批量下载
pub async fn playlist_song_urls(url: &str) -> Option<(String, Vec<String>)> {
    let http = client()?;
    let page = follow_short(&http, url).await;
    let id = match parse_target(&page)? {
        NeteaseTarget::Playlist(id) => id,
        _ => return None,
    };
    let v: serde_json::Value = http
        .get(format!(
            "https://music.163.com/api/playlist/detail?id={id}"
        ))
        .send()
        .await
        .ok()?
        .json()
        .await
        .ok()?;
    let result = v.get("result")?;
    let name = result
        .get("name")
        .and_then(|n| n.as_str())
        .unwrap_or("歌单")
        .to_string();
    let tracks = result.get("tracks")?.as_array()?;
    let urls = tracks
        .iter()
        .filter_map(|t| t.get("id").and_then(|i| i.as_u64()))
        .map(|id| format!("https://music.163.com/song?id={id}"))
        .collect();
    Some((name, urls))
}

async fn resolve_playlist(id: &str) -> Option<super::Extracted> {
    let http = client()?;
    let v: serde_json::Value = http
        .get(format!(
            "https://music.163.com/api/playlist/detail?id={id}"
        ))
        .send()
        .await
        .ok()?
        .json()
        .await
        .ok()?;
    let result = v.get("result")?;
    let name = result.get("name").and_then(|n| n.as_str()).unwrap_or("歌单");
    let tracks = result.get("tracks").and_then(|t| t.as_array());
    let count = tracks.map(|t| t.len()).unwrap_or(0);
    let sample: Vec<String> = tracks
        .map(|t| {
            t.iter()
                .take(3)
                .filter_map(|x| x.get("name").and_then(|n| n.as_str()))
                .map(|s| s.to_string())
                .collect()
        })
        .unwrap_or_default();
    let sample_note = if sample.is_empty() {
        String::new()
    } else {
        format!("，如《{}》等", sample.join("》《"))
    };
    Some(super::Extracted {
        direct_url: String::new(),
        filename: format!("{name}（匿名可见 {count} 首{sample_note}）"),
        referer: None,
        notice: Some(
            "网易云音乐歌单：匿名可列出曲目，但歌曲音频需逐首解析下载（免费歌可直接下，VIP 歌曲需会员账号）。请把歌单里的单曲分享链接逐条粘贴到新建任务".into(),
        ),
        webview_plan: None,
    })
}

pub async fn resolve(url: &str) -> Option<super::Extracted> {
    let lower = url.to_lowercase();
    if !lower.contains("music.163.com") && !lower.contains("163cn.tv") {
        return None;
    }
    let http = client()?;
    let page = follow_short(&http, url).await;
    match parse_target(&page)? {
        NeteaseTarget::Song(id) => resolve_song(&id).await,
        NeteaseTarget::Playlist(id) => resolve_playlist(&id).await,
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_parse_target() {
        assert!(matches!(
            super::parse_target("https://music.163.com/song?id=5255640"),
            Some(super::NeteaseTarget::Song(ref id)) if id == "5255640"
        ));
        assert!(matches!(
            super::parse_target("https://music.163.com/playlist?id=13698367648"),
            Some(super::NeteaseTarget::Playlist(_))
        ));
        assert!(super::parse_target("https://example.com/foo").is_none());
    }

    /// 真实歌单短链：163cn.tv → 歌单结构汇总
    #[tokio::test]
    async fn test_netease_real_playlist() {
        if std::env::var("FEILIU_NET_TEST").is_err() {
            return;
        }
        let info = super::super::resolve("https://163cn.tv/bk5ZmvAb")
            .await
            .expect("网易云歌单解析失败");
        assert!(
            info.filename.contains("8090") || info.filename.contains("首"),
            "应解析出歌单摘要: {}",
            info.filename
        );
        assert!(info.notice.is_some(), "歌单应有逐首下载引导");
        println!("netease 歌单 OK: {}", info.filename);
    }

    /// 真实单曲（歌单第一首，免费）：详情 + 免登录直链
    #[tokio::test]
    async fn test_netease_real_song() {
        if std::env::var("FEILIU_NET_TEST").is_err() {
            return;
        }
        let info = super::super::resolve("https://music.163.com/song?id=5255640")
            .await
            .expect("网易云歌曲解析失败");
        assert!(
            info.filename.contains("黄昏") && info.filename.contains("周传雄"),
            "应解析出歌名与歌手: {}",
            info.filename
        );
        assert!(
            info.direct_url.contains("music.126.net"),
            "免费歌应有音频直链: {}",
            info.direct_url
        );
        println!("netease 歌曲 OK: {}", info.filename);
    }
}
