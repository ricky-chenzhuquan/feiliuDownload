//! B 站番剧（bangumi）解析：ss/ep 链接 → pgc API → 单集 MP4 直链 / 整季批量。
//!
//! 匿名链路（实测 ss2722《学战都市Asterisk》12 集）：season API 匿名全通 →
//! playurl（platform=html5）返回**单文件** MP4 durl（206/ftyp 头实测）——
//! 番剧匿名画质上限 360p（高画质需登录，诚实现状）；直链需 Referer
//! （与普通 B 站视频一致）。ss 页 = 整季（批量，上限走设置），ep 页 = 单集直下。
use serde_json::Value;

use super::helpers::clean_filename;

pub const BILI_REFERER: &str = "https://www.bilibili.com/";
const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0 Safari/537.36";

fn client() -> Option<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent(UA)
        .build()
        .ok()
}

/// 提取 ss / ep id：/bangumi/play/ss123 或 /bangumi/play/ep456
fn parse_season_ep(url: &str) -> (Option<u64>, Option<u64>) {
    let grab = |marker: &str| -> Option<u64> {
        let idx = url.find(marker)?;
        let rest = &url[idx + marker.len()..];
        let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
        digits.parse().ok()
    };
    (grab("ss"), grab("ep"))
}

async fn fetch_season(http: &reqwest::Client, sid: Option<u64>, epid: Option<u64>) -> Option<Value> {
    let query = match (sid, epid) {
        (Some(s), _) => format!("season_id={s}"),
        (None, Some(e)) => format!("ep_id={e}"),
        _ => return None,
    };
    let resp = http
        .get(format!("https://api.bilibili.com/pgc/view/web/season?{query}"))
        .header("Referer", BILI_REFERER)
        .send()
        .await
        .ok()?;
    let v: Value = resp.json().await.ok()?;
    if v.get("code").and_then(|c| c.as_i64()) != Some(0) {
        return None; // -404 等
    }
    v.get("result").cloned()
}

/// 单集 playurl 结果：可下直链 / 会员专享（匿名仅预览）/ 无流
enum EpFetch {
    Stream(String),
    Preview,
    NoStream,
}

async fn fetch_episode(http: &reqwest::Client, ep_id: u64) -> EpFetch {
    let Ok(resp) = http
        .get(format!(
            "https://api.bilibili.com/pgc/player/web/playurl?ep_id={ep_id}&qn=64&fnval=0&platform=html5&high_quality=1"
        ))
        .header("Referer", BILI_REFERER)
        .send()
        .await
    else {
        return EpFetch::NoStream;
    };
    let Ok(v) = resp.json::<Value>().await else {
        return EpFetch::NoStream;
    };
    if v.get("code").and_then(|c| c.as_i64()) != Some(0) {
        return EpFetch::NoStream;
    }
    let Some(result) = v.get("result") else {
        return EpFetch::NoStream;
    };
    // 会员专享集：匿名只能拿到试看预览片段——诚实提示而非静默失败
    if result.get("is_preview").and_then(|p| p.as_i64()) == Some(1) {
        return EpFetch::Preview;
    }
    // 单文件 mp4 才可用（引擎不做分片合并）
    if result.get("type").and_then(|t| t.as_str()) != Some("MP4") {
        return EpFetch::NoStream;
    }
    match result
        .get("durl")
        .and_then(|d| d.as_array())
        .and_then(|a| a.first())
        .and_then(|seg| seg.get("url"))
        .and_then(|u| u.as_str())
    {
        Some(u) => EpFetch::Stream(u.to_string()),
        None => EpFetch::NoStream,
    }
}

/// 集是否付费（badge 非空 = 会员/预告/限定等）
fn is_paid(ep: &Value) -> bool {
    ep.get("badge")
        .and_then(|b| b.as_str())
        .map_or(false, |b| !b.trim().is_empty())
}

