//! URL 探测：通过 HEAD / Range-GET 获取文件名、大小、是否支持断点续传。
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::OnceLock;

use percent_encoding::percent_decode_str;
use reqwest::header::{
    ACCEPT_RANGES, CONTENT_DISPOSITION, CONTENT_LENGTH, CONTENT_RANGE, ETAG, HeaderMap,
    LAST_MODIFIED,
};
use reqwest::{Client, StatusCode};

pub const USER_AGENT_VALUE: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36";

static CLIENT_DEFAULT: OnceLock<Client> = OnceLock::new();
static CLIENT_H1: OnceLock<Client> = OnceLock::new();

/// HTTP/2 自适应降级：默认允许 h2（多路复用省握手），
/// 读取停滞超时累计 ≥3 次则整个会话降级为 HTTP/1.1（h2 挂流问题的兜底）。
static HTTP1_MODE: AtomicBool = AtomicBool::new(false);
static STALL_STRIKES: AtomicU32 = AtomicU32::new(0);

pub fn note_stall() {
    let n = STALL_STRIKES.fetch_add(1, Ordering::Relaxed) + 1;
    if n >= 3 {
        HTTP1_MODE.store(true, Ordering::Relaxed);
    }
}

fn build_client(http1_only: bool) -> Client {
    let mut b = Client::builder()
        .user_agent(USER_AGENT_VALUE)
        .redirect(reqwest::redirect::Policy::limited(10))
        .connect_timeout(std::time::Duration::from_secs(15))
        .read_timeout(std::time::Duration::from_secs(30));
    if http1_only {
        b = b.http1_only();
    }
    b.build().expect("failed to build http client")
}

/// 探测/轻量请求用的客户端（默认 h2 自适应）
pub fn client() -> Client {
    if HTTP1_MODE.load(Ordering::Relaxed) {
        CLIENT_H1.get_or_init(|| build_client(true)).clone()
    } else {
        CLIENT_DEFAULT
            .get_or_init(|| build_client(false))
            .clone()
    }
}

/// 分片下载用客户端：跟随自适应降级开关
pub fn download_client() -> Client {
    client()
}

#[derive(Debug, Clone, Default)]
pub struct ProbeInfo {
    pub filename: Option<String>,
    pub total_size: Option<u64>,
    pub accept_ranges: bool,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    /// 站点解析出的真实直链（如 B 站视频页 → mp4 直链）
    pub resolved_url: Option<String>,
    /// 站点要求的 Referer（下载时随请求携带）
    pub referer: Option<String>,
    pub content_type: Option<String>,
    /// 站点限制提示（如阿里云盘需登录转存），有值时不启动下载
    pub notice: Option<String>,
    /// WebView 代取计划（透传自 extract 层）
    pub webview_plan: Option<crate::downloader::extract::WebviewExtractPlan>,
}

/// 先做站点解析，再 Range-GET 优先（206 带真实大小；部分播放接口
/// 的 HEAD 会返回错误长度），失败退回 HEAD。整体失败重试一次（应对
/// snssdk 等站点的间歇性首连失败）。
pub async fn probe(client: &Client, url: &str) -> Result<ProbeInfo, String> {
    // 站点解析对瞬态网络失败也重试一次（82dj/snssdk 等间歇超时实测）
    let extracted = match crate::downloader::extract::resolve(url).await {
        Some(e) => Some(e),
        None => {
            tokio::time::sleep(std::time::Duration::from_millis(700)).await;
            crate::downloader::extract::resolve(url).await
        }
    };
    // direct_url 为空（如网盘/歌单仅列结构）→ 用原页面 URL 探测
    let target = extracted
        .as_ref()
        .map(|e| e.direct_url.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or(url);

    let mut last_err: Option<String> = None;
    for _attempt in 0..3 {
        let mut info = {
            // 诊断日志：探测失败时输出真实原因（状态码/网络错误）
            let r = range_probe_detail(client, target, None).await;
            let h = head_probe_detail(client, target, None).await;
            match (r, h) {
                (Some(i), _) => i,
                (None, Some(i)) => i,
                (None, None) => {
                    eprintln!(
                        "[feiliu-probe] 两路探测均失败: url={}（详见上方错误行）",
                        target
                    );
                    last_err = Some("无法访问该地址：连接失败或返回错误状态".to_string());
                    continue;
                }
            }
        };

        if let Some(ex) = &extracted {
            info.filename = Some(ex.filename.clone());
            // direct_url 为空（如 82dj/蓝奏云走 WebView 代取）时不能产出
            // Some("") —— 前端会把空串当 URL 回传，探测 builder error
            if !ex.direct_url.is_empty() {
                info.resolved_url = Some(ex.direct_url.clone());
            }
            info.referer = ex.referer.clone();
        info.notice = ex.notice.clone();
        info.webview_plan = ex.webview_plan.clone();
        }
        // 站点限制提示（歌单/网盘）→ 作为正常结果返回，由上层决定批量/拒绝
        if info.notice.is_some() {
            return Ok(info);
        }
        // 网页链接（未解析出媒体直链且无代取计划）→ 明确报错，
        // 避免把 HTML 当文件下载；有 webview_plan 的站点是有意走代取的网页
        if info.resolved_url.is_none() && info.webview_plan.is_none() {
            let ctype = info.content_type.as_deref().unwrap_or("");
            if ctype.contains("text/html") {
                return Err("这是一个网页地址而不是文件直链。请复制视频/文件的分享链接或直链（若为支持的站点会自动解析）".into());
            }
        }
        return Ok(info);
    }
    Err(last_err.unwrap_or_else(|| "探测失败".into()))
}

/// 对已知直链带 Referer 探测（WebView 代取站点的真实直链二次探测）：
/// 82dj 等站点直链防盗链——无 Referer 直接 404，探测必须携带。
/// Range 优先（206 带真实大小），HEAD 兜底，整体重试一次。纯 best-effort。
pub async fn probe_direct_ref(
    client: &Client,
    url: &str,
    referer: Option<&str>,
) -> Option<ProbeInfo> {
    for _attempt in 0..2 {
        if let Some(i) = range_probe_detail(client, url, referer).await {
            return Some(i);
        }
        if let Some(i) = head_probe_detail(client, url, referer).await {
            return Some(i);
        }
        tokio::time::sleep(std::time::Duration::from_millis(700)).await;
    }
    None
}



pub async fn head_probe_detail(
    client: &Client,
    url: &str,
    referer: Option<&str>,
) -> Option<ProbeInfo> {
    let mut req = client.head(url);
    if let Some(r) = referer {
        req = req.header(reqwest::header::REFERER, r);
    }
    let resp = match req.send().await {
        Ok(r) => {
            eprintln!("[feiliu-probe] head: status={} url={}", r.status(), url);
            r
        }
        Err(e) => {
            eprintln!("[feiliu-probe] head 网络错误: url={} err={}", url, e);
            return None;
        }
    };
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
    let content_type = get_str(&headers, &reqwest::header::CONTENT_TYPE);
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
        content_type,
        ..Default::default()
    })
}

