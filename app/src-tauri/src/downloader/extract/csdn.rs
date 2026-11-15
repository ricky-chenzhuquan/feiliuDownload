//! CSDN 下载中心（download.csdn.net）资源解析。
//!
//! 匿名可解析资源信息（标题/大小/积分/VIP 标记，来自页面 __INITIAL_STATE__）；
//! **下载直链必须登录 CSDN 账号**（VIP 或积分兑换，服务端在点击时生成），
//! 与阿里云盘/百度网盘同属登录墙，产出资源摘要 + 明确提示。
use super::helpers::clean_filename;

fn page_client() -> Option<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0 Safari/537.36")
        .redirect(reqwest::redirect::Policy::limited(8))
        .build()
        .ok()
}

/// 从 URL 判断是否 CSDN 下载中心链接
pub fn is_csdn_download(url: &str) -> bool {
    let lower = url.to_lowercase();
    lower.contains("download.csdn.net/download/") || lower.contains("csdn.net/download/")
}

/// 从 __INITIAL_STATE__ 区域内提取字段值（区域内正则，避免全局误匹配）
fn state_field(html: &str, pat: &str) -> Option<String> {
    let idx = html.find("__INITIAL_STATE__")?;
    let region = &html[idx..(idx + 400_000).min(html.len())];
    let re = regex_lite(pat)?;
    let caps = re.captures(region)?;
    Some(caps.get(1)?.as_str().to_string())
}

/// 极简正则封装（一次性编译失败返回 None）
fn regex_lite(pat: &str) -> Option<regex::Regex> {
    regex::Regex::new(pat).ok()
}

pub async fn resolve(url: &str) -> Option<super::Extracted> {
    if !is_csdn_download(url) {
        return None;
    }
    let http = page_client()?;
    // CSDN 间歇性返回风控页/空页，重试两次
    let mut html = String::new();
    for attempt in 0..3 {
        let resp = http.get(url).send().await.ok()?;
        let text = resp.text().await.ok()?;
        if text.contains("__INITIAL_STATE__") && text.contains("<title>") {
            html = text;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(800 * (attempt as u64 + 1))).await;
    }
    if html.is_empty() {
        return None;
    }

    // 标题：<title>xxx 资源-CSDN下载</title> 或 state 内 title 字段
    let title = html
        .find("<title>")
        .and_then(|i| {
            let start = i + "<title>".len();
            let end = html[start..].find("</title>")? + start;
            Some(html[start..end].trim().to_string())
        })
        .map(|t| t.replace(" 资源-CSDN下载", "").trim().to_string())
        .filter(|t| !t.is_empty())
        .or_else(|| state_field(&html, r#""title":"([^"]{3,80})""#))?;

    // 大小：fileSize 数字（字节）
    let size_note = state_field(&html, r#""fileSize":(\d+)"#)
        .and_then(|s| s.parse::<u64>().ok())
        .map(crate::downloader::engine::human_size)
        .map(|s| format!("（{s}）"))
        .unwrap_or_default();

    // 积分/VIP 要求
    let price = state_field(&html, r#""price":(\d+)"#)
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);
    let price_note = if price > 0 {
        format!("，需 {price} 积分")
    } else {
        String::new()
    };

    Some(super::Extracted {
        direct_url: String::new(), // 直链登录后由服务端生成
        filename: clean_filename(&format!("{title}{size_note}"), "zip")
            .trim_end_matches(".zip")
            .to_string(),
        referer: Some("https://download.csdn.net/".into()),
        notice: Some(format!(
            "CSDN 下载中心资源：已解析资源信息{price_note}，但下载必须登录 CSDN 账号（VIP 或积分兑换），直链由服务端登录态生成（平台限制）"
        )),
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_is_csdn_download() {
        assert!(super::is_csdn_download(
            "https://download.csdn.net/download/qq_36804580/10522936?ops_request_misc=x"
        ));
        assert!(!super::is_csdn_download("https://www.csdn.net/blog/123"));
    }

    /// 真实资源：匿名解析标题/大小/价格 + 登录提示
    #[tokio::test]
    async fn test_csdn_real_link() {
        if std::env::var("FEILIU_NET_TEST").is_err() {
            return;
        }
        let info = super::super::resolve(
            "https://download.csdn.net/download/qq_36804580/10522936",
        )
        .await
        .expect("CSDN 解析失败");
        assert!(
            info.filename.contains("前端") || info.filename.contains("模板"),
            "应解析出资源标题: {}",
            info.filename
        );
        assert!(info.notice.unwrap().contains("登录"), "应包含登录提示");
        println!("csdn OK: {}", info.filename);
    }
}
