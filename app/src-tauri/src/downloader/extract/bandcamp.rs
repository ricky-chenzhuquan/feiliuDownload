//! Bandcamp 解析：{artist}.bandcamp.com 的专辑/曲目页 → 内嵌 data-tralbum JSON → mp3-128 流直链。
//!
//! 匿名全链路（实测 iksre.bandcamp.com：页面 200 → trackinfo[].file["mp3-128"]
//! → bcbits 流 206/真 MP3 头验证）。流地址带 ts 签名有时效，适合即取即下。
//! 曲目页直接给直链；专辑页列结构（所有曲目的流地址同在页面 JSON 里，
//! 后续轮可做一键批量）。
use serde_json::Value;

use super::helpers::clean_filename;

/// 桌面 Chrome UA：Bandcamp 页面对桌面 UA 返回完整 tralbum（实测移动 UA 会缺 artist 字段）
const DESKTOP_UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0 Safari/537.36";

fn client() -> Option<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent(DESKTOP_UA)
        .connect_timeout(std::time::Duration::from_secs(15))
        .build()
        .ok()
}

/// 提取 data-tralbum 属性值并做 HTML 实体反转义（属性内 & 为 &amp;、引号为 &quot;）
fn extract_tralbum(html_text: &str) -> Option<Value> {
    let marker = "data-tralbum=\"";
    let start = html_text.find(marker)? + marker.len();
    let rest = &html_text[start..];
    let end = rest.find('"')?;
    let raw = html_unescape(&rest[..end]);
    serde_json::from_str(&raw).ok()
}

