//! UC 网盘分享解析（r4 轮 6 落地）。
//!
//! ⚠️ 平台收紧（r6 轮1 实锤 2026-11-28）：`file/download` 匿名调用返回 HTTP 401 +
//! code 31001 "require login [pc web guest only allows single image]"——访客免登录
//! 下载已被平台关闭（仅放行单张图片），与夸克（同家族 API，同日 31001）同步收紧。
//! 匿名仍可 token→列表（列结构/文件名可用），下载步给诚实登录墙提示；
//! 登录态下载路线（夸克/百度先例同构）挂后续轮。
//!
//! 链路（匿名至列表，真实验证 2026-11-22；下载步 2026-11-28 起需登录）：
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
    is_dir: bool,
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
    pdir_fid: &str,
) -> Result<Vec<UcFile>, String> {
    let url = format!(
        "{API_BASE}/transfer_share/detail?pwd_id={key}&passcode={passcode}&stoken={}&entry=ft&fr=pc&pr=UCBrowser&pdir_fid={pdir_fid}&fetch_file_list=1&_page=1&_size=100",
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
        .map(|it| UcFile {
            is_dir: !it.get("file").and_then(|f| f.as_bool()).unwrap_or(false),
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

/// 扁平文件清单：根目录文件在前，随后各子文件夹内容（一层级、最多 20 个文件夹、
/// 总量 500 上限——批量上限另有 settings.batch_limit 兜底）。返回 (分享标题, 清单)。
async fn walk_files(
    client: &reqwest::Client,
    key: &str,
    stoken: &str,
    passcode: &str,
    title: &str,
) -> Result<(String, Vec<UcFile>), String> {
    let root = list_files(client, key, stoken, passcode, "0").await?;
    let mut files: Vec<UcFile> = Vec::new();
    let mut folders: Vec<String> = Vec::new();
    for f in root {
        if f.is_dir {
            if folders.len() < 20 {
                folders.push(f.fid.clone());
            }
        } else {
            files.push(f);
        }
    }
    for fid in &folders {
        if files.len() >= 500 {
            break;
        }
        let children = list_files(client, key, stoken, passcode, fid).await?;
        files.extend(children);
    }
    Ok((title.to_string(), files))
}

/// 序号形态 URL（批量逐个建任务用）：uc 分享 base + feiliu_i=N
fn indexed_url(base: &str, i: usize) -> String {
    let base = base.split(['?', '&']).next().unwrap_or(base);
    format!("{base}?feiliu_i={i}")
}

fn feiliu_index(url: &str) -> Option<usize> {
    let lower = url.to_lowercase();
    for m in ["?feiliu_i=", "&feiliu_i="] {
        if let Some(p) = lower.find(m) {
            let digits: String = url[p + m.len()..]
                .chars()
                .take_while(|c| c.is_ascii_digit())
                .collect();
            if !digits.is_empty() {
                return digits.parse().ok();
            }
        }
    }
    None
}

/// UC 文件夹/多文件批量：扁平清单 → 每文件一个 feiliu_i 序号 URL（同 gofile 批量模式）
pub async fn folder_file_urls(url: &str) -> Option<(String, Vec<String>)> {
    let lower = url.to_lowercase();
    if !lower.contains("uc.cn") || !lower.contains("/s/") {
        return None;
    }
    let key = share_key(url)?;
    let passcode = passcode_of(url);
    let jar = std::sync::Arc::new(reqwest::cookie::Jar::default());
    let client = cookie_client(&jar).ok()?;
    let (stoken, title) = fetch_stoken_title(&client, &key, &passcode).await?;
    let (_, files) = walk_files(&client, &key, &stoken, &passcode, &title).await.ok()?;
    if files.is_empty() {
        return None;
    }
    let base = url.split(['?', '&']).next().unwrap_or(url).to_string();
    let urls = (0..files.len()).map(|i| indexed_url(&base, i)).collect();
    let name = if title.is_empty() { format!("uc_{key}") } else { title };
    Some((name, urls))
}

async fn fetch_stoken_title(
    client: &reqwest::Client,
    key: &str,
    passcode: &str,
) -> Option<(String, String)> {
    let tk = uc_post_json(
        client,
        &format!("{API_BASE}/share/sharepage/token?entry=ft&fr=pc&pr=UCBrowser"),
        serde_json::json!({ "pwd_id": key, "passcode": passcode }),
    )
    .await
    .ok()?;
    let stoken = tk.pointer("/data/stoken")?.as_str()?.to_string();
    let title = tk
        .pointer("/data/title")
        .and_then(|t| t.as_str())
        .unwrap_or("")
        .to_string();
    Some((stoken, title))
}

pub async fn resolve(url: &str) -> Option<Extracted> {
    let lower = url.to_lowercase();
    if !lower.contains("uc.cn") || !lower.contains("/s/") {
        return None;
    }
    let key = share_key(url)?;
    let passcode = passcode_of(url);
    let idx = feiliu_index(url);

    let jar = std::sync::Arc::new(reqwest::cookie::Jar::default());
    let client = match cookie_client(&jar) {
        Ok(c) => c,
        Err(_) => return None,
    };

    // 1) token → stoken（含分享标题）
    let (stoken, title) = fetch_stoken_title(&client, &key, &passcode).await?;

    // 2) 扁平清单（根文件 + 子文件夹一层）
    let (_, files) = match walk_files(&client, &key, &stoken, &passcode, &title).await {
        Ok(x) => x,
        Err(e) => {
            // 带提取码但未提供等场景：诚实报错
            return Some(Extracted {
                direct_url: String::new(),
                filename: format!("uc_{key}"),
                referer: Some(DL_REFERER.into()),
                notice: Some(format!("UC 网盘分享解析失败：{e}")),
                cookies: None,
                user_agent: None,
                webview_plan: None,
            });
        }
    };
    let target = match idx {
        Some(i) => files.get(i),
        None => files.iter().find(|f| f.size > 0),
    };
    let target = match target {
        Some(f) if f.size > 0 => f.clone(),
        // 空清单/序号越界/只有空文件：folder_file_urls 走批量入口
        _ if idx.is_none() => {
            return Some(Extracted {
                direct_url: String::new(),
                filename: title.is_empty().then(|| format!("uc_{key}")).unwrap_or(title.clone()),
                referer: Some(DL_REFERER.into()),
                notice: Some(
                    "UC 网盘分享仅含文件夹（或空文件），可点击「一键添加」批量下载其中的文件".into(),
                ),
                cookies: None,
                user_agent: None,
                webview_plan: None,
            });
        }
        _ => return None,
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
    .await;
    let direct = match dl {
        Ok(v) => v,
        // 31001=require login（r6 轮1 实锤：平台关闭访客下载，仅放行单张图片）
        Err(e) if e.starts_with("UC code=31001") => {
            return Some(Extracted {
                direct_url: String::new(),
                filename: target.name.clone(),
                referer: Some(DL_REFERER.into()),
                notice: Some(
                    "UC 网盘平台已关闭访客免登录下载（仅放行单张图片），需登录 UC 账号后方可下载「".to_string()
                        + &target.name
                        + "」。",

                ),
                cookies: None,
                user_agent: None,
                webview_plan: None,
            });
        }
        Err(_) => return None,
    };
    let direct = direct
        .pointer("/data/0/download_url")
        .and_then(|u| u.as_str())?
        .to_string();
    if direct.is_empty() {
        return None;
    }

    // 4) 会话 cookie 透传：__pugs 缺失时直链 403（真实验证）
    let dl_origin = reqwest::Url::parse(&direct).ok()?;
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
        user_agent: None,
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
        // r6 轮1 实锤：平台关闭访客免登录下载（31001 "guest only allows single image"）
        // 匿名解析仍列出结构与文件名，但下载步给诚实登录墙提示
        assert!(
            info.notice.as_deref().unwrap_or("").contains("登录") && info.direct_url.is_empty(),
            "访客下载被平台关闭后应给诚实登录墙提示: {:?}",
            info.notice
        );
        assert!(
            info.filename.ends_with(".mp4"),
            "真实文件名: {}",
            info.filename
        );
    }

    /// 文件夹批量清单（真实夹具：图库1 → 9 个 zip，feiliu_i 序号 URL）
    #[tokio::test]
    async fn test_uc_folder_file_urls() {
        if std::env::var("FEILIU_NET_TEST").is_err() {
            return;
        }
        let (name, urls) = super::folder_file_urls("https://drive.uc.cn/s/a55f8f6ebd234?public=1")
            .await
            .expect("UC 批量清单解析失败");
        assert!(!name.is_empty(), "清单标题应来自分享 title: {name}");
        assert!(urls.len() >= 9, "图库1 应有 ≥9 个文件: {}", urls.len());
        assert!(urls[0].contains("feiliu_i=0"), "序号 URL 形态: {}", urls[0]);
        assert!(urls.iter().all(|u| u.contains("/s/a55f8f6ebd234")), "URL 必须同源");
    }

    /// feiliu_i 序号解析：批量第 N 个文件应解析出对应直链
    #[tokio::test]
    async fn test_uc_indexed_resolve() {
        if std::env::var("FEILIU_NET_TEST").is_err() {
            return;
        }
        let info = super::super::resolve_retry("https://drive.uc.cn/s/a55f8f6ebd234?public=1&feiliu_i=0")
            .await
            .expect("UC 序号解析失败");
        // r6 轮1 实锤：访客下载被平台关闭，序号形态同样走诚实登录墙提示
        assert!(
            info.notice.as_deref().unwrap_or("").contains("登录") && info.direct_url.is_empty(),
            "序号形态应给诚实登录墙提示: {:?}",
            info.notice
        );
    }

    /// 纯文件夹分享：扁平清单进入子文件夹一层，根解析产出首个文件直链
    /// （批量入口 folder_file_urls 覆盖全量；空清单才走诚实 notice，无公开空夹具）
    #[tokio::test]
    async fn test_uc_folder_share_resolves_first_file() {
        if std::env::var("FEILIU_NET_TEST").is_err() {
            return;
        }
        let info = super::super::resolve_retry("https://drive.uc.cn/s/a55f8f6ebd234?public=1")
            .await
            .expect("UC 解析失败");
        // r6 轮1 实锤：文件夹分享进入子级取到首个文件后，下载步同样给诚实登录墙提示
        assert!(
            info.notice.as_deref().unwrap_or("").contains("登录") && info.direct_url.is_empty(),
            "文件夹分享首文件应给诚实登录墙提示: {:?}",
            info.notice
        );
    }
}
