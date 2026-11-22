//! 夸克网盘（pan.quark.cn）分享链接解析。
//!
//! 匿名链（r4 轮3 + r5 轮6 真机验证）：`share/sharepage/token {pwd_id, passcode}`
//! → stoken → `share/sharepage/detail?pwd_id&stoken&pdir_fid=...` 列结构（含子目录
//! 遍历，#275 类深层分享按 QkTool 经验限深限宽）。**stoken 双形态坑**：detail 的
//! query 参数要 URL 编码（`+`→`%2B` 等），file/download 的 body 里要原文——
//! r5 轮6 真机实锤（encoded 进 body 报 14001 非法 token）。
//!
//! 访客直下：`file/download {fids, pwd_id, stoken(原文), fids_token}`，≤10MiB 匿名
//! 可下（netdisk-fast-download #298）；超限文件报 23018（访客大小限制）或 41020
//! 「转存文件token校验异常」（r5 轮6 真机：12MB 文件匿名直下实测）。
//!
//! 登录态（quark_login 会话，__pus 核心）：转存 `drive-h.quark.cn/.../sharepage/save
//! {pwd_id, stoken, to_pdir_fid, scene:"link", fid_list, fid_token_list}` → task 轮询
//! → saved fid → `file/download {fids:[saved_fid]}` → download_url（dl-*-pds.quark.cn
//! OSS 签名直链）。下载头 = 会话 cookie + `Referer: https://pan.quark.cn/`。
//! __puus 直链凭据 1h 过期由 auth/pc/flush 刷新（服务器随响应 Set-Cookie）。
//!
//! 纪律红线：只做用户自己登录态下的分享内容提取（转存到自己网盘再取直链），
//! 不碰会员加速/容量外直取。
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use serde_json::json;

const DESKTOP_UA_VALUE: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0 Safari/537.36";
const TOKEN_URL: &str = "https://drive-pc.quark.cn/1/clouddrive/share/sharepage/token?pr=ucpro&fr=pc";
const DETAIL_URL: &str = "https://drive-pc.quark.cn/1/clouddrive/share/sharepage/detail?pr=ucpro&fr=pc";
const DOWNLOAD_URL: &str = "https://drive-pc.quark.cn/1/clouddrive/file/download?pr=ucpro&fr=pc";
const SAVE_URL: &str = "https://drive-h.quark.cn/1/clouddrive/share/sharepage/save?pr=ucpro&fr=pc";
const TASK_URL: &str = "https://drive-pc.quark.cn/1/clouddrive/task?pr=ucpro&fr=pc";
const REFERER: &str = "https://pan.quark.cn/";

/// 遍历深度/宽度上限（QkTool #275 经验：防把整棵目录树请求一遍）
const MAX_DEPTH: usize = 3;
const MAX_FOLDERS_PER_LEVEL: usize = 5;

fn client(session: Option<&str>) -> Option<reqwest::Client> {
    let mut b = reqwest::Client::builder()
        .user_agent(DESKTOP_UA_VALUE)
        .redirect(reqwest::redirect::Policy::limited(8));
    if let Some(c) = session {
        let mut headers = reqwest::header::HeaderMap::new();
        if let Ok(v) = reqwest::header::HeaderValue::from_str(&format!("Cookie: {c}")) {
            headers.insert(reqwest::header::COOKIE, v);
        }
        b = b.default_headers(headers);
    }
    b.build().ok()
}

/// 从 URL 提取 pwd_id（/s/{id}，容忍 #/list/share 后缀与 query）
fn parse_share_url(url: &str) -> Option<String> {
    let lower = url.to_lowercase();
    let idx = lower.find("/s/")? + 3;
    let id: String = url[idx..]
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric())
        .collect();
    if id.len() < 8 {
        return None;
    }
    Some(id)
}

#[derive(Debug, Clone)]
struct ShareFile {
    fid: String,
    name: String,
    size: u64,
    token: String,
}

