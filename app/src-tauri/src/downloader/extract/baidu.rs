//! 百度网盘（pan.baidu.com）分享链接解析。
//!
//! 匿名流程（2026-11 实测）：分享页拿 shareid → POST /share/verify 提交
//! 提取码（响应 Set-Cookie 种下 BDCLND，手动随后续请求携带）→ 重取页面拿
//! share_uk → /share/list 列出结构。**匿名无下载直链**。
//!
//! 登录态流程（BDUSS 会话，应用内 WebView 登录收割，见 baidu_login）：
//! 同一链路携带 BDUSS/STOKEN，share/list 返回的 dlink 直取下载——dlink
//! 强制 `User-Agent: netdisk`（普通 UA 403）+ 会话 cookie。批量走
//! feiliu_i 序号 URL 逐文件解析（同 UC/gofile 批量模式）。
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

/// 从响应 Set-Cookie 头里摘某个 cookie 的 "name=value" 片段
fn capture_set_cookie(resp: &reqwest::Response, name: &str) -> Option<String> {
    for v in resp.headers().get_all(reqwest::header::SET_COOKIE) {
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

/// 提交提取码；响应 Set-Cookie 里的 BDCLND 拼回 cookie 链
async fn verify_pwd(
    http: &reqwest::Client,
    surl: &str,
    pwd: &str,
    cookie: &str,
) -> Option<String> {
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
    capture_set_cookie(&resp, "BDCLND")
}

async fn list_share(
    http: &reqwest::Client,
    shareid: &str,
    uk: &str,
    parent: Option<&str>,
    cookie: &str,
) -> Option<Vec<serde_json::Value>> {
    let url = match parent {
        None => format!(
            "https://pan.baidu.com/share/list?shareid={shareid}&uk={uk}&root=1"
        ),
        Some(pid) => format!(
            "https://pan.baidu.com/share/list?shareid={shareid}&uk={uk}&dir=1&parent_id={pid}"
        ),
    };
    let mut req = http.get(url).header("Referer", "https://pan.baidu.com/s/1");
    if !cookie.is_empty() {
        req = req.header(reqwest::header::COOKIE, cookie);
    }
    let resp = req.send().await.ok()?;
    let v: serde_json::Value = resp.json().await.ok()?;
    if v.get("errno").and_then(|e| e.as_i64()) != Some(0) {
        return None;
    }
    Some(v.get("list")?.as_array()?.clone())
}

fn item_dlink(item: &serde_json::Value) -> Option<String> {
    item.get("dlink")
        .and_then(|d| d.as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// 登录态上下文：cookie 链（含 BDCLND）+ shareid + uk
async fn login_context(
    http: &reqwest::Client,
    surl: &str,
    pwd: Option<&str>,
    session: &str,
) -> Option<(String, String, String)> {
    let mut cookie = session.to_string();
    let (_, html) = fetch_page_html(http, surl, &cookie).await?;
    let shareid = html_field(&html, "shareid")?;
    if let Some(pwd) = pwd {
        if let Some(bdclnd) = verify_pwd(http, surl, pwd, &cookie).await {
            cookie = append_cookie(&cookie, &bdclnd);
        }
    }
    let (_, html2) = fetch_page_html(http, surl, &cookie).await?;
    let uk = html_field(&html2, "share_uk").or_else(|| html_field(&html2, "uk"))?;
    if uk == "0" {
        return None; // 提取码错误或链接失效
    }
    Some((cookie, shareid, uk))
}

/// 递归扁平展开分享内全部文件（name, dlink, size），BFS 深度上限 3 层
/// （parent=None 从根开始；iterative 避免async fn 递归装箱）
async fn walk_files(
    http: &reqwest::Client,
    shareid: &str,
    uk: &str,
    cookie: &str,
    parent: Option<&str>,
    out: &mut Vec<(String, String, u64)>,
) -> Option<()> {
    let mut level: Vec<Option<String>> = vec![parent.map(str::to_string)];
    for _depth in 0..=3 {
        if level.is_empty() {
            break;
        }
        let mut next: Vec<Option<String>> = Vec::new();
        for pid in &level {
            let items = list_share(http, shareid, uk, pid.as_deref(), cookie).await?;
            for it in items {
                let name = it
                    .get("server_filename")
                    .and_then(|n| n.as_str())
                    .unwrap_or("文件")
                    .to_string();
                if it.get("isdir").and_then(|d| d.as_i64()) == Some(1) {
                    let fid = it
                        .get("fs_id")
                        .and_then(|f| f.as_i64())
                        .map(|f| f.to_string());
                    if let Some(p) = fid {
                        next.push(Some(p));
                    }
                } else {
                    let dlink = item_dlink(&it).unwrap_or_default();
                    let size = it.get("size").and_then(|s| s.as_u64()).unwrap_or(0);
                    out.push((name.clone(), dlink, size));
                }
            }
        }
        level = next;
    }
    Some(())
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

    // 2) 有提取码 → verify；Set-Cookie 捕获 BDCLND 追加
    if let Some(pwd) = pwd.as_deref() {
        if let Some(bdclnd) = verify_pwd(&http, &surl, pwd, &cookie).await {
            cookie = append_cookie(&cookie, &bdclnd);
        }
    }

    // 3) 重取页面拿 share_uk
    let (_, html2) = fetch_page_html(&http, &surl, &cookie).await?;
    let uk = html_field(&html2, "share_uk").or_else(|| html_field(&html2, "uk"))?;
    if uk == "0" {
        return None; // 提取码错误或链接失效
    }

    // 4) feiliu_i 序号（批量逐文件）：扁平展开取第 N 个文件
    if let Some(idx) = feiliu_index(url) {
        let mut files = Vec::new();
        walk_files(&http, &shareid, &uk, &cookie, None, &mut files).await?;
        let (name, dlink, _size) = files.into_iter().nth(idx)?;
        return Some(build_extracted(name, dlink, &cookie, login));
    }

    // 5) 常规：列根目录
    let items = match list_share(&http, &shareid, &uk, None, &cookie).await {
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
        .filter(|i| i.get("isdir").and_then(|d| d.as_i64()) == Some(0))
        .collect();
    let folders = items.len() - files.len();

    // 6) 登录态 + 单文件 + dlink 在手 → 直接可下载
    if login && files.len() == 1 && folders == 0 {
        if let (Some(name), Some(dlink)) = (
            files[0]
                .get("server_filename")
                .and_then(|n| n.as_str())
                .map(str::to_string),
            item_dlink(files[0]),
        ) {
            return Some(build_extracted(name, dlink, &cookie, true));
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
        let size = f.get("size").and_then(|s| s.as_u64()).unwrap_or(0);
        let note = if size > 0 {
            format!("（{}）", crate::downloader::engine::human_size(size))
        } else {
            String::new()
        };
        let notice = if login {
            "百度网盘分享：已登录但未能取得该文件的下载直链（平台限制，可能需转存到自己网盘后下载）".to_string()
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
    let (cookie, shareid, uk) = login_context(&http, &surl, pwd.as_deref(), &session).await?;
    let mut files = Vec::new();
    walk_files(&http, &shareid, &uk, &cookie, None, &mut files).await?;
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

    /// 登录态解析（真实会话文件存在时才跑）：dlink 直取 + netdisk UA + 会话 cookie
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
            "https://pan.baidu.com/s/112xu4YaSMrg8t1FPIg_8lg?pwd=kN66",
        )
        .await
        .expect("百度网盘登录态解析失败");
        assert!(
            !info.direct_url.is_empty(),
            "登录态应取得 dlink 直链: {}",
            info.filename
        );
        assert_eq!(info.user_agent.as_deref(), Some(super::NETDISK_UA));
        assert!(info.cookies.is_some(), "dlink 下载必须带会话 cookie");
        println!(
            "baidu-login OK: {} dlink={}..",
            info.filename,
            &info.direct_url[..60.min(info.direct_url.len())]
        );
    }
}