/// HTML 实体反转义（页面属性值常用集合）
fn html_unescape(s: &str) -> String {
    s.replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

/// 专辑页 → (专辑名, 曲目页 URL 列表)。全部曲目的流地址就在专辑页 JSON 里，
/// 批量时逐首走单曲解析流程（流地址带 ts 签名有时效，不提前直链化）
pub async fn album_track_urls(url: &str) -> Option<(String, Vec<String>)> {
    let lower = url.to_lowercase();
    if !lower.contains("bandcamp.com") || !lower.contains("/album/") {
        return None;
    }
    let scheme_end = url.find("://")? + 3;
    let host_end = url[scheme_end..]
        .find('/')
        .map(|i| scheme_end + i)
        .unwrap_or(url.len());
    let base = &url[..host_end];
    let http = client()?;
    let page = http.get(url).send().await.ok()?.text().await.ok()?;
    let tralbum = extract_tralbum(&page)?;
    let artist = tralbum
        .get("artist")
        .and_then(|a| a.as_str())
        .unwrap_or("")
        .to_string();
    let album_title = tralbum
        .get("current")
        .and_then(|c| c.get("title"))
        .and_then(|t| t.as_str())
        .unwrap_or("专辑");
    let tracks = tralbum.get("trackinfo")?.as_array()?;
    let mut urls = Vec::new();
    for t in tracks {
        let Some(link) = t.get("title_link").and_then(|x| x.as_str()) else {
            continue; // 无独立曲目页（极少数本地曲目），批量时跳过
        };
        urls.push(format!("{}{}", base, link));
    }
    if urls.is_empty() {
        return None;
    }
    let label = if artist.is_empty() {
        album_title.to_string()
    } else {
        format!("{artist}《{album_title}》")
    };
    Some((label, urls))
}

pub async fn resolve(url: &str) -> Option<super::Extracted> {
    let lower = url.to_lowercase();
    if !lower.contains("bandcamp.com") || !(lower.contains("/album/") || lower.contains("/track/"))
    {
        return None;
    }
    let http = client()?;
    let page = http.get(url).send().await.ok()?.text().await.ok()?;
    let tralbum = extract_tralbum(&page)?;
    let artist = tralbum
        .get("artist")
        .and_then(|a| a.as_str())
        .unwrap_or("")
        .to_string();
    let tracks = tralbum.get("trackinfo")?.as_array()?.clone();

    // 曲目页 → 第一个带 mp3-128 流的曲目给直链
    let is_track_page = lower.contains("/track/");
    if is_track_page {
        for t in &tracks {
            let Some(stream) = t
                .get("file")
                .and_then(|f| f.get("mp3-128"))
                .and_then(|u| u.as_str())
            else {
                continue;
            };
            let title = t.get("title").and_then(|x| x.as_str()).unwrap_or("track");
            let filename = if artist.is_empty() {
                clean_filename(title, "mp3")
            } else {
                clean_filename(&format!("{artist} - {title}"), "mp3")
            };
            return Some(super::Extracted {
                direct_url: stream.to_string(),
                filename,
                referer: None,
                notice: None,
                cookies: None,
                user_agent: None,
                webview_plan: None,
            });
        }
        return None; // 曲目页但无可用流（付费专属等）
    }

    // 专辑页 → 列结构引导（同 github 多资产同款）
    let total = tracks.len();
    if total == 0 {
        return None;
    }
    let album_title = tralbum
        .get("current")
        .and_then(|c| c.get("title"))
        .and_then(|t| t.as_str())
        .unwrap_or("专辑");
    let sample: Vec<String> = tracks
        .iter()
        .filter_map(|t| t.get("title").and_then(|x| x.as_str()))
        .map(|s| s.to_string())
        .take(3)
        .collect();
    let sample_note = if sample.is_empty() {
        String::new()
    } else {
        format!("，如「{}」等", sample.join("」「"))
    };
    let label = if artist.is_empty() {
        album_title.to_string()
    } else {
        format!("{artist}《{album_title}》")
    };
    // clean_filename 恒追加 ".{ext}"，空扩展名会留尾点——先清洗再去尾点
    let label = clean_filename(&label, "").trim_end_matches('.').to_string();
    Some(super::Extracted {
        direct_url: String::new(),
        filename: format!("{label}（{total} 首曲目{sample_note}）").to_string(),
        referer: None,
        notice: Some(format!(
            "Bandcamp 专辑「{label}」含 {total} 首曲目{sample_note}。单曲流地址匿名可下：点「一键添加」按设置上限批量下载，或粘贴单曲页链接单独下载",
        )),
        cookies: None,
        user_agent: None,
        webview_plan: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_tralbum() {
        // 属性内 & 与引号经 HTML 转义，必须反转义后 JSON 才合法
        let html_text = r#"<script></script><div data-tralbum="{&quot;artist&quot;:&quot;A&amp;B&quot;,&quot;trackinfo&quot;:[{&quot;title&quot;:&quot;t1&quot;,&quot;file&quot;:{&quot;mp3-128&quot;:&quot;https://t4.bcbits.com/stream/x/mp3-128/1?p=0&amp;ts=1&amp;t=a&quot;}}]}"></div>"#;
        let v = extract_tralbum(html_text).expect("应解析成功");
        assert_eq!(v.get("artist").and_then(|a| a.as_str()), Some("A&B"));
        let url = v["trackinfo"][0]["file"]["mp3-128"].as_str().unwrap();
        assert!(url.contains("?p=0&ts=1&t=a"), "URL 内实体应还原: {url}");
    }

    /// 真实专辑（big-ups 编辑推荐在架专辑，长期存在）：列结构 + 引导
    #[tokio::test]
    async fn test_bandcamp_real_album() {
        if std::env::var("FEILIU_NET_TEST").is_err() {
            return;
        }
        let info = super::super::resolve_retry(
            "https://iksre.bandcamp.com/album/in-all-her-forms-sound-meditations-vol-2",
        )
        .await
        .expect("Bandcamp 专辑解析失败");
        assert!(info.notice.is_some(), "专辑页应列结构引导");
        assert!(
            info.filename.contains("曲目") && info.filename.contains("IKSRE"),
            "应含艺术家与曲目数: {}",
            info.filename
        );
        println!("bandcamp album OK: {}", info.filename);
    }

    /// 真实专辑批量：专辑名 + 全部曲目页 URL（批量命令的数据源）
    #[tokio::test]
    async fn test_bandcamp_album_batch_urls() {
        if std::env::var("FEILIU_NET_TEST").is_err() {
            return;
        }
        let (name, urls) = album_track_urls(
            "https://iksre.bandcamp.com/album/in-all-her-forms-sound-meditations-vol-2",
        )
        .await
        .expect("专辑批量解析失败");
        assert!(
            name.contains("IKSRE") && name.contains("in all her forms"),
            "专辑名应含艺术家与标题: {name}"
        );
        assert!(urls.len() >= 10, "应列出全部曲目: {}", urls.len());
        assert!(
            urls.iter().all(|u| u.contains("/track/")),
            "全部应为曲目页 URL: {:?}",
            &urls[..2.min(urls.len())]
        );
        println!("bandcamp batch OK: {name}（{} 首）", urls.len());
    }

    /// 真实曲目（上专辑第 1 首）：mp3-128 直链 + 艺术家文件名
    #[tokio::test]
    async fn test_bandcamp_real_track() {
        if std::env::var("FEILIU_NET_TEST").is_err() {
            return;
        }
        let info = super::super::resolve_retry("https://iksre.bandcamp.com/track/lake-shore")
            .await
            .expect("Bandcamp 曲目解析失败");
        assert!(
            info.direct_url.contains("bcbits.com/stream/"),
            "直链应为 bcbits 流: {}",
            info.direct_url
        );
        assert!(info.notice.is_none(), "曲目页应直接给直链");
        assert!(
            info.filename.contains("IKSRE") && info.filename.ends_with(".mp3"),
            "文件名应为 艺术家 - 曲目.mp3: {}",
            info.filename
        );
        println!("bandcamp track OK: {}", info.filename);
    }
}
