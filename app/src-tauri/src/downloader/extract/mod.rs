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
pub mod helpers;
mod kuaishou;
pub mod lanzou;
pub mod netease;
mod twitter;
mod weibo;
mod xhs;

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
    // 通用兜底：og:video / <video src>（任意含视频的网页）
    generic::resolve(url).await
}
