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

/// plan.js（文件夹批量）：单 webview 会话状态机（跨导航 sessionStorage 持久）。
/// 文件夹页收割 /f/t-{JWT} 预授权链接（r4 轮 21 发现，altcha 自动过）→ 逐个
/// 导航 → t- 页 patch fetch + click 普通下载 → 轮询 sessionStorage → data.downurl
/// + 标题前缀文件名。r4 轮 27 起配 init_js UA 伪装（检测门通过，downurl 激活 206）。
const CT_FOLDER_JS: &str = r##"(()=>{ const NAV=String(performance.timeOrigin);
if(window.__fl_nav===NAV) return window.__fl_ctf_r||'';
window.__fl_nav=NAV;
(async()=>{ try{
  const S="ctf_batch_state";
  let st=null; try{ st=JSON.parse(sessionStorage.getItem(S)||"null"); }catch(e){}
  if(!(st&&typeof st.idx==="number")){
    let links=[];
    for(let i=0;i<45;i++){ links=Array.from(document.querySelectorAll('a[href*="/f/t-"]')); if(links.length) break; await new Promise(r=>setTimeout(r,1000)); }
    if(!links.length){ return; }
    const items=links.map(a=>({link:new URL(a.getAttribute("href"),location.origin).href,name:(a.textContent||"").trim().replace(/\s+/g," ")})).filter(x=>x.name&&x.name!=="普通下载");
    if(!items.length){ return; }
    st={items:items,idx:0,results:[]};
    sessionStorage.setItem(S,JSON.stringify(st));
    location.href=st.items[0].link;
    return;
  }
  const item=st.items[st.idx];
  if(!item){ sessionStorage.removeItem(S); window.__fl_ctf_r=''; return; }
  let btn=null;
  for(let i=0;i<30;i++){ btn=Array.from(document.querySelectorAll("button")).find(b=>b.textContent.includes("普通下载")); if(btn) break; await new Promise(r=>setTimeout(r,1000)); }
  if(!btn){ st.idx=st.idx+1; sessionStorage.setItem(S,JSON.stringify(st)); if(st.idx<st.items.length){ location.href=st.items[st.idx].link; } return; }
  sessionStorage.setItem("ctf_full","[]");
  const of=window.fetch;
  window.fetch=async function(...args){ const u=typeof args[0]==='string'?args[0]:(args[0]&&args[0].url)||''; const res=await of.apply(this,args);
    if(u.includes("/url/download")){ try{ const t=await res.clone().text(); const arr=JSON.parse(sessionStorage.getItem("ctf_full")||"[]"); arr.push(t.slice(0,8000)); sessionStorage.setItem("ctf_full",JSON.stringify(arr.slice(-6))); }catch(e){} }
    return res; };
  btn.click();
  let raw='[]', last='', stable=0;
  for(let i=0;i<30;i++){ const cur=sessionStorage.getItem("ctf_full")||"[]"; if(cur===last){ stable++; if(stable>=4 && cur!=="[]") break; } else { last=cur; stable=0; } await new Promise(r=>setTimeout(r,1000)); }
  raw=sessionStorage.getItem("ctf_full")||"[]";
  try{ const arr=JSON.parse(raw); let u="";
    for(const s of arr){ try{ const j=JSON.parse(s); if(j.code===200 && j.data && j.data.downurl){ u=j.data.downurl; } }catch(e){} }
    if(u){ const nm=(item.name||"").replace(/[\s\.]*\d+(\.\d+)?\s*[KMG]?B\s*$/i,"").trim()||item.name; st.results.push({name:nm,url:u}); } }catch(e){}
  st.idx=st.idx+1; sessionStorage.setItem(S,JSON.stringify(st));
  if(st.idx<st.items.length){ location.href=st.items[st.idx].link; return; }
  sessionStorage.removeItem(S);
  if(!st.results.length){ window.__fl_ctf_r=''; return; }
  window.__fl_ctf_r=JSON.stringify({url:st.results[0].url,name:st.results[0].name,total:st.results.length,files:st.results.map(r=>({name:r.name,link:r.url}))});
}catch(e){ window.__fl_ctf_r=''; } })();
return window.__fl_ctf_r||'';
})()"##;


/// document-start UA/特征伪装（城通内核检测在页面 JS 启动时运行，eval 时太晚）
const CT_INIT_JS: &str = r##"(() => {
  try {
    if (!window.chrome) window.chrome = {runtime: {}, loadTimes: function(){}, csi: function(){}, app: {isInstalled: false}};
    Object.defineProperty(Navigator.prototype, "userAgent", {get: function(){ return "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36"; }, configurable: true});
    Object.defineProperty(Navigator.prototype, "platform", {get: function(){ return "Win32"; }, configurable: true});
    Object.defineProperty(Navigator.prototype, "webdriver", {get: function(){ return false; }, configurable: true});
  } catch (e) {}
})();"##;

