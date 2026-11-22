//! 123 云盘（123pan，主域 www.123pan.com + 轮换域 123684/123865）分享解析。
//!
//! 端点事实（r4 轮1/2 + r5 轮10 真机实锤）：
//! - 列表 `GET /b/api/share/get?shareKey=&SharePwd=&ParentFileId=&Page=1` 匿名 200
//!   免签（现行 API 不校验 auth-key——pan-ye.http 2023 反爬记录已废弃）；
//!   条目 Type 0=文件（Etag/S3KeyFlag 齐全）1=目录（Etag 空，ParentFileId 递归）。
//! - 下载 `POST /b/api/share/download/info {ShareKey, FileID, S3keyFlag, Size, Etag}`
//!   匿名走到最后报 **5112「您需要注册登录或付费后下载」**=流量包墙精确拦截面
//!   （分享方流量包/登录 10GB/月/付费三选一，r5 轮10）。
//! - 登录态（pan123_login 会话 sso-token）：同端点带 Cookie 头 + `Authorization:`
//!   头（**不带 Bearer 前缀**，qaiu 123.py 口径）→ code 0 → data.DownloadUrl 直链。
//!
//! 提取码：分享文本「提取码:xxx」在 URL 之外——粘贴识别拼 ?pwd=（百度先例同款），
//! 无码分享 SharePwd 传空。
//! 纪律红线：登录态只做用户自己账号的分享内容提取（官方 10GB/月流量权益），
//! 不碰付费绕过。
use serde_json::json;

const DESKTOP_UA_VALUE: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/136.0.0.0 Safari/537.36";
const LIST_URL: &str = "https://www.123865.com/b/api/share/get";
const DL_URL: &str = "https://www.123865.com/b/api/share/download/info";

/// 遍历深度/宽度上限
const MAX_DEPTH: usize = 3;
const MAX_FOLDERS_PER_LEVEL: usize = 5;

/// 共享 HTTP 客户端（会话可选：Cookie 头 + Authorization 头双形态）
fn client(session: Option<&str>) -> Option<reqwest::Client> {
    let mut b = reqwest::Client::builder()
        .user_agent(DESKTOP_UA_VALUE)
        .redirect(reqwest::redirect::Policy::limited(8));
    if let Some(c) = session {
        let mut headers = reqwest::header::HeaderMap::new();
        if let Ok(v) = reqwest::header::HeaderValue::from_str(&format!("Cookie: {c}")) {
            headers.insert(reqwest::header::COOKIE, v);
        }
        // sso-token 的值即 JWT 本体（qaiu 口径：不带 Bearer 前缀）
        if let Some(tok) = c
            .split(';')
            .find_map(|kv| {
                let (k, v) = kv.trim().split_once('=')?;
                (k == "sso-token").then(|| v.to_string())
            })
            .and_then(|v| reqwest::header::HeaderValue::from_str(&v).ok())
        {
            headers.insert(reqwest::header::AUTHORIZATION, tok);
        }
        b = b.default_headers(headers);
    }
    b.build().ok()
}

/// 从 URL 提取 shareKey 与提取码（?pwd= 粘贴识别先例）
fn parse_share_url(url: &str) -> Option<(String, String)> {
    let lower = url.to_lowercase();
    let idx = ["123pan.com/s/", "123684.com/s/", "123865.com/s/"]
        .iter()
        .find_map(|m| lower.find(m).map(|i| i + m.len()))?;
    let key: String = url[idx..]
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    if key.len() < 6 {
        return None;
    }
    let pwd = url
        .split("pwd=")
        .nth(1)
        .map(|p| p.trim_end_matches(['&', '#', '/']).to_string())
        .unwrap_or_default();
    Some((key, pwd))
}

#[derive(Debug, Clone)]
struct PanFile {
    file_id: i64,
    name: String,
    size: u64,
    s3key_flag: String,
    etag: String,
}

