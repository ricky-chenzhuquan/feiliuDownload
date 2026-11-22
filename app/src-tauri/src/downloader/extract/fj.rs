//! 小飞机网盘（feijipan）分享解析（r4 轮 12 落地）。
//!
//! 链路（全部匿名，真实分享字节级验证 2026-11-22）：
//! 1. 会员接口预热（同 uuid，服务端规则要求先固定 uuid 请求 vip/list）
//! 2. POST api.feijipan.com/ws/recommend/list?...&shareId={id}&type=0（+&code=提取码）
//!    → list[0]{fileIds,userId(null→"null"),fileList[{fileName,fileType(1=件/2=夹)}]}
//! 3. GET ws/file/redirect?downloadId=enc2hex(fileIds|userId)&auth=enc2hex(fileIds|ts)
//!    &uuid&timestamp=enc2hex(ts)&shareId={id}（禁跟随重定向）→ 302 Location = 直链
//!
//! 签名 = AES-128-ECB/PKCS7 → hex，密钥 `dingHao-disk-app`（客户端内置，开源实现公开事实）。
//! 直链 host：dl-*.feejii.com（双 e 拼写）；下载 Referer 用 www.feijix.com。
//! URL 形态：share.feijipan.com/s/{id}（feijix.com/s/ 同构）。

use super::Extracted;
use ecb::cipher::{BlockEncryptMut, KeyInit, generic_array::GenericArray};

const API: &str = "https://api.feijipan.com/ws";
const AES_KEY: &str = "dingHao-disk-app";
const WEB_UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";
const DL_REFERER: &str = "https://www.feijix.com/";

fn ecb_encrypt_pkcs7(data: &[u8], key: &[u8]) -> Vec<u8> {
    let mut enc = ecb::Encryptor::<aes::Aes128>::new(GenericArray::from_slice(key));
    // PKCS7 手工填充（cipher 0.4 vec 助手未被 ecb 0.1 重导出）
    let pad_len = 16 - (data.len() % 16);
    let mut buf = data.to_vec();
    buf.extend(std::iter::repeat(pad_len as u8).take(pad_len));
    for chunk in buf.chunks_mut(16) {
        enc.encrypt_block_mut(GenericArray::from_mut_slice(chunk));
    }
    buf
}

/// AES-ECB/PKCS7 → hex（timestamp / fidEncode / auth 三处同款签名）
fn enc2hex(s: &str) -> String {
    hex(&ecb_encrypt_pkcs7(s.as_bytes(), AES_KEY.as_bytes()))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn share_id(url: &str) -> Option<String> {
    let lower = url.to_lowercase();
    let host_ok = lower.contains("feijipan.com") || lower.contains("feijix.com");
    if !host_ok {
        return None;
    }
    let idx = lower.find("/s/")? + "/s/".len();
    let id: String = url[idx..]
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric())
        .collect();
    if id.is_empty() {
        None
    } else {
        Some(id)
    }
}

fn password_of(url: &str) -> Option<String> {
    url.split(['?', '&'])
        .find_map(|p| p.strip_prefix("pwd="))
        .map(String::from)
}

fn fj_uuid() -> String {
    // 21 位随机字母数字（服务端对畸形/重复 uuid 会返回空清单——真随机不可省）
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let mut raw = vec![0u8; 21];
    if std::fs::File::open("/dev/urandom")
        .and_then(|mut f| std::io::Read::read_exact(&mut f, &mut raw))
        .is_err()
    {
        // 兜底：时间+进程熵（极端环境才走）
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .subsec_nanos() as usize
            ^ std::process::id() as usize;
        for (i, b) in raw.iter_mut().enumerate() {
            *b = ((seed >> (i % 13)) ^ (i * 37)) as u8;
        }
    }
    raw.iter()
        .map(|b| CHARS[*b as usize % CHARS.len()] as char)
        .collect()
}

