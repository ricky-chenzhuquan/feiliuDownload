//! 站点解析层：把视频/网盘分享页地址解析为可直接下载的媒体流地址。
//!
//! 模块结构（每站点一个文件，统一走 `Extracted`）：
//! - `bilibili`：B 站视频页（含 b23.tv 短链）
//! - `douyin`：抖音（含 v.douyin.com 短链，ttwid Cookie 方案，去水印）
//! - `kuaishou`：快手
//! - `xhs`：小红书（含 xhslink.cn/.com 短链）
//! - `weibo`：微博（/status/{id} 与 weibo.com/{用户}/{博文}）
//! - `alipan`：阿里云盘分享（匿名列出结构；下载需登录）
//! - `baidu`：百度网盘分享（提取码自动验证 + 匿名列出结构；下载需登录/客户端）
//! - `csdn`：CSDN 下载中心资源（匿名列出资源信息；下载需登录 VIP/积分）
//! - `netease`：网易云音乐（单曲免登录直链 outer/url；VIP 歌与歌单给出引导）
//! - `twitter`：Twitter/X 视频推文（syndication API + react-tweet token 算法）
//! - `lanzou`：蓝奏云分享（结构列出；直链被 TLS 指纹墙拦，notice 引导）
//! - `dj82`：82DJ 舞曲网（音频 src 前端计算，WebView 代取；直链需 Referer）
//! - `generic`：通用兜底（og:video / <video src>）

mod alipan;
mod baidu;
mod csdn;
mod dj82;
mod bilibili;
mod douyin;
mod generic;
pub mod helpers;
mod kuaishou;
mod lanzou;
pub mod netease;
mod twitter;
mod weibo;
mod xhs;

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

/// 识别并解析；非支持的站点返回 None（走普通 HTTP 下载）
pub async fn resolve(url: &str) -> Option<Extracted> {
    let lower = url.to_lowercase();
    // B 站（含 b23.tv 短链）
    if lower.contains("bilibili.com/video") || lower.contains("b23.tv/") {
        return bilibili::resolve(url).await;
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
    // 通用兜底：og:video / <video src>（任意含视频的网页）
    generic::resolve(url).await
}
