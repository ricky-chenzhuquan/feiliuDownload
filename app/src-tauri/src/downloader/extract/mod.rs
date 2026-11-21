//! 站点解析层：把视频/网盘分享页地址解析为可直接下载的媒体流地址。
//!
//! 模块结构（每站点一个文件，统一走 `Extracted`）：
//! - `bilibili`：B 站视频页（含 b23.tv 短链）与 space 合集/系列页（一键批量）
//! - `bangumi`：B 站番剧 ss/ep 页（pgc API → 单集 MP4 直链；整季一键批量）
//! - `douyin`：抖音（含 v.douyin.com 短链，ttwid Cookie 方案，去水印）
//! - `kuaishou`：快手
//! - `xhs`：小红书（含 xhslink.cn/.com 短链）
//! - `weibo`：微博（/status/{id} 与 weibo.com/{用户}/{博文}）
//! - `alipan`：阿里云盘分享（匿名列出结构；下载需登录）
//! - `baidu`：百度网盘分享（提取码自动验证 + 匿名列出结构；下载需登录/客户端）
//! - `csdn`：CSDN 下载中心资源（匿名列出资源信息；下载需登录 VIP/积分）
//! - `netease`：网易云音乐（单曲免登录直链 outer/url；VIP 歌与歌单给出引导）
//! - `twitter`：Twitter/X 视频推文（syndication API + react-tweet token 算法）
//! - `lanzou`：蓝奏云分享（单文件 WebView 代取直链；文件夹列表 + 一键批量）
//! - `dj82`：82DJ 舞曲网（音频 src 前端计算，WebView 代取；直链需 Referer）
//! - `github`：GitHub Releases（页面链接 → 资产直链/结构，匿名 API 全可用）
//! - `archive`：Internet Archive 条目页（匿名 metadata API → 媒体直链，公开无防盗链）
//! - `bandcamp`：Bandcamp 专辑/曲目页（内嵌 data-tralbum JSON → mp3-128 流直链）
//! - `generic`：通用兜底（og:video / <video src>）

mod alipan;
mod archive;
pub mod bandcamp;
mod baidu;
mod csdn;
mod dj82;
pub mod bangumi;
pub mod bilibili;
mod douyin;
mod generic;
mod github;
pub mod gofile;
pub mod helpers;
mod kuaishou;
pub mod lanzou;
pub mod netease;
mod pixiv;
mod tiktok;
mod twitter;
pub mod uc;
pub mod weibo;
mod xhs;

/// 测试专用：解析带一次重试（真实网络套件中相邻测试触发同站风控的偶发失败）
#[cfg(test)]
pub(crate) async fn resolve_retry(url: &str) -> Option<Extracted> {
    match resolve(url).await {
        Some(e) => Some(e),
        None => {
            tokio::time::sleep(std::time::Duration::from_millis(1200)).await;
            resolve(url).await
        }
    }
}

#[cfg(test)]
mod canonical_tests {
    use super::*;

    #[test]
    fn test_canonical_key_offline() {
        // 同一 BV 的不同形态 → 同一键
        assert_eq!(
            tokio::runtime::Runtime::new()
                .unwrap()
                .block_on(canonical_key("https://www.bilibili.com/video/BV1GJ411x7h7")),
            tokio::runtime::Runtime::new()
                .unwrap()
                .block_on(canonical_key(
                    "https://www.bilibili.com/video/BV1GJ411x7h7/?share_source=copy_web"
                ))
        );
        // 分 P 不同 → 键不同
        let p1 = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(canonical_key("https://www.bilibili.com/video/BV1x?p=1"));
        let p2 = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(canonical_key("https://www.bilibili.com/video/BV1x?p=2"));
        assert_ne!(p1, p2);
        // 直链类无规范键
        assert_eq!(
            tokio::runtime::Runtime::new()
                .unwrap()
                .block_on(canonical_key("https://example.com/file.mp4")),
            None
        );
    }

    /// 真实小红书短链（xhslink.cn）：应解析出 xhs:{笔记id} 规范键
    #[tokio::test]
    async fn test_canonical_key_xhslink_real() {
        if std::env::var("FEILIU_NET_TEST").is_err() {
            return;
        }
        let key = canonical_key("https://xhslink.cn/o/1O0WRKQAjGa")
            .await
            .expect("小红书短链应有规范键");
        assert!(
            key.starts_with("xhs:") && key.len() > "xhs:".len() + 15,
            "应解析出 xhs:笔记id 规范键: {key}"
        );
        println!("canonical xhslink OK: {key}");
    }