/// 换 stoken（原文；调用方进 query 需自行编码）
async fn get_stoken(
    http: &reqwest::Client,
    pwd_id: &str,
    passcode: &str,
) -> Result<String, String> {
    let body = json!({"pwd_id": pwd_id, "passcode": passcode});
    let r = http
        .post(TOKEN_URL)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("stoken 请求失败: {e}"))?;
    let v: serde_json::Value = r.json().await.map_err(|e| format!("stoken 响应解析失败: {e}"))?;
    match v.get("code").and_then(|c| c.as_i64()) {
        Some(0) => v
            .pointer("/data/stoken")
            .and_then(|s| s.as_str())
            .map(String::from)
            .ok_or_else(|| "stoken 响应缺 data.stoken".into()),
        Some(41009) => Err("提取码错误或缺失".into()),
        Some(c) => Err(format!(
            "夸克分享不可用(code={c}): {}",
            v.get("message").and_then(|m| m.as_str()).unwrap_or("?")
        )),
        None => Err("stoken 响应缺 code".into()),
    }
}

/// 列目录带 dir 标记（list_dir 的原始形态）
async fn list_dir_raw(
    http: &reqwest::Client,
    pwd_id: &str,
    stoken: &str,
    pdir_fid: &str,
) -> Result<Vec<(bool, ShareFile)>, String> {
    let stoken_enc = utf8_percent_encode(stoken, NON_ALPHANUMERIC).to_string();
    let url = format!(
        "{DETAIL_URL}&pwd_id={pwd_id}&stoken={stoken_enc}&pdir_fid={pdir_fid}&force=0&_page=1&_size=50&_fetch_banner=0&_fetch_share=0&_fetch_total=1&_sort=file_type:asc,updated_at:desc"
    );
    let r = http
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("detail 请求失败: {e}"))?;
    let v: serde_json::Value = r.json().await.map_err(|e| format!("detail 响应解析失败: {e}"))?;
    if v.get("code").and_then(|c| c.as_i64()) != Some(0) {
        return Err("detail code!=0".into());
    }
    let mut out = Vec::new();
    if let Some(list) = v.pointer("/data/list").and_then(|l| l.as_array()) {
        for f in list {
            out.push((
                f.get("dir").and_then(|x| x.as_bool()).unwrap_or(false),
                ShareFile {
                    fid: f.get("fid").and_then(|x| x.as_str()).unwrap_or("").into(),
                    name: f
                        .get("file_name")
                        .and_then(|x| x.as_str())
                        .unwrap_or("quark_file")
                        .into(),
                    size: f.get("size").and_then(|x| x.as_u64()).unwrap_or(0),
                    token: f
                        .get("share_fid_token")
                        .and_then(|x| x.as_str())
                        .unwrap_or("")
                        .into(),
                },
            ));
        }
    }
    Ok(out)
}

/// 遍历分享树收集文件（根优先；限深限宽，QkTool #275 经验）
async fn walk_files(
    http: &reqwest::Client,
    pwd_id: &str,
    stoken: &str,
) -> Result<Vec<ShareFile>, String> {
    let mut files: Vec<ShareFile> = Vec::new();
    let mut folders: Vec<String> = Vec::new();
    if let Ok(items) = list_dir_raw(http, pwd_id, stoken, "0").await {
        for (dir_flag, f) in items {
            if dir_flag {
                folders.push(f.fid);
            } else {
                files.push(f);
            }
        }
    }
    let mut depth = 0;
    while depth < MAX_DEPTH && !folders.is_empty() && files.len() < 50 {
        let mut next: Vec<String> = Vec::new();
        for fid in folders.drain(..).take(MAX_FOLDERS_PER_LEVEL) {
            if let Ok(items) = list_dir_raw(http, pwd_id, stoken, &fid).await {
                for (dir_flag, f) in items {
                    if dir_flag {
                        next.push(f.fid);
                    } else {
                        files.push(f);
                    }
                }
            }
        }
        folders = next;
        depth += 1;
    }
    Ok(files)
}

