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
    // B 站（含 b23.tv 短链）
    if lower.contains("bilibili.com/video") || lower.contains("b23.tv/") {
        return resolve_bilibili(url).await;
    }
    // 抖音（含 v.douyin.com 短链）
    if lower.contains("douyin.com") || lower.contains("iesdouyin.com") {
        return resolve_douyin(url).await;
    }
    // 快手
    if lower.contains("kuaishou.com") || lower.contains("chenzhongtech.com") {
        return resolve_kuaishou(url).await;
    }
    // 小红书
    if lower.contains("xiaohongshu.com") || lower.contains("xhslink.com") {
        return resolve_xhs(url).await;
    }
    // 微博
    if lower.contains("weibo.c") || lower.contains("weibo.com") {
        return resolve_weibo(url).await;
    }
    // 通用兜底：og:video / <video src>（任意含视频的网页）
    resolve_generic(url).await
}

/// 短链跟随重定向拿最终地址（各平台通用）
async fn follow_redirect(http: &reqwest::Client, url: &str) -> Option<String> {
    let resp = http.get(url).send().await.ok()?;
    Some(resp.url().to_string())
}

/// 从 HTML 中提取 `window.XXX = {...};` 形式的 JSON
fn extract_json_after(html: &str, marker: &str) -> Option<serde_json::Value> {
    let idx = html.find(marker)?;
    let rest = &html[idx + marker.len()..];
    // 跳过赋值符号与空白
    let rest = rest.trim_start();
    let rest = rest
        .strip_prefix('=')
        .map(|r| r.trim_start())
        .unwrap_or(rest);
    // 平衡扫描 JSON 边界（对象或数组）
    let bytes = rest.as_bytes();
    let (open, close) = match bytes.first()? {
        b'{' => (b'{', b'}'),
        b'[' => (b'[', b']'),
        _ => return None,
    };
    let mut depth = 0usize;
    let mut in_str = false;
    let mut esc = false;
    for (i, &b) in bytes.iter().enumerate() {
        if in_str {
            if esc {
                esc = false;
            } else if b == b'\\' {
                esc = true;
            } else if b == b'"' {
                in_str = false;
            }
            continue;
        }
        match b {
            b'"' => in_str = true,
            b if b == open => depth += 1,
            b if b == close => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return serde_json::from_slice(&bytes[..=i]).ok();
                }
            }
            _ => {}
        }
    }
    None
}

/// 递归查找 JSON 中首个指定 key 的值
fn find_key<'a>(v: &'a serde_json::Value, key: &str) -> Option<&'a serde_json::Value> {
    match v {
        serde_json::Value::Object(m) => {
            if let Some(x) = m.get(key) {
                return Some(x);
            }
            for (_, x) in m {
                if let Some(found) = find_key(x, key) {
                    return Some(found);
                }
            }
            None
        }
        serde_json::Value::Array(a) => a.iter().find_map(|x| find_key(x, key)),
        _ => None,
    }
}

/// 递归查找 JSON 中首个字符串值（key 匹配任意以 suffix 结尾或 contains）
fn find_str<'a>(v: &'a serde_json::Value, contains: &str) -> Option<&'a str> {
    match v {
        serde_json::Value::Object(m) => {
            for (k, x) in m {
                if k.contains(contains) {
                    if let Some(s) = x.as_str() {
                        return Some(s);
                    }
                }
                if let Some(f) = find_str(x, contains) {
                    return Some(f);
                }
            }
            None
        }
        serde_json::Value::Array(a) => a.iter().find_map(|x| find_str(x, contains)),
        _ => None,
    }
}

fn clean_filename(title: &str, ext: &str) -> String {
    let cleaned: String = title
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '\n' | '\r' => ' ',
            _ => c,
        })
        .collect();
    let cleaned = cleaned.trim().to_string();
    if cleaned.is_empty() {
        format!("video_{:?}.{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0), ext)
    } else {
        let truncated: String = cleaned.chars().take(80).collect();
        format!("{truncated}.{ext}")
    }
}

// ---------- 抖音（去水印） ----------