/// 集显示名：long_title 优先（如「华焰之魔女」），否则 title（如「1」），否则 ep{id}
fn ep_label(ep: &Value, idx: usize) -> String {
    let long = ep
        .get("long_title")
        .and_then(|t| t.as_str())
        .filter(|s| !s.trim().is_empty());
    if let Some(t) = long {
        return t.to_string();
    }
    let title = ep
        .get("title")
        .and_then(|t| t.as_str())
        .filter(|s| !s.trim().is_empty());
    match title {
        Some(t) => t.to_string(),
        None => format!("EP{}", idx + 1),
    }
}

pub async fn resolve(url: &str) -> Option<super::Extracted> {
    let lower = url.to_lowercase();
    if !lower.contains("bilibili.com/bangumi/play/") {
        return None;
    }
    let (sid, epid) = parse_season_ep(url);
    if sid.is_none() && epid.is_none() {
        return None;
    }
    let http = client()?;
    let season = fetch_season(&http, sid, epid).await?;
    let title = season
        .get("title")
        .and_then(|t| t.as_str())
        .unwrap_or("番剧")
        .to_string();
    let episodes = season.get("episodes")?.as_array()?.clone();

    // 整季页 → 列结构 + 批量引导（badge 非空的会员/预告集不计入可下集合）
    if lower.contains("/ss") && epid.is_none() {
        let total = episodes.len();
        if total == 0 {
            return None;
        }
        let free: Vec<&Value> = episodes.iter().filter(|e| !is_paid(e)).collect();
        let paid = total - free.len();
        let sample: Vec<String> = free
            .iter()
            .enumerate()
            .take(3)
            .map(|(i, e)| ep_label(e, i))
            .collect();
        let sample_note = if sample.is_empty() {
            String::new()
        } else {
            format!("，免费集如「{}」等", sample.join("」「"))
        };
        let paid_note = if paid > 0 {
            format!("，另有 {paid} 集会员专享")
        } else {
            String::new()
        };
        return Some(super::Extracted {
            direct_url: String::new(),
            filename: format!(
                "{}（{}/{} 集免费{paid_note}）",
                clean_filename(&title, ""),
                free.len(),
                total
            )
            .trim_end_matches('.')
            .to_string(),
            referer: Some(BILI_REFERER.to_string()),
            notice: Some(format!(
                "B站番剧「{title}」共 {total} 集{paid_note}{sample_note}。免费集匿名可下（画质以登录状态为准）：点「一键添加」按设置上限批量下载，或粘贴单集页链接单独下载",
            )),
            webview_plan: None,
        });
    }

    // 单集页 → playurl 直链
    let target_ep = epid?;
    let ep = episodes
        .iter()
        .enumerate()
        .find(|(_, e)| e.get("id").and_then(|i| i.as_u64()) == Some(target_ep));
    let label = match &ep {
        Some((i, e)) => ep_label(e, *i),
        None => format!("EP{target_ep}"),
    };
    match fetch_episode(&http, target_ep).await {
        EpFetch::Stream(direct) => Some(super::Extracted {
            direct_url: direct,
            filename: clean_filename(&format!("{title} - {label}"), "mp4"),
            referer: Some(BILI_REFERER.to_string()),
            notice: None,
            webview_plan: None,
        }),
        EpFetch::Preview => Some(super::Extracted {
            direct_url: String::new(),
            filename: clean_filename(&format!("{title} - {label}"), "mp4"),
            referer: Some(BILI_REFERER.to_string()),
            notice: Some(format!(
                "「{title} - {label}」为会员专享剧集，匿名只能获取试看预览片段，无法完整下载（登录大会员后可看）"
            )),
            webview_plan: None,
        }),
        EpFetch::NoStream => None,
    }
}

