//! GoFile（gofile.io）网盘分享解析。
//!
//! 实测结论（2026-11-21 r3 轮 5/7，真机自检裁决）：
//! - 匿名 API 链路：guest 账号（POST api.gofile.io/accounts）→ contents 接口
//!   强制 `X-Website-Token` 签名（wt.obf.js 混淆 + 服务端轮换密钥）——Rust 侧
//!   无法复制签名 → **WebView 代取**：页面自带 guest 流程 + window.generateWT
//! - 页面内提取：accountToken cookie（非 httpOnly）+ window.generateWT(token)
//!   → GET /contents/{code}（Bearer + X-Website-Token）→ 遍历 children 收文件
//! - 直链 {server}.gofile.io/download/web/{fileId}/{name} **校验会话 cookie**：
//!   带 WebView 现场提取的 cookie = 206 可断点续传，裸链/过期 token = 302
//! - 鉴权形态错误返回 error-notPremium（误导性），限流返回 error-rateLimit
//! - JSON 结果 {url, cookie, name} 走轮 6 的 WebView JSON 通道，引擎 cookie
//!   透传（TaskMeta.cookies）四路径贯穿；多文件分享 v1 取第一个文件
use super::WebviewExtractPlan;

const GOFILE_REFERER: &str = "https://gofile.io/";

/// plan.js：等页面 guest 账号就绪 → generateWT 签名 → contents API →
/// 遍历 children 收全部文件 → URL 带 ?feiliu_i=N 时返回该文件的单文件形态
/// {url,cookie,name}，否则返回列表形态 {url,cookie,name,total,files:[{name,link}]}。
/// （2026-11-21 dev 自检 3 真机验证：提取成功，下载 206）
const GOFILE_JS: &str = r##"(()=>{ if(!window.__fl_go){ window.__fl_go=1; (async()=>{ try{
  let token=''; for(let i=0;i<16;i++){ token=(document.cookie.match(/accountToken=([^;]+)/)||[])[1]||''; if(token) break; await new Promise(r=>setTimeout(r,500)); }
  const m=location.pathname.match(/\/d\/([A-Za-z0-9]+)/); if(!m||!token){ window.__fl_go_r=''; return; }
  const wt=await window.generateWT(token);
  const r=await fetch('https://api.gofile.io/contents/'+m[1]+'?page=1&pageSize=100&sortField=name&sortDirection=1',{headers:{'Authorization':'Bearer '+token,'X-Website-Token':wt,'X-BL':navigator.language||''}});
  const j=await r.json();
  if(j.status!=='ok'){ window.__fl_go_r=''; return; }
  const files=[]; (function walk(n){ const kids=n.children?Object.values(n.children):[]; for(const c of kids){ if(c.type==='file') files.push({link:c.link,name:c.name}); else walk(c);} })(j.data);
  if(!files.length){ window.__fl_go_r=''; return; }
  const qi=new URLSearchParams(location.search).get('feiliu_i');
  const one=(f)=>JSON.stringify({url:f.link,cookie:document.cookie,name:f.name});
  if(qi!=null && files[+qi]){ window.__fl_go_r=one(files[+qi]); return; }
  window.__fl_go_r=JSON.stringify({url:files[0].link,cookie:document.cookie,name:files[0].name,files:files.map(f=>({name:f.name,link:f.link}))});
}catch(e){ window.__fl_go_r=''; } })(); }
return window.__fl_go_r||'';
})()"##;

/// 识别 gofile.io/d/{code} 分享链接，提取短码
fn parse_share_code(url: &str) -> Option<String> {
    let lower = url.to_lowercase();
    if !lower.contains("gofile.io") {
        return None;
    }
    let idx = lower.find("/d/")?;
    let rest = &url[idx + "/d/".len()..];
    // 短码含连字符形态（UUID 直链分享，如 bae7f048-132f-4243-a904-8e51506c282b
    // ——2026-11-21 用户浏览实例发现；纯字母数字短码同样覆盖）
    let code: String = rest
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '-')
        .collect();
    let code = code.trim_end_matches('-');
    if code.is_empty() {
        None
    } else {
        Some(code.to_string())
    }
}

/// 批量序号：?feiliu_i=N（批量命令逐文件建任务的参数，canonical 键含之）
fn parse_batch_index(url: &str) -> Option<usize> {
    let lower = url.to_lowercase();
    for m in ["?feiliu_i=", "&feiliu_i="] {
        if let Some(p) = lower.find(m) {
            let digits: String = url[p + m.len()..]
                .chars()
                .take_while(|c| c.is_ascii_digit())
                .collect();
            if let Ok(n) = digits.parse::<usize>() {
                return Some(n);
            }
        }
    }
    None
}

