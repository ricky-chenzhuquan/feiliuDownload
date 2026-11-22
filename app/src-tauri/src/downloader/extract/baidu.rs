//! 百度网盘（pan.baidu.com）分享链接解析。
//!
//! 匿名流程（2026-11 实测）：分享页拿 shareid → POST /share/verify 提交
//! 提取码（响应 Set-Cookie 种下 BDCLND，手动随后续请求携带）→ 重取页面拿
//! share_uk → /share/list?root=1 列出根结构。**匿名无下载直链**。
//!
//! 登录态流程（BDUSS 会话，应用内 WebView 登录收割，见 baidu_login）：
//! 转存链 share/transfer（转存到 /我的资源，errno=4 已转存幂等兼容）→
//! /api/filemetas 拿 dlink（8h 有效）——dlink 强制 `User-Agent: netdisk`
//! （普通 UA 403）+ 会话 cookie。批量走 feiliu_i 序号 URL 逐文件解析
//! （同 UC/gofile 批量模式）。2026-11-22 真机全链 206 实测验证。
//!
//! 子目录列表（2026-11-22 真浏览器抓包实锤）：新版分享子目录请求 =
//! `/share/list?uk&shareid&sekey={BDCLND}&is_from_web=true&dir={分享内路径}
//! &page=1&num=100&order=name&desc=0&showempty=0&view_mode=1&web=1
//! &channel=chunlei&app_id=250528&clienttype=0` —— sekey 即 verify 种下的
//! BDCLND 会话票，dir 用条目自带的 path 字段导航（parent_id 全形态
//! errno=2）。列表条目大整数（fs_id/isdir）为字符串形态，as_id/as_flag
//! 双形态解析。
//!
//! cookie 管理为手动链（客户端不启用 cookie_store，避免与手动 Cookie 头
//! 竞争）：验证种下的 BDCLND 从 Set-Cookie 捕获追加。
const DESKTOP_UA_VALUE: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0 Safari/537.36";
/// dlink 下载强制 UA（普通 UA 403）
pub const NETDISK_UA: &str = "netdisk";

/// 从 URL 提取 surl 与提取码
fn parse_share_url(url: &str) -> Option<(String, Option<String>)> {
    let idx = url.find("/s/1")?;
    let rest = &url[idx + 3..]; // 保留 "1" 前缀后的 key（surl 去掉 1）
    let key: String = rest
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    if key.len() < 20 {
        return None;
    }
    // surl = key 去掉首位的 1
    let surl = key.strip_prefix('1').unwrap_or(&key).to_string();
    // pwd 从 query 提取
    let pwd = url
        .split("pwd=")
        .nth(1)
        .map(|p| p.split('&').next().unwrap_or(p).to_string());
    Some((surl, pwd))
}

/// feiliu_i 序号（批量逐文件建任务用，同 UC/gofile 模式）
fn feiliu_index(url: &str) -> Option<usize> {
    for m in ["?feiliu_i=", "&feiliu_i="] {
        if let Some(pos) = url.find(m) {
            let rest = &url[pos + m.len()..];
            let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
            return digits.parse().ok();
        }
    }
    None
}