/// 列一层（Type 0=文件 1=目录；Etag 空为目录的兜底判据）
async fn list_dir(
    http: &reqwest::Client,
    share_key: &str,
    pwd: &str,
    parent_id: i64,
) -> Result<Vec<(bool, PanFile)>, String> {
    let url = format!(
        "{LIST_URL}?limit=100&next=0&orderBy=file_name&orderDirection=asc&shareKey={share_key}&SharePwd={pwd}&ParentFileId={parent_id}&Page=1&event=homeListFile&operateType=1"
    );
    let r = http
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("列表请求失败: {e}"))?;
    let v: serde_json::Value = r.json().await.map_err(|e| format!("列表响应解析失败: {e}"))?;
    if v.get("code").and_then(|c| c.as_i64()) != Some(0) {
        return Err(format!(
            "123 云盘列表失败(code={:?}): {}",
            v.get("code").and_then(|c| c.as_i64()),
            v.get("message").and_then(|m| m.as_str()).unwrap_or("?")
        ));
    }
    let mut out = Vec::new();
    if let Some(list) = v.pointer("/data/InfoList").and_then(|l| l.as_array()) {
        for f in list {
            let file_id = f.get("FileId").and_then(|x| x.as_i64()).unwrap_or(0);
            if file_id == 0 {
                continue;
            }
            let etag = f
                .get("Etag")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string();
            let is_dir = f.get("Type").and_then(|x| x.as_i64()).unwrap_or(0) != 0 || etag.is_empty();
            out.push((
                is_dir,
                PanFile {
                    file_id,
                    name: f
                        .get("FileName")
                        .and_then(|x| x.as_str())
                        .unwrap_or("123pan_file")
                        .into(),
                    size: f.get("Size").and_then(|x| x.as_u64()).unwrap_or(0),
                    s3key_flag: f
                        .get("S3KeyFlag")
                        .and_then(|x| x.as_str())
                        .unwrap_or("")
                        .into(),
                    etag,
                },
            ));
        }
    }
    Ok(out)
}

/// 遍历分享树收集文件（根优先；限深限宽）
async fn walk_files(
    http: &reqwest::Client,
    share_key: &str,
    pwd: &str,
) -> Result<Vec<PanFile>, String> {
    let mut files: Vec<PanFile> = Vec::new();
    let mut folders: Vec<i64> = Vec::new();
    for (is_dir, f) in list_dir(http, share_key, pwd, 0).await? {
        if is_dir {
            folders.push(f.file_id);
        } else {
            files.push(f);
        }
    }
    let mut depth = 0;
    while depth < MAX_DEPTH && !folders.is_empty() && files.len() < 50 {
        let mut next: Vec<i64> = Vec::new();
        for fid in folders.drain(..).take(MAX_FOLDERS_PER_LEVEL) {
            for (is_dir, f) in list_dir(http, share_key, pwd, fid)
                .await
                .unwrap_or_default()
            {
                if is_dir {
                    next.push(f.file_id);
                } else {
                    files.push(f);
                }
            }
        }
        folders = next;
        depth += 1;
    }
    Ok(files)
}

/// 下载解析：匿名/登录态同端点，5112=流量包墙诚实提示
async fn download_info(
    http: &reqwest::Client,
    share_key: &str,
    f: &PanFile,
) -> Result<String, String> {
    let body = json!({
        "ShareKey": share_key,
        "FileID": f.file_id,
        "S3keyFlag": f.s3key_flag,
        "Size": f.size,
        "Etag": f.etag,
    });
    let r = http
        .post(DL_URL)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("下载解析请求失败: {e}"))?;
    let v: serde_json::Value = r
        .json()
        .await
        .map_err(|e| format!("下载解析响应解析失败: {e}"))?;
    match v.get("code").and_then(|c| c.as_i64()) {
        Some(0) => v
            .pointer("/data/DownloadUrl")
            .and_then(|u| u.as_str())
            .filter(|u| !u.is_empty())
            .map(String::from)
            .ok_or_else(|| "下载解析响应缺 DownloadUrl".into()),
        // 5112=注册登录或付费后下载（流量包墙精确拦截面，r5 轮10 实锤）
        Some(5112) => Err("GUEST_PAYWALL".into()),
        Some(41009 | 31001) => Err("提取码错误或分享已失效".into()),
        Some(c) => Err(format!(
            "123 云盘直链获取失败(code={c}): {}",
            v.get("message").and_then(|m| m.as_str()).unwrap_or("?")
        )),
        None => Err("下载解析响应缺 code".into()),
    }
}

fn notice_extracted(filename: String, notice: String) -> super::Extracted {
    super::Extracted {
        direct_url: String::new(),
        filename,
        referer: Some("https://www.123pan.com/".into()),
        notice: Some(notice),
        cookies: None,
        user_agent: None,
        webview_plan: None,
    }
}

