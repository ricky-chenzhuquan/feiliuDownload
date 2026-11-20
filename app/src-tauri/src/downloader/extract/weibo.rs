//! 微博：m.weibo.cn 视频博文 → WebView 代取 → statuses/show API → stream_url。
//!
//! 实测结论（2026-11-21，官网真实数据验证）：
//! - 匿名 HTTP 全线被新浪游客墙拦截：m.weibo.cn/status/{bid} 页、
//!   /statuses/show?id= 接口、getIndex API 均 302 → passport 游客系统
//!   （genvisitor 端点也已 302，curl 无法铸访客身份）→ **WebView 代取必需**
//!   （页面 JS 自动完成游客流程）
//! - WebView 页面上下文：`GET /statuses/show?id={bid}` → ok:1 →
//!   page_info.media_info.stream_url（f.video.weibocdn.com 签名 mp4）
//! - CDN 直链带 `Referer: https://m.weibo.cn/` → **200 完整 mp4，
//!   Range 206 可断点续传**（无 cookie 要求；引擎 per-task referer 已覆盖）
//! - 真实数据源：m.weibo.cn 首页热门 feed 官方数据收割（bid + 官方 stream_url）
use super::WebviewExtractPlan;

const WB_REFERER: &str = "https://m.weibo.cn/";

fn normalize_status_url(url: &str) -> Option<String> {
    if url.contains("m.weibo.cn") {
        return Some(url.to_string());
    }
    if let Some(i) = url.find("/status/") {
        let rest = &url[i + "/status/".len()..];
        let id: String = rest
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric())
            .collect();
        if !id.is_empty() {
            return Some(format!("https://m.weibo.cn/status/{id}"));
        }
    }
    // 新版 weibo.com/{用户id}/{博文bid}——锚定 weibo.com/ 后取段，
    // 带/不带 scheme 均可（原实现按 segs[1] 判 uid，带 scheme 时 segs[1]
    // 是域名段，导致完整 URL 解析失败）
    if let Some(host_idx) = url.find("weibo.com/") {
        let after = &url[host_idx + "weibo.com/".len()..];
        let path = after.split(['?', '#']).next().unwrap_or(after);
        let segs: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        // segs[0] = 数字 uid，segs[1] = 博文 bid（字母数字混合）
        if segs.len() >= 2
            && segs[0].chars().all(|c| c.is_ascii_digit())
            && !segs[1].is_empty()
        {
            return Some(format!("https://m.weibo.cn/status/{}", segs[1]));
        }
    }
    None // 登录页等无效链接
}

/// plan.js：等游客流程完成 → statuses/show 拿 stream_url → JSON {url, name}。
/// （2026-11-21 浏览器逐环节真实验证：游客墙自动过、API 返回 ok:1、
/// CDN+Referer 下载 206）
const WEIBO_JS: &str = r##"(()=>{ if(!window.__fl_wb){ window.__fl_wb=1; (async()=>{ try{
  const m=location.pathname.match(/status\/([A-Za-z0-9]+)/); if(!m){ window.__fl_wb_r=''; return; }
  let d='';
  for(let i=0;i<20;i++){
    try{
      const r=await fetch('/statuses/show?id='+m[1],{credentials:'include'});
      const j=await r.json();
      if(j.ok && j.data && j.data.page_info && j.data.page_info.type==='video'){
        const mi=(j.data.page_info.media_info||{}).stream_url||'';
        if(mi.startsWith('http')){
          const title=(j.data.page_info.page_title||((j.data.user||{}).screen_name||'weibo')+'微博视频');
          d=JSON.stringify({url:mi,name:title});
          break;
        }
      }
    }catch(e){}
    await new Promise(r=>setTimeout(r,600));
  }
  window.__fl_wb_r=d;
}catch(e){ window.__fl_wb_r=''; } })(); }
return window.__fl_wb_r||'';
})()"##;

pub async fn resolve(url: &str) -> Option<super::Extracted> {
    let target = normalize_status_url(url)?;
    Some(super::Extracted {
        // 直链由 WebView 代取产出（游客墙：匿名 HTTP 全线 302）
        direct_url: String::new(),
        // 真实标题由代取结果 name 覆盖（引擎命名优先级链：wv.name > info.filename）
        filename: format!("weibo_{}", target.rsplit('/').next().unwrap_or("video")),
        referer: Some(WB_REFERER.into()),
        notice: None,
        webview_plan: Some(WebviewExtractPlan {
            url: target,
            js: WEIBO_JS.into(),
            timeout_ms: 30_000,
        }),
    })
}

#[cfg(test)]
mod normalize_tests {
    use super::*;

    /// 新版格式带 scheme（此前 segs[1] 命中域名段而解析失败）
    #[test]
    fn test_normalize_new_format_with_scheme() {
        assert_eq!(
            normalize_status_url("https://weibo.com/1234567/AbCdEfG"),
            Some("https://m.weibo.cn/status/AbCdEfG".into())
        );
    }

    /// 分享文本中的无 scheme 形态（保持原有可用行为）
    #[test]
    fn test_normalize_new_format_without_scheme() {
        assert_eq!(
            normalize_status_url("weibo.com/1234567/AbCdEfG?from=share"),
            Some("https://m.weibo.cn/status/AbCdEfG".into())
        );
    }

    /// 老格式 /status/{id} 与登录页拒绝（行为不变）
    #[test]
    fn test_normalize_status_and_login_reject() {
        assert_eq!(
            normalize_status_url("https://m.weibo.cn/status/51234567890"),
            Some("https://m.weibo.cn/status/51234567890".into())
        );
        assert_eq!(
            normalize_status_url(
                "https://weibo.com/newlogin?tabtype=weibo&gid=102803"
            ),
            None
        );
    }

    /// 真实公开视频博文（2026-11-21 官网热门 feed 官方数据收割）：
    /// 解析层产出 WebView 代取计划（游客墙下匿名 HTTP 全线 302）
    #[tokio::test]
    async fn test_weibo_real_video_post() {
        if std::env::var("FEILIU_NET_TEST").is_err() {
            return;
        }
        let info = super::super::resolve_retry("https://m.weibo.cn/status/Rse19cSin")
            .await
            .expect("weibo 解析失败");
        let plan = info.webview_plan.expect("weibo 必须走 WebView 代取（游客墙）");
        assert!(
            plan.url.ends_with("/status/Rse19cSin"),
            "代取 URL 应为状态页: {}",
            plan.url
        );
        assert!(
            plan.js.contains("/statuses/show?id=") && plan.js.contains("stream_url"),
            "plan.js 应含 statuses/show 调用"
        );
        assert_eq!(
            info.referer.as_deref(),
            Some(super::WB_REFERER),
            "CDN 需 m.weibo.cn Referer（实测无 Referer 403）"
        );
        println!("weibo probe OK: {} → {}", info.filename, plan.url);
    }
}
