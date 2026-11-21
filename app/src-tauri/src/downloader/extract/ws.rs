//! 文叔叔（wenshushu）分享解析（r4 轮 9 落地）。
//!
//! 链路（全部匿名，自建夹具字节级验证 2026-11-22，见 scripts/wss_fixture.py）：
//! 1. POST ap/login/anonymous {dev_info:"{}"} → data.token（匿名会话）
//! 2. POST ap/task/mgrtask {tid, password} → data{ufileid(pid), boxid(bid), file_size}
//! 3. POST ap/ufile/list {start,sort,bid,pid,type:1,options,size} → fileList[{fname,fid}]
//! 4. POST ap/dl/sign {consumeCode:0,type:1,ufileid} → data.url（URL decode=直链）
//!
//! 头：移动 UA + X-Token；下载链无 A-code 签名（上传才有）。
//! URL 形态：www.wenshushu.cn/f/{tid} 与短域 c.wss.ink/f/{tid}；
//! 直链 CDN down.wss.show（cdn_sign 时效签名）。
//! mgrtask code 1013 = 分享不存在/过期。v1 取 fileList 首个文件。

use super::Extracted;

const API: &str = "https://www.wenshushu.cn";
const MOBILE_UA: &str = "Mozilla/5.0 (Linux; Android 6.0; Nexus 5 Build/MRA58N) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/111.0.0.0 Mobile Safari/537.36";

fn share_tid(url: &str) -> Option<String> {
    let lower = url.to_lowercase();
    let host_ok = lower.contains("wenshushu.cn") || lower.contains("wss.ink");
    if !host_ok {
        return None;
    }
    let idx = lower.find("/f/")? + "/f/".len();
    let tid: String = url[idx..]
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric())
        .collect();
    if tid.is_empty() {
        None
    } else {
        Some(tid)
    }
}

fn password_of(url: &str) -> String {
    url.split(['?', '&'])
        .find_map(|p| p.strip_prefix("pwd="))
        .unwrap_or("")
        .to_string()
}

async fn post_json(
    client: &reqwest::Client,
    path: &str,
    payload: serde_json::Value,
    token: Option<&str>,
) -> Result<serde_json::Value, String> {
    let mut req = client
        .post(format!("{API}{path}"))
        .json(&payload);
    if let Some(t) = token {
        req = req.header("X-Token", t);
    }
    let resp = req.send().await.map_err(|e| format!("文叔叔请求失败: {e}"))?;
    let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    let code = json.get("code").and_then(|c| c.as_i64()).unwrap_or(-1);
    if code != 0 {
        let msg = json.get("message").and_then(|m| m.as_str()).unwrap_or("");
        return Err(format!("文叔叔 code={code}: {msg}"));
    }
    Ok(json)
}