/// 抖音 ttwid：注册接口 → 回调 Set-Cookie。会话内缓存（有效期一年）。
async fn douyin_ttwid() -> Option<String> {
    if let Some(t) = DOUYIN_TTWID.get() {
        return Some(t.clone());
    }
    let http = reqwest::Client::builder()
        .user_agent(MOBILE_UA)
        .build()
        .ok()?;
    let resp = http
        .post("https://ttwid.bytedance.com/ttwid/union/register/")
        .json(&serde_json::json!({
            "region": "cn",
            "aid": 1768,
            "needFid": false,
            "service": "www.ixigua.com",
            "migrate_info": {"ticket": "", "source": "node"},
            "cbUrlProtocol": "https",
            "union": true
        }))
        .send()
        .await
        .ok()?;
    let redirect = resp
        .json::<serde_json::Value>()
        .await
        .ok()?
        .get("redirect_url")?
        .as_str()?
        .to_string();
    let resp2 = http.get(&redirect).send().await.ok()?;
    for v in resp2.headers().get_all(reqwest::header::SET_COOKIE) {
        if let Ok(sv) = v.to_str() {
            if let Some(idx) = sv.find("ttwid=") {
                let val = sv[idx + 6..].split(';').next()?.to_string();
                let _ = DOUYIN_TTWID.set(val.clone());
                return Some(val);
            }
        }
    }
    None
}

static DOUYIN_TTWID: std::sync::OnceLock<String> = std::sync::OnceLock::new();

async fn resolve_douyin(url: &str) -> Option<Extracted> {
    let http = reqwest::Client::builder()
        .user_agent(MOBILE_UA)
        .redirect(reqwest::redirect::Policy::limited(8))
        .build()
        .ok()?;
    // v.douyin.com 短链 / iesdouyin 分享页 → 归一化
    let target = if url.to_lowercase().contains("v.douyin.com")
        || url.to_lowercase().contains("iesdouyin.com")
    {
        follow_redirect(&http, url).await.unwrap_or_else(|| url.to_string())
    } else {
        url.to_string()
    };
    // 取视频数字 ID（/video/{digits}）
    let id: String = {
        let idx = target.find("/video/")?;
        let rest = &target[idx + "/video/".len()..];
        let mut id = String::new();
        for c in rest.chars() {
            if c.is_ascii_digit() {
                id.push(c);
            } else if !id.is_empty() {
                break;
            }
        }
        if id.len() < 15 {
            return None;
        }
        id
    };

    // iesdouyin 分享页：需携带 ttwid Cookie 才会在 SSR 中内嵌视频数据
    let ttwid = douyin_ttwid().await;
    let mut req = http.get(format!("https://www.iesdouyin.com/share/video/{id}/"));
    if let Some(t) = &ttwid {
        req = req.header(reqwest::header::COOKIE, format!("ttwid={t}"));
    }
    let html = req.send().await.ok()?.text().await.ok()?;
    let data = extract_json_after(&html, "window._ROUTER_DATA")?;
    let item = find_key(&data, "item_list")?.as_array()?.first()?.clone();

    // 描述 → 文件名
    let desc = item
        .get("desc")
        .and_then(|d| d.as_str())
        .unwrap_or("douyin_video")
        .to_string();

    // play_addr 的 url_list 第一个；playwm→play = 去水印
    let play_addr = find_key(&item, "play_addr")?;
    let raw = find_str(play_addr, "url_list")
        .map(|s| s.to_string())
        .or_else(|| {
            play_addr
                .get("url_list")
                .and_then(|l| l.as_array())
                .and_then(|a| a.first())
                .and_then(|u| u.as_str())
                .map(|s| s.to_string())
        })?;
    // playwm → play：取原始无水印流
    let mut nowm = raw.replace("/playwm/", "/play/");
    if !nowm.contains('?') {
        nowm.push_str("?ratio=720p&line=0");
    }

    Some(Extracted {
        direct_url: nowm,
        filename: clean_filename(&desc, "mp4"),
        referer: None,
    })
}

// ---------- 快手 ----------

async fn resolve_kuaishou(url: &str) -> Option<Extracted> {
    let http = reqwest::Client::builder()
        .user_agent(MOBILE_UA)
        .redirect(reqwest::redirect::Policy::limited(8))
        .build()
        .ok()?;
    let target = follow_redirect(&http, url).await.unwrap_or_else(|| url.to_string());
    // short-video 页
    if !target.contains("kuaishou.com") && !target.contains("chenzhongtech.com") {
        return None;
    }
    let html = http.get(&target).send().await.ok()?.text().await.ok()?;
    // __APOLLO_STATE__ 中递归找 photoUrl（源文件地址，通常无水印）
    let data = extract_json_after(&html, "__APOLLO_STATE__")?;
    let photo_url = find_str(&data, "photoUrl")?.to_string();
    if !photo_url.starts_with("http") {
        return None;
    }
    // 标题：找 caption 字段
    let caption = find_str(&data, "caption").unwrap_or("kuaishou_video").to_string();
    Some(Extracted {
        direct_url: photo_url,
        filename: clean_filename(&caption, "mp4"),
        referer: Some("https://www.kuaishou.com/".into()),
    })
}

