//! 城通网盘（ctfile）分享解析（r4 轮 17 落地）。
//!
//! 实测结论（2026-11-22 r4 轮 11/16 真浏览器测绘）：
//! - 旧 webapi.ctfile.com/getfile.php 已被 SPA 替代（nid 正则实现上游也失效）；
//!   现代链路=密码页（URL pwd 参数自动预填）→ ALTCHA PoW 浏览器 JS 自动完成
//!   （10-15s）→ 自动进文件页 → **POST api2.ctfile.com/url/download**
//!   JSON {context:{kind:"file",id,folder_id,fk,passcode,type_id,access_pass}}
//!   → code 200 + data.downurl
//! - access_pass 为 ALTCHA 换发的 JWT，localStorage 存取但**单次消费**
//!   （replay 403）→ plan.js 必须「patch fetch 拦截页面自身调用」而非重放
//! - 文件名来源 = document.title 前缀（"烈火文明.rar - jjn1987@..."）
//! - **WebView 代取**（altcha 是浏览器自动解的 PoW，页面内流程可全自动跑通；
//!   下载 Referer 用分享页本身）
use super::WebviewExtractPlan;

const CT_REFERER: &str = "https://www.ctfile.com/";

/// plan.js：等文件页出现（ALTCHA 自动验证 + 密码自动提交后）→ patch fetch
/// 捕获页面自身的 /url/download 响应（access_pass 单次消费，不能重放）→
/// JS click 普通下载 → 轮询 sessionStorage → data.downurl + 标题前缀文件名。
const CT_JS: &str = r##"(()=>{ if(!window.__fl_ct){ window.__fl_ct=1; (async()=>{ try{
  let btn=null;
  for(let i=0;i<50;i++){ btn=Array.from(document.querySelectorAll("button")).find(b=>b.textContent.includes("普通下载")); if(btn) break; await new Promise(r=>setTimeout(r,1000)); }
  if(!btn){ window.__fl_ct_r=''; return; }
  sessionStorage.setItem("ctf_full","");
  const of=window.fetch;
  window.fetch=async function(...args){ const u=typeof args[0]==='string'?args[0]:(args[0]&&args[0].url)||''; const res=await of.apply(this,args);
    if(u.includes("/url/download")){ try{ const t=await res.clone().text(); sessionStorage.setItem("ctf_full",t.slice(0,8000)); }catch(e){} }
    return res; };
  btn.click();
  let raw='';
  for(let i=0;i<40;i++){ raw=sessionStorage.getItem("ctf_full")||""; if(raw) break; await new Promise(r=>setTimeout(r,1000)); }
  if(!raw){ window.__fl_ct_r=''; return; }
  const j=JSON.parse(raw);
  const url=(j.data&&j.data.downurl)||"";
  const name=(document.title.split(" - ")[0]||"").trim();
  window.__fl_ct_r=JSON.stringify({url:url,name:name});
}catch(e){ window.__fl_ct_r=''; } })(); }
return window.__fl_ct_r||'';
})()"##;

/// 识别城通分享：{uid子域}.ctfile.com/f/{uid-fid-chk}（访问码 ?p=xxx）
fn parse_share_url(url: &str) -> Option<String> {
    let lower = url.to_lowercase();
    if !lower.contains("ctfile.com") || !lower.contains("/f/") {
        return None;
    }
    // 保留 query（?p= 提取码，页面自动预填）
    let idx = lower.find("/f/")?;
    let rest = &url[idx + "/f/".len()..];
    let key: String = rest
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '-')
        .collect();
    if key.is_empty() {
        None
    } else {
        Some(key)
    }
}

pub async fn resolve(url: &str) -> Option<super::Extracted> {
    let key = parse_share_url(url)?;
    // 代取页面 URL 原样透传（含 ?p= 提取码，页面自动预填触发 altcha 流程）
    let plan_url = url.to_string();
    Some(super::Extracted {
        // 直链由 WebView 代取产出（ALTCHA 自动验证 + access_pass 单次消费）
        direct_url: String::new(),
        // 真实文件名由代取结果 name 覆盖（引擎命名优先级链：wv.name > info.filename）
        filename: format!("ct_{key}"),
        referer: Some(CT_REFERER.into()),
        notice: None,
        cookies: None,
        webview_plan: Some(WebviewExtractPlan {
            url: plan_url,
            visible: true,
            js: CT_JS.into(),
            // ALTCHA PoW 10-15s + 下载轮询，比 gofile 更长
            timeout_ms: 90_000,
        }),
    })
}

#[cfg(test)]
mod tests {
    /// 真实公开分享（r4 轮 11 收割）：解析层应产出 WebView 代取计划
    #[tokio::test]
    async fn test_ct_real_share() {
        if std::env::var("FEILIU_NET_TEST").is_err() {
            return;
        }
        let info = super::super::resolve_retry(
            "https://url11.ctfile.com/f/9603111-996209741-b3814a?p=5425",
        )
        .await
        .expect("城通解析失败");
        assert!(
            info.webview_plan.is_some(),
            "城通必须走 WebView 代取（ALTCHA 墙）"
        );
        assert!(info.direct_url.is_empty(), "解析层不产直链");
        assert!(
            info.filename.starts_with("ct_"),
            "占位文件名: {}",
            info.filename
        );
        assert!(
            info.webview_plan.as_ref().unwrap().url.contains("p=5425"),
            "提取码须透传给代取页面"
        );
    }

    #[test]
    fn test_parse_share_url() {
        assert_eq!(
            super::parse_share_url("https://url11.ctfile.com/f/9603111-996209741-b3814a?p=5425"),
            Some("9603111-996209741-b3814a".into())
        );
        assert_eq!(
            super::parse_share_url("https://ctfile.com/f/64115194-17569800420720-06c697"),
            Some("64115194-17569800420720-06c697".into())
        );
        assert_eq!(super::parse_share_url("https://www.ctfile.com/"), None);
    }
}