/// 路径/query 值 percent-encode（UTF-8 逐字节；'/' 不编码）
fn urlencoding_path(s: &str) -> String {
    s.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || matches!(b, b'/' | b'_' | b'-' | b'.' | b'~') {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

fn client() -> Option<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent(DESKTOP_UA_VALUE)
        .redirect(reqwest::redirect::Policy::limited(8))
        .build()
        .ok()
}

/// 追加 cookie 到链（"a=1; b=2" 形态；同名覆盖旧值）
fn append_cookie(chain: &str, kv: &str) -> String {
    let (name, _rest) = kv.split_once('=').unwrap_or((kv, ""));
    let name = name.trim();
    let kept: Vec<&str> = chain
        .split("; ")
        .filter(|p| {
            let n = p.split('=').next().unwrap_or("").trim();
            !n.is_empty() && n != name
        })
        .collect();
    let mut parts: Vec<String> = kept.into_iter().map(str::to_string).collect();
    parts.push(kv.trim().to_string());
    parts.join("; ")
}


async fn fetch_page_html(
    http: &reqwest::Client,
    surl: &str,
    cookie: &str,
) -> Option<(String, String)> {
    let mut req = http.get(format!("https://pan.baidu.com/s/1{surl}"));
    if !cookie.is_empty() {
        req = req.header(reqwest::header::COOKIE, cookie);
    }
    let resp = req.send().await.ok()?;
    let final_url = resp.url().to_string();
    let html = resp.text().await.ok()?;
    Some((final_url, html))
}

fn html_field(html: &str, key: &str) -> Option<String> {
    // 兼容 JSON 形态（"shareid":5370699210）与 JS 形态（share_uk:"824329580"）
    for pat in [format!("\"{key}\":"), format!("{key}:")] {
        let mut from = 0;
        while let Some(rel) = html[from..].find(&pat) {
            let abs = from + rel + pat.len();
            let rest = html[abs..].strip_prefix('"').unwrap_or(&html[abs..]);
            let v: String = rest
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric())
                .collect();
            if !v.is_empty() && v.chars().any(|c| c.is_ascii_digit()) {
                return Some(v);
            }
            from = abs; // 该处值非数字，继续向后找
        }
    }
    None
}

/// 提交提取码；返回 (BDCLND cookie 片段, randsk 会话票)——randsk 是新版
/// share/list 子目录请求 sekey 参数的真身（BDCLND 只是它的 cookie 形态）
async fn verify_pwd(
    http: &reqwest::Client,
    surl: &str,
    pwd: &str,
    cookie: &str,
) -> Option<(String, Option<String>)> {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let resp = http
        .post(format!(
            "https://pan.baidu.com/share/verify?surl={surl}&t={ts}"
        ))
        .header(
            "Referer",
            format!("https://pan.baidu.com/share/init?surl={surl}"),
        )
        .header(
            reqwest::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded",
        )
        .header(reqwest::header::COOKIE, cookie)
        .body(format!("pwd={pwd}"))
        .send()
        .await
        .ok()?;
    let headers = resp.headers().clone();
    let bdclnd = capture_set_cookie_from(&headers, "BDCLND")?;
    // randsk 从响应体取（新版返回 {err_msg, errno, randsk}）
    let randsk = resp
        .json::<serde_json::Value>()
        .await
        .ok()
        .and_then(|v| v.get("randsk").and_then(|r| r.as_str()).map(str::to_string));
    Some((bdclnd, randsk))
}

/// 从已克隆的响应头里摘某个 cookie 的 "name=value" 片段
fn capture_set_cookie_from(headers: &reqwest::header::HeaderMap, name: &str) -> Option<String> {
    for v in headers.get_all(reqwest::header::SET_COOKIE) {
        if let Ok(s) = v.to_str() {
            for part in s.split(';') {
                let p = part.trim();
                if p.starts_with(&format!("{name}=")) {
                    return Some(p.to_string());
                }
            }
        }
    }
    None
}

/// 列出分享结构：None=根目录（root=1）；Some(path)=子目录（新版参数集
/// sekey=BDCLND + dir=path，2026-11-22 真浏览器抓包实锤）
async fn list_share(
    http: &reqwest::Client,
    shareid: &str,
    uk: &str,
    dir_path: Option<&str>,
    sekey: &str,
    cookie: &str,
) -> Option<Vec<serde_json::Value>> {
    let url = match dir_path {
        None => format!(
            "https://pan.baidu.com/share/list?shareid={shareid}&uk={uk}&root=1&web=1&app_id=250528&clienttype=0"
        ),
        Some(dir) => {
            // sekey=randsk 已是 URL 编码形态（%2F 开头），直接拼原文勿再编码
            format!(
                "https://pan.baidu.com/share/list?uk={uk}&shareid={shareid}&sekey={sekey}&is_from_web=true&dir={}&page=1&num=1000&order=name&desc=0&showempty=0&view_mode=1&web=1&channel=chunlei&app_id=250528&clienttype=0",
                urlencoding_path(dir)
            )
        }
    };
    let mut req = http.get(url).header("Referer", "https://pan.baidu.com/s/1");
    if !cookie.is_empty() {
        req = req.header(reqwest::header::COOKIE, cookie);
    }
    let resp = req.send().await.ok()?;
    let v: serde_json::Value = resp.json().await.ok()?;
    if v.get("errno").and_then(as_flag) != Some(0) {
        return None;
    }
    Some(v.get("list")?.as_array()?.clone())
}

