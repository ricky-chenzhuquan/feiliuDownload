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
//! - `generic`：通用兜底（og:video / <video src>）

mod alipan;
mod baidu;
mod bilibili;
mod douyin;
mod generic;
pub mod helpers;
mod kuaishou;
mod weibo;
mod xhs;

/// 解析结果：直链 + 建议文件名 + 站点要求头 + 提示
pub struct Extracted {
    pub direct_url: String,
    pub filename: String,
    pub referer: Option<String>,
    /// 站点限制说明（如阿里云盘需登录转存）；有值时引擎不启动下载，前端展示提示
    pub notice: Option<String>,
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
    // 通用兜底：og:video / <video src>（任意含视频的网页）
    generic::resolve(url).await
}
