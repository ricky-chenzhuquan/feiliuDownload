//! 微博：m.weibo.cn 状态页 `$render_data` → media_info.stream_url（无水印 mp4）。
//! 支持老格式 /status/{id} 与新版 weibo.com/{用户}/{博文}。
use super::helpers::{json_client, clean_filename, extract_json_after, find_str};

fn normalize_status_url(url: &str) -> Option<String> {
    if url.contains("m.weibo.cn") {
        return Some(url.to_string());
    }
    if let Some(i) = url.find("/status/") {
        let rest = &url[i + "/status/".len()..];
        let id: String = rest
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric())
            .collect();
        if !id.is_empty() {
            return Some(format!("https://m.weibo.cn/status/{id}"));
        }
    }
    // 新版 weibo.com/{用户id}/{博文bid}——锚定 weibo.com/ 后取段，
    // 带/不带 scheme 均可（原实现按 segs[1] 判 uid，带 scheme 时 segs[1]
    // 是域名段，导致完整 URL 解析失败）
    if let Some(host_idx) = url.find("weibo.com/") {
        let after = &url[host_idx + "weibo.com/".len()..];
        let path = after.split(['?', '#']).next().unwrap_or(after);
        let segs: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        // segs[0] = 数字 uid，segs[1] = 博文 bid（字母数字混合）
        if segs.len() >= 2
            && segs[0].chars().all(|c| c.is_ascii_digit())
            && !segs[1].is_empty()
        {
            return Some(format!("https://m.weibo.cn/status/{}", segs[1]));
        }
    }
    None // 登录页等无效链接
}

pub async fn resolve(url: &str) -> Option<super::Extracted> {
    let http = json_client()?;
    let target = normalize_status_url(url)?;
    let html = http.get(&target).send().await.ok()?.text().await.ok()?;
    let data = extract_json_after(&html, "$render_data = ")?;
    let stream = find_str(&data, "stream_url")?.to_string();
    if !stream.starts_with("http") {
        return None;
    }
    // 标题：status_title 优先，去残留 HTML 标签
    let title = find_str(&data, "status_title")
        .unwrap_or("weibo_video")
        .chars()
        .filter(|c| *c != '<' && *c != '>')
        .collect::<String>();
    Some(super::Extracted {
        direct_url: stream,
        filename: clean_filename(&title, "mp4"),
        referer: Some("https://m.weibo.cn/".into()),
        notice: None,
            webview_plan: None,
    })
}

#[cfg(test)]
mod normalize_tests {
    use super::*;

    /// 新版格式带 scheme（此前 segs[1] 命中域名段而解析失败）
    #[test]
    fn test_normalize_new_format_with_scheme() {
        assert_eq!(
            normalize_status_url("https://weibo.com/1234567/AbCdEfG"),
            Some("https://m.weibo.cn/status/AbCdEfG".into())
        );
    }

    /// 分享文本中的无 scheme 形态（保持原有可用行为）
    #[test]
    fn test_normalize_new_format_without_scheme() {
        assert_eq!(
            normalize_status_url("weibo.com/1234567/AbCdEfG?from=share"),
            Some("https://m.weibo.cn/status/AbCdEfG".into())
        );
    }

    /// 老格式 /status/{id} 与登录页拒绝（行为不变）
    #[test]
    fn test_normalize_status_and_login_reject() {
        assert_eq!(
            normalize_status_url("https://m.weibo.cn/status/51234567890"),
            Some("https://m.weibo.cn/status/51234567890".into())
        );
        assert_eq!(
            normalize_status_url(
                "https://weibo.com/newlogin?tabtype=weibo&gid=102803"
            ),
            None
        );
    }
}