/// 数字/字符串双形态取 id（新版 share/list 大整数以字符串传输防 JS 精度丢失）
fn as_id(v: &serde_json::Value) -> Option<String> {
    if let Some(i) = v.as_i64() {
        return Some(i.to_string());
    }
    v.as_str().map(str::to_string)
}

/// 数字/字符串双形态取整数标记（isdir/errno/size 等）
fn as_flag(v: &serde_json::Value) -> Option<i64> {
    v.as_i64().or_else(|| v.as_str().and_then(|s| s.parse().ok()))
}

/// 登录态上下文：cookie 链（含 BDCLND）+ shareid + uk + randsk 会话票
#[allow(clippy::type_complexity)]
async fn login_context(
    http: &reqwest::Client,
    surl: &str,
    pwd: Option<&str>,
    session: &str,
) -> Option<(String, String, String, Option<String>)> {
    let mut cookie = session.to_string();
    let (_, html) = fetch_page_html(http, surl, &cookie).await?;
    let shareid = html_field(&html, "shareid")?;
    let mut randsk = None;
    if let Some(pwd) = pwd {
        if let Some((bdclnd, r)) = verify_pwd(http, surl, pwd, &cookie).await {
            cookie = append_cookie(&cookie, &bdclnd);
            randsk = r;
        }
    }
    let (_, html2) = fetch_page_html(http, surl, &cookie).await?;
    let uk = html_field(&html2, "share_uk").or_else(|| html_field(&html2, "uk"))?;
    if uk == "0" {
        return None; // 提取码错误或链接失效
    }
    Some((cookie, shareid, uk, randsk))
}

/// BFS 扁平展开分享内全部文件（name, dlink, size），深度上限 3 层。
/// 条目自带 path 字段（分享内绝对路径），子目录导航直接用它（dir=path，
/// sekey=randsk 会话票）。
async fn walk_files(
    http: &reqwest::Client,
    shareid: &str,
    uk: &str,
    sekey: &str,
    cookie: &str,
    out: &mut Vec<(String, String, u64)>,
) -> Option<()> {
    let mut level: Vec<String> = Vec::new(); // 空 = 根（root=1）
    for _depth in 0..=3 {
        let mut next: Vec<String> = Vec::new();
        let items = list_share(
            http,
            shareid,
            uk,
            if level.is_empty() { None } else { Some(&level[0]) },
            sekey,
            cookie,
        )
        .await?;
        for it in items {
            let name = it
                .get("server_filename")
                .and_then(|n| n.as_str())
                .unwrap_or("文件")
                .to_string();
            let path = it
                .get("path")
                .and_then(|p| p.as_str())
                .unwrap_or("")
                .to_string();
            if it.get("isdir").and_then(as_flag) == Some(1) {
                if !path.is_empty() {
                    next.push(path);
                }
            } else {
                let fs_id = it.get("fs_id").and_then(as_id).unwrap_or_default();
                let size = it
                    .get("size")
                    .and_then(as_flag)
                    .and_then(|s| u64::try_from(s).ok())
                    .unwrap_or(0);
                out.push((name, fs_id, size));
            }
        }
        if next.is_empty() {
            break;
        }
        level = next;
    }
    Some(())
}

