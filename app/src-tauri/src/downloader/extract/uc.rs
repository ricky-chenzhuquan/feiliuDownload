//! UC 网盘分享解析（r4 轮 6 落地）。
//!
//! 链路（全部匿名，真实验证 2026-11-22）：
//! - token：POST pc-api.uc.cn/1/clouddrive/share/sharepage/token {pwd_id,passcode} → stoken
//! - 列表：GET .../transfer_share/detail?pwd_id&passcode&stoken&entry=ft&fr=pc&pr=UCBrowser → list{fid,file_name,size,file,share_fid_token}
//! - 下载：POST .../file/download {fids,pwd_id,stoken,fids_token} → data[0].download_url
//! - **关键**：API 流程会下发 `__pugs` 会话 cookie，下载直链必须携带该 cookie 才
//!   206（缺失=403；Referer 不敏感）。cookie 由 reqwest cookie_provider 全程收集。
//! - 直链 host：dl-*.pds.uc.cn（UC 自有 CDN，非夸克 drive.quark.cn 家族——后者
//!   dl-guest 链接有 CDN 签名 412 死端，见 r4 轮 3-5 探测档案）。
//!
//! v1 范围：单文件分享直链；纯文件夹分享诚实提示（批量后续轮）。

use super::Extracted;
use reqwest::cookie::CookieStore;

const API_BASE: &str = "https://pc-api.uc.cn/1/clouddrive";
const WEB_UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";
const DL_REFERER: &str = "https://fast.uc.cn/";

fn share_key(url: &str) -> Option<String> {
    let lower = url.to_lowercase();
    let idx = lower.find("/s/")? + "/s/".len();
    let code: String = url[idx..]
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric())
        .collect();
    if code.is_empty() {
        None
    } else {
        Some(code)
    }
}

fn passcode_of(url: &str) -> String {
    url.split(['?', '&'])
        .find_map(|p| p.strip_prefix("pwd="))
        .unwrap_or("")
        .to_string()
}

#[derive(Clone)]
struct UcFile {
    fid: String,
    name: String,
    size: u64,
    token: String,
}

fn cookie_client(
    jar: &std::sync::Arc<reqwest::cookie::Jar>,
) -> reqwest::Result<reqwest::Client> {
    reqwest::Client::builder()
        .cookie_provider(jar.clone())
        .user_agent(WEB_UA)
        .timeout(std::time::Duration::from_secs(20))
        .build()
}

async fn uc_post_json(
    client: &reqwest::Client,
    url: &str,
    body: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let resp = client
        .post(url)
        .header("Origin", "https://drive.uc.cn")
        .header("Referer", "https://drive.uc.cn/")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("UC API 请求失败: {e}"))?;
    let status = resp.status();
    let text = resp.text().await.map_err(|e| e.to_string())?;
    let json: serde_json::Value =
        serde_json::from_str(&text).map_err(|_| format!("UC 返回非 JSON（HTTP {status}）"))?;
    let code = json.get("code").and_then(|c| c.as_i64()).unwrap_or(-1);
    if code != 0 {
        let msg = json.get("message").and_then(|m| m.as_str()).unwrap_or("");
        return Err(format!("UC code={code}: {msg}"));
    }
    Ok(json)
}

async fn list_files(
    client: &reqwest::Client,
    key: &str,
    stoken: &str,
    passcode: &str,
) -> Result<Vec<UcFile>, String> {
    let url = format!(
        "{API_BASE}/transfer_share/detail?pwd_id={key}&passcode={passcode}&stoken={}&entry=ft&fr=pc&pr=UCBrowser",
        urlencoding_like(stoken)
    );
    let resp = client
        .get(&url)
        .header("Origin", "https://drive.uc.cn")
        .header("Referer", "https://drive.uc.cn/")
        .send()
        .await
        .map_err(|e| format!("UC 列表请求失败: {e}"))?;
    let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    let code = json.get("code").and_then(|c| c.as_i64()).unwrap_or(-1);
    if code != 0 {
        let msg = json.get("message").and_then(|m| m.as_str()).unwrap_or("");
        return Err(format!("UC 列表 code={code}: {msg}"));
    }
    let list = json
        .pointer("/data/list")
        .and_then(|l| l.as_array())
        .cloned()
        .unwrap_or_default();
    Ok(list
        .into_iter()
        .filter(|it| it.get("file").and_then(|f| f.as_bool()).unwrap_or(false))
        .map(|it| UcFile {
            fid: it.get("fid").and_then(|v| v.as_str()).unwrap_or("").into(),
            name: it
                .get("file_name")
                .and_then(|v| v.as_str())
                .unwrap_or("ucfile")
                .into(),
            size: it.get("size").and_then(|v| v.as_u64()).unwrap_or(0),
            token: it
                .get("share_fid_token")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .into(),
        })
        .collect())
}

