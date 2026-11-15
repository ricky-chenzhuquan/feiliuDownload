//! 百度网盘（pan.baidu.com）分享链接解析。
//!
//! 匿名流程（2026-11 实测）：取分享页拿 shareid → POST /share/verify 提交
//! 提取码（种下 BDCLND 会话 Cookie，依赖 reqwest cookie store）→ 重取页面拿
//! share_uk → /share/list 列出结构。**下载直链必须登录**，文件夹还需官方
//! 客户端（网页端点「下载」直接弹「安装客户端」对话框），因此产出内容摘要
//! + 明确提示，由用户在官网/客户端转存下载。
const DESKTOP_UA_VALUE: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0 Safari/537.36";

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

fn desktop_client() -> Option<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent(DESKTOP_UA_VALUE)
        .redirect(reqwest::redirect::Policy::limited(8))
        .cookie_store(true) // 百度验证流程依赖会话 Cookie
        .build()
        .ok()
}

async fn fetch_page_html(
    http: &reqwest::Client,
    surl: &str,
) -> Option<(String, String)> {
    let resp = http
        .get(format!("https://pan.baidu.com/s/1{surl}"))
        .send()
        .await
        .ok()?;
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

async fn list_share(
    http: &reqwest::Client,
    shareid: &str,
    uk: &str,
    parent: Option<&str>,
) -> Option<Vec<serde_json::Value>> {
    let url = match parent {
        None => format!(
            "https://pan.baidu.com/share/list?shareid={shareid}&uk={uk}&root=1"
        ),
        Some(pid) => format!(
            "https://pan.baidu.com/share/list?shareid={shareid}&uk={uk}&dir=1&parent_id={pid}"
        ),
    };
    let resp = http
        .get(url)
        .header("Referer", "https://pan.baidu.com/s/1")
        .send()
        .await
        .ok()?;
    let v: serde_json::Value = resp.json().await.ok()?;
    if v.get("errno").and_then(|e| e.as_i64()) != Some(0) {
        return None;
    }
    Some(v.get("list")?.as_array()?.clone())
}

pub async fn resolve(url: &str) -> Option<super::Extracted> {
    let (surl, pwd) = parse_share_url(url)?;
    let http = desktop_client()?;

    // 1) 首次访问：拿 Cookie 与 shareid（无密码时会重定向到 /share/init）
    let (_, html) = fetch_page_html(&http, &surl).await?;
    let shareid = html_field(&html, "shareid")?;

    // 2) 有提取码 → verify（Cookie 自动种下 BDCLND）
    if let Some(pwd) = &pwd {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        let _ = http
            .post(format!(
                "https://pan.baidu.com/share/verify?surl={surl}&t={ts}"
            ))
            .header("Referer", format!("https://pan.baidu.com/share/init?surl={surl}"))
            .header(reqwest::header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(format!("pwd={pwd}"))
            .send()
            .await
            .ok();
    }

    // 3) 重取页面拿 share_uk
    let (_, html2) = fetch_page_html(&http, &surl).await?;
    let uk = html_field(&html2, "share_uk").or_else(|| html_field(&html2, "uk"))?;
    if uk == "0" {
        return None; // 提取码错误或链接失效
    }

    // 4) 列出根目录
    let items = list_share(&http, &shareid, &uk, None).await?;
    let files: Vec<&serde_json::Value> = items
        .iter()
        .filter(|i| i.get("isdir").and_then(|d| d.as_i64()) == Some(0))
        .collect();
    let folders = items.len() - files.len();

    let (filename, _size_note) = if files.len() == 1 && folders == 0 {
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
        (format!("{name}{note}"), String::new())
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
        (format!("{first}（{summary}）"), String::new())
    };

    Some(super::Extracted {
        direct_url: String::new(), // 匿名无下载直链
        filename,
        referer: Some("https://pan.baidu.com/".into()),
        notice: Some(
            "百度网盘分享：应用可列出分享内容（提取码已自动验证），但下载必须登录百度账号，文件夹还需官方客户端或先转存到自己网盘（平台限制，匿名无下载直链）".into(),
        ),
    })
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

    /// 真实分享：密码验证 + share/list 匿名列出结构
    #[tokio::test]
    async fn test_baidu_real_link() {
        if std::env::var("FEILIU_NET_TEST").is_err() {
            return;
        }
        let info = super::super::resolve(
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
}