/// 访客直下（≤10MiB）；超限/校验异常返回 Err(诚实说明)
async fn guest_download(
    http: &reqwest::Client,
    pwd_id: &str,
    stoken: &str,
    f: &ShareFile,
) -> Result<String, String> {
    let body = json!({
        "fids": [f.fid],
        "pwd_id": pwd_id,
        "stoken": stoken, // 原文（r5 轮6：encoded 报 14001）
        "fids_token": [f.token],
    });
    let r = http
        .post(DOWNLOAD_URL)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("访客直下请求失败: {e}"))?;
    let v: serde_json::Value = r.json().await.map_err(|e| format!("访客直下响应解析失败: {e}"))?;
    match v.get("code").and_then(|c| c.as_i64()) {
        Some(0) => v
            .pointer("/data/0/download_url")
            .and_then(|u| u.as_str())
            .map(String::from)
            .ok_or_else(|| "访客直下响应缺 download_url".into()),
        // 23018=访客大小限制；41020=转存文件 token 校验异常（超限实测形态）
        Some(c @ (23018 | 41020)) => Err(format!("GUEST_LIMIT:{c}")),
        Some(31001) => Err("未登录或会话已失效".into()),
        Some(c) => Err(format!(
            "夸克直链获取失败(code={c}): {}",
            v.get("message").and_then(|m| m.as_str()).unwrap_or("?")
        )),
        None => Err("访客直下响应缺 code".into()),
    }
}

