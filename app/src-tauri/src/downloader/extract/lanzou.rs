//! 蓝奏云（lanzou 系列域名）分享解析。
//!
//! 实测结论（2026-11-18 / 2026-11-20）：
//! - 分享结构匿名可解析：文件夹页提取 sign/t/k/puid/uid → filemoreajax 列表
//!   （zt:1，字段 name_all/id/size，id 即短链后缀）；单文件页 title 含文件名
//! - 直链获取被 TLS 指纹墙拦截：fn 页的 ajaxfile POST 仅真实浏览器指纹可过
//!   （curl/rustls 均 403）→ 应用内嵌 WebView 代取（真 Safari 指纹）已实现
//! - 文件夹批量：列表 id → 逐个单文件页 URL → 引擎批量逐个 add_task
//!   （每个文件走一次 WebView 代取，速度较慢属预期）
use serde_json::Value;

const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0 Safari/537.36";

/// 蓝奏云系列域名（子域多变：ydb6./www. 等）
pub fn is_lanzou(url: &str) -> bool {
    let lower = url.to_lowercase();
    ["lanzou.com", "lanzoui.com", "lanzouw.com", "lanzoue.com", "lanzouf.com", "lanzn.com", "lanpv.com", "lanzoub.com"]
        .iter()
        .any(|d| lower.contains(d))
}

fn page_client() -> Option<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent(UA)
        .redirect(reqwest::redirect::Policy::limited(8))
        .cookie_store(true)
        .build()
        .ok()
}

/// 从文件夹页 JS 变量区提取参数（'lx':2 / var ibjlt7 = '123' 等混淆形态）
fn js_field(html: &str, key: &str) -> Option<String> {
    // JSON 形态 'key':'value' / 'key':123
    let pats = [
        format!("'{key}':'"),
        format!("'{key}':"),
        format!("var {key} = '"),
    ];
    for pat in pats {
        if let Some(idx) = html.find(&pat) {
            let rest = &html[idx + pat.len()..];
            let v: String = rest
                .chars()
                .take_while(|c| {
                    c.is_ascii_alphanumeric()
                        || matches!(c, '_' | '-' | '/' | '+' | '=')
                })
                .collect();
            // _hhxv0 之类变量名不做值（长度>60 视为误匹配）
            if !v.is_empty() && v.len() <= 200 {
                return Some(v);
            }
        }
    }
    None
}