/// 转存链拿 dlink：share/transfer（转存到「我的资源」）→ /api/filemetas。
/// errno=0 新转存、errno=4 已转存（duplicated.list[0].fs_id 指向既有文件）都算通。
/// 返回 (dlink, 新/既有网盘 fs_id)。转存语义：文件出现在用户网盘 /我的资源。
async fn fetch_dlink(
    http: &reqwest::Client,
    cookie: &str,
    shareid: &str,
    uk: &str,
    sekey: &str,
    fs_id: &str,
) -> Option<(String, String)> {
    // 1) 转存（path=/我的资源，实测存在；/来自：分享 目录不存在会 errno=2）
    let resp = http
        .post(format!(
            "https://pan.baidu.com/share/transfer?shareid={shareid}&from={uk}&sekey={sekey}&app_id=250528&clienttype=0&web=1"
        ))
        .header("Referer", "https://pan.baidu.com/s/1")
        .header(
            reqwest::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded",
        )
        .header(reqwest::header::COOKIE, cookie)
        .body(format!(
            "fsidlist=%5B{fs_id}%5D&path=%2F%E6%88%91%E7%9A%84%E8%B5%84%E6%BA%90"
        ))
        .send()
        .await
        .ok()?;
    let b: serde_json::Value = resp.json().await.ok()?;
    match b.get("errno").and_then(as_flag) {
        Some(0) | Some(4) => {}
        _ => return None, // 风控/容量/权限等失败，诚实上抛
    }
    let to_fs = b
        .pointer("/info/0/to_fs_id")
        .or_else(|| b.pointer("/data/success/0/to_fs_id"))
        .or_else(|| b.pointer("/duplicated/list/0/fs_id"))
        .or_else(|| b.pointer("/info/0/fs_id"))
        .and_then(as_id)?;
    // 2) filemetas → dlink（8h 有效）
    let resp = http
        .get(format!(
            "https://pan.baidu.com/api/filemetas?fsids=%5B{to_fs}%5D&dlink=1&app_id=250528&clienttype=0&web=1"
        ))
        .header("Referer", "https://pan.baidu.com/")
        .header(reqwest::header::COOKIE, cookie)
        .send()
        .await
        .ok()?;
    let b: serde_json::Value = resp.json().await.ok()?;
    let dlink = b
        .pointer("/info/0/dlink")
        .or_else(|| b.pointer("/list/0/dlink"))
        .and_then(|d| d.as_str())
        .map(str::to_string)?;
    Some((dlink, to_fs))
}

/// dlink 任务的 Extracted：直链 + 会话 cookie + 强制 netdisk UA（缺任一 403）
fn build_extracted(name: String, dlink: String, cookie: &str, login: bool) -> super::Extracted {
    super::Extracted {
        direct_url: dlink,
        filename: name,
        referer: Some("https://pan.baidu.com/".into()),
        notice: None,
        cookies: if login { Some(cookie.to_string()) } else { None },
        user_agent: Some(NETDISK_UA.into()),
        webview_plan: None,
    }
}

pub async fn resolve(url: &str) -> Option<super::Extracted> {
    resolve_inner(url, None).await
}

/// 登录态解析（BDUSS 会话，baidu_login 收割）
pub async fn resolve_with_cookies(url: &str, session: &str) -> Option<super::Extracted> {
    resolve_inner(url, Some(session)).await
}