/// 登录态转存链：save → task 轮询 → saved fid → file/download 直链
async fn transfer_download(
    http: &reqwest::Client,
    pwd_id: &str,
    stoken: &str,
    f: &ShareFile,
) -> Result<String, String> {
    // 1) 转存（scene=link；search_exit 由服务端默认去重，errno 41004 之类视为已转存复用）
    let save_body = json!({
        "pwd_id": pwd_id,
        "stoken": stoken,
        "pdir_fid": "0",
        "to_pdir_fid": "0", // 转存到「来自：分享」根（App 端默认位）
        "scene": "link",
        "fid_list": [f.fid],
        "fid_token_list": [f.token],
        "pdir_save_all": false,
    });
    let r = http
        .post(SAVE_URL)
        .json(&save_body)
        .send()
        .await
        .map_err(|e| format!("转存请求失败: {e}"))?;
    let v: serde_json::Value = r.json().await.map_err(|e| format!("转存响应解析失败: {e}"))?;
    if v.get("code").and_then(|c| c.as_i64()) != Some(0) {
        return Err(format!(
            "转存失败(code={:?}): {}",
            v.get("code").and_then(|c| c.as_i64()),
            v.get("message").and_then(|m| m.as_str()).unwrap_or("?")
        ));
    }
    // 2) task 轮询（save 响应 data.task_id；轮询 /task?task_id&retry_index）
    let task_id = v
        .pointer("/data/task_id")
        .and_then(|t| t.as_str())
        .map(String::from)
        .ok_or("转存响应缺 task_id")?;
    let mut saved_fids: Vec<String> = Vec::new();
    for retry in 0..15u32 {
        tokio::time::sleep(std::time::Duration::from_millis(600)).await;
        let url = format!("{TASK_URL}&task_id={task_id}&retry_index={retry}");
        let r = http
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("task 轮询失败: {e}"))?;
        let tv: serde_json::Value = r.json().await.map_err(|e| format!("task 响应解析失败: {e}"))?;
        let status = tv.pointer("/data/status").and_then(|s| s.as_i64()).unwrap_or(0);
        match status {
            2 => {
                // 完成：save_task.save_as_save_as_response.save_as_top_fids（多级结构，容错取数组）
                saved_fids = tv
                    .pointer("/data/save_task/save_as_save_as_response/save_as_top_fids")
                    .and_then(|x| x.as_array())
                    .map(|a| {
                        a.iter()
                            .filter_map(|s| s.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();
                if saved_fids.is_empty() {
                    return Err("转存完成但响应缺 save_as_top_fids".into());
                }
                break;
            }
            1 => continue, // 进行中
            s => return Err(format!("转存任务异常状态: {s}")),
        }
    }
    if saved_fids.is_empty() {
        return Err("转存任务轮询超时".into());
    }
    // 3) file/download {fids:[saved_fid]} → download_url
    let dl_body = json!({"fids": saved_fids});
    let r = http
        .post(DOWNLOAD_URL)
        .json(&dl_body)
        .send()
        .await
        .map_err(|e| format!("转存直链请求失败: {e}"))?;
    let dv: serde_json::Value = r
        .json()
        .await
        .map_err(|e| format!("转存直链响应解析失败: {e}"))?;
    match dv.get("code").and_then(|c| c.as_i64()) {
        Some(0) => dv
            .pointer("/data/0/download_url")
            .and_then(|u| u.as_str())
            .map(String::from)
            .ok_or_else(|| "转存直链响应缺 download_url".into()),
        Some(c) => Err(format!(
            "转存直链获取失败(code={c}): {}",
            dv.get("message").and_then(|m| m.as_str()).unwrap_or("?")
        )),
        None => Err("转存直链响应缺 code".into()),
    }
}

async fn resolve_inner(url: &str, session: Option<&str>) -> Option<super::Extracted> {
    let pwd_id = parse_share_url(url)?;
    let passcode = url
        .split("pwd=")
        .nth(1)
        .map(|p| p.trim_end_matches(['&', '#', '/']).to_string())
        .unwrap_or_default();
    let http = client(session)?;
    let login = session.is_some();

    let stoken = match get_stoken(&http, &pwd_id, &passcode).await {
        Ok(s) => s,
        Err(e) => {
            return Some(super::Extracted {
                direct_url: String::new(),
                filename: format!("夸克分享 {pwd_id}"),
                referer: Some(REFERER.into()),
                notice: Some(e),
                cookies: None,
                user_agent: None,
                webview_plan: None,
            });
        }
    };

    // 列结构（根优先，深层分享遍历）
    let files = match walk_files(&http, &pwd_id, &stoken).await {
        Ok(fs) if !fs.is_empty() => fs,
        _ => {
            return Some(super::Extracted {
                direct_url: String::new(),
                filename: format!("夸克分享 {pwd_id}"),
                referer: Some(REFERER.into()),
                notice: Some("分享目录为空，或只包含空文件夹".into()),
                cookies: None,
                user_agent: None,
                webview_plan: None,
            });
        }
    };
    // 单文件分享直接用；批量按 feiliu_i 序号选文件（gofile 同款防撞键）
    let index: usize = url
        .split("feiliu_i=")
        .nth(1)
        .and_then(|v| {
            let digits: String = v.chars().take_while(|c| c.is_ascii_digit()).collect();
            digits.parse().ok()
        })
        .unwrap_or(0);
    if index >= files.len() {
        return Some(super::Extracted {
            direct_url: String::new(),
            filename: format!("夸克分享 {pwd_id}"),
            referer: Some(REFERER.into()),
            notice: Some(format!("序号 {index} 超出分享文件数（共 {} 个）", files.len())),
            cookies: None,
            user_agent: None,
            webview_plan: None,
        });
    }
    let f = &files[index];

    // 登录态：转存链直下
    if login {
        return match transfer_download(&http, &pwd_id, &stoken, f).await {
            Ok(direct) => Some(super::Extracted {
                direct_url: direct,
                filename: f.name.clone(),
                referer: Some(REFERER.into()),
                notice: None,
                cookies: session.map(String::from),
                user_agent: None,
                webview_plan: None,
            }),
            Err(e) => Some(super::Extracted {
                direct_url: String::new(),
                filename: f.name.clone(),
                referer: Some(REFERER.into()),
                notice: Some(format!("夸克转存直下失败：{e}")),
                cookies: None,
                user_agent: None,
                webview_plan: None,
            }),
        };
    }

    // 匿名：访客直下（≤10MiB）
    match guest_download(&http, &pwd_id, &stoken, f).await {
        Ok(direct) => Some(super::Extracted {
            direct_url: direct,
            filename: f.name.clone(),
            referer: Some(REFERER.into()),
            notice: None,
            cookies: None,
            user_agent: None,
            webview_plan: None,
        }),
        Err(e) if e.starts_with("GUEST_LIMIT") => Some(super::Extracted {
            direct_url: String::new(),
            filename: f.name.clone(),
            referer: Some(REFERER.into()),
            notice: Some(format!(
                "「{}」({:.1} MB) 超出夸克访客免登录大小限制（≤10MiB）。在设置页登录夸克网盘后即可自动转存并直链下载。",
                f.name,
                f.size as f64 / 1048576.0
            )),
            cookies: None,
            user_agent: None,
            webview_plan: None,
        }),
        Err(e) => Some(super::Extracted {
            direct_url: String::new(),
            filename: f.name.clone(),
            referer: Some(REFERER.into()),
            notice: Some(e),
            cookies: None,
            user_agent: None,
            webview_plan: None,
        }),
    }
}

pub async fn resolve(url: &str) -> Option<super::Extracted> {
    resolve_inner(url, None).await
}

/// 批量清单：遍历分享树返回文件名+大小（engine add_quark_batch 数量来源）
pub async fn list_files(url: &str) -> Result<Vec<(String, u64)>, String> {
    let pwd_id = parse_share_url(url).ok_or("非夸克分享链接")?;
    let passcode = url
        .split("pwd=")
        .nth(1)
        .map(|p| p.trim_end_matches(['&', '#', '/']).to_string())
        .unwrap_or_default();
    let http = client(None).ok_or("HTTP 客户端初始化失败")?;
    let stoken = get_stoken(&http, &pwd_id, &passcode).await?;
    let files = walk_files(&http, &pwd_id, &stoken).await?;
    Ok(files.into_iter().map(|f| (f.name, f.size)).collect())
}

/// 登录态解析（quark_login 会话，__pus 核心）
pub async fn resolve_with_cookies(url: &str, session: &str) -> Option<super::Extracted> {
    resolve_inner(url, Some(session)).await
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 从 URL 提取 pwd_id
    #[test]
    fn test_parse_share_url() {
        assert_eq!(
            parse_share_url("https://pan.quark.cn/s/d34c18e9d62a#/list/share"),
            Some("d34c18e9d62a".into())
        );
        assert_eq!(
            parse_share_url("https://pan.quark.cn/s/fde352875597?pwd=abc"),
            Some("fde352875597".into())
        );
        assert_eq!(parse_share_url("https://pan.baidu.com/s/1abc"), None);
        assert_eq!(parse_share_url("https://pan.quark.cn/s/short"), None);
    }

    /// 真实链路（FEILIU_NET_TEST）：live 分享（GitHub issue 收割，r5 轮6 验活）
    /// 根目录为文件夹 → 遍历收集 8 个文件 → 全部 >10MiB → 访客超限诚实提示
    #[tokio::test]
    async fn test_quark_real_link() {
        if std::env::var("FEILIU_NET_TEST").is_err() {
            return;
        }
        let info = super::super::resolve_retry("https://pan.quark.cn/s/d34c18e9d62a#/list/share")
            .await
            .expect("夸克解析失败");
        // 根目录是「BiliTools最新版」文件夹，遍历后取到子目录文件名
        assert!(
            info.filename.contains("bilitools") || info.filename.contains("BiliTools"),
            "应遍历到子目录文件: {}",
            info.filename
        );
        assert!(
            info.notice.as_deref().unwrap_or("").contains("访客") && info.direct_url.is_empty(),
            "超限文件应给诚实提示而非直链: {:?}",
            info.notice
        );
        println!("quark OK: {} | {}", info.filename, info.notice.unwrap_or_default());
    }

    /// 真实链路（FEILIU_NET_TEST）：批量清单 = walk 遍历 8 个文件（d34c18e9d62a）
    #[tokio::test]
    async fn test_quark_list_files_real() {
        if std::env::var("FEILIU_NET_TEST").is_err() {
            return;
        }
        let files = list_files("https://pan.quark.cn/s/d34c18e9d62a")
            .await
            .expect("夸克清单解析失败");
        assert_eq!(files.len(), 8, "应遍历出 8 个文件");
        assert!(
            files.iter().all(|(n, sz)| !n.is_empty() && *sz > 10_000_000),
            "清单应含文件名与 >10MiB 大小"
        );
        println!("quark-list OK: {} 个文件", files.len());
    }
}