/// 从页面 iframe src 提取文件 id（单文件页 → /iXXXX → ajaxfile file 数字在 fn 页）
fn file_page_name(html: &str) -> Option<String> {
    let idx = html.find("<title>")?;
    let start = idx + "<title>".len();
    let end = html[start..].find("</title>")? + start;
    let title = html[start..end].trim().to_string();
    // 去掉 " - 蓝奏云" 尾巴
    let name = title
        .split(" - 蓝奏云")
        .next()
        .unwrap_or(&title)
        .trim()
        .to_string();
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

/// 文件夹页 → 拉取 filemoreajax 列表，返回 (final_url, items)
/// （sign/t/k/puid/uid 提取 + POST；t/k 变量名每次请求随机化，按值形态定位）
async fn fetch_folder_items(
    http: &reqwest::Client,
    url: &str,
) -> Option<(String, Vec<Value>)> {
    let resp = http.get(url).send().await.ok()?;
    let final_url = resp.url().to_string();
    let html = resp.text().await.ok()?;
    if !html.contains("filemoreajax") {
        return None;
    }

    let sign = js_field(&html, "sign")?;
    let fid = js_field(&html, "fid")?;
    let uid = js_field(&html, "uid")?;
    let puid = js_field(&html, "puid").unwrap_or_default();
    // t/k 的变量名每次请求随机化（ibjlt7/iblrqr…），按值的形态定位：
    // t = 10~13 位时间戳；k = 32 位 hex（'k':<变量名> → var <名> = '<hex>'）
    let re_t = regex::Regex::new(r"var\s+\w+\s*=\s*'(\d{10,13})'").unwrap();
    let t = re_t
        .captures(&html)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
        .unwrap_or_default();
    let re_kvar = regex::Regex::new(r"'k'\s*:\s*(\w+)").unwrap();
    let k = re_kvar
        .captures(&html)
        .and_then(|c| c.get(1))
        .and_then(|m| {
            let name = m.as_str();
            let re_def = regex::Regex::new(&format!("var\\s+{}\\s*=\\s*'([0-9a-f]{{32}})'", regex::escape(name))).unwrap();
            re_def.captures(&html).and_then(|c| c.get(1)).map(|x| x.as_str().to_string())
        })
        .unwrap_or_default();

    let ajax_url = format!("https://{}filemoreajax.php?file={}", host_of(&final_url), fid);
    let resp = http
        .post(&ajax_url)
        .header("Referer", &final_url)
        .header("X-Requested-With", "XMLHttpRequest")
        .form(&[
            ("lx", "2"),
            ("fid", fid.as_str()),
            ("uid", uid.as_str()),
            ("puid", puid.as_str()),
            ("pg", "1"),
            ("rep", "0"),
            ("t", t.as_str()),
            ("k", k.as_str()),
            ("up", "1"),
            ("vip", "0"),
            ("sign", sign.as_str()),
            ("webfoldersign", ""),
        ])
        .send()
        .await
        .ok()?;
    let v: Value = resp.json().await.ok()?;

    let zt = v
        .get("zt")
        .map(|z| {
            z.as_i64()
                .or_else(|| z.as_str().and_then(|s| s.parse::<i64>().ok()))
                .unwrap_or(-1)
        })
        .unwrap_or(-1);
    if zt != 1 {
        return None;
    }
    let items = v.get("text")?.as_array()?.clone();
    if items.is_empty() {
        return None;
    }
    Some((final_url, items))
}

/// 文件夹内全部单文件页链接（item.id 即短链后缀，引擎批量逐个 add_task 用）
pub async fn folder_file_urls(url: &str) -> Option<(String, Vec<String>)> {
    let http = page_client()?;
    let (final_url, items) = fetch_folder_items(&http, url).await?;
    let host = host_of(&final_url);
    let name = url
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or("蓝奏云文件夹")
        .trim_end_matches('.')
        .trim()
        .to_string();
    let urls = items
        .iter()
        .filter_map(|i| i.get("id").and_then(|v| v.as_str()).map(|s| s.to_string()))
        .map(|id| format!("{}/{}", host.trim_end_matches('/'), id))
        .collect();
    Some((name, urls))
}

pub async fn resolve(url: &str) -> Option<super::Extracted> {
    if !is_lanzou(url) {
        return None;
    }
    let http = page_client()?;
    let resp = http.get(url).send().await.ok()?;
    let final_url = resp.url().to_string();
    let html = resp.text().await.ok()?;

    // 单文件页（含 fn iframe）：内嵌 WebView 代取直链
    if !html.contains("filemoreajax") {
        if let Some(name) = file_page_name(&html) {
            let iframe_src = {
                // find-based 提取（正则 raw string 与 " 转义纠缠，直接字符串查找）
                let marker = "src=\"/fn?";
                let i = html.find(marker)?;
                let start = i + marker.len() - 1; // 指向 "
                let rest = &html[start + 1..];
                let end = rest.find('"')?;
                Some(format!("/fn?{}", &rest[..end]))
            };
            let webview_plan = iframe_src.map(|src| {
                let abs = if src.starts_with("http") {
                    src
                } else {
                    format!("https://{}{}", host_of(&final_url).trim_end_matches('/'), src)
                };
                super::WebviewExtractPlan {
                    url: abs,
                    js: r##"(()=>{const a=document.querySelector("#tourl a");return a?JSON.stringify(a.href):"";})()"##.into(),
                    timeout_ms: 20_000,
                }
            });
            return Some(super::Extracted {
                direct_url: String::new(),
                cookies: None,
                filename: name,
                referer: Some("https://www.lanzou.com/".into()),
                notice: if webview_plan.is_some() {
                    None
                } else {
                    Some("蓝奏云分享：页面缺少直链生成所需信息，请在官网页面下载".into())
                },
                webview_plan,
            });
        }
        return None;
    }

    // 文件夹分享：列表 + 一键批量引导
    let (_, items) = fetch_folder_items(&http, url).await?;
    let total = items.len();
    let sample: Vec<String> = items
        .iter()
        .filter_map(|i| i.get("name_all").and_then(|n| n.as_str()))
        .map(|s| s.chars().take(20).collect())
        .take(3)
        .collect();
    let sample_note = if sample.is_empty() {
        String::new()
    } else {
        format!("，如「{}」等", sample.join("」「"))
    };
    let folder_name = url
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or("蓝奏云分享")
        .to_string();

    Some(super::Extracted {
        direct_url: String::new(),
        filename: format!("{}（{} 个文件{sample_note}）", folder_name.trim_end_matches('.').trim(), total),
        referer: Some("https://www.lanzou.com/".into()),
        notice: Some(
            "蓝奏云文件夹：可一键批量添加文件（逐个通过内嵌 WebView 解析直链，速度较慢属预期；单次上限可在设置中调整）".into(),
        ),
        cookies: None,
        webview_plan: None,
    })
}

fn host_of(url: &str) -> String {
    let start = url.find("://").map(|i| i + 3).unwrap_or(0);
    let rest = &url[start..];
    let end = rest.find('/').unwrap_or(rest.len());
    format!("{}/", &rest[..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_lanzou() {
        assert!(is_lanzou("https://ydb6.lanzoui.com/s/ydb666"));
        assert!(is_lanzou("https://www.lanzouw.com/iabc123"));
        assert!(!is_lanzou("https://example.com/foo"));
    }

    #[test]
    fn test_js_field() {
        let html = "var ibjlt7 = '1795016163'; var _hhxv0 = 'abb8dfa367b086124d035d6d8798cdd3'; 'sign':'VWZWMwxnVTEB','uid':'326027'";
        assert_eq!(js_field(html, "ibjlt7"), Some("1795016163".into()));
        assert_eq!(js_field(html, "uid"), Some("326027".into()));
        assert_eq!(
            js_field(html, "sign"),
            Some("VWZWMwxnVTEB".into())
        );
    }

    #[test]
    fn test_file_page_name() {
        let html = "<title>漫画屋 xxx.apk - 蓝奏云</title>";
        assert_eq!(file_page_name(html), Some("漫画屋 xxx.apk".into()));
    }

    /// 单文件页应产出 WebView 代取计划（fn iframe URL + 读取 JS）
    #[tokio::test]
    async fn test_lanzou_single_file_plan() {
        if std::env::var("FEILIU_NET_TEST").is_err() {
            return;
        }
        let info = super::super::resolve_retry("https://ydb6.lanzoui.com/iD1r64gc2wuh")
            .await
            .expect("蓝奏云单文件解析失败");
        assert!(info.notice.is_none(), "有 plan 时不应有 notice");
        let plan = info.webview_plan.expect("应产出 WebView 代取计划");
        assert!(plan.url.contains("/fn?"), "fn URL: {}", plan.url);
        assert!(plan.js.contains("#tourl"), "读取 JS 应查询 #tourl");
        println!("lanzou 单文件 plan OK: {} → {}", info.filename, plan.url);
    }

    /// 真实分享（GitHub issue 来源）：文件夹结构列出 + 批量链接列表
    #[tokio::test]
    async fn test_lanzou_real_folder() {
        if std::env::var("FEILIU_NET_TEST").is_err() {
            return;
        }
        let info = super::super::resolve_retry("https://ydb6.lanzoui.com/s/ydb666")
            .await
            .expect("蓝奏云解析失败");
        assert!(
            info.filename.contains("个文件"),
            "应列出文件夹结构: {}",
            info.filename
        );
        assert!(info.notice.is_some(), "应有批量引导提示");
        let (name, urls) = super::folder_file_urls("https://ydb6.lanzoui.com/s/ydb666")
            .await
            .expect("文件夹批量列表失败");
        assert!(!name.is_empty(), "文件夹名应非空");
        assert!(!urls.is_empty(), "应产出至少一个文件链接");
        assert!(
            urls[0].contains("lanzoui.com/i"),
            "文件链接应为单文件页形态: {}",
            urls[0]
        );
        println!("lanzou 文件夹 OK: {}（{} 个文件）", name, urls.len());
    }
}