pub async fn range_probe_detail(
    client: &Client,
    url: &str,
    referer: Option<&str>,
) -> Option<ProbeInfo> {
    let mut req = client
        .get(url)
        .header(reqwest::header::RANGE, "bytes=0-0");
    if let Some(r) = referer {
        req = req.header(reqwest::header::REFERER, r);
    }
    let resp = match req.send().await
    {
        Ok(r) => {
            eprintln!("[feiliu-probe] range: status={} url={}", r.status(), url);
            r
        }
        Err(e) => {
            eprintln!("[feiliu-probe] range 网络错误: url={} err={}", url, e);
            return None;
        }
    };
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
            content_type: get_str(&headers, &reqwest::header::CONTENT_TYPE),
            ..Default::default()
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
            content_type: get_str(&headers, &reqwest::header::CONTENT_TYPE),
            ..Default::default()
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

    /// 直链带 Referer 二次探测：已知大小文件（假 Referer 不影响服务器响应，
    /// 验证头部传递与大小解析链路）
    #[tokio::test]
    async fn test_probe_direct_ref() {
        if std::env::var("FEILIU_NET_TEST").is_err() {
            return;
        }
        let c = client();
        let info = probe_direct_ref(
            &c,
            "https://proof.ovh.net/files/10Mb.dat",
            Some("https://www.82dj.com/"),
        )
        .await
        .expect("直链二次探测失败");
        assert_eq!(
            info.total_size,
            Some(10 * 1024 * 1024),
            "应拿到真实文件大小: {:?}",
            info.total_size
        );
        assert!(info.accept_ranges, "proof.ovh 支持 Range");
        println!("probe_direct_ref OK: {:?}", info.total_size);
    }

    /// 诊断（临时）：82dj 直链 + 引擎同款 reqwest 客户端——验证 CDN 是否
    /// 按 TLS/HTTP 客户端指纹拦截（curl 206 但 rustls 404）
    #[tokio::test]
    async fn test_diag_82dj_reqwest_fingerprint() {
        let url = std::fs::read_to_string("/tmp/82dj_31533_url.txt")
            .unwrap_or_default()
            .trim()
            .to_string();
        if url.is_empty() {
            println!("无测试直链，跳过");
            return;
        }
        let c = client();
        let resp = c
            .get(&url)
            .header(reqwest::header::RANGE, "bytes=0-0")
            .header(reqwest::header::REFERER, "https://www.82dj.com/")
            .send()
            .await
            .unwrap();
        println!(
            "[诊断] reqwest + Referer → HTTP {}（404=指纹拦截实锤，206=链路无恙）",
            resp.status()
        );
        if resp.status().as_u16() == 206 {
            let cr = resp
                .headers()
                .get(reqwest::header::CONTENT_RANGE)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .to_string();
            println!("[诊断] content-range: {cr}");
        }
    }
}
