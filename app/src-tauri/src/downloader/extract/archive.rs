//! Internet Archive（archive.org）解析：details 条目页 → 匿名 metadata API → 媒体直链。
//!
//! 下载直链公开且无防盗链（实测 302 → ca.archive.org 节点，Range 206）。
//! 文件名必须取自 metadata 文件列表（真实条目文件名与页面标题完全不同，
//! 且常带子目录如 `Content/big_buck_bunny_720p_surround.mp4`，不能拼接猜测）。
//! 选取策略：视频优先、同类型取最大；条目无媒体文件（纯文本/软件等）→ 列结构引导。
use serde_json::Value;

use super::helpers::clean_filename;

/// 需要识别的媒体格式，三档优先级：现代派生视频（通用可播）> 其它视频（原始大文件）> 音频
const PREFERRED_VIDEO_EXTS: [&str; 3] = ["mp4", "mkv", "webm"];
const OTHER_VIDEO_EXTS: [&str; 5] = ["mpg", "mpeg", "avi", "ogv", "mov"];
const AUDIO_EXTS: [&str; 6] = ["mp3", "flac", "ogg", "m4a", "wav", "aiff"];

fn client() -> Option<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent(super::helpers::MOBILE_UA)
        .build()
        .ok()
}

/// 识别 archive.org details 条目页，提取 identifier
fn parse_item_url(url: &str) -> Option<String> {
    let lower = url.to_lowercase();
    if !lower.contains("archive.org") || !lower.contains("/details/") {
        return None;
    }
    let idx = lower.find("/details/")?;
    let rest = &url[idx + "/details/".len()..];
    let id = rest
        .split(['/', '?', '#'])
        .next()
        .unwrap_or("")
        .trim_end_matches('/');
    if id.is_empty() {
        return None;
    }
    Some(id.to_string())
}

