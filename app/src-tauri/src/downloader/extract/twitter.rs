//! Twitter/X 视频推文解析（cdn.syndication.twimg.com，Vercel react-tweet 同款 token 算法）。
//!
//! 流程：x.com/twitter.com 的 /status/{id} → 计算 token（(id/1e15·π) 转 36 进制
//! 去掉 0 与小数点）→ syndication API 返回 JSON（作者/文本/媒体）→ 取最高码率
//! mp4 变体直链。m3u8（x-mpegURL）变体跳过，直链 video.twimg.com 无需登录。
use super::helpers::clean_filename;

fn client() -> Option<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent(super::helpers::MOBILE_UA)
        .build()
        .ok()
}

/// react-tweet 的 token 算法（公开 MIT 实现）：
/// (Number(id) / 1e15 * Math.PI).toString(36).replace(/(0+|\.)/g, '')
fn syndication_token(id: &str) -> String {
    let x = (id.parse::<f64>().unwrap_or(0.0) / 1e15) * std::f64::consts::PI;
    // JS 的 Number#toString(36) 对小数：整数部分 base36 + '.' + 小数部分 base36
    let digits = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut s = String::new();
    let mut ip = x as u64;
    let mut fp = x - (x as u64) as f64;
    if ip == 0 {
        s.push('0');
    }
    while ip > 0 {
        s.insert(0, digits[(ip % 36) as usize] as char);
        ip /= 36;
    }
    if fp > 0.0 {
        s.push('.');
        for _ in 0..20 {
            fp *= 36.0;
            let d = fp as u64;
            s.push(digits[d as usize] as char);
            fp -= d as f64;
        }
    }
    s.chars().filter(|c| *c != '0' && *c != '.').collect()
}

/// 从 URL 提取推文 ID（/status/{digits}）
fn parse_tweet_id(url: &str) -> Option<String> {
    let idx = url.find("/status/")?;
    let rest = &url[idx + "/status/".len()..];
    let id: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    if id.len() >= 10 && id.len() <= 20 {
        Some(id)
    } else {
        None
    }
}

pub async fn resolve(url: &str) -> Option<super::Extracted> {
    let lower = url.to_lowercase();
    if !lower.contains("x.com/") && !lower.contains("twitter.com/") {
        return None;
    }
    let id = parse_tweet_id(url)?;
    let http = client()?;
    let token = syndication_token(&id);
    let resp = http
        .get(format!(
            "https://cdn.syndication.twimg.com/tweet-result?id={id}&token={token}"
        ))
        .send()
        .await
        .ok()?;
    if !resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .contains("json")
    {
        return None; // 推文不存在/已删除 → HTML 404
    }
    let v: serde_json::Value = resp.json().await.ok()?;
    if v.get("__typename").and_then(|t| t.as_str()) != Some("Tweet") {
        return None;
    }
    let author = v
        .get("user")
        .and_then(|u| u.get("name"))
        .and_then(|n| n.as_str())
        .unwrap_or("tweet");
    let text = v.get("text").and_then(|t| t.as_str()).unwrap_or("");
    let title = format!("{author}：{text}");

    // 所有 mp4 变体中取最高码率（跳过 m3u8）
    let mut best: (u64, String) = (0, String::new());
    for m in v
        .get("mediaDetails")
        .and_then(|m| m.as_array())
        .map(|a| a.as_slice())
        .unwrap_or(&[])
    {
        for var in m
            .get("video_info")
            .and_then(|vi| vi.get("variants"))
            .and_then(|vs| vs.as_array())
            .map(|a| a.as_slice())
            .unwrap_or(&[])
        {
            if var.get("content_type").and_then(|c| c.as_str()) == Some("video/mp4") {
                let bitrate = var.get("bitrate").and_then(|b| b.as_u64()).unwrap_or(0);
                if let Some(u) = var.get("url").and_then(|u| u.as_str()) {
                    if bitrate >= best.0 {
                        best = (bitrate, u.to_string());
                    }
                }
            }
        }
    }
    if best.1.is_empty() {
        return None; // 纯图推文无视频
    }

    Some(super::Extracted {
        direct_url: best.1,
        filename: clean_filename(&title, "mp4"),
        referer: None,
        notice: None,
            webview_plan: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// react-tweet 同款 token：JS Number#toString(36) 语义对齐
    #[test]
    fn test_syndication_token() {
        // NASA 视频推文实测值（2026-11-18 验证 API 200）
        assert_eq!(syndication_token("2122404409292337592"), "557qa7jxyh4pwz65pzaor");
    }

    #[test]
    fn test_parse_tweet_id() {
        assert_eq!(
            parse_tweet_id("https://x.com/NASA/status/2122404409292337592"),
            Some("2122404409292337592".into())
        );
        assert_eq!(
            parse_tweet_id("https://twitter.com/NASA/status/2122404409292337592?s=20"),
            Some("2122404409292337592".into())
        );
        assert_eq!(parse_tweet_id("https://x.com/NASA"), None);
    }

    /// 真实推文：token → JSON → 最高码率 mp4 直链
    #[tokio::test]
    async fn test_twitter_real_video() {
        if std::env::var("FEILIU_NET_TEST").is_err() {
            return;
        }
        // 环境门控：syndication 域名被网络层重置（本机无代理环境）时跳过——
        // 连不上是环境问题不该报红；连得上但 API 失效则照样失败报警
        if tokio::net::TcpStream::connect("cdn.syndication.twimg.com:443")
            .await
            .is_err()
        {
            eprintln!("skip: cdn.syndication.twimg.com 网络不可达（本机无代理环境）");
            return;
        }
        let info = super::super::resolve_retry("https://x.com/NASA/status/2122404409292337592")
            .await
            .expect("Twitter 解析失败");
        assert!(
            info.direct_url.contains("video.twimg.com"),
            "直链应来自 video.twimg.com: {}",
            info.direct_url
        );
        assert!(
            !info.filename.starts_with("tweet"),
            "应取到作者+文本文件名: {}",
            info.filename
        );
        println!("twitter OK: {} → {} bytes", info.filename, info.direct_url.len());
    }
}