async fn resolve_inner(url: &str, session: Option<&str>) -> Option<super::Extracted> {
    let (share_key, pwd) = parse_share_url(url)?;
    let http = client(session)?;
    let login = session.is_some();

    let files = match walk_files(&http, &share_key, &pwd).await {
        Ok(fs) if !fs.is_empty() => fs,
        Ok(_) => {
            return Some(notice_extracted(
                format!("123 云盘分享 {share_key}"),
                "分享目录为空，或只包含空文件夹".into(),
            ));
        }
        Err(e) => return Some(notice_extracted(format!("123 云盘分享 {share_key}"), e)),
    };
    // 批量按 feiliu_i 序号选文件（gofile 同款）
    let index: usize = url
        .split("feiliu_i=")
        .nth(1)
        .and_then(|v| {
            let digits: String = v.chars().take_while(|c| c.is_ascii_digit()).collect();
            digits.parse().ok()
        })
        .unwrap_or(0);
    if index >= files.len() {
        return Some(notice_extracted(
            format!("123 云盘分享 {share_key}"),
            format!("序号 {index} 超出分享文件数（共 {} 个）", files.len()),
        ));
    }
    let f = &files[index];

    match download_info(&http, &share_key, f).await {
        Ok(direct) => Some(super::Extracted {
            direct_url: direct,
            filename: f.name.clone(),
            referer: Some("https://www.123pan.com/".into()),
            notice: None,
            cookies: session.map(String::from),
            user_agent: None,
            webview_plan: None,
        }),
        Err(e) if e == "GUEST_PAYWALL" => {
            Some(notice_extracted(
                f.name.clone(),
                if login {
                    format!(
                        "「{name}」下载失败：123 云盘登录态流量已用尽或分享方流量包受限（免费用户每月 10GB 提取流量）",
                        name = f.name
                    )
                } else {
                    format!(
                        "「{name}」({size:.1} MB)：123 云盘分享下载需登录（注册用户每月 10GB 免费提取流量）。在设置页登录 123 云盘后即可下载。",
                        name = f.name,
                        size = f.size as f64 / 1048576.0
                    )
                },
            ))
        }
        Err(e) => Some(notice_extracted(f.name.clone(), e)),
    }
}

pub async fn resolve(url: &str) -> Option<super::Extracted> {
    resolve_inner(url, None).await
}

/// 批量清单：遍历分享树返回文件名+大小（engine add_pan123_batch 数量来源）
pub async fn list_files(url: &str) -> Result<Vec<(String, u64)>, String> {
    let (share_key, pwd) = parse_share_url(url).ok_or("非 123 云盘分享链接")?;
    let http = client(None).ok_or("HTTP 客户端初始化失败")?;
    let files = walk_files(&http, &share_key, &pwd).await?;
    Ok(files.into_iter().map(|f| (f.name, f.size)).collect())
}

/// 登录态解析（pan123_login 会话，sso-token）
pub async fn resolve_with_cookies(url: &str, session: &str) -> Option<super::Extracted> {
    resolve_inner(url, Some(session)).await
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 从 URL 提取 shareKey（主域+轮换域）与提取码
    #[test]
    fn test_parse_share_url() {
        assert_eq!(
            parse_share_url("https://www.123865.com/s/iaKtVv-FTaCd?pwd=qaiu"),
            Some(("iaKtVv-FTaCd".into(), "qaiu".into()))
        );
        assert_eq!(
            parse_share_url("https://www.123pan.com/s/iaKtVv-qOECd"),
            Some(("iaKtVv-qOECd".into(), String::new()))
        );
        assert_eq!(
            parse_share_url("https://www.123684.com/s/LH3rTd-pENed"),
            Some(("LH3rTd-pENed".into(), String::new()))
        );
        assert_eq!(parse_share_url("https://pan.baidu.com/s/1abc"), None);
        assert_eq!(parse_share_url("https://www.123865.com/s/ab"), None);
    }

    /// 真实链路（FEILIU_NET_TEST，国内站）：qaiu demo 夹具（提取码 qaiu）
    /// 根层目录 demo → 递归 1 层 4 文件（3 文件+pdf 目录再递归）
    #[tokio::test]
    async fn test_pan123_real_link() {
        if std::env::var("FEILIU_NET_TEST").is_err() {
            return;
        }
        let info = super::super::resolve_retry(
            "https://www.123865.com/s/iaKtVv-FTaCd?pwd=qaiu",
        )
        .await
        .expect("123 云盘解析失败");
        // 递归遍历应取到子目录文件（mp4/pdf/jpg）
        assert!(
            info.filename.contains(".mp4")
                || info.filename.contains(".pdf")
                || info.filename.contains(".jpg"),
            "应遍历到子目录文件: {}",
            info.filename
        );
        // 匿名必被 5112 流量包墙拦——诚实提示而非直链
        assert!(
            info.direct_url.is_empty()
                && info
                    .notice
                    .as_deref()
                    .unwrap_or("")
                    .contains("登录 123 云盘"),
            "匿名应给登录引导诚实提示: {:?}",
            info.notice
        );
        println!("pan123 OK: {} | {}", info.filename, info.notice.unwrap_or_default());
    }

    /// 真实链路（FEILIU_NET_TEST）：canonical 序号键
    #[tokio::test]
    async fn test_pan123_canonical() {
        let k0 = super::super::canonical_key("https://www.123865.com/s/iaKtVv-FTaCd").await;
        assert_eq!(k0, Some("pan123:iaKtVv-FTaCd".into()));
        let k2 = super::super::canonical_key("https://www.123865.com/s/iaKtVv-FTaCd?feiliu_i=2").await;
        assert_eq!(k2, Some("pan123:iaKtVv-FTaCd:i2".into()));
        assert_ne!(k0, k2);
    }
}