/// stoken 含特殊字符（/ + =），query 里必须编码；reqwest 不自动编码手拼 URL。
fn urlencoding_like(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

pub async fn resolve(url: &str) -> Option<Extracted> {
    let lower = url.to_lowercase();
    if !lower.contains("uc.cn") || !lower.contains("/s/") {
        return None;
    }
    let key = share_key(url)?;
    let passcode = passcode_of(url);

    let jar = std::sync::Arc::new(reqwest::cookie::Jar::default());
    let client = match cookie_client(&jar) {
        Ok(c) => c,
        Err(_) => return None,
    };

    // 1) token → stoken
    let tk = uc_post_json(
        &client,
        &format!("{API_BASE}/share/sharepage/token?entry=ft&fr=pc&pr=UCBrowser"),
        serde_json::json!({ "pwd_id": key, "passcode": passcode }),
    )
    .await
    .ok()?;
    let stoken = tk.pointer("/data/stoken")?.as_str()?.to_string();

    // 2) 根目录文件（纯文件夹分享 v1 诚实提示）
    let files = list_files(&client, &key, &stoken, &passcode).await.ok()?;
    let target = match files.iter().min_by_key(|f| f.size) {
        Some(f) if f.size > 0 => f.clone(),
        // 只有 0 字节占位/纯文件夹
        _ => {
            return Some(Extracted {
                direct_url: String::new(),
                filename: format!("uc_{key}"),
                referer: Some(DL_REFERER.into()),
                notice: Some(
                    "UC 网盘分享仅含文件夹（或空文件），当前版本只支持单文件分享的直链下载；请分享具体文件链接".into(),
                ),
                cookies: None,
                webview_plan: None,
            });
        }
    };

    // 3) 下载直链
    let dl = uc_post_json(
        &client,
        &format!("{API_BASE}/file/download?entry=ft&fr=pc&pr=UCBrowser"),
        serde_json::json!({
            "fids": [target.fid],
            "pwd_id": key,
            "stoken": stoken,
            "fids_token": [target.token],
        }),
    )
    .await
    .ok()?;
    let direct = dl
        .pointer("/data/0/download_url")
        .and_then(|u| u.as_str())?
        .to_string();
    if direct.is_empty() {
        return None;
    }

    // 4) 会话 cookie 透传：__pugs 缺失时直链 403（真实验证）
    let dl_origin =
        reqwest::Url::parse(&direct).ok()?;
    let cookie = jar
        .cookies(&dl_origin)
        .and_then(|v| v.to_str().ok().map(String::from))
        .filter(|s| !s.is_empty());

    Some(Extracted {
        direct_url: direct,
        filename: target.name.clone(),
        referer: Some(DL_REFERER.into()),
        notice: None,
        cookies: cookie,
        webview_plan: None,
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_share_key_parse() {
        assert_eq!(
            super::share_key("https://drive.uc.cn/s/12450d1694844?public=1"),
            Some("12450d1694844".into())
        );
        assert_eq!(
            super::share_key("https://fast.uc.cn/s/a55f8f6ebd234"),
            Some("a55f8f6ebd234".into())
        );
        assert_eq!(super::share_key("https://www.uc.cn/"), None);
    }

    #[test]
    fn test_passcode_parse() {
        assert_eq!(super::passcode_of("https://drive.uc.cn/s/abc?pwd=x9k2"), "x9k2");
        assert_eq!(super::passcode_of("https://drive.uc.cn/s/abc?public=1"), "");
    }

    #[test]
    fn test_urlencoding_like() {
        assert_eq!(super::urlencoding_like("aB3-_.~"), "aB3-_.~");
        assert_eq!(super::urlencoding_like("a/b+c=d"), "a%2Fb%2Bc%3Dd");
    }

    /// 真实公开分享（r4 轮 6 验证）：解析层应产出 pds.uc.cn 直链 + __pugs cookie
    #[tokio::test]
    async fn test_uc_real_share() {
        if std::env::var("FEILIU_NET_TEST").is_err() {
            return;
        }
        let info = super::super::resolve_retry("https://drive.uc.cn/s/12450d1694844?public=1")
            .await
            .expect("UC 解析失败");
        assert!(
            info.direct_url.contains(".pds.uc.cn/"),
            "直链必须是 UC 自有 CDN（pds.uc.cn）: {}",
            info.direct_url
        );
        let cookies = info.cookies.expect("__pugs 会话 cookie 必须透传");
        assert!(cookies.contains("__pugs="), "cookie 必须含 __pugs: {cookies}");
        assert!(
            info.filename.ends_with(".mp4"),
            "真实文件名: {}",
            info.filename
        );
        assert!(info.notice.is_none(), "可下载分享不应有 notice");
    }

    /// 文件夹分享诚实提示（不凑数）
    #[tokio::test]
    async fn test_uc_folder_share_honest_notice() {
        if std::env::var("FEILIU_NET_TEST").is_err() {
            return;
        }
        let info = super::super::resolve_retry("https://drive.uc.cn/s/a55f8f6ebd234?public=1")
            .await
            .expect("UC 解析失败");
        assert!(info.direct_url.is_empty(), "纯文件夹分享不产直链");
        let notice = info.notice.expect("必须有诚实提示");
        assert!(notice.contains("文件夹"), "提示应说明文件夹限制: {notice}");
    }
}