/// plan.js：等文件页出现（ALTCHA 自动验证 + 密码自动提交后）→ patch fetch
/// 捕获页面自身的 /url/download 响应（access_pass 单次消费，不能重放）→
/// JS click 普通下载 → 轮询 sessionStorage → data.downurl + 标题前缀文件名。
const CT_JS: &str = r##"(()=>{ if(!window.__fl_ct){ window.__fl_ct=1; (async()=>{ try{
  let btn=null;
  for(let i=0;i<50;i++){ btn=Array.from(document.querySelectorAll("button")).find(b=>b.textContent.includes("普通下载")); if(btn) break; await new Promise(r=>setTimeout(r,1000)); }
  if(!btn){ window.__fl_ct_r=''; return; }
  sessionStorage.setItem("ctf_full","[]");
  const of=window.fetch;
  window.fetch=async function(...args){ const u=typeof args[0]==='string'?args[0]:(args[0]&&args[0].url)||''; const res=await of.apply(this,args);
    if(u.includes("/url/download")){ try{ const t=await res.clone().text(); const arr=JSON.parse(sessionStorage.getItem("ctf_full")||"[]"); arr.push(t.slice(0,8000)); sessionStorage.setItem("ctf_full",JSON.stringify(arr.slice(-6))); }catch(e){} }
    return res; };
  btn.click();
  // 等响应稳定（倒计时中间态：取末次 code=200 的 downurl）
  let raw='[]', last='', stable=0;
  for(let i=0;i<30;i++){ const cur=sessionStorage.getItem("ctf_full")||"[]"; if(cur===last){ stable++; if(stable>=4 && cur!=="[]") break; } else { last=cur; stable=0; } await new Promise(r=>setTimeout(r,1000)); }
  raw=sessionStorage.getItem("ctf_full")||"[]";
  let url='', name=(document.title.split(" - ")[0]||"").trim();
  try{ const arr=JSON.parse(raw);
    for(const s of arr){ try{ const j=JSON.parse(s); if(j.code===200 && j.data && j.data.downurl){
      url=j.data.downurl;
      // audit 激活：响应自带 audit_url+签名参数，页面流程靠它激活 CDN 直链（503→206）
      const au=(j.data.audit_url || (j.data.audit && j.data.audit.url) || "");
      const ap=(j.data.audit && j.data.audit.params) || null;
      if(au && ap){ try{ await fetch(au, {method:"POST", headers:{"Content-Type":"application/json"}, body:JSON.stringify(ap)}); }catch(e){} }
    } }catch(e){} }
  }catch(e){}
  if(!url){ window.__fl_ct_r=''; return; }
  // 激活后内页复核（带 cookie Range）
  let stt=0; try{ const rr=await fetch(url, {headers:{Range:"bytes=0-63"}}); stt=rr.status; }catch(e){ stt=-1; }
  if(stt!==206 && stt!==200){ await new Promise(r=>setTimeout(r,6000)); try{ const rr2=await fetch(url, {headers:{Range:"bytes=0-63"}}); stt=rr2.status; }catch(e){ stt=-2; } }
  window.__fl_ct_r=JSON.stringify({url:url,name:name,total:1,files:[{name:name,link:url}]});
}catch(e){ window.__fl_ct_r=''; } })(); }
return window.__fl_ct_r||'';
})()"##;

/// 识别城通分享：{uid子域}.ctfile.com/f/{uid-fid-chk} 或 /d/{uid-fid-chk}（访问码 ?p=xxx）
fn parse_share_url(url: &str) -> Option<String> {
    let lower = url.to_lowercase();
    if !lower.contains("ctfile.com") {
        return None;
    }
    let marker = if lower.contains("/f/") {
        "/f/"
    } else if lower.contains("/d/") {
        "/d/"
    } else {
        return None;
    };
    // 保留 query（?p= 提取码，页面自动预填）
    let idx = lower.find(marker)?;
    let rest = &url[idx + marker.len()..];
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

/// 是否文件夹分享（/d/ 路径）
fn is_folder_url(url: &str) -> bool {
    url.to_lowercase().contains("/d/")
}

pub async fn resolve(url: &str) -> Option<super::Extracted> {
    let lower = url.to_lowercase();
    if !lower.contains("ctfile.com") {
        return None;
    }
    let key = parse_share_url(url)?;
    // 代取页面 URL 原样透传（含 ?p= 提取码，页面自动预填触发 altcha 流程）
    let plan_url = url.to_string();
    if is_folder_url(url) {
        // 文件夹批量：r4 轮 27 UA 伪装过检后 downurl 激活恢复（206），状态机重新启用
        return Some(super::Extracted {
            direct_url: String::new(),
            filename: format!("ct_folder_{key}"),
            referer: Some(CT_REFERER.into()),
            notice: None,
            cookies: None,
            user_agent: None,
            webview_plan: Some(WebviewExtractPlan {
                url: plan_url,
                visible: true,
                init_js: Some(CT_INIT_JS.into()),
                js: CT_FOLDER_JS.into(),
                // altcha + N×(导航+点击+捕获)，按文件数放宽
                timeout_ms: 240_000,
            }),
        });
    }
    Some(super::Extracted {
        // 直链由 WebView 代取产出（ALTCHA 自动验证 + access_pass 单次消费）
        direct_url: String::new(),
        // 真实文件名由代取结果 name 覆盖（引擎命名优先级链：wv.name > info.filename）
        filename: format!("ct_{key}"),
        referer: Some(CT_REFERER.into()),
        notice: None,
        cookies: None,
        user_agent: None,
        webview_plan: Some(WebviewExtractPlan {
            url: plan_url,
            visible: true,
            init_js: Some(CT_INIT_JS.into()),
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
