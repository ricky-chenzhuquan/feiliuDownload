//! 各站点解析共享工具：UA、短链跟随、页面内嵌 JSON 提取、字段查找、文件名清洗。

/// 移动端 UA：让各站点返回移动版页面/接口数据
pub const MOBILE_UA: &str = "Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1";

/// 短链跟随重定向拿最终地址（各平台通用）
pub async fn follow_redirect(http: &reqwest::Client, url: &str) -> Option<String> {
    let resp = http.get(url).send().await.ok()?;
    Some(resp.url().to_string())
}

/// 从 HTML 中提取 `window.XXX = {...};` 形式的 JSON（平衡扫描边界）
pub fn extract_json_after(html: &str, marker: &str) -> Option<serde_json::Value> {
    let idx = html.find(marker)?;
    let rest = &html[idx + marker.len()..];
    let rest = rest.trim_start();
    let rest = rest
        .strip_prefix('=')
        .map(|r| r.trim_start())
        .unwrap_or(rest);
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
                    // 页面内嵌 JSON 常含 JS 的 undefined（如小红书压缩态），替换后才能解析
                    let mut text = String::from_utf8_lossy(&bytes[..=i]).into_owned();
                    if text.contains("undefined") {
                        for (a, b) in [
                            (": undefined", ": null"),
                            (":undefined", ": null"),
                            (", undefined", ", null"),
                            (",undefined", ", null"),
                            ("[undefined", "[null"),
                            ("[ undefined", "[null"),
                        ] {
                            text = text.replace(a, b);
                        }
                    }
                    return serde_json::from_str(&text).ok();
                }
            }
            _ => {}
        }
    }
    None
}

/// 递归查找 JSON 中指定 key 的值
pub fn find_key<'a>(v: &'a serde_json::Value, key: &str) -> Option<&'a serde_json::Value> {
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

/// 递归查找 JSON 中首个「key 含指定子串」的字符串值
pub fn find_str<'a>(v: &'a serde_json::Value, contains: &str) -> Option<&'a str> {
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

/// 标题清洗为安全文件名（去非法字符、截断）
pub fn clean_filename(title: &str, ext: &str) -> String {
    let cleaned: String = title
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '\n' | '\r' => ' ',
            _ => c,
        })
        .collect();
    let cleaned = cleaned.trim().to_string();
    if cleaned.is_empty() {
        format!(
            "video_{}.{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            ext
        )
    } else {
        let truncated: String = cleaned.chars().take(80).collect();
        format!("{truncated}.{ext}")
    }
}

/// 构建 JSON 客户端（移动 UA + 重定向跟随，各站点解析用）
pub fn json_client() -> Option<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent(MOBILE_UA)
        .redirect(reqwest::redirect::Policy::limited(8))
        .build()
        .ok()
}

/// B 站 playurl 直链 CDN 选择（2026-11-20 r3 轮 2 实锤）：官方按请求概率下发
/// `os=mcdn` 的 P2P 边缘节点（如 edge.mountaintoys.cn:4483 奇异域名），同一 durl
/// 的 backup_url 里有标准 bilivideo.com 主域候选——优先标准主域，仅在无候选时
/// 接受 mcdn（仍是 B 站官方签名直链，匿名可达但域名观感差、稳定性未知）。
pub fn prefer_bilivideo<'a>(candidates: impl Iterator<Item = &'a str>) -> Option<&'a str> {
    let mut fallback: Option<&'a str> = None;
    for c in candidates {
        if c.contains("bilivideo.com") {
            return Some(c);
        }
        if fallback.is_none() {
            fallback = Some(c);
        }
    }
    fallback
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_router_data_extract() {
        let html = r#"<script>window._ROUTER_DATA = {"loaderData":{"video_(1234)/page":{"video_info_res":{"item_list":[{"desc":"测试视频标题","video":{"play_addr":{"uri":"v0300","url_list":["https://www.douyin.com/aweme/v1/play/?video_id=v0300&ratio=720p&line=0"]}}}]}}}};</script>"#;
        let data = extract_json_after(html, "window._ROUTER_DATA").expect("json 提取失败");
        let item = find_key(&data, "item_list")
            .unwrap()
            .as_array()
            .unwrap()[0]
            .clone();
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
        assert_eq!(
            find_str(&data, "photoUrl"),
            Some("https://photo.kuaishou.com/x.mp4")
        );
        assert_eq!(find_str(&data, "caption"), Some("快手测试"));
    }

    #[test]
    fn test_render_data_extract() {
        let html = r#"var $render_data = [{"data":{"page_info":{"media_info":{"stream_url":"https://video.weibo.com/x.mp4"},"status_title":"微博测试"}}}][0] || {};"#;
        let data = extract_json_after(html, "$render_data = ").expect("json 提取失败");
        assert_eq!(
            find_str(&data, "stream_url"),
            Some("https://video.weibo.com/x.mp4")
        );
        assert_eq!(find_str(&data, "status_title"), Some("微博测试"));
    }

    #[test]
    fn test_undefined_compressed_and_spaced() {
        // 小红书压缩态 + 带空格两种 undefined 格式都要替换
        let html = r#"window.__INITIAL_STATE__={"a":undefined,"b": undefined,"c":[undefined,{"d":"x"}]}"#;
        let data = extract_json_after(html, "__INITIAL_STATE__").expect("解析失败");
        assert!(data.get("a").is_some());
        assert_eq!(find_key(&data, "d").and_then(|v| v.as_str()), Some("x"));
    }

    #[test]
    fn test_clean_filename() {
        assert!(clean_filename("标题:含/非法*字符", "mp4").ends_with(".mp4"));
        assert!(!clean_filename("标题:含/非法*字符", "mp4").contains(':'));
    }

    #[test]
    fn test_prefer_bilivideo() {
        // mcdn 边缘节点在前、bilivideo 在 backup：应选标准主域
        assert_eq!(
            prefer_bilivideo(
                [
                    "https://x5zu89j.edge.mountaintoys.cn:4483/upgcxcode/a.mp4?e=xx",
                    "https://upos-sz-mirrorcoso1.bilivideo.com/upgcxcode/a.mp4?e=xx",
                ]
                .into_iter()
            ),
            Some("https://upos-sz-mirrorcoso1.bilivideo.com/upgcxcode/a.mp4?e=xx")
        );
        // bilivideo 在首位：直接选中
        assert_eq!(
            prefer_bilivideo(
                ["https://cn-jsnt-ct-01-02.bilivideo.com/x.mp4", "https://upos.bilivideo.com/x.mp4"].into_iter()
            ),
            Some("https://cn-jsnt-ct-01-02.bilivideo.com/x.mp4")
        );
        // 全是 mcdn：接受首个（仍为官方签名直链）
        assert_eq!(
            prefer_bilivideo(
                ["https://edge.mountaintoys.cn:4483/x.mp4", "https://edge2.foo.cn/x.mp4"].into_iter()
            ),
            Some("https://edge.mountaintoys.cn:4483/x.mp4")
        );
        assert_eq!(prefer_bilivideo(std::iter::empty()), None);
    }
}