    /// TikTok：短链与长链同 id 同键（跨形态查重）
    #[tokio::test]
    async fn test_canonical_key_tiktok() {
        let k1 = canonical_key("https://www.tiktok.com/@silhouettesbelow/video/7705115037366619423").await;
        assert_eq!(k1, Some("tiktok:7705115037366619423".into()));
        let k2 = canonical_key("https://vt.tiktok.com/ZSpSrc45f/").await;
        assert_eq!(k2, k1, "短链跟随跳转后应与长链同键");
    }

    /// GoFile：同分享不同批量序号是不同任务（批量建任务防撞查重）
    #[tokio::test]
    async fn test_canonical_key_gofile() {
        let k0 = canonical_key("https://gofile.io/d/d1AQzWks").await;
        assert_eq!(k0, Some("gofile:d1AQzWks".into()), "无序号键 = gofile:短码");
        let k2 = canonical_key("https://gofile.io/d/d1AQzWks?feiliu_i=2").await;
        assert_eq!(k2, Some("gofile:d1AQzWks:i2".into()), "序号入键");
        assert_ne!(k0, k2, "不同序号必须不同键（批量任务不得被判重复）");
        let k2b = canonical_key("https://www.gofile.io/d/d1AQzWks?feiliu_i=2&x=1").await;
        assert_eq!(k2b, k2, "www/附加参数不影响序号键");
        assert_eq!(canonical_key("https://gofile.io/uploadfile").await, None);
    }

    /// 真实抖音短链（v.douyin.com）：应解析出 douyin:{id} 规范键
    #[tokio::test]
    async fn test_canonical_key_douyin_short_real() {
        if std::env::var("FEILIU_NET_TEST").is_err() {
            return;
        }
        let key = canonical_key("https://v.douyin.com/13Rs-NJ7TdY/")
            .await
            .expect("抖音短链应有规范键");
        assert!(
            key.starts_with("douyin:") && key.len() > "douyin:".len() + 9,
            "应解析出 douyin:id 规范键: {key}"
        );
        println!("canonical douyin OK: {key}");
    }

    #[test]
    fn test_canonical_key_more_sites() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        // 快手
        assert_eq!(
            rt.block_on(canonical_key(
                "https://www.kuaishou.com/short-video/3xabc123def456"
            )),
            Some("ks:3xabc123def456".into())
        );
        // Twitter（x.com 与 twitter.com 同键）
        assert_eq!(
            rt.block_on(canonical_key("https://x.com/NASA/status/2122404409292337592")),
            rt.block_on(canonical_key(
                "https://twitter.com/NASA/status/2122404409292337592?s=20"
            ))
        );
        // 微博新旧两形态：键取博文 id
        assert_eq!(
            rt.block_on(canonical_key("https://weibo.com/1234567/AbCdEfG")),
            Some("wb:AbCdEfG".into())
        );
        assert_eq!(
            rt.block_on(canonical_key("https://m.weibo.cn/status/51234567890")),
            Some("wb:51234567890".into())
        );
        // 82DJ
        assert_eq!(
            rt.block_on(canonical_key("https://www.82dj.com/play/31534")),
            Some("dj82:31534".into())
        );
    }

    /// 真实 b23.tv 短链：键应与 www 形态一致（跨形态去重的核心场景）
    #[tokio::test]
    async fn test_canonical_key_b23tv_real() {
        if std::env::var("FEILIU_NET_TEST").is_err() {
            return;
        }
        let short = canonical_key("https://b23.tv/BV1GJ411x7h7")
            .await
            .expect("短链应有规范键");
        let www = canonical_key("https://www.bilibili.com/video/BV1GJ411x7h7")
            .await
            .expect("www 应有规范键");
        assert_eq!(short, www, "b23.tv 与 www 形态应映射同一键: {short} vs {www}");
        println!("canonical b23.tv OK: {short}");
    }
}

/// WebView 代取计划：直链被 TLS 指纹墙拦的站点（如蓝奏云），
/// 交给应用内嵌 WebView（真浏览器指纹）加载 url 并轮询执行 js 取结果
#[derive(Debug, Clone, serde::Serialize)]
pub struct WebviewExtractPlan {
    pub url: String,
    pub js: String,
    pub timeout_ms: u64,
}

/// 解析结果：直链 + 建议文件名 + 站点要求头 + 提示
pub struct Extracted {
    pub direct_url: String,
    pub filename: String,
    pub referer: Option<String>,
    /// 站点限制说明（如阿里云盘需登录转存）；有值时引擎不启动下载，前端展示提示
    pub notice: Option<String>,
    /// 会话 cookie（UC 网盘等：API 流程下发的 __pugs 必须随下载请求才 206）
    pub cookies: Option<String>,
    /// 需要内嵌 WebView 代取直链的计划；engine.add_task 时执行
    pub webview_plan: Option<WebviewExtractPlan>,
}