async fn resolve_inner(url: &str, session: Option<&str>) -> Option<super::Extracted> {
    let (surl, pwd) = parse_share_url(url)?;
    let login = session.is_some();
    let http = client()?;

    // 匿名：cookie 链从空起（BDCLND 由 verify 捕获）；登录：从会话起
    let mut cookie = session.unwrap_or("").to_string();

    // 1) 首次访问：拿 shareid（无密码时会重定向到 /share/init）
    let (_, html) = fetch_page_html(&http, &surl, &cookie).await?;
    let shareid = html_field(&html, "shareid")?;

    // 2) 有提取码 → verify；Set-Cookie 捕获 BDCLND 追加 + randsk 会话票
    let mut randsk = None;
    if let Some(pwd) = pwd.as_deref() {
        if let Some((bdclnd, r)) = verify_pwd(&http, &surl, pwd, &cookie).await {
            cookie = append_cookie(&cookie, &bdclnd);
            randsk = r;
        }
    }

    // 3) 重取页面拿 share_uk
    let (_, html2) = fetch_page_html(&http, &surl, &cookie).await?;
    let uk = html_field(&html2, "share_uk").or_else(|| html_field(&html2, "uk"))?;
    if uk == "0" {
        return None; // 提取码错误或链接失效
    }

    // 4) feiliu_i 序号（批量逐文件）：扁平展开取第 N 个文件 → 转存链拿 dlink
    if let Some(idx) = feiliu_index(url) {
        let mut files = Vec::new();
        walk_files(&http, &shareid, &uk, randsk.as_deref().unwrap_or(""), &cookie, &mut files)
            .await?;
        let (name, fs_id, _size) = files.into_iter().nth(idx)?;
        let Some(randsk_v) = randsk.as_deref().filter(|_| login) else {
            return None; // feiliu_i URL 由批量内部生成，登录态专用
        };
        let Some((dlink, _)) = fetch_dlink(&http, &cookie, &shareid, &uk, randsk_v, &fs_id).await
        else {
            return None; // 转存失败（容量/风控）诚实失败，重试机制兜底
        };
        return Some(build_extracted(name, dlink, &cookie, true));
    }

    // 5) 常规：列根目录
    let items = match list_share(&http, &shareid, &uk, None, "", &cookie).await {
        Some(i) => i,
        None if pwd.is_none() && !login => {
            // 带提取码的分享但输入未携带 pwd：诚实提示而非「无法解析」
            // （分享文本的提取码在 URL 之外，前端已自动拼接 pwd=）
            return Some(super::Extracted {
                direct_url: String::new(),
                filename: "百度网盘分享（需提取码）".into(),
                referer: Some("https://pan.baidu.com/".into()),
                notice: Some(
                    "该百度网盘分享需要提取码：请粘贴带「提取码：xxxx」的完整分享文本（会自动识别），或链接后附 ?pwd=提取码".into(),
                ),
                cookies: None,
                user_agent: None,
                webview_plan: None,
            });
        }
        None => return None, // 提取码错误/链接失效
    };
    let files: Vec<&serde_json::Value> = items
        .iter()
        .filter(|i| i.get("isdir").and_then(as_flag) == Some(0))
        .collect();
    let folders = items.len() - files.len();

    // 6) 登录态 + 单文件 → 转存链直接拿 dlink 可下载
    if login && files.len() == 1 && folders == 0 {
        let f = files[0];
        if let (Some(name), Some(fs_id)) = (
            f.get("server_filename").and_then(|n| n.as_str()).map(str::to_string),
            f.get("fs_id").and_then(as_id),
        ) {
            if let Some((dlink, _)) = fetch_dlink(
                &http,
                &cookie,
                &shareid,
                &uk,
                randsk.as_deref().unwrap_or(""),
                &fs_id,
            )
            .await
            {
                return Some(build_extracted(name, dlink, &cookie, true));
            }
        }
    }

    // 7) 摘要 + 诚实提示（匿名无直链；登录态多文件引导批量）
    let (filename, notice) = if files.len() == 1 && folders == 0 {
        let f = files[0];
        let name = f
            .get("server_filename")
            .and_then(|n| n.as_str())
            .unwrap_or("文件")
            .to_string();
        let size = f
            .get("size")
            .and_then(as_flag)
            .and_then(|s| u64::try_from(s).ok())
            .unwrap_or(0);
        let note = if size > 0 {
            format!("（{}）", crate::downloader::engine::human_size(size))
        } else {
            String::new()
        };
        let notice = if login {
            "百度网盘分享：已登录但转存链未取得下载直链（平台限制或容量不足，可在官网转存后下载）".to_string()
        } else {
            "百度网盘分享：应用可列出分享内容（提取码已自动验证），但匿名无下载直链——在「设置 → 百度网盘」登录自己的账号后重试即可直接下载".to_string()
        };
        (format!("{name}{note}"), notice)
    } else {
        let first = items
            .first()
            .and_then(|i| i.get("server_filename"))
            .and_then(|n| n.as_str())
            .unwrap_or("分享内容");
        let summary = if folders > 0 && files.len() > 0 {
            format!("{} 个文件夹、{} 个文件", folders, files.len())
        } else if folders > 0 {
            format!("{} 个文件夹", folders)
        } else {
            format!("{} 个文件", files.len())
        };
        let notice = if login {
            "百度网盘分享：多个文件可一键批量下载（登录态直链）".to_string()
        } else {
            "百度网盘分享：应用可列出分享内容（提取码已自动验证），但匿名无下载直链——在「设置 → 百度网盘」登录自己的账号后即可直接下载或批量下载".to_string()
        };
        (format!("{first}（{summary}）"), notice)
    };

    Some(super::Extracted {
        direct_url: String::new(), // 匿名/未取得直链
        filename,
        referer: Some("https://pan.baidu.com/".into()),
        notice: Some(notice),
        cookies: None,
        user_agent: None,
        webview_plan: None,
    })
}