pub async fn resolve(url: &str) -> Option<Extracted> {
    let lower = url.to_lowercase();
    let host_ok = lower.contains("wenshushu.cn") || lower.contains("wss.ink");
    if !host_ok || !lower.contains("/f/") {
        return None;
    }
    let tid = share_tid(url)?;
    let password = password_of(url);

    let client = reqwest::Client::builder()
        .user_agent(MOBILE_UA)
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .ok()?;

    // 1) 匿名 token
    let anon = post_json(
        &client,
        "/ap/login/anonymous",
        serde_json::json!({ "dev_info": "{}" }),
        None,
    )
    .await
    .ok()?;
    let token = anon.pointer("/data/token")?.as_str()?.to_string();

    // 2) 分享信息（code!=0 → 过期/不存在/密码错 → 诚实 notice）
    let mgr = match post_json(
        &client,
        "/ap/task/mgrtask",
        serde_json::json!({ "tid": tid, "password": password }),
        Some(&token),
    )
    .await
    {
        Ok(m) => m,
        Err(e) => {
            return Some(Extracted {
                direct_url: String::new(),
                filename: format!("ws_{tid}"),
                referer: None,
                notice: Some(format!("文叔叔分享无法访问（可能已过期/提取码错误）：{e}")),
                cookies: None,
                webview_plan: None,
            });
        }
    };
    let pid = mgr.pointer("/data/ufileid")?.as_str()?.to_string();
    let bid = mgr.pointer("/data/boxid")?.as_str()?.to_string();

    // 3) 文件清单（v1 取首个文件）
    let list = post_json(
        &client,
        "/ap/ufile/list",
        serde_json::json!({
            "start": 0,
            "sort": { "name": "asc" },
            "bid": bid,
            "pid": pid,
            "type": 1,
            "options": { "uploader": "true" },
            "size": 50,
        }),
        Some(&token),
    )
    .await
    .ok()?;
    let fl = list.pointer("/data/fileList/0")?;
    let fname = fl
        .get("fname")
        .and_then(|v| v.as_str())
        .unwrap_or("wsfile")
        .to_string();
    let fid = fl.get("fid")?.as_str()?.to_string();

    // 4) 直链签名（URL decode：& 等被服务端转义）
    let sign = post_json(
        &client,
        "/ap/dl/sign",
        serde_json::json!({ "consumeCode": 0, "type": 1, "ufileid": fid }),
        Some(&token),
    )
    .await
    .ok()?;
    let raw = sign.pointer("/data/url")?.as_str()?;
    let direct = raw
        .replace("\\/", "/")
        .replace("%3A", ":")
        .replace("%2F", "/")
        .replace("%3F", "?")
        .replace("%3D", "=")
        .replace("%26", "&");
    if direct.is_empty() {
        return Some(Extracted {
            direct_url: String::new(),
            filename: fname,
            referer: None,
            notice: Some("文叔叔文件已失效（直链为空）".into()),
            cookies: None,
            webview_plan: None,
        });
    }

    Some(Extracted {
        direct_url: direct,
        filename: fname,
        referer: None,
        notice: None,
        cookies: None,
        webview_plan: None,
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_tid_parse() {
        assert_eq!(
            super::share_tid("https://www.wenshushu.cn/f/d1fve4ykd08"),
            Some("d1fve4ykd08".into())
        );
        assert_eq!(
            super::share_tid("https://c.wss.ink/f/lez4cxos4el"),
            Some("lez4cxos4el".into())
        );
        assert_eq!(super::share_tid("https://www.wenshushu.cn/"), None);
        assert_eq!(super::share_tid("https://wss.ink/"), None);
    }

    #[test]
    fn test_password_parse() {
        assert_eq!(super::password_of("https://www.wenshushu.cn/f/abc?pwd=1234"), "1234");
        assert_eq!(super::password_of("https://c.wss.ink/f/abc"), "");
    }

    /// 真实夹具（r4 轮 8 自建，expire=1 天；过期后跑 scripts/wss_fixture.py 重造并更新 tid）
    #[tokio::test]
    async fn test_ws_real_share() {
        if std::env::var("FEILIU_NET_TEST").is_err() {
            return;
        }
        let info = super::super::resolve_retry("https://c.wss.ink/f/lez4cxos4el")
            .await
            .expect("文叔叔解析失败");
        assert!(
            info.direct_url.contains("down.wss.show") || info.direct_url.contains("http"),
            "直链形态异常: {}",
            info.direct_url
        );
        assert_eq!(info.filename, "feiliu_fixture.txt", "真实文件名");
        assert!(info.notice.is_none(), "可下载分享不应有 notice");
        // 直链应可 Range 探测（引擎建任务时验证大小），这里验证 URL 可用性
        let client = reqwest::Client::builder()
            .user_agent(super::MOBILE_UA)
            .build()
            .unwrap();
        let resp = client
            .get(&info.direct_url)
            .header("Range", "bytes=0-1023")
            .send()
            .await
            .expect("直链请求失败");
        assert!(resp.status().is_success(), "直链 GET 状态: {}", resp.status());
        let body = resp.bytes().await.expect("读直链失败");
        assert!(!body.is_empty(), "直链应返回字节");
    }
}
