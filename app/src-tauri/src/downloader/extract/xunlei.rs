//! 迅雷云盘（pan.xunlei.com）分享链接识别与诚实引导。
//!
//! 端点事实（r5 轮22-24 IAB 实测）：页内 share API（api-pan.xunlei.com/
//! drive/v1/share?share_id&pass_code&pass_code_token）匿名可列（页内 JS 自动
//! 完成 captcha init），但请求需三头（x-device-id/x-client-id/x-captcha-token，
//! 全可从页面 localStorage 构造）——纯 API 死端（轮 23 device_id is empty），
//! 列表需 WebView 代取（挂待办）。**下载=页面无访客入口，转存登录墙**
//! （保存到云盘后 file/download）——本模块现行形态=URL 识别 + canonical
//! 键 + 诚实引导 notice（避免 probe 链路把分享页当普通 HTTP 下载乱试）。
//! 登录态全功能（页内转存+直链）挂后续大项，待用户迅雷账号登录。
const REFERER: &str = "https://pan.xunlei.com/";

/// 从 URL 提取 share_id（/s/{id}）与提取码（?pwd=）
fn parse_share_url(url: &str) -> Option<String> {
    let lower = url.to_lowercase();
    let idx = lower.find("/s/")? + 3;
    let id: String = url[idx..]
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
        .collect();
    if id.len() < 10 {
        return None;
    }
    Some(id)
}

pub async fn resolve(url: &str) -> Option<super::Extracted> {
    let share_id = parse_share_url(url)?;
    let pwd = url
        .split("pwd=")
        .nth(1)
        .map(|p| p.trim_end_matches(['&', '#', '/']).to_string())
        .unwrap_or_default();
    let name_part = if pwd.is_empty() {
        format!("迅雷分享 {share_id}")
    } else {
        format!("迅雷分享 {share_id}（提取码已识别）")
    };
    Some(super::Extracted {
        direct_url: String::new(),
        filename: name_part,
        referer: Some(REFERER.into()),
        notice: Some(
            "迅雷云盘分享：需登录迅雷账号后「保存到我的云盘」再下载（页面无访客下载入口，转存为平台机制）。应用暂不支持迅雷登录态，请直接在迅雷官网操作。".into(),
        ),
        cookies: None,
        user_agent: None,
        webview_plan: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 从 URL 提取 share_id 与提取码识别
    #[test]
    fn test_parse_share_url() {
        assert_eq!(
            parse_share_url("https://pan.xunlei.com/s/VP5Rd1MFgeOo_nug9cPs_io_A1?pwd=dvvq"),
            Some("VP5Rd1MFgeOo_nug9cPs_io_A1".into())
        );
        assert_eq!(
            parse_share_url("https://pan.xunlei.com/s/short"),
            None
        );
        assert_eq!(parse_share_url("https://pan.baidu.com/s/1abc"), None);
    }

    /// 真实夹具（FEILIU_NET_TEST，国内站）：识别+诚实引导（永久有效分享）
    #[tokio::test]
    async fn test_xunlei_real_link() {
        if std::env::var("FEILIU_NET_TEST").is_err() {
            return;
        }
        let info = super::super::resolve_retry(
            "https://pan.xunlei.com/s/VP5Rd1MFgeOo_nug9cPs_io_A1?pwd=dvvq",
        )
        .await
        .expect("迅雷解析失败");
        assert!(
            info.filename.contains("迅雷分享"),
            "应识别为迅雷分享: {}",
            info.filename
        );
        assert!(
            info.direct_url.is_empty()
                && info
                    .notice
                    .as_deref()
                    .unwrap_or("")
                    .contains("保存到我的云盘"),
            "应给转存登录墙诚实引导: {:?}",
            info.notice
        );
        println!("xunlei OK: {}", info.filename);
    }
}