/// 整季 → (番剧名, 单集页 URL 列表)；批量时逐集走单集流程
pub async fn season_ep_urls(url: &str) -> Option<(String, Vec<String>)> {
    let lower = url.to_lowercase();
    if !lower.contains("bilibili.com/bangumi/play/") {
        return None;
    }
    let (sid, _) = parse_season_ep(url);
    if sid.is_none() {
        return None;
    }
    let http = client()?;
    let season = fetch_season(&http, sid, None).await?;
    let title = season
        .get("title")
        .and_then(|t| t.as_str())
        .unwrap_or("番剧")
        .to_string();
    let episodes = season.get("episodes")?.as_array()?;
    // 批量只取免费集（badge 为空）：会员/预告集 playurl 只出预览片段，加进去必然失败
    let urls: Vec<String> = episodes
        .iter()
        .filter(|e| !is_paid(e))
        .filter_map(|e| e.get("id").and_then(|i| i.as_u64()))
        .map(|id| format!("https://www.bilibili.com/bangumi/play/ep{id}"))
        .collect();
    if urls.is_empty() {
        return None;
    }
    Some((title, urls))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_season_ep() {
        assert_eq!(
            parse_season_ep("https://www.bilibili.com/bangumi/play/ss2722/?spm=x"),
            (Some(2722), None)
        );
        assert_eq!(
            parse_season_ep("https://www.bilibili.com/bangumi/play/ep70866"),
            (None, Some(70866))
        );
        assert_eq!(parse_season_ep("https://www.bilibili.com/video/BV1x"), (None, None));
    }

    /// 真实番剧（学战都市Asterisk，长期在架）：整季列表
    #[tokio::test]
    async fn test_bangumi_real_season() {
        if std::env::var("FEILIU_NET_TEST").is_err() {
            return;
        }
        let (name, urls) = season_ep_urls("https://www.bilibili.com/bangumi/play/ss2722")
            .await
            .expect("番剧解析失败");
        assert!(name.contains("Asterisk"), "应取到番剧名: {name}");
        // 12 集中 11 集为「会员」专享（badge 非空），批量只应包含免费的 EP1
        assert_eq!(urls.len(), 1, "批量应只含免费集: {urls:?}");
        assert!(urls[0].contains("ep70866"), "免费集应为 EP1: {}", urls[0]);
        println!("bangumi season OK: {name}（{} 集免费）", urls.len());
    }

    /// 真实单集：匿名 MP4 直链（完整剧集非预览）
    #[tokio::test]
    async fn test_bangumi_real_episode() {
        if std::env::var("FEILIU_NET_TEST").is_err() {
            return;
        }
        let info = super::super::resolve_retry("https://www.bilibili.com/bangumi/play/ep70866")
            .await
            .expect("番剧单集解析失败");
        assert!(
            info.direct_url.contains("bilivideo.com"),
            "直链应为 bilivideo: {}",
            info.direct_url
        );
        assert!(info.notice.is_none(), "单集应直接给直链");
        assert!(
            info.filename.contains("学战都市Asterisk"),
            "文件名应含番剧名: {}",
            info.filename
        );
        assert_eq!(info.referer.as_deref(), Some(BILI_REFERER), "直链必须带 Referer");
        println!("bangumi episode OK: {}", info.filename);
    }

    /// 真实会员专享集（学战都市 EP2，badge=会员）：诚实提示而非静默失败
    #[tokio::test]
    async fn test_bangumi_paid_episode_notice() {
        if std::env::var("FEILIU_NET_TEST").is_err() {
            return;
        }
        let info = super::super::resolve_retry("https://www.bilibili.com/bangumi/play/ep72067")
            .await
            .expect("会员集解析失败");
        let notice = info.notice.clone().unwrap_or_default();
        assert!(
            notice.contains("会员"),
            "会员集应给出明确提示: {notice}"
        );
        assert!(
            info.direct_url.is_empty(),
            "会员集不应产出直链: {}",
            info.direct_url
        );
        println!("bangumi paid OK: {}", info.filename);
    }
}