// ---------- 小红书 ----------

async fn resolve_xhs(url: &str) -> Option<Extracted> {
    let http = reqwest::Client::builder()
        .user_agent(MOBILE_UA)
        .redirect(reqwest::redirect::Policy::limited(8))
        .build()
        .ok()?;
    let target = follow_redirect(&http, url).await.unwrap_or_else(|| url.to_string());
    if !target.contains("xiaohongshu.com") {
        return None;
    }
    let html = http.get(&target).send().await.ok()?.text().await.ok()?;
    let data = extract_json_after(&html, "__INITIAL_STATE__")?;
    // 视频地址：masterUrl（无水印原片）；图文笔记无 masterUrl → 返回 None
    let master = find_str(&data, "masterUrl")?.to_string();
    if !master.starts_with("http") {
        return None;
    }
    let desc = find_str(&data, "desc").unwrap_or("xhs_video").to_string();
    Some(Extracted {
        direct_url: master,
        filename: clean_filename(&desc, "mp4"),
        referer: Some("https://www.xiaohongshu.com/".into()),
    })
}

// ---------- 微博 ----------

async fn resolve_weibo(url: &str) -> Option<Extracted> {
    let http = reqwest::Client::builder()
        .user_agent(MOBILE_UA)
        .redirect(reqwest::redirect::Policy::limited(8))
        .build()
        .ok()?;
    let target = if url.contains("m.weibo.cn") {
        url.to_string()
    } else if let Some(i) = url.find("/status/") {
        // m.weibo.cn/status/{id} 或 weibo.com/status/{id}
        let rest = &url[i + "/status/".len()..];
        let id: String = rest
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric())
            .collect();
        if id.is_empty() {
            return None;
        }
        format!("https://m.weibo.cn/status/{id}")
    } else {
        // 新版 weibo.com 分享：weibo.com/{用户id}/{博文bid}
        let path = url.split(['?', '#']).next().unwrap_or(url);
        let segs: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        if segs.len() >= 3 && segs[1].chars().all(|c| c.is_ascii_digit()) {
            format!("https://m.weibo.cn/status/{}", segs[2])
        } else {
            return None; // 登录页等无效链接
        }
    };
    let html = http.get(&target).send().await.ok()?.text().await.ok()?;
    let data = extract_json_after(&html, "$render_data = ")?;
    // page_info.media_info.stream_url（无水印 mp4）
    let stream = find_str(&data, "stream_url")?.to_string();
    if !stream.starts_with("http") {
        return None;
    }
    let title = find_str(&data, "status_title")
        .or_else(|| find_str(&data, "text"))
        .and_then(|t| {
            // 去掉 HTML 标签
            Some(
                t.to_string()
                    .replace("<br />", " ")
                    .replace("<br/>", " "),
            )
        })
        .map(|t| {
            t.chars()
                .filter(|c| *c != '<' && *c != '>')
                .collect::<String>()
        })
        .unwrap_or_else(|| "weibo_video".into());
    Some(Extracted {
        direct_url: stream,
        filename: clean_filename(&title, "mp4"),
        referer: Some("https://m.weibo.cn/".into()),
    })
}

// ---------- 通用兜底：og:video / <video src> ----------