/// 路径段百分号编码（保留 `/` 分层；空格、引号、中文等转 %XX）
fn encode_path_segment(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

/// 从 metadata JSON 选目标媒体：档位优先（派生视频 > 原始视频 > 音频），同档取 size 最大
fn pick_media(files: &[Value]) -> Option<(String, String)> {
    // (tier, name, ext)；tier 越小越优先
    let mut best: Option<(i64, i64, String, String)> = None; // (tier, size, name, ext)
    for f in files {
        let name = f.get("name").and_then(|n| n.as_str()).unwrap_or("");
        let size: i64 = f
            .get("size")
            .and_then(|s| s.as_str())
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        let ext = name.rsplit('.').next().unwrap_or("").to_lowercase();
        let tier = if PREFERRED_VIDEO_EXTS.contains(&ext.as_str()) {
            0
        } else if OTHER_VIDEO_EXTS.contains(&ext.as_str()) {
            1
        } else if AUDIO_EXTS.contains(&ext.as_str()) {
            2
        } else {
            continue;
        };
        let better = match &best {
            None => true,
            Some((btier, bsize, _, _)) => tier < *btier || (tier == *btier && size > *bsize),
        };
        if better {
            best = Some((tier, size, name.to_string(), ext));
        }
    }
    best.map(|(_, _, name, ext)| (name, ext))
}

pub async fn resolve(url: &str) -> Option<super::Extracted> {
    let id = parse_item_url(url)?;
    let http = client()?;
    let meta: Value = http
        .get(format!("https://archive.org/metadata/{id}"))
        .send()
        .await
        .ok()?
        .json()
        .await
        .ok()?;
    let title = meta
        .get("metadata")
        .and_then(|m| m.get("title"))
        .and_then(|t| t.as_str())
        .unwrap_or(&id)
        .to_string();
    let files = meta.get("files")?.as_array()?.clone();

    if let Some((name, ext)) = pick_media(&files) {
        // 直链：按路径段编码（子目录保留 /）
        let encoded: Vec<String> = name.split('/').map(encode_path_segment).collect();
        let direct = format!("https://archive.org/download/{id}/{}", encoded.join("/"));
        let filename = clean_filename(&title, &ext);
        return Some(super::Extracted {
            direct_url: direct,
            filename,
            referer: None,
            notice: None,
            cookies: None,
            user_agent: None,
            webview_plan: None,
        });
    }

    // 无媒体文件：列结构引导（与 github 多资产同款）
    let total = files.len();
    let sample: Vec<String> = files
        .iter()
        .filter_map(|f| f.get("name").and_then(|n| n.as_str()))
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
            "{}（{} 个文件{sample_note}）",
            clean_filename(&title, ""),
            total
        )
        .trim_end_matches('.')
        .to_string(),
        referer: None,
        notice: Some(format!(
            "Internet Archive：条目「{title}」含 {total} 个文件但无可直接下载的音视频{sample_note}。文件直链公开（https://archive.org/download/{id}/文件名），粘贴具体文件链接即可下载",
        )),
        cookies: None,
        user_agent: None,
        webview_plan: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_item_url() {
        assert_eq!(
            parse_item_url("https://archive.org/details/BigBuckBunny_124"),
            Some("BigBuckBunny_124".into())
        );
        assert_eq!(
            parse_item_url("https://archive.org/details/nasa-apollo11-flight-journal/?query=x"),
            Some("nasa-apollo11-flight-journal".into())
        );
        assert!(parse_item_url("https://archive.org/about/").is_none());
        assert!(parse_item_url("https://example.com/details/x").is_none());
    }

    #[test]
    fn test_encode_path_segment() {
        assert_eq!(encode_path_segment("big.mp4"), "big.mp4");
        assert_eq!(encode_path_segment("a b.mp4"), "a%20b.mp4");
        assert_eq!(encode_path_segment("视频.mp4"), "%E8%A7%86%E9%A2%91.mp4");
        // 段级调用：子目录的 / 由 resolve 先按段拆分保留，段内不应出现
        assert_eq!(encode_path_segment("a/b"), "a%2Fb");
    }

    #[test]
    fn test_pick_media() {
        let mk = |name: &str, size: &str| {
            serde_json::json!({"name": name, "size": size, "format": "h.264"})
        };
        // 原始 avi 更大也优先选派生 mp4（通用可播）
        let files = vec![
            mk("thumb.jpg", "100"),
            mk("BigBuckBunny_124.avi", "400000000"),
            mk("small.mp4", "500"),
            mk("Content/big_buck_bunny_720p_surround.mp4", "61878609"),
            mk("audio_track.mp3", "3000"),
        ];
        let (name, ext) = pick_media(&files).expect("应选出视频");
        assert_eq!(name, "Content/big_buck_bunny_720p_surround.mp4");
        assert_eq!(ext, "mp4");

        // 只有原始视频时选 avi 档最大
        let files_o = vec![mk("x.avi", "400000000"), mk("y.mpg", "100000")];
        let (name_o, ext_o) = pick_media(&files_o).expect("应选出原始视频");
        assert_eq!(name_o, "x.avi");
        assert_eq!(ext_o, "avi");

        // 只有音频时选音频
        let files2 = vec![mk("a.mp3", "3000"), mk("b.flac", "9000")];
        let (name2, ext2) = pick_media(&files2).expect("应选出音频");
        assert_eq!(name2, "b.flac");
        assert_eq!(ext2, "flac");

        // 无媒体文件
        let files3 = vec![mk("book.pdf", "1000")];
        assert!(pick_media(&files3).is_none());
    }

    /// 真实条目（Big Buck Bunny 经典镜像，长期稳定）：视频直链 + 标题文件名
    #[tokio::test]
    async fn test_archive_real_item() {
        if std::env::var("FEILIU_NET_TEST").is_err() {
            return;
        }
        let info = super::super::resolve_retry("https://archive.org/details/BigBuckBunny_124")
            .await
            .expect("Internet Archive 解析失败");
        assert!(
            info.direct_url.starts_with("https://archive.org/download/BigBuckBunny_124/"),
            "直链应为 archive.org/download 形态: {}",
            info.direct_url
        );
        assert!(info.notice.is_none(), "有媒体文件应直接给直链");
        assert!(
            !info.filename.is_empty() && info.filename.ends_with(".mp4"),
            "文件名应带标题与扩展名: {}",
            info.filename
        );
        println!("archive OK: {} ← {}", info.filename, info.direct_url);
    }

    /// 真实纯音频条目（LibriVox 有声书，48 个 mp3 无视频）：音频档直链
    #[tokio::test]
    async fn test_archive_real_audio_item() {
        if std::env::var("FEILIU_NET_TEST").is_err() {
            return;
        }
        let info = super::super::resolve_retry(
            "https://archive.org/details/nonfiction013_librivox",
        )
        .await
        .expect("音频条目解析失败");
        assert!(
            info.direct_url.ends_with(".mp3"),
            "无视频条目应选音频档 mp3: {}",
            info.direct_url
        );
        assert!(info.notice.is_none(), "有音频应直接给直链");
        assert!(
            info.filename.ends_with(".mp3"),
            "文件名扩展名应为 mp3: {}",
            info.filename
        );
        println!("archive audio OK: {}", info.filename);
    }
}