/// 站点规范身份键：同一内容的不同 URL 形态（短链、带分享参数、www/移动域）
/// 映射到同一键，供引擎跨形态查重。None = 无规范键（HTTP 直链等，按原 URL 去重）。
/// 纯 URL 解析为主（无网络）；仅 b23.tv 短链需跟随一次重定向。
pub async fn canonical_key(url: &str) -> Option<String> {
    let lower = url.to_lowercase();
    // b23.tv 短链 → 跟随重定向后递归
    if lower.contains("b23.tv/") {
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .user_agent(helpers::MOBILE_UA)
            .build()
            .ok()?;
        let resp = client.get(url).send().await.ok()?;
        let loc = resp
            .headers()
            .get(reqwest::header::LOCATION)?
            .to_str()
            .ok()?
            .to_string();
        if loc.starts_with("http") {
            return Box::pin(canonical_key(&loc)).await;
        }
        return None;
    }
    // B 站视频（BV 号 + 分 P）
    if lower.contains("bilibili.com/video") {
        let idx = url.find("/video/")? + "/video/".len();
        let bvid: String = url[idx..]
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric())
            .collect();
        if !bvid.starts_with("BV") {
            return None;
        }
        let page = ["?p=", "&p="]
            .iter()
            .find_map(|m| {
                let i = lower.find(m)?;
                let digits: String = url[i + m.len()..]
                    .chars()
                    .take_while(|c| c.is_ascii_digit())
                    .collect();
                digits.parse::<u64>().ok().filter(|n| *n >= 1)
            })
            .unwrap_or(1);
        return Some(format!("bili:{bvid}:p{page}"));
    }
    // TikTok（视频 id 键；vt/vm 短链跟随跳转后递归）
    if lower.contains("vt.tiktok.com") || lower.contains("vm.tiktok.com") {
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .user_agent(helpers::MOBILE_UA)
            .build()
            .ok()?;
        let resp = client.get(url).send().await.ok()?;
        let loc = resp
            .headers()
            .get(reqwest::header::LOCATION)?
            .to_str()
            .ok()?
            .to_string();
        if loc.starts_with("http") && loc.to_lowercase().contains("/video/") {
            return Box::pin(canonical_key(&loc)).await;
        }
        return None;
    }
    if lower.contains("tiktok.com") && lower.contains("/video/") {
        let idx = lower.find("/video/")? + "/video/".len();
        let id: String = url[idx..]
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .collect();
        if id.len() >= 10 {
            return Some(format!("tiktok:{id}"));
        }
        return None;
    }
    // GoFile 分享（短码 + 批量序号：同分享不同序号是不同任务）
    if lower.contains("gofile.io") && lower.contains("/d/") {
        let idx = lower.find("/d/")? + "/d/".len();
        let code: String = url[idx..]
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '-')
            .collect();
        let code = code.trim_end_matches('-').to_string();
        if !code.is_empty() {
            let fi = ["?feiliu_i=", "&feiliu_i="].iter().find_map(|m| {
                let p = lower.find(m)?;
                let digits: String = url[p + m.len()..]
                    .chars()
                    .take_while(|c| c.is_ascii_digit())
                    .collect();
                if digits.is_empty() {
                    None
                } else {
                    Some(digits)
                }
            });
            return Some(match fi {
                Some(i) => format!("gofile:{code}:i{i}"),
                None => format!("gofile:{code}"),
            });
        }
        return None;
    }
    // UC 网盘分享（fast/drive 子域同一 short key）
    if lower.contains("uc.cn") && lower.contains("/s/") {
        let idx = lower.find("/s/")? + "/s/".len();
        let code: String = url[idx..]
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric())
            .collect();
        if !code.is_empty() {
            let fi = ["?feiliu_i=", "&feiliu_i="].iter().find_map(|m| {
                let p = lower.find(m)?;
                let digits: String = url[p + m.len()..]
                    .chars()
                    .take_while(|c| c.is_ascii_digit())
                    .collect();
                if digits.is_empty() {
                    None
                } else {
                    Some(digits)
                }
            });
            return Some(match fi {
                Some(i) => format!("uc:{code}:i{i}"),
                None => format!("uc:{code}"),
            });
        }
        return None;
    }
    // B 站番剧（ep/ss）
    if lower.contains("bilibili.com/bangumi/play/") {
        for (marker, tag) in [("ep", "bgm-ep:"), ("ss", "bgm-ss:")] {
            if let Some(i) = lower.find(&format!("/{marker}")) {
                let digits: String = url[i + marker.len() + 1..]
                    .chars()
                    .take_while(|c| c.is_ascii_digit())
                    .collect();
                if !digits.is_empty() {
                    return Some(format!("{tag}{digits}"));
                }
            }
        }
        return None;
    }
    // 抖音短链（v.douyin.com/xxx）→ 跟随重定向后递归
    if lower.contains("v.douyin.com/") {
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .user_agent(helpers::MOBILE_UA)
            .build()
            .ok()?;
        let resp = client.get(url).send().await.ok()?;
        let loc = resp
            .headers()
            .get(reqwest::header::LOCATION)?
            .to_str()
            .ok()?
            .to_string();
        if loc.starts_with("http") {
            return Box::pin(canonical_key(&loc)).await;
        }
        return None;
    }
    // 抖音（视频/图集 id）
    if lower.contains("douyin.com") || lower.contains("iesdouyin.com") {
        for marker in ["/video/", "/note/", "share/video/", "share/note/"] {
            if let Some(i) = lower.find(marker) {
                let digits: String = url[i + marker.len()..]
                    .chars()
                    .take_while(|c| c.is_ascii_digit())
                    .collect();
                if digits.len() >= 10 {
                    return Some(format!("douyin:{digits}"));
                }
            }
        }
        return None;
    }
    // 小红书短链（xhslink.cn/.com）→ 跟随重定向后递归
    if lower.contains("xhslink.cn/") || lower.contains("xhslink.com/") {
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .user_agent(helpers::MOBILE_UA)
            .build()
            .ok()?;
        let resp = client.get(url).send().await.ok()?;
        let loc = resp
            .headers()
            .get(reqwest::header::LOCATION)?
            .to_str()
            .ok()?
            .to_string();
        if loc.starts_with("http") {
            return Box::pin(canonical_key(&loc)).await;
        }
        return None;
    }
    // 小红书（笔记 id）
    if lower.contains("xiaohongshu.com") {
        for marker in ["/explore/", "/discovery/item/"] {
            if let Some(i) = lower.find(marker) {
                let id: String = url[i + marker.len()..]
                    .chars()
                    .take_while(|c| c.is_ascii_hexdigit())
                    .collect();
                if id.len() >= 16 {
                    return Some(format!("xhs:{id}"));
                }
            }
        }
        return None;
    }
    // 网易云单曲
    if lower.contains("music.163.com") {
        if let Some(i) = lower.find("song?id=") {
            let digits: String = url[i + "song?id=".len()..]
                .chars()
                .take_while(|c| c.is_ascii_digit())
                .collect();
            if !digits.is_empty() {
                return Some(format!("ne:{digits}"));
            }
        }
        return None;
    }
    // 快手（short-video/{hex32id} 与官方分享落地页 fw/photo/{photoId}）
    if lower.contains("kuaishou.com") || lower.contains("chenzhongtech.com") {
        for marker in ["/short-video/", "/fw/photo/"] {
            if let Some(i) = lower.find(marker) {
                let id: String = url[i + marker.len()..]
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric())
                    .collect();
                if !id.is_empty() {
                    return Some(format!("ks:{id}"));
                }
            }
        }
        return None;
    }
    // Twitter/X（推文 id）
    if lower.contains("x.com/") || lower.contains("twitter.com/") {
        if let Some(i) = lower.find("/status/") {
            let digits: String = url[i + "/status/".len()..]
                .chars()
                .take_while(|c| c.is_ascii_digit())
                .collect();
            if !digits.is_empty() {
                return Some(format!("tw:{digits}"));
            }
        }
        return None;
    }
    // 微博（/status/{id} 与 新版 weibo.com/{用户}/{博文}）
    if lower.contains("weibo.c") {
        if let Some(i) = lower.find("/status/") {
            let id: String = url[i + "/status/".len()..]
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric())
                .collect();
            if !id.is_empty() {
                return Some(format!("wb:{id}"));
            }
        }
        // 新版 weibo.com/{用户}/{博文}（带或不带 scheme 均可）
        if let Some(host_idx) = lower.find("weibo.com/") {
            let after = &url[host_idx + "weibo.com/".len()..];
            let path = after.split(['?', '#']).next().unwrap_or(after);
            let segs: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
            if segs.len() >= 2 && !segs[1].is_empty() {
                return Some(format!("wb:{}", segs[1]));
            }
        }
        return None;
    }
    // 82DJ（play/{id}）
    if lower.contains("82dj.com/play/") {
        let idx = lower.find("/play/")? + "/play/".len();
        let digits: String = url[idx..]
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .collect();
        if !digits.is_empty() {
            return Some(format!("dj82:{digits}"));
        }
        return None;
    }
    // Internet Archive 条目
    if lower.contains("archive.org") && lower.contains("/details/") {
        let idx = lower.find("/details/")? + "/details/".len();
        let id: String = url[idx..]
            .split(['/', '?', '#'])
            .next()
            .unwrap_or("")
            .to_string();
        if !id.is_empty() {
            return Some(format!("ia:{id}"));
        }
        return None;
    }
    None
}

