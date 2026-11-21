//! GitHub Releases 解析：release 页面 / latest 链接 → 资产列表与直链。
//!
//! 匿名 API 完全可用（实测 FeiLiu 自己的 v0.1.0：4 资产）；资产直链
//! （browser_download_url）即最终下载地址，GitHub CDN 无防盗链。
//! 单资产直接给直链；多资产列结构（用户按平台选择）。
use serde_json::Value;

use super::helpers::clean_filename;

fn client() -> Option<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent(super::helpers::MOBILE_UA)
        .build()
        .ok()
}

/// 识别 github release 链接并提取 (owner, repo, tag)
/// 支持 /releases/tag/v1.0 与 /releases/latest（latest 时 tag=空）
fn parse_release_url(url: &str) -> Option<(String, String, String)> {
    let lower = url.to_lowercase();
    if !lower.contains("github.com/") || !lower.contains("/releases") {
        return None;
    }
    // https://github.com/{owner}/{repo}/releases/tag/{tag} | /releases/latest
    let idx = url.find("/releases/")?;
    let owner_repo = url[..idx].split('/').filter(|s| !s.is_empty()).collect::<Vec<_>>();
    if owner_repo.len() < 2 {
        return None;
    }
    let owner = owner_repo[owner_repo.len() - 2].to_string();
    let repo = owner_repo[owner_repo.len() - 1].to_string();
    let tag = url[idx + "/releases/".len()..]
        .trim_end_matches('/')
        .trim_start_matches("tag/")
        .split(['?', '#'])
        .next()
        .unwrap_or("")
        .to_string();
    Some((owner, repo, tag))
}

async fn fetch_release(http: &reqwest::Client, api_url: &str) -> Option<Value> {
    let resp = http.get(api_url).send().await.ok()?;
    if !resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .contains("json")
    {
        return None; // 404 HTML 等
    }
    resp.json().await.ok()
}

pub async fn resolve(url: &str) -> Option<super::Extracted> {
    let (owner, repo, tag) = parse_release_url(url)?;
    let http = client()?;
    let api = match tag.as_str() {
        "" | "latest" => format!(
            "https://api.github.com/repos/{owner}/{repo}/releases/latest"
        ),
        t => format!("https://api.github.com/repos/{owner}/{repo}/releases/tags/{t}"),
    };
    let rel = fetch_release(&http, &api).await?;
    if rel.get("message").is_some() && rel.get("assets").is_none() {
        return None; // Not Found
    }
    let assets = rel.get("assets")?.as_array()?;
    if assets.is_empty() {
        return None; // 空 release
    }

    let release_name = rel
        .get("name")
        .and_then(|n| n.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or(&tag)
        .to_string();

    // 单资产 → 直接给直链；多资产 → 列结构（用户到官网按平台选）
    if assets.len() == 1 {
        let a = &assets[0];
        let name = a.get("name").and_then(|n| n.as_str()).unwrap_or("asset");
        let direct = a
            .get("browser_download_url")
            .and_then(|u| u.as_str())?
            .to_string();
        return Some(super::Extracted {
            direct_url: direct,
            filename: clean_filename(name, "bin"),
            referer: None,
            notice: None,
            cookies: None,
            webview_plan: None,
        });
    }

    let sample: Vec<String> = assets
        .iter()
        .filter_map(|a| a.get("name").and_then(|n| n.as_str()))
        .map(|s| s.to_string())
        .take(3)
        .collect();
    let sample_note = if sample.is_empty() {
        String::new()
    } else {
        format!("，如「{}」等", sample.join("」「"))
    };
    Some(super::Extracted {
        direct_url: String::new(),
        filename: format!(
            "{}（{} 个资产{sample_note}）",
            clean_filename(&release_name, ""),
            assets.len()
        )
        .trim_end_matches('.')
        .to_string(),
        referer: None,
        notice: Some(format!(
            "GitHub Release：该版本含 {} 个资产（不同平台安装包），请到 https://github.com/{owner}/{repo}/releases/tag/{tag} 选择对应平台的资产直接下载（GitHub 直链无需登录，粘贴资产链接即可）",
            assets.len()
        )),
        cookies: None,
        webview_plan: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_release_url() {
        assert_eq!(
            parse_release_url("https://github.com/ricky-chenzhuquan/feiliuDownload/releases/tag/v0.1.0"),
            Some(("ricky-chenzhuquan".into(), "feiliuDownload".into(), "v0.1.0".into()))
        );
        assert_eq!(
            parse_release_url("https://github.com/ricky-chenzhuquan/feiliuDownload/releases/latest"),
            Some(("ricky-chenzhuquan".into(), "feiliuDownload".into(), "latest".into()))
        );
        assert!(parse_release_url("https://github.com/x/y/issues/1").is_none());
    }

    /// 真实 release（FeiLiu 自己的 v0.1.0，4 资产）：列结构 + 引导
    #[tokio::test]
    async fn test_github_real_release() {
        if std::env::var("FEILIU_NET_TEST").is_err() {
            return;
        }
        let info = super::super::resolve_retry(
            "https://github.com/ricky-chenzhuquan/feiliuDownload/releases/tag/v0.1.0",
        )
        .await
        .expect("GitHub Release 解析失败");
        assert!(
            info.filename.contains("4 个资产") || info.filename.contains("v0.1.0"),
            "应列出资产结构: {}",
            info.filename
        );
        assert!(info.notice.is_some(), "多资产应有引导提示");
        println!("github OK: {}", info.filename);
    }
}
