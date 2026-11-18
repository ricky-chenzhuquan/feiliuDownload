//! Bilibili：视频页（含 b23.tv 短链）→ view API → playurl html5 MP4 直链；
//! space 合集页（collectiondetail?sid=）→ seasons_archives_list 匿名列表 →
//! 结构摘要 + 一键批量（引擎逐个按单视频流程解析建任务）。
//!
//! 实测（2026-11-20）：`seasons_archives_list?mid=&season_id=&page_num=1&page_size=100`
//! 匿名全通（64 个合集视频一次拿全，含 bvid/title），需带 space Referer。
use serde::Deserialize;

use super::helpers::{clean_filename, json_client};

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

/// seasons_archives_list 响应（合集元信息 + 视频列表）
#[derive(Deserialize)]
struct BiliSeasonResp {
    code: i64,
    #[serde(default)]
    data: Option<BiliSeasonData>,
}

#[derive(Deserialize)]
struct BiliSeasonData {
    #[serde(default)]
    meta: Option<BiliSeasonMeta>,
    #[serde(default)]
    archives: Option<Vec<BiliSeasonArchive>>,
}

#[derive(Deserialize, Default)]
struct BiliSeasonMeta {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    total: Option<i64>,
}

#[derive(Deserialize)]
struct BiliSeasonArchive {
    bvid: String,
    #[serde(default)]
    title: Option<String>,
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

/// 识别 space 合集页并提取 (mid, sid)：
/// https://space.bilibili.com/{mid}/channel/collectiondetail?sid={sid}[&其他参数]
fn parse_collection(url: &str) -> Option<(String, String)> {
    let lower = url.to_lowercase();
    if !lower.contains("space.bilibili.com") || !lower.contains("collectiondetail") {
        return None;
    }
    let idx = lower.find("space.bilibili.com/")?;
    let rest = &url[idx + "space.bilibili.com/".len()..];
    let mid: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    if mid.is_empty() {
        return None;
    }
    let qpos = lower.find("?sid=").or_else(|| lower.find("&sid="))?;
    let sid: String = url[qpos + 5..].chars().take_while(|c| c.is_ascii_digit()).collect();
    if sid.is_empty() {
        return None;
    }
    Some((mid, sid))
}

/// 拉取合集列表（匿名 + space Referer；page_size=100 一次拿全）
async fn fetch_season(http: &reqwest::Client, mid: &str, sid: &str) -> Option<BiliSeasonResp> {
    http.get(format!(
        "https://api.bilibili.com/x/polymer/web-space/seasons_archives_list?mid={mid}&season_id={sid}&page_num=1&page_size=100"
    ))
    .header(reqwest::header::REFERER, "https://space.bilibili.com/")
    .send()
    .await
    .ok()?
    .json()
    .await
    .ok()
}

/// 合集页 → 结构摘要（合集名 + 数量 + 样例标题），notice 引导一键批量
async fn resolve_collection(
    http: &reqwest::Client,
    url: &str,
) -> Option<super::Extracted> {
    let (mid, sid) = parse_collection(url)?;
    let resp = fetch_season(http, &mid, &sid).await?;
    if resp.code != 0 {
        return Some(super::Extracted {
            direct_url: String::new(),
            filename: "B站合集".into(),
            referer: None,
            notice: Some(format!(
                "B站合集解析失败（接口 code={}）：合集可能不存在、已失效，或为「系列」类型页面",
                resp.code
            )),
            webview_plan: None,
        });
    }
    let data = resp.data?;
    let archives = data.archives?;
    if archives.is_empty() {
        return Some(super::Extracted {
            direct_url: String::new(),
            filename: "B站合集（空）".into(),
            referer: None,
            notice: Some("B站合集：该合集下没有可见视频".into()),
            webview_plan: None,
        });
    }
    let meta = data.meta.unwrap_or_default();
    let name = meta.name.unwrap_or_else(|| "合集".into());
    let total = meta.total.unwrap_or(archives.len() as i64);
    let sample: Vec<String> = archives
        .iter()
        .take(3)
        .filter_map(|a| a.title.clone())
        .collect();
    let sample_note = if sample.is_empty() {
        String::new()
    } else {
        format!("，如《{}》等", sample.join("》《"))
    };
    Some(super::Extracted {
        direct_url: String::new(),
        filename: clean_filename(&format!("{name}（{total} 个视频{sample_note}）"), "")
            .trim_end_matches('.')
            .to_string(),
        referer: Some("https://www.bilibili.com/".into()),
        notice: Some(
            "B站合集：可一键批量添加前 30 个视频（自动逐个解析直链建任务，上限 30 防直链过期）".into(),
        ),
        webview_plan: None,
    })
}

/// 合集内全部视频页链接（引擎批量逐个 add_task 用）
pub async fn collection_video_urls(url: &str) -> Option<(String, Vec<String>)> {
    let http = json_client()?;
    let (mid, sid) = parse_collection(url)?;
    let resp = fetch_season(&http, &mid, &sid).await?;
    if resp.code != 0 {
        return None;
    }
    let data = resp.data?;
    let archives = data.archives?;
    let name = data
        .meta
        .and_then(|m| m.name)
        .unwrap_or_else(|| "合集".into());
    let urls = archives
        .iter()
        .map(|a| format!("https://www.bilibili.com/video/{}", a.bvid))
        .collect();
    Some((name, urls))
}

pub async fn resolve(url: &str) -> Option<super::Extracted> {
    let http = json_client()?;
    // 合集页：结构摘要 + 批量引导（不产直链，由引擎逐个解析单视频）
    if parse_collection(url).is_some() {
        return resolve_collection(&http, url).await;
    }
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
            webview_plan: None,
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

    #[test]
    fn test_parse_collection() {
        assert_eq!(
            super::parse_collection(
                "https://space.bilibili.com/503344993/channel/collectiondetail?sid=416786"
            ),
            Some(("503344993".into(), "416786".into()))
        );
        // 带附加参数与 &sid= 形态
        assert_eq!(
            super::parse_collection(
                "https://space.bilibili.com/503344993/channel/collectiondetail?spm_id_from=1&sid=416786"
            ),
            Some(("503344993".into(), "416786".into()))
        );
        // 非合集页一律不识别
        assert_eq!(
            super::parse_collection("https://www.bilibili.com/video/BV1GJ411x7h7"),
            None
        );
        assert_eq!(
            super::parse_collection("https://space.bilibili.com/503344993"),
            None
        );
        assert_eq!(
            super::parse_collection("https://space.bilibili.com/503344993/channel/seriesdetail?sid=1"),
            None
        );
    }

    /// 真实合集（Python 数学建模系列讲座）：结构摘要 + 批量视频列表
    #[tokio::test]
    async fn test_bilibili_real_collection() {
        if std::env::var("FEILIU_NET_TEST").is_err() {
            return;
        }
        let url = "https://space.bilibili.com/503344993/channel/collectiondetail?sid=416786";
        let info = super::super::resolve(url)
            .await
            .expect("B站合集解析失败");
        assert!(
            info.filename.contains("个视频") && info.filename.contains("数学建模"),
            "应解析出合集名与视频数: {}",
            info.filename
        );
        assert!(info.notice.is_some(), "合集应有批量引导提示");
        let (name, urls) = super::collection_video_urls(url)
            .await
            .expect("合集视频列表获取失败");
        assert!(!name.is_empty(), "合集名应非空");
        assert!(urls.len() >= 30, "该合集应有大量视频: {}", urls.len());
        assert!(
            urls[0].contains("BV1oY4y147LF"),
            "首个视频 bvid 应正确: {}",
            urls[0]
        );
        println!("bilibili 合集 OK: {}（{} 个视频）", name, urls.len());
    }
}
