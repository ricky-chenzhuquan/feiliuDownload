//! 阿里云盘（alipan.com / aliyundrive.com）分享链接解析。
//!
//! 匿名可获取分享结构（share_token + list_by_share，需 X-Canary/x-device-id/
//! x-share-token 头，2026-11 实测）；**下载直链必须登录**（网页端同样如此），
//! 因此解析产出分享内容摘要 + 明确提示，由用户在官网转存或登录后下载。
use std::sync::OnceLock;

use super::helpers::json_client;
use rand_free::random_id;

static DEVICE_ID: OnceLock<String> = OnceLock::new();

/// 会话级设备 ID（阿里风控要求，任意稳定字符串即可）
fn device_id() -> String {
    DEVICE_ID
        .get_or_init(|| random_id(21))
        .clone()
}

fn share_headers(token: &str) -> Vec<(&'static str, String)> {
    vec![
        ("X-Canary", "client=web,app=share,version=v2.3.1".into()),
        ("x-device-id", device_id()),
        ("x-share-token", token.to_string()),
    ]
}

async fn get_share_token(http: &reqwest::Client, share_id: &str) -> Option<String> {
    let resp = http
        .post("https://api.aliyundrive.com/v2/share_link/get_share_token")
        .json(&serde_json::json!({ "share_id": share_id }))
        .send()
        .await
        .ok()?;
    let v: serde_json::Value = resp.json().await.ok()?;
    v.get("share_token")?.as_str().map(|s| s.to_string())
}

async fn list_by_share(
    http: &reqwest::Client,
    token: &str,
    share_id: &str,
    parent: &str,
    limit: u32,
) -> Option<Vec<serde_json::Value>> {
    let mut req = http
        .post("https://api.aliyundrive.com/adrive/v2/file/list_by_share")
        .json(&serde_json::json!({
            "share_id": share_id,
            "parent_file_id": parent,
            "limit": limit,
            "order_by": "name",
            "order_direction": "ASC"
        }));
    for (k, v) in share_headers(token) {
        req = req.header(k, v);
    }
    let resp = req.send().await.ok()?;
    let v: serde_json::Value = resp.json().await.ok()?;
    Some(v.get("items")?.as_array()?.clone())
}

fn extract_share_id(url: &str) -> Option<String> {
    let idx = url.find("/s/")?;
    let rest = &url[idx + 3..];
    let id: String = rest
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric())
        .collect();
    if id.len() >= 8 {
        Some(id)
    } else {
        None
    }
}

pub async fn resolve(url: &str) -> Option<super::Extracted> {
    let share_id = extract_share_id(url)?;
    let http = json_client()?;
    let token = get_share_token(&http, &share_id).await?;
    let items = list_by_share(&http, &token, &share_id, "root", 50).await?;

    let files: Vec<&serde_json::Value> = items.iter().filter(|i| i.get("type").and_then(|t| t.as_str()) == Some("file")).collect();
    let folders = items.len() - files.len();

    // 单文件分享 → 直接给文件名与大小；多文件 → 汇总
    let (filename, _size_note) = if files.len() == 1 && folders == 0 {
        let f = files[0];
        let name = f.get("name").and_then(|n| n.as_str()).unwrap_or("文件").to_string();
        let size = f.get("size").and_then(|s| s.as_u64()).unwrap_or(0);
        let size_note = if size > 0 {
            format!("（{}）", crate::downloader::engine::human_size(size))
        } else {
            String::new()
        };
        (format!("{name}{size_note}"), String::new())
    } else {
        let first = items
            .first()
            .and_then(|i| i.get("name"))
            .and_then(|n| n.as_str())
            .unwrap_or("分享内容");
        let summary = if folders > 0 && files.len() > 0 {
            format!("{} 个文件夹、{} 个文件", folders, files.len())
        } else if folders > 0 {
            format!("{} 个文件夹", folders)
        } else {
            format!("{} 个文件", files.len())
        };
        (format!("{first}（{summary}）"), String::new())
    };

    Some(super::Extracted {
        direct_url: String::new(), // 匿名无下载直链
        filename,
        referer: Some("https://www.alipan.com/".into()),
        notice: Some(
            "阿里云盘分享：应用可列出分享内容，但网盘文件下载需要在阿里云盘官网登录后转存到自己网盘再下载（平台限制，匿名 API 不提供下载直链）".into(),
        ),
        cookies: None,
        user_agent: None,
        webview_plan: None,
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_extract_share_id() {
        assert_eq!(
            super::extract_share_id("https://www.alipan.com/s/4KZmWKgfnJc"),
            Some("4KZmWKgfnJc".into())
        );
        assert_eq!(super::extract_share_id("https://example.com/foo"), None);
    }

    /// 真实分享链接：匿名可列出结构（文件夹 1 项）
    #[tokio::test]
    async fn test_alipan_real_link() {
        if std::env::var("FEILIU_NET_TEST").is_err() {
            return;
        }
        let info = super::super::resolve_retry("https://www.alipan.com/s/4KZmWKgfnJc")
            .await
            .expect("阿里云盘解析失败");
        assert!(
            info.filename.contains("唐艺") || info.filename.contains("文件夹"),
            "应解析出分享内容摘要: {}",
            info.filename
        );
        assert!(info.notice.is_some(), "应包含需登录提示");
        println!("alipan OK: {}", info.filename);
    }
}

// 轻量随机 ID 生成（避免引入 rand 依赖）
mod rand_free {
    pub fn random_id(len: usize) -> String {
        let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
        let mut seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(12345);
        let mut out = String::new();
        for _ in 0..len {
            // xorshift
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            out.push(alphabet[(seed % alphabet.len() as u64) as usize] as char);
        }
        out
    }
}
