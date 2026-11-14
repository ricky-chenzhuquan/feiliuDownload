//! 站点解析层：把视频网页地址解析为可直接下载的媒体流地址。
//! 当前支持：bilibili.com 视频页（含 b23.tv 短链）。
//!
//! 原理：B 站 web-interface/view 拿 cid，再以 html5 平台请求
//! player/playurl，得到无需合并的单文件 MP4 直链（通常 720p 内）。

use serde::Deserialize;

/// 移动端 UA：让 playurl 返回单文件 mp4（durl）而不是 dash 分离流
const MOBILE_UA: &str = "Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1";

pub struct Extracted {
    pub direct_url: String,
    pub filename: String,
    pub referer: Option<String>,
}

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

/// 识别并解析；非支持的站点返回 None（走普通 HTTP 下载）
pub async fn resolve(url: &str) -> Option<Extracted> {
    let lower = url.to_lowercase();
    // b23.tv 短链也先放进来，resolve 时跟随重定向
    if !(lower.contains("bilibili.com/video") || lower.contains("b23.tv/")) {
        return None;
    }
    resolve_bilibili(url).await
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

/// b23.tv 短链跟随重定向拿真实视频页地址
async fn follow_short_link(http: &reqwest::Client, url: &str) -> String {
    if !url.to_lowercase().contains("b23.tv/") {
        return url.to_string();
    }
    match http.get(url).send().await {
        Ok(resp) => resp.url().to_string(),
        Err(_) => url.to_string(),
    }
}

async fn resolve_bilibili(url: &str) -> Option<Extracted> {
    let http = reqwest::Client::builder().user_agent(MOBILE_UA).build().ok()?;
    let page = follow_short_link(&http, url).await;
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
    // 单分段常见；多段时取第一段（html5 durl 通常为单文件）
    let seg = play.data?.durl?.into_iter().next()?;
    let direct_url = seg.url;

    // 标题清洗为文件名（去掉文件系统非法字符）
    let cleaned: String = title
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => ' ',
            _ => c,
        })
        .collect();
    let cleaned = cleaned.trim().to_string();
    let filename = if cleaned.is_empty() {
        format!("bilibili_{bvid}.mp4")
    } else {
        format!("{cleaned}.mp4")
    };

    Some(Extracted {
        direct_url,
        filename,
        referer: Some("https://www.bilibili.com/".into()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_bvid() {
        assert_eq!(
            parse_bvid("https://www.bilibili.com/video/BV1kMWz6CESY/?share_source=copy_web"),
            Some("BV1kMWz6CESY".into())
        );
        assert_eq!(parse_bvid("https://example.com/foo"), None);
    }
}
