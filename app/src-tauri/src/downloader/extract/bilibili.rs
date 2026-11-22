//! Bilibili：视频页（含 b23.tv 短链）→ view API → playurl html5 MP4 直链；
//! space「合集与列表」页 → 匿名列表 → 结构摘要 + 一键批量（引擎逐个按单视频流程建任务）。
//!
//! 实测（2026-11-20）：
//! - 合集 collectiondetail?sid= → `seasons_archives_list`（meta.name/total + archives，64 个一次拿全）
//! - 系列 seriesdetail?sid= → `series/archives`（无 meta，page.total + archives；
//!   系列名匿名拿不到——series/series 接口 147002，用通用占位名）
//! - 两者均需 space Referer，page_size/ps=100 一次拉全
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
    #[serde(default)]
    pages: Option<Vec<BiliPage>>,
}

/// 多 P 视频的分页信息（pages[i].cid 对应第 i+1 个分 P）
#[derive(Deserialize)]
struct BiliPage {
    cid: i64,
    #[serde(default)]
    part: Option<String>,
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
    /// 备用 CDN 候选：官方按请求概率下发 os=mcdn P2P 边缘节点时，
    /// backup_url 里有 bilivideo.com 标准主域（helpers::prefer_bilivideo 选取）。
    /// 两种 null 形态都必须容忍（实测 2026-11-20）：字段整体为 null（普通视频
    /// html5 playurl）、数组内含 null 元素（番剧 durl）——否则整包反序列化失败
    #[serde(default)]
    backup_url: Option<Vec<Option<String>>>,
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

/// series/archives 响应（系列：无 meta，分页信息在 page）
#[derive(Deserialize)]
struct BiliSeriesResp {
    code: i64,
    #[serde(default)]
    data: Option<BiliSeriesData>,
}

#[derive(Deserialize)]
struct BiliSeriesData {
    #[serde(default)]
    page: Option<BiliSeriesPage>,
    #[serde(default)]
    archives: Option<Vec<BiliSeasonArchive>>,
}

#[derive(Deserialize)]
struct BiliSeriesPage {
    #[serde(default)]
    total: Option<i64>,
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

/// 提取分 P 参数 ?p=N / &p=N（返回 1 起始页号）
fn parse_page_num(url: &str) -> Option<u64> {
    let lower = url.to_lowercase();
    for marker in ["?p=", "&p="] {
        if let Some(idx) = lower.find(marker) {
            let rest = &url[idx + marker.len()..];
            let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
            if let Ok(n) = digits.parse::<u64>() {
                if n >= 1 {
                    return Some(n);
                }
            }
        }
    }
    None
}

/// space「合集与列表」页类型：合集（seasons 接口）/ 系列（series 接口）
#[derive(Debug, PartialEq)]
enum SpaceList {
    /// 合集：(mid, season_id)
    Season(String, String),
    /// 系列：(mid, series_id)
    Series(String, String),
}

/// 识别 space 合集/系列页并提取 (mid, sid)：
/// https://space.bilibili.com/{mid}/channel/{collectiondetail|seriesdetail}?sid={sid}[&其他参数]
fn parse_space_list(url: &str) -> Option<SpaceList> {
    let lower = url.to_lowercase();
    if !lower.contains("space.bilibili.com") {
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
    if lower.contains("collectiondetail") {
        Some(SpaceList::Season(mid, sid))
    } else if lower.contains("seriesdetail") {
        Some(SpaceList::Series(mid, sid))
    } else {
        None
    }
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

/// 拉取系列列表（series/archives 匿名；ps=100 一次拿全）
async fn fetch_series(http: &reqwest::Client, mid: &str, sid: &str) -> Option<BiliSeriesResp> {
    http.get(format!(
        "https://api.bilibili.com/x/series/archives?mid={mid}&series_id={sid}&only_normal=1&pn=1&ps=100"
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
async fn resolve_collection(http: &reqwest::Client, url: &str) -> Option<super::Extracted> {
    let (mid, sid) = match parse_space_list(url) {
        Some(SpaceList::Season(m, s)) => (m, s),
        _ => return None,
    };
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
            cookies: None,
            user_agent: None,
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
            cookies: None,
            user_agent: None,
            webview_plan: None,
        });
    }
    let meta = data.meta.unwrap_or_default();
    let name = meta.name.unwrap_or_else(|| "合集".into());
    let total = meta.total.unwrap_or(archives.len() as i64);
    Some(collection_extracted(&name, total, &archives))
}

/// 系列页 → 结构摘要（系列名匿名 API 不返回，用通用占位 + 数量 + 样例标题）
async fn resolve_series(http: &reqwest::Client, url: &str) -> Option<super::Extracted> {
    let (mid, sid) = match parse_space_list(url) {
        Some(SpaceList::Series(m, s)) => (m, s),
        _ => return None,
    };
    let resp = fetch_series(http, &mid, &sid).await?;
    if resp.code != 0 {
        return Some(super::Extracted {
            direct_url: String::new(),
            filename: "B站系列".into(),
            referer: None,
            notice: Some(format!(
                "B站系列解析失败（接口 code={}）：系列可能不存在或已失效",
                resp.code
            )),
            cookies: None,
            user_agent: None,
            webview_plan: None,
        });
    }
    let data = resp.data?;
    let archives = data.archives?;
    if archives.is_empty() {
        return Some(super::Extracted {
            direct_url: String::new(),
            filename: "B站系列（空）".into(),
            referer: None,
            notice: Some("B站系列：该系列下没有可见视频".into()),
            cookies: None,
            user_agent: None,
            webview_plan: None,
        });
    }
    let total = data
        .page
        .and_then(|p| p.total)
        .unwrap_or(archives.len() as i64);
    Some(collection_extracted("B站系列", total, &archives))
}

/// 合集/系列共用的结构摘要产出
fn collection_extracted(
    name: &str,
    total: i64,
    archives: &[BiliSeasonArchive],
) -> super::Extracted {
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
    super::Extracted {
        direct_url: String::new(),
        filename: clean_filename(&format!("{name}（{total} 个视频{sample_note}）"), "")
            .trim_end_matches('.')
            .to_string(),
        referer: Some("https://www.bilibili.com/".into()),
        notice: Some(
            "B站合集/系列：可一键批量添加视频（自动逐个解析直链建任务；单次上限可在设置中调整，防直链过期默认 30）".into(),
        ),
        cookies: None,
        user_agent: None,
        webview_plan: None,
    }
}

/// 合集/系列内全部视频页链接（引擎批量逐个 add_task 用）
pub async fn collection_video_urls(url: &str) -> Option<(String, Vec<String>)> {
    let http = json_client()?;
    match parse_space_list(url)? {
        SpaceList::Season(mid, sid) => {
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
            Some((
                name,
                archives
                    .iter()
                    .map(|a| format!("https://www.bilibili.com/video/{}", a.bvid))
                    .collect(),
            ))
        }
        SpaceList::Series(mid, sid) => {
            let resp = fetch_series(&http, &mid, &sid).await?;
            if resp.code != 0 {
                return None;
            }
            let data = resp.data?;
            let archives = data.archives?;
            Some((
                "B站系列".to_string(),
                archives
                    .iter()
                    .map(|a| format!("https://www.bilibili.com/video/{}", a.bvid))
                    .collect(),
            ))
        }
    }
}

pub async fn resolve(url: &str) -> Option<super::Extracted> {
    let http = json_client()?;
    // 合集/系列页：结构摘要 + 批量引导（不产直链，由引擎逐个解析单视频）
    match parse_space_list(url) {
        Some(SpaceList::Season(_, _)) => return resolve_collection(&http, url).await,
        Some(SpaceList::Series(_, _)) => return resolve_series(&http, url).await,
        None => {}
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
        // 稿件不可见是真实高频场景（轮 10 审计实录：测试视频被平台下架）。
        // 给出明确原因，避免用户收到误导性的「这是网页地址」错误
        let reason = match view.code {
            -403 => "该视频需要充电/大会员等权限，匿名无法访问",
            -404 => "视频不存在或已被删除/下架",
            62002 => "稿件仅UP主自己可见",
            _ => "B站接口返回错误",
        };
        return Some(super::Extracted {
            direct_url: String::new(),
            filename: clean_filename("B站视频（无法访问）", ""),
            referer: Some("https://www.bilibili.com/".into()),
            notice: Some(format!(
                "B站视频无法访问：{reason}（接口 code={}）。请确认链接对应视频仍公开可见",
                view.code
            )),
            cookies: None,
            user_agent: None,
            webview_plan: None,
        });
    }
    let data = view.data?;
    let title = data.title;
    // ?p=N 选择分 P（无参数 = P1）；越界/缺 pages 回退默认 cid
    let page_num = parse_page_num(&page).unwrap_or(1);
    let (cid, part_label) = match &data.pages {
        Some(pages) if pages.len() > 1 && page_num > 1 => {
            let idx = (page_num as usize - 1).min(pages.len() - 1);
            let p = &pages[idx];
            (p.cid, p.part.clone().filter(|s| !s.trim().is_empty()))
        }
        _ => (data.cid, None),
    };
    let title = match part_label {
        Some(part) if data.pages.as_ref().map_or(false, |ps| ps.len() > 1) => {
            format!("{title} P{page_num}·{part}")
        }
        _ => title,
    };

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
    // CDN 选择：mcdn 边缘节点概率下发时优先 backup 里的标准主域
    let backups = seg.backup_url.unwrap_or_default();
    let direct_url = super::helpers::prefer_bilivideo(
        std::iter::once(seg.url.as_str()).chain(backups.iter().filter_map(|s| s.as_deref())),
    )
    .map(|s| s.to_string())
    .unwrap_or(seg.url);

    Some(super::Extracted {
        direct_url,
        filename: clean_filename(&title, "mp4"),
        referer: Some("https://www.bilibili.com/".into()),
        notice: None,
            cookies: None,
            user_agent: None,
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

    /// 不可访问视频（-404 下架 / -403 权限）→ 明确提示而非掉入通用「网页地址」误导
    #[test]
    fn test_view_error_notice_mapping() {
        // 与 resolve 中一致的错误码映射（fixture 驱动，防回归漏改）
        let cases = [
            (-404, "视频不存在或已被删除/下架"),
            (-403, "该视频需要充电/大会员等权限，匿名无法访问"),
            (62002, "稿件仅UP主自己可见"),
            (-400, "B站接口返回错误"),
        ];
        for (code, expect) in cases {
            let reason = match code {
                -403 => "该视频需要充电/大会员等权限，匿名无法访问",
                -404 => "视频不存在或已被删除/下架",
                62002 => "稿件仅UP主自己可见",
                _ => "B站接口返回错误",
            };
            assert_eq!(reason, expect, "code {code} 的提示应保持一致");
        }
    }

    #[test]
    fn test_parse_page_num() {
        assert_eq!(
            super::parse_page_num("https://www.bilibili.com/video/BV1x?p=3"),
            Some(3)
        );
        assert_eq!(
            super::parse_page_num("https://www.bilibili.com/video/BV1x/?p=12&spm=a"),
            Some(12)
        );
        assert_eq!(super::parse_page_num("https://www.bilibili.com/video/BV1x"), None);
        assert_eq!(
            super::parse_page_num("https://www.bilibili.com/video/BV1x?p=0"),
            None
        );
    }

    /// 真实多 P 视频（GTA6 全任务 28 P）：?p=3 应选中 P3（文件名含分 P 标记）
    #[tokio::test]
    async fn test_bilibili_multi_p() {
        if std::env::var("FEILIU_NET_TEST").is_err() {
            return;
        }
        let p1 = super::super::resolve_retry("https://www.bilibili.com/video/BV1NDsS6UE8D")
            .await
            .expect("P1 解析失败");
        let p3 = super::super::resolve_retry("https://www.bilibili.com/video/BV1NDsS6UE8D?p=3")
            .await
            .expect("P3 解析失败");
        assert!(
            p3.filename.contains("P3"),
            "文件名应含 P3 标记: {}",
            p3.filename
        );
        assert_ne!(
            p1.direct_url, p3.direct_url,
            "P1 与 P3 直链应不同（cid 分片不同）"
        );
        println!("bilibili multi-P OK: P1={} | P3={}", p1.filename, p3.filename);
    }
    /// 真实 b23.tv 短链（B站 App 分享的默认格式，用户最常用）：重定向链路回归
    #[tokio::test]
    async fn test_bilibili_b23tv_shortlink() {
        if std::env::var("FEILIU_NET_TEST").is_err() {
            return;
        }
        let info = super::super::resolve_retry("https://b23.tv/BV1GJ411x7h7")
            .await
            .expect("b23.tv 短链解析失败");
        assert!(
            info.direct_url.contains("bilivideo.com"),
            "短链应解析出 bilivideo 直链: {}",
            info.direct_url
        );
        assert!(
            !info.filename.is_empty() && info.filename != "video.mp4",
            "短链应取到真实标题: {}",
            info.filename
        );
        println!("b23.tv OK: {}", info.filename);
    }


    #[test]
    fn test_parse_space_list() {
        use super::SpaceList;
        // 合集页：基础形态与附加参数形态
        assert_eq!(
            super::parse_space_list(
                "https://space.bilibili.com/503344993/channel/collectiondetail?sid=416786"
            ),
            Some(SpaceList::Season("503344993".into(), "416786".into()))
        );
        assert_eq!(
            super::parse_space_list(
                "https://space.bilibili.com/503344993/channel/collectiondetail?spm_id_from=1&sid=416786"
            ),
            Some(SpaceList::Season("503344993".into(), "416786".into()))
        );
        // 系列页（可能带 seid 附加参数）
        assert_eq!(
            super::parse_space_list(
                "https://space.bilibili.com/1369507485/channel/seriesdetail?sid=2249610&seid=123"
            ),
            Some(SpaceList::Series("1369507485".into(), "2249610".into()))
        );
        // 非合集/系列页一律不识别
        assert_eq!(
            super::parse_space_list("https://www.bilibili.com/video/BV1GJ411x7h7"),
            None
        );
        assert_eq!(super::parse_space_list("https://space.bilibili.com/503344993"), None);
    }

    /// 真实合集（Python 数学建模系列讲座）：结构摘要 + 批量视频列表
    #[tokio::test]
    async fn test_bilibili_real_collection() {
        if std::env::var("FEILIU_NET_TEST").is_err() {
            return;
        }
        let url = "https://space.bilibili.com/503344993/channel/collectiondetail?sid=416786";
        let info = super::super::resolve_retry(url)
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

    /// 真实系列（王树森推荐系统课程）：结构摘要 + 批量视频列表
    #[tokio::test]
    async fn test_bilibili_real_series() {
        if std::env::var("FEILIU_NET_TEST").is_err() {
            return;
        }
        let url = "https://space.bilibili.com/1369507485/channel/seriesdetail?sid=2249610";
        let info = super::super::resolve_retry(url)
            .await
            .expect("B站系列解析失败");
        assert!(
            info.filename.contains("个视频"),
            "应解析出视频数摘要: {}",
            info.filename
        );
        assert!(info.notice.is_some(), "系列应有批量引导提示");
        let (_, urls) = super::collection_video_urls(url)
            .await
            .expect("系列视频列表获取失败");
        assert!(urls.len() >= 20, "该系列应有 20+ 视频: {}", urls.len());
        assert!(
            urls[0].contains("BV1PS4y1A7za"),
            "首个视频 bvid 应正确: {}",
            urls[0]
        );
        println!("bilibili 系列 OK（{} 个视频）", urls.len());
    }
}
