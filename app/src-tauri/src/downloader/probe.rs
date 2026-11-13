//! URL 探测：通过 HEAD / Range-GET 获取文件名、大小、是否支持断点续传。
use std::sync::OnceLock;

use percent_encoding::percent_decode_str;
use reqwest::header::{
    ACCEPT_RANGES, CONTENT_DISPOSITION, CONTENT_LENGTH, CONTENT_RANGE, ETAG, HeaderMap,
    LAST_MODIFIED,
};
use reqwest::{Client, StatusCode};

pub const USER_AGENT_VALUE: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36";

static CLIENT: OnceLock<Client> = OnceLock::new();

/// 全局共享 HTTP 客户端（连接池 + rustls + 超时保护）
pub fn client() -> Client {
    CLIENT
        .get_or_init(|| {
            Client::builder()
                .user_agent(USER_AGENT_VALUE)
                .redirect(reqwest::redirect::Policy::limited(10))
                .connect_timeout(std::time::Duration::from_secs(15))
                .read_timeout(std::time::Duration::from_secs(30))
                .http1_only()
                .build()
                .expect("failed to build http client")
        })
        .clone()
}

#[derive(Debug, Clone, Default)]
pub struct ProbeInfo {
    pub filename: Option<String>,
    pub total_size: Option<u64>,
    pub accept_ranges: bool,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
}

/// 先 HEAD，失败或信息不全时退回 Range-GET
pub async fn probe(client: &Client, url: &str) -> Result<ProbeInfo, String> {
    if let Some(info) = head_probe(client, url).await {
        return Ok(info);
    }
    range_probe(client, url)
        .await
        .ok_or_else(|| "无法访问该地址：连接失败或返回错误状态".to_string())
}

async fn head_probe(client: &Client, url: &str) -> Option<ProbeInfo> {
    let resp = client.head(url).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let headers = resp.headers().clone();
    let total_size = headers
        .get(CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok());
    let accept_ranges = headers
        .get(ACCEPT_RANGES)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.eq_ignore_ascii_case("bytes"))
        .unwrap_or(false);
    let filename = parse_filename(&headers, url);
    if total_size.is_none() && filename.is_none() {
        return None; // 信息太少，退回 range 探测
    }
    Some(ProbeInfo {
        filename,
        total_size,
        accept_ranges,
        etag: get_str(&headers, &ETAG),
        last_modified: get_str(&headers, &LAST_MODIFIED),
    })
}

pub async fn range_probe(client: &Client, url: &str) -> Option<ProbeInfo> {
    let resp = client
        .get(url)
        .header(reqwest::header::RANGE, "bytes=0-0")
        .send()
        .await
        .ok()?;
    let status = resp.status();
    let headers = resp.headers().clone();

    if status == StatusCode::PARTIAL_CONTENT {
        // content-range: bytes 0-0/123456789
        let total = headers
            .get(CONTENT_RANGE)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.rsplit('/').next())
            .and_then(|v| v.parse::<u64>().ok());
        return Some(ProbeInfo {
            filename: parse_filename(&headers, url),
            total_size: total,
            accept_ranges: true,
            etag: get_str(&headers, &ETAG),
            last_modified: get_str(&headers, &LAST_MODIFIED),
        });
    }
    if status.is_success() {
        // 服务器忽略 Range，返回完整内容：不支持断点
        let total = headers
            .get(CONTENT_LENGTH)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok());
        return Some(ProbeInfo {
            filename: parse_filename(&headers, url),
            total_size: total,
            accept_ranges: false,
            etag: get_str(&headers, &ETAG),
            last_modified: get_str(&headers, &LAST_MODIFIED),
        });
    }
    None
}

fn get_str(headers: &HeaderMap, key: &reqwest::header::HeaderName) -> Option<String> {
    headers
        .get(key)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.to_string())
}

/// 从 Content-Disposition / URL 提取文件名
pub fn parse_filename(headers: &HeaderMap, url: &str) -> Option<String> {
    if let Some(cd) = headers.get(CONTENT_DISPOSITION).and_then(|v| v.to_str().ok()) {
        for part in cd.split(';') {
            let part = part.trim();
            if let Some(rest) = part.strip_prefix("filename*=") {
                if let Some(f) = parse_ext_value(rest) {
                    return Some(sanitize_filename(&f)?);
                }
            }
        }
        for part in cd.split(';') {
            let part = part.trim();
            if let Some(rest) = part.strip_prefix("filename=") {
                let f = rest.trim().trim_matches('"');
                if let Some(f) = sanitize_filename(f) {
                    return Some(f);
                }
            }
        }
    }
    // 退回 URL 路径最后一段
    let path = url.split(['?', '#']).next().unwrap_or(url);
    if let Some(seg) = path.rsplit('/').next() {
        if !seg.is_empty() {
            let decoded = percent_decode_str(seg).decode_utf8_lossy().into_owned();
            return sanitize_filename(&decoded);
        }
    }
    None
}

/// 解析 filename*=UTF-8''xxx 形式的扩展值
fn parse_ext_value(v: &str) -> Option<String> {
    let mut it = v.splitn(3, '\'');
    let charset = it.next()?.trim();
    let value = it.nth(1)?; // 跳过 lang
    if !charset.eq_ignore_ascii_case("utf-8") {
        return None;
    }
    Some(percent_decode_str(value).decode_utf8_lossy().into_owned())
}

/// 清洗文件名中的非法字符；非法则返回 None
pub fn sanitize_filename(name: &str) -> Option<String> {
    let mut cleaned: String = name
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '\0' => '_',
            c if (c as u32) < 0x20 => '_',
            c => c,
        })
        .collect();
    cleaned = cleaned.trim().trim_matches('.').to_string();
    if cleaned.chars().count() > 180 {
        cleaned = cleaned.chars().take(180).collect();
    }
    if cleaned.is_empty() {
        None
    } else {
        Some(cleaned)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize() {
        assert_eq!(sanitize_filename("a/b\\c.txt"), Some("a_b_c.txt".into()));
        assert_eq!(sanitize_filename(""), None);
        assert_eq!(sanitize_filename("..."), None);
        assert_eq!(sanitize_filename("视频 720.mp4"), Some("视频 720.mp4".into()));
    }

    #[test]
    fn test_ext_value() {
        assert_eq!(
            parse_ext_value("UTF-8''4HU.TV_720.mp4"),
            Some("4HU.TV_720.mp4".into())
        );
    }
}
