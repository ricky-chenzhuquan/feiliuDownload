//! 小红书：短链（xhslink.cn/.com）→ discovery 页 `__INITIAL_STATE__` → masterUrl 原片。
use super::helpers::{json_client, clean_filename, extract_json_after, find_str};

pub async fn resolve(url: &str) -> Option<super::Extracted> {
    let http = json_client()?;
    let target = super::helpers::follow_redirect(&http, url)
        .await
        .unwrap_or_else(|| url.to_string());
    if !target.contains("xiaohongshu.com") {
        return None;
    }
    let html = http.get(&target).send().await.ok()?.text().await.ok()?;
    let data = extract_json_after(&html, "__INITIAL_STATE__")?;
    // masterUrl 为无水印原片；图文笔记无此字段 → None（走普通流程）
    let master = find_str(&data, "masterUrl")?.to_string();
    if !master.starts_with("http") {
        return None;
    }
    // 标题：note 页 desc 字段（可能多行，清洗后使用）
    let desc = find_str(&data, "desc").unwrap_or("xhs_video").to_string();
    Some(super::Extracted {
        direct_url: master,
        filename: clean_filename(&desc, "mp4"),
        referer: Some("https://www.xiaohongshu.com/".into()),
        notice: None,
            cookies: None,
            user_agent: None,
            webview_plan: None,
    })
}

#[cfg(test)]
mod tests {
    /// 真实短链：xhslink.cn → discovery 页 → xhscdn 无水印直链
    #[tokio::test]
    async fn test_xhs_real_link() {
        if std::env::var("FEILIU_NET_TEST").is_err() {
            return;
        }
        let info = super::super::resolve_retry("https://xhslink.cn/o/1O0WRKQAjGa")
            .await
            .expect("小红书解析失败");
        assert!(
            info.direct_url.contains("xhscdn.com"),
            "直链应来自 xhscdn: {}",
            info.direct_url
        );
        println!("xhs OK: {} ({} bytes)", info.filename, info.direct_url.len());
    }
}