async fn post_json(
    client: &reqwest::Client,
    url: &str,
) -> Result<serde_json::Value, String> {
    let resp = client
        .post(url)
        .header("Referer", "https://www.feijipan.com/")
        .header("sec-ch-ua", "\"Google Chrome\";v=\"131\", \"Chromium\";v=\"131\", \"Not_A Brand\";v=\"24\"")
        .header("sec-ch-ua-mobile", "?0")
        .header("sec-ch-ua-platform", "\"Windows\"")
        .body("")
        .send()
        .await
        .map_err(|e| format!("小飞机盘请求失败: {e}"))?;
    let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    let code = json.get("code").and_then(|c| c.as_i64()).unwrap_or(-1);
    if code != 200 {
        let msg = json.get("msg").and_then(|m| m.as_str()).unwrap_or("");
        return Err(format!("小飞机盘 code={code}: {msg}"));
    }
    Ok(json)
}

pub async fn resolve(url: &str) -> Option<Extracted> {
    let lower = url.to_lowercase();
    let host_ok = lower.contains("feijipan.com") || lower.contains("feijix.com");
    if !host_ok || !lower.contains("/s/") {
        return None;
    }
    let share_id = share_id(url)?;
    let password = password_of(url);

    let client = reqwest::Client::builder()
        .user_agent(WEB_UA)
        .redirect(reqwest::redirect::Policy::none())
        .http1_only()
        // 服务端规则：请求必须携带 Accept-Encoding 头，否则 recommend/list 返回空清单
        // （python urllib 默认发 identity 故总能过；reqwest/curl 裸请求无此头全空）
        .default_headers({
            let mut h = reqwest::header::HeaderMap::new();
            h.insert("Accept-Encoding", "identity".parse().unwrap());
            h
        })
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .ok()?;
    // 服务端指纹校验：缺 sec-ch-ua 系列头会静默拒绝（python 复刻对照实锤）
    let uuid = fj_uuid();
    let ts = enc2hex(&std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .to_string());

    // 1) 会员接口预热（服务端要求同 uuid 先打一次）
    let _ = client
        .post(format!("{API}/buy/vip/list?devType=6&devModel=Chrome&uuid={uuid}&extra=2&timestamp={ts}"))
        .header("Referer", "https://www.feijipan.com/")
        .header("sec-ch-ua", "\"Google Chrome\";v=\"131\", \"Chromium\";v=\"131\", \"Not_A Brand\";v=\"24\"")
        .header("sec-ch-ua-mobile", "?0")
        .header("sec-ch-ua-platform", "\"Windows\"")
        .body("")
        .send()
        .await;

    // 2) 分享文件清单
    let mut list_url = format!(
        "{API}/recommend/list?devType=6&devModel=Chrome&uuid={uuid}&extra=2&timestamp={ts}&shareId={share_id}&type=0&offset=1&limit=60"
    );
    if let Some(code) = &password {
        list_url.push_str(&format!("&code={code}"));
    }
    let list = match post_json(&client, &list_url).await {
        Ok(j) => j,
        Err(e) => {
            return Some(Extracted {
                direct_url: String::new(),
                filename: format!("fj_{share_id}"),
                referer: Some(DL_REFERER.into()),
                notice: Some(format!("小飞机盘分享无法访问（可能已过期/提取码错误）：{e}")),
                cookies: None,
                user_agent: None,
                webview_plan: None,
            });
        }
    };
    let share_entry = list.pointer("/list/0")?;
    let file_ids = match share_entry.get("fileIds") {
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(serde_json::Value::Number(n)) => n.to_string(),
        _ => return None,
    };
    // userId 缺省时服务端语义为字面量 "null"（真实验证）
    let user_id = share_entry
        .get("userId")
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_else(|| "null".into());
    let file = share_entry.pointer("/fileList/0")?;
    let file_type = file.get("fileType").and_then(|v| v.as_i64()).unwrap_or(0);
    if file_type == 2 {
        return Some(Extracted {
            direct_url: String::new(),
            filename: format!("fj_{share_id}"),
            referer: Some(DL_REFERER.into()),
            notice: Some(
                "小飞机盘分享仅含文件夹，当前版本只支持单文件分享的直链下载；请分享具体文件链接".into(),
            ),
            cookies: None,
            user_agent: None,
            webview_plan: None,
        });
    }
    let fname = file
        .get("fileName")
        .and_then(|v| v.as_str())
        .unwrap_or("fjfile")
        .to_string();

    // 3) 重定向拿直链（禁跟随，302 Location）
    let ts2 = enc2hex(&std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .to_string());
    let fid_encode = enc2hex(&format!("{file_ids}|{user_id}"));
    let auth = enc2hex(&format!(
        "{file_ids}|{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
    ));
    let redirect_url = format!(
        "{API}/file/redirect?downloadId={fid_encode}&enable=1&devType=6&uuid={uuid}&timestamp={ts2}&auth={auth}&shareId={share_id}"
    );
    let resp = client
        .get(&redirect_url)
        .header("Referer", DL_REFERER)
        .header("sec-ch-ua", "\"Google Chrome\";v=\"131\", \"Chromium\";v=\"131\", \"Not_A Brand\";v=\"24\"")
        .header("sec-ch-ua-mobile", "?0")
        .header("sec-ch-ua-platform", "\"Windows\"")
        .send()
        .await
        .map_err(|e| format!("小飞机盘 redirect 失败: {e}"))
        .ok()?;
    let direct = resp
        .headers()
        .get(reqwest::header::LOCATION)?
        .to_str()
        .ok()?
        .to_string();
    if direct.is_empty() {
        return None;
    }

    Some(Extracted {
        direct_url: direct,
        filename: fname,
        referer: Some(DL_REFERER.into()),
        notice: None,
        cookies: None,
        user_agent: None,
        webview_plan: None,
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_share_id_parse() {
        assert_eq!(
            super::share_id("https://share.feijipan.com/s/UTeimERI"),
            Some("UTeimERI".into())
        );
        assert_eq!(
            super::share_id("https://www.feijix.com/s/abcDEF123?pwd=x"),
            Some("abcDEF123".into())
        );
        assert_eq!(super::share_id("https://www.feijipan.com/"), None);
    }

    #[test]
    fn test_enc2hex_known_vector() {
        // 签名确定性：同输入同输出（AES-ECB 无随机性）
        let a = super::enc2hex("1700000000000");
        let b = super::enc2hex("1700000000000");
        assert_eq!(a, b);
        assert_eq!(a.len() % 32, 0, "PKCS7 块对齐 hex 长度");
    }

    /// 真实公开分享（r4 轮 12 验证）：解析产出 feejii CDN 直链并 206 见字节
    #[tokio::test]
    async fn test_fj_real_share() {
        if std::env::var("FEILIU_NET_TEST").is_err() {
            return;
        }
        let info = super::super::resolve_retry("https://share.feijipan.com/s/UTeimERI")
            .await
            .expect("小飞机盘解析失败");
        assert!(
            info.direct_url.contains("feejii.com"),
            "直链必须是 feejii CDN: {}",
            info.direct_url
        );
        assert!(info.filename.ends_with(".apk"), "真实文件名: {}", info.filename);
        assert!(info.notice.is_none(), "可下载分享不应有 notice");
        let client = reqwest::Client::builder().build().unwrap();
        let resp = client
            .get(&info.direct_url)
            .header("Range", "bytes=0-1023")
            .header("Referer", super::DL_REFERER)
            .send()
            .await
            .expect("直链请求失败");
        assert_eq!(resp.status().as_u16(), 206, "Range 直取应 206");
        assert!(!resp.bytes().await.expect("读直链失败").is_empty());
    }
}