async fn resolve_generic(url: &str) -> Option<Extracted> {
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
    // og:video 系列优先
    for prop in ["og:video:secure_url", "og:video:url", "og:video"] {
        if let Some(u) = find_meta(&html, prop) {
            if u.starts_with("http") {
                let title = find_meta(&html, "og:title").unwrap_or("video");
                return Some(Extracted {
                    direct_url: u.to_string(),
                    filename: clean_filename(title, "mp4"),
                    referer: None,
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
                    return Some(Extracted {
                        direct_url: u.to_string(),
                        filename: clean_filename("page_video", "mp4"),
                        referer: None,
                    });
                }
            }
        }
    }
    None
}

fn find_meta<'a>(html: &'a str, prop: &str) -> Option<&'a str> {
    // <meta property="og:video" content="...">
    let needle = format!("{prop}\" content=\"");
    let idx = html.find(&needle)?;
    let rest = &html[idx + needle.len()..];
    let end = rest.find('"')?;
    Some(&rest[..end])
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
async fn probe_total(url: &str) -> u64 {
    let client = reqwest::Client::new();
    if let Ok(resp) = client
        .get(url)
        .header(reqwest::header::RANGE, "bytes=0-0")
        .send()
        .await
    {
        if let Some(cr) = resp.headers().get(reqwest::header::CONTENT_RANGE) {
            if let Ok(s) = cr.to_str() {
                if let Some(t) = s.rsplit('/').next() {
                    return t.parse().unwrap_or(0);
                }
            }
        }
    }
    0
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

    #[test]
    fn test_router_data_extract() {
        let html = r#"<script>window._ROUTER_DATA = {"loaderData":{"video_(1234)/page":{"video_info_res":{"item_list":[{"desc":"测试视频标题","video":{"play_addr":{"uri":"v0300","url_list":["https://www.douyin.com/aweme/v1/play/?video_id=v0300&ratio=720p&line=0"]}}}]}}}};</script>"#;
        let data = extract_json_after(html, "window._ROUTER_DATA").expect("json 提取失败");
        let item = find_key(&data, "item_list").unwrap().as_array().unwrap()[0].clone();
        assert_eq!(item.get("desc").unwrap(), "测试视频标题");
        let raw = find_key(&item, "play_addr")
            .and_then(|pa| pa.get("url_list"))
            .and_then(|l| l.as_array())
            .and_then(|a| a.first())
            .and_then(|u| u.as_str())
            .unwrap();
        assert!(raw.contains("aweme/v1/play"));
    }

    #[test]
    fn test_apollo_state_extract() {
        let html = r#"<script>window.__APOLLO_STATE__ = {"defaultClient":{"$ROOT_QUERY.shortVideo:meta(\"id\")":{"photo":{"caption":"快手测试","photoUrl":"https://photo.kuaishou.com/x.mp4"}}}};</script>"#;
        let data = extract_json_after(html, "__APOLLO_STATE__").expect("json 提取失败");
        assert_eq!(find_str(&data, "photoUrl"), Some("https://photo.kuaishou.com/x.mp4"));
        assert_eq!(find_str(&data, "caption"), Some("快手测试"));
    }

    #[test]
    fn test_render_data_extract() {
        let html = r#"var $render_data = [{"data":{"page_info":{"media_info":{"stream_url":"https://video.weibo.com/x.mp4"},"status_title":"微博测试"}}}][0] || {};"#;
        let data = extract_json_after(html, "$render_data = ").expect("json 提取失败");
        assert_eq!(find_str(&data, "stream_url"), Some("https://video.weibo.com/x.mp4"));
        assert_eq!(find_str(&data, "status_title"), Some("微博测试"));
    }

    #[test]
    fn test_og_video_meta() {
        let html = r#"<meta property="og:video:secure_url" content="https://cdn.example.com/video.mp4" /><meta property="og:title" content="通用页面视频" />"#;
        assert_eq!(find_meta(html, "og:video:secure_url"), Some("https://cdn.example.com/video.mp4"));
        assert_eq!(find_meta(html, "og:title"), Some("通用页面视频"));
    }

    #[test]
    fn test_clean_filename() {
        assert!(clean_filename("标题:含/非法*字符", "mp4").ends_with(".mp4"));
        assert!(!clean_filename("标题:含/非法*字符", "mp4").contains(':'));
    }

    #[tokio::test]
    async fn test_douyin_real_link() {
        if std::env::var("FEILIU_NET_TEST").is_err() {
            return;
        }
        let info = resolve("https://v.douyin.com/13Rs-NJ7TdY/")
            .await
            .expect("抖音解析失败");
        assert!(
            info.direct_url.contains("/play/"),
            "应为无水印 /play/ 直链: {}",
            info.direct_url
        );
        assert!(
            !info.filename.starts_with("douyin_video"),
            "应取到标题文件名: {}",
            info.filename
        );
        println!(
            "douyin OK: {} → {} bytes",
            info.filename,
            probe_total(&info.direct_url).await
        );
    }

    #[test]
    fn test_douyin_url_id() {
        let target = "https://www.douyin.com/video/7301234567890123456?x=1";
        let idx = target.find("/video/").unwrap();
        let rest = &target[idx + "/video/".len()..];
        let mut id = String::new();
        for c in rest.chars() {
            if c.is_ascii_digit() { id.push(c); } else if !id.is_empty() { break; }
        }
        assert_eq!(id, "7301234567890123456");
    }
}