/// 识别并解析；非支持的站点返回 None（走普通 HTTP 下载）
pub async fn resolve(url: &str) -> Option<Extracted> {
    let lower = url.to_lowercase();
    // B 站（视频页 + b23.tv 短链 + space 合集页）
    if lower.contains("bilibili.com/video")
        || lower.contains("b23.tv/")
        || lower.contains("space.bilibili.com")
    {
        return bilibili::resolve(url).await;
    }
    // B 站番剧（ss/ep 页 → pgc API）
    if lower.contains("bilibili.com/bangumi/play/") {
        return bangumi::resolve(url).await;
    }
    // 抖音（含 v.douyin.com 短链）
    if lower.contains("douyin.com") || lower.contains("iesdouyin.com") {
        return douyin::resolve(url).await;
    }
    // 快手
    if lower.contains("kuaishou.com") || lower.contains("chenzhongtech.com") {
        return kuaishou::resolve(url).await;
    }
    // 小红书（含 xhslink.cn/.com 短链）
    if lower.contains("xiaohongshu.com")
        || lower.contains("xhslink.com")
        || lower.contains("xhslink.cn")
    {
        return xhs::resolve(url).await;
    }
    // 微博
    if lower.contains("weibo.c") || lower.contains("weibo.com") {
        return weibo::resolve(url).await;
    }
    // 阿里云盘分享
    if lower.contains("alipan.com/s/") || lower.contains("aliyundrive.com/s/") {
        return alipan::resolve(url).await;
    }
    // 百度网盘分享
    if lower.contains("pan.baidu.com/s/") {
        return baidu::resolve(url).await;
    }
    // CSDN 下载中心
    if lower.contains("download.csdn.net") {
        return csdn::resolve(url).await;
    }
    // 82DJ 舞曲网（WebView 代取音频直链）
    if lower.contains("82dj.com") {
        return dj82::resolve(url).await;
    }
    // 网易云音乐（含 163cn.tv 短链）
    if lower.contains("music.163.com") || lower.contains("163cn.tv") {
        return netease::resolve(url).await;
    }
    // Twitter/X 视频推文
    if lower.contains("x.com/") || lower.contains("twitter.com/") {
        return twitter::resolve(url).await;
    }
    // 蓝奏云分享（系列域名，子域多变）
    if lower.contains("lanzou") || lower.contains("lanzn.com") || lower.contains("lanpv.com") {
        return lanzou::resolve(url).await;
    }
    // GitHub Releases（页面链接 → 资产结构/直链）
    if lower.contains("github.com/") && lower.contains("/releases") {
        return github::resolve(url).await;
    }
    // Internet Archive 条目页（匿名 metadata API → 媒体直链）
    if lower.contains("archive.org") && lower.contains("/details/") {
        return archive::resolve(url).await;
    }
    // Bandcamp 专辑/曲目页（内嵌 tralbum JSON → mp3-128 流直链）
    if lower.contains("bandcamp.com") && (lower.contains("/album/") || lower.contains("/track/")) {
        return bandcamp::resolve(url).await;
    }
    // TikTok 视频页（匿名 itemStruct → aweme 同源跳转票，引擎直取）
    if lower.contains("tiktok.com") {
        return tiktok::resolve(url).await;
    }
    // Pixiv 作品页（匿名 ajax API → pximg 原始图直链，CDN 需 Referer）
    if lower.contains("pixiv.net") {
        return pixiv::resolve(url).await;
    }
    // UC 网盘分享（原生匿名 API 链路 + __pugs 会话 cookie 透传下载）
    if lower.contains("uc.cn") && lower.contains("/s/") {
        return uc::resolve(url).await;
    }
    // GoFile 网盘分享（wt 签名墙 → WebView 代取 + 会话 cookie 透传下载）
    if lower.contains("gofile.io") {
        return gofile::resolve(url).await;
    }
    // 通用兜底：og:video / <video src>（任意含视频的网页）
    generic::resolve(url).await
}