pub async fn resolve(url: &str) -> Option<super::Extracted> {
    let code = parse_share_code(url)?;
    // 批量序号需透传给代取页面（plan.js 按 location.search 选文件）
    let plan_url = match parse_batch_index(url) {
        Some(i) => format!("https://gofile.io/d/{code}?feiliu_i={i}"),
        None => format!("https://gofile.io/d/{code}"),
    };
    Some(super::Extracted {
        // 直链由 WebView 代取产出（页面内 wt 签名 + 会话 cookie）
        direct_url: String::new(),
        // 真实文件名由代取结果 name 覆盖（引擎命名优先级链：wv.name > info.filename）
        filename: format!("gofile_{code}"),
        referer: Some(GOFILE_REFERER.into()),
        notice: None,
        cookies: None,
        webview_plan: Some(WebviewExtractPlan {
            url: plan_url,
            js: GOFILE_JS.into(),
            timeout_ms: 30_000,
        }),
    })
}

#[cfg(test)]
mod tests {
    /// 真实公开分享（r3 轮 5 自建夹具）：解析层应产出 WebView 代取计划
    #[tokio::test]
    async fn test_gofile_real_share() {
        if std::env::var("FEILIU_NET_TEST").is_err() {
            return;
        }
        let info = super::super::resolve_retry("https://gofile.io/d/d1AQzWks")
            .await
            .expect("gofile 解析失败");
        assert!(
            info.webview_plan.is_some(),
            "gofile 必须走 WebView 代取（wt 签名墙）"
        );
        assert!(info.direct_url.is_empty(), "解析层不产直链");
        assert!(
            info.filename.starts_with("gofile_"),
            "占位文件名: {}",
            info.filename
        );
        let plan = info.webview_plan.unwrap();
        assert!(
            plan.url.ends_with("/d/d1AQzWks"),
            "代取 URL 应为分享页: {}",
            plan.url
        );
        assert!(
            plan.js.contains("generateWT") && plan.js.contains("api.gofile.io/contents"),
            "plan.js 应含签名与 contents 调用"
        );
        println!("gofile probe OK: {} → {}", info.filename, plan.url);
    }

    /// 真实 UUID 形态分享码（连字符不能截断）
    #[tokio::test]
    async fn test_gofile_real_uuid_share() {
        if std::env::var("FEILIU_NET_TEST").is_err() {
            return;
        }
        let info = super::super::resolve_retry(
            "https://gofile.io/d/bae7f048-132f-4243-a904-8e51506c282b",
        )
        .await
        .expect("uuid 分享解析失败");
        let plan = info.webview_plan.expect("uuid 分享应有代取计划");
        assert!(
            plan.url.ends_with("/d/bae7f048-132f-4243-a904-8e51506c282b"),
            "代取 URL 必须保留完整 UUID 短码: {}",
            plan.url
        );
        assert_eq!(info.filename, "gofile_bae7f048-132f-4243-a904-8e51506c282b");
        println!("gofile uuid OK: {}", info.filename);
    }

    /// URL 形态：带 www / 尾部斜杠 / 附加路径均可提取短码；批量序号解析
    #[test]
    fn test_parse_share_code() {
        assert_eq!(
            super::parse_share_code("https://gofile.io/d/d1AQzWks"),
            Some("d1AQzWks".into())
        );
        assert_eq!(
            super::parse_share_code("https://www.gofile.io/d/d1AQzWks/"),
            Some("d1AQzWks".into())
        );
        assert_eq!(super::parse_share_code("https://gofile.io/uploadfile"), None);
        assert_eq!(super::parse_share_code("https://example.com/d/abc"), None);
        assert_eq!(
            super::parse_share_code("https://gofile.io/d/bae7f048-132f-4243-a904-8e51506c282b"),
            Some("bae7f048-132f-4243-a904-8e51506c282b".into()),
            "UUID 形态短码连字符不得截断"
        );
        assert_eq!(super::parse_batch_index("https://gofile.io/d/d1AQzWks?feiliu_i=2"), Some(2));
        assert_eq!(super::parse_batch_index("https://gofile.io/d/d1AQzWks"), None);
        assert_eq!(super::parse_batch_index("https://gofile.io/d/x?feiliu_i=0"), Some(0));
    }
}