/// 百度网盘分享一键批量（登录态）：扁平清单 → 每文件一个 feiliu_i 序号 URL
/// （同 UC/gofile 批量模式；会话必须已登录）
pub async fn folder_file_urls(url: &str) -> Option<(String, Vec<String>)> {
    let session = crate::downloader::baidu_login::load_session()?.cookies;
    let (surl, pwd) = parse_share_url(url)?;
    let http = client()?;
    let (cookie, shareid, uk, randsk) =
        login_context(&http, &surl, pwd.as_deref(), &session).await?;
    let mut files = Vec::new();
    walk_files(&http, &shareid, &uk, randsk.as_deref().unwrap_or(""), &cookie, &mut files).await?;
    if files.is_empty() {
        return None;
    }
    let base = url.split(['?', '&']).next().unwrap_or(url).to_string();
    let urls = (0..files.len())
        .map(|i| format!("{base}?pwd={}&feiliu_i={}", pwd.as_deref().unwrap_or(""), i))
        .collect();
    let name = files
        .first()
        .map(|(n, _, _)| n.clone())
        .unwrap_or_else(|| format!("baidu_{surl}"));
    Some((name, urls))
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_parse_share_url() {
        let (surl, pwd) =
            super::parse_share_url("https://pan.baidu.com/s/1z3VmdlAW-Gy4nlu-sF09aQ?pwd=31e7")
                .unwrap();
        assert_eq!(surl, "z3VmdlAW-Gy4nlu-sF09aQ");
        assert_eq!(pwd.as_deref(), Some("31e7"));
        assert!(super::parse_share_url("https://example.com/foo").is_none());
    }

    #[test]
    fn test_append_cookie() {
        let c = super::append_cookie("BDUSS=abc; STOKEN=def", "BDCLND=xyz");
        assert!(c.contains("BDUSS=abc") && c.contains("BDCLND=xyz"));
        // 同名覆盖
        let c2 = super::append_cookie(&c, "BDCLND=zzz");
        assert!(c2.contains("BDCLND=zzz") && !c2.contains("xyz"));
    }

    #[test]
    fn test_feiliu_index() {
        assert_eq!(
            super::feiliu_index("https://pan.baidu.com/s/1abc?pwd=kN66&feiliu_i=3"),
            Some(3)
        );
        assert_eq!(super::feiliu_index("https://pan.baidu.com/s/1abc"), None);
    }

    #[test]
    fn test_urlencoding_path() {
        assert_eq!(super::urlencoding_path("/a b/c"), "/a%20b/c");
        assert_eq!(
            super::urlencoding_path("/喋血复仇"),
            "/%E5%96%8B%E8%A1%80%E5%A4%8D%E4%BB%87"
        );
    }

    /// 真实分享：密码验证 + share/list 匿名列出结构
    #[tokio::test]
    async fn test_baidu_real_link() {
        if std::env::var("FEILIU_NET_TEST").is_err() {
            return;
        }
        let info = super::super::resolve_retry(
            "https://pan.baidu.com/s/1z3VmdlAW-Gy4nlu-sF09aQ?pwd=31e7",
        )
        .await
        .expect("百度网盘解析失败");
        assert!(
            info.filename.contains("听歌吧") || info.filename.contains("文件夹"),
            "应解析出分享内容: {}",
            info.filename
        );
        assert!(info.notice.is_some(), "应包含需登录提示");
        println!("baidu OK: {}", info.filename);
    }
    #[tokio::test]
    async fn test_baidu_real_share_with_pwd() {
        if std::env::var("FEILIU_NET_TEST").is_err() {
            return;
        }
        let info = super::super::resolve_retry(
            "https://pan.baidu.com/s/112xu4YaSMrg8t1FPIg_8lg?pwd=kN66",
        )
        .await
        .expect("百度网盘真实分享解析失败");
        assert!(
            info.notice.is_some(),
            "匿名无直链，应有转存引导提示: {:?}",
            info.notice
        );
        assert!(
            info.filename.contains("喋血复仇"),
            "应列出真实内容: {}",
            info.filename
        );
        assert!(info.direct_url.is_empty(), "匿名无下载直链");
        println!(
            "baidu real OK: {} | notice={:?}",
            info.filename,
            info.notice
                .as_deref()
                .map(|n| n.chars().take(20).collect::<String>())
        );
    }

    /// 登录态解析 e2e（真实会话存在时才跑）：feiliu_i 序号取文件 → dlink
    /// 直链 + netdisk UA + 会话 cookie，并裁决 dlink 真实可下（Range 1KB）
    #[tokio::test]
    async fn test_baidu_login_resolve() {
        if std::env::var("FEILIU_NET_TEST").is_err() {
            return;
        }
        let Some(session) = crate::downloader::baidu_login::load_session() else {
            println!("baidu-login skip: 无会话文件（未登录）");
            return;
        };
        let info = super::super::resolve_retry(
            "https://pan.baidu.com/s/112xu4YaSMrg8t1FPIg_8lg?pwd=kN66&feiliu_i=0",
        )
        .await
        .expect("百度网盘登录态解析失败");
        assert!(
            !info.direct_url.is_empty(),
            "登录态应取得 dlink 直链: {} | notice={:?}",
            info.filename,
            info.notice
        );
        assert_eq!(info.user_agent.as_deref(), Some(super::NETDISK_UA));
        assert!(info.cookies.is_some(), "dlink 下载必须带会话 cookie");
        println!(
            "baidu-login OK: {} dlink={}..",
            info.filename,
            &info.direct_url[..60.min(info.direct_url.len())]
        );
        // dlink 真实可下裁决：netdisk UA + 会话 cookie + Range 1KB
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::limited(5))
            .build()
            .unwrap();
        let resp = client
            .get(&info.direct_url)
            .header(reqwest::header::USER_AGENT, super::NETDISK_UA)
            .header(reqwest::header::COOKIE, session.cookies)
            .header(reqwest::header::RANGE, "bytes=0-1023")
            .send()
            .await
            .expect("dlink 请求失败");
        let status = resp.status();
        let ctype = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        let bytes = resp.bytes().await.map(|b| b.len()).unwrap_or(0);
        println!("baidu-dlink download: HTTP {status} type={ctype:?} bytes={bytes}");
        assert!(
            status.as_u16() == 200 || status.as_u16() == 206,
            "dlink 下载应 200/206（netdisk UA + 会话 cookie），实得 {status}"
        );
        assert!(bytes > 0, "dlink 应返回真实字节");
    }
}
