import { useEffect, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";
import {
  checkForUpdate,
  getSettings,
  isMobile,
  pickFolder,
  setSettings,
  installUpdate,
  baiduLogin,
  baiduLoginStatus,
  baiduLogout,
  quarkLogin,
  quarkLoginStatus,
  quarkLogout,
  pan123Login,
  pan123LoginStatus,
  pan123Logout,
  type BaiduLoginStatus,
  type PanLoginStatus,
  type QuarkLoginStatus,
  type Settings,
  type UpdateInfo,
} from "../lib/api";
import { IconFolder } from "./Icons";

interface Props {
  onLoaded?: (s: Settings) => void;
}

export default function SettingsView({ onLoaded }: Props) {
  const [appVersion, setAppVersion] = useState<string>("");
  const [settings, setLocal] = useState<Settings | null>(null);
  useEffect(() => {
    getVersion()
      .then(setAppVersion)
      .catch(() => setAppVersion(""));
  }, []);
  const [mobile, setMobile] = useState(false);
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);
  const [updateState, setUpdateState] = useState<
    "idle" | "checking" | UpdateInfo | "error"
  >("idle");
  const [installing, setInstalling] = useState(false);
  // 百度网盘登录态
  const [baiduStatus, setBaiduStatus] = useState<BaiduLoginStatus | null>(null);
  const [baiduBusy, setBaiduBusy] = useState(false);
  const [baiduMsg, setBaiduMsg] = useState<string | null>(null);

  useEffect(() => {
    getSettings().then((s) => {
      setLocal(s);
      onLoaded?.(s);
    });
    isMobile().then(setMobile);
    baiduLoginStatus().then(setBaiduStatus).catch(() => {});
  }, []);

  const refreshBaidu = () => {
    baiduLoginStatus().then(setBaiduStatus).catch(() => {});
  };

  const baiduLoginAction = async () => {
    setBaiduBusy(true);
    setBaiduMsg(null);
    try {
      const msg = await baiduLogin();
      setBaiduMsg(msg);
      refreshBaidu();
    } catch (e) {
      setBaiduMsg(String(e));
    } finally {
      setBaiduBusy(false);
    }
  };

  const baiduLogoutAction = async () => {
    setBaiduBusy(true);
    try {
      await baiduLogout();
      setBaiduMsg(null);
      refreshBaidu();
    } finally {
      setBaiduBusy(false);
    }
  };

  // 夸克网盘登录态
  const [quarkStatus, setQuarkStatus] = useState<QuarkLoginStatus | null>(null);
  const [quarkBusy, setQuarkBusy] = useState(false);
  const [quarkMsg, setQuarkMsg] = useState<string | null>(null);

  useEffect(() => {
    quarkLoginStatus().then(setQuarkStatus).catch(() => {});
  }, []);

  const refreshQuark = () => {
    quarkLoginStatus().then(setQuarkStatus).catch(() => {});
  };

  const quarkLoginAction = async () => {
    setQuarkBusy(true);
    setQuarkMsg(null);
    try {
      const msg = await quarkLogin();
      setQuarkMsg(msg);
      refreshQuark();
    } catch (e) {
      setQuarkMsg(String(e));
    } finally {
      setQuarkBusy(false);
    }
  };

  const quarkLogoutAction = async () => {
    setQuarkBusy(true);
    try {
      await quarkLogout();
      setQuarkMsg(null);
      refreshQuark();
    } finally {
      setQuarkBusy(false);
    }
  };

  // 123 云盘登录态
  const [panStatus, setPanStatus] = useState<PanLoginStatus | null>(null);
  const [panBusy, setPanBusy] = useState(false);
  const [panMsg, setPanMsg] = useState<string | null>(null);

  useEffect(() => {
    pan123LoginStatus().then(setPanStatus).catch(() => {});
  }, []);

  const refreshPan = () => {
    pan123LoginStatus().then(setPanStatus).catch(() => {});
  };

  const panLoginAction = async () => {
    setPanBusy(true);
    setPanMsg(null);
    try {
      const msg = await pan123Login();
      setPanMsg(msg);
      refreshPan();
    } catch (e) {
      setPanMsg(String(e));
    } finally {
      setPanBusy(false);
    }
  };

  const panLogoutAction = async () => {
    setPanBusy(true);
    try {
      await pan123Logout();
      setPanMsg(null);
      refreshPan();
    } finally {
      setPanBusy(false);
    }
  };

  if (!settings) return <div className="page-loading">加载中…</div>;

  const save = async () => {
    setSaving(true);
    try {
      const s = await setSettings(settings);
      setLocal(s);
      setSaved(true);
      setTimeout(() => setSaved(false), 1500);
    } finally {
      setSaving(false);
    }
  };

  const chooseDir = async () => {
    const p = await pickFolder();
    if (p) setLocal({ ...settings, download_dir: p });
  };

  const check = async () => {
    setUpdateState("checking");
    try {
      const info = await checkForUpdate();
      setUpdateState(info);
    } catch {
      setUpdateState("error");
    }
  };

  const install = async () => {
    setInstalling(true);
    try {
      await installUpdate(); // 成功后自动重启
    } catch (e) {
      alert(String(e));
    } finally {
      setInstalling(false);
    }
  };

  const renderUpdate = () => {
    if (updateState === "idle")
      return (
        <button className="btn ghost" onClick={check}>
          检查更新
        </button>
      );
    if (updateState === "checking") return <span className="dim">检查中…</span>;
    if (updateState === "error")
      return (
        <>
          <span className="dim err">检查失败（网络或无可用发布）</span>
          <button className="btn ghost" onClick={check}>
            重试
          </button>
        </>
      );
    if (updateState.available)
      return (
        <>
          <span className="upd-ok">
            新版本 <b>v{updateState.version}</b> 可用
          </span>
          <button className="btn primary" onClick={install} disabled={installing}>
            {installing ? "下载中…" : "下载并安装"}
          </button>
        </>
      );
    return <span className="dim">当前已是最新版本 ✓</span>;
  };

  return (
    <div className="settings-page">
      <div className="settings-card">
        <h3>下载设置</h3>

        {!mobile && (
          <div className="field">
            <label>默认保存位置</label>
            <div className="url-row">
              <input className="text-input" value={settings.download_dir} readOnly />
              <button className="btn ghost" onClick={chooseDir}>
                <IconFolder size={15} /> 选择…
              </button>
            </div>
          </div>
        )}
        {mobile && (
          <div className="field">
            <label>保存位置</label>
            <div className="mobile-dir-hint">文件保存在应用文档目录（iOS 系统限制）</div>
          </div>
        )}

        <div className="field">
          <label>
            每任务连接数
            <span className="label-hint">{settings.max_connections} 连接</span>
          </label>
          <input
            type="range"
            min={1}
            max={64}
            value={settings.max_connections}
            onChange={(e) =>
              setLocal({ ...settings, max_connections: Number(e.target.value) })
            }
          />
          <div className="range-marks">
            <span>1</span>
            <span>16</span>
            <span>32</span>
            <span>64</span>
          </div>
        </div>

        <div className="field">
          <label>
            批量添加上限
            <span className="label-hint">歌单/合集/专辑/番剧一键批量单次最多加 {settings.batch_limit} 个</span>
          </label>
          <input
            type="number"
            min={1}
            max={500}
            value={settings.batch_limit}
            onChange={(e) => {
              const v = Number(e.target.value);
              if (Number.isFinite(v)) setLocal({ ...settings, batch_limit: v });
            }}
            onBlur={(e) => {
              const v = Math.round(Number(e.target.value));
              const clamped = Number.isFinite(v) ? Math.min(500, Math.max(1, v)) : 30;
              setLocal({ ...settings, batch_limit: clamped });
            }}
          />
        </div>

        <div className="field">
          <label>
            同时下载任务数
            <span className="label-hint">{settings.max_concurrent_tasks} 个</span>
          </label>
          <input
            type="range"
            min={1}
            max={10}
            value={settings.max_concurrent_tasks}
            onChange={(e) =>
              setLocal({ ...settings, max_concurrent_tasks: Number(e.target.value) })
            }
          />
          <div className="range-marks">
            <span>1</span>
            <span>5</span>
            <span>10</span>
          </div>
        </div>

        <label className="check-row">
          <input
            type="checkbox"
            checked={settings.auto_resume_on_launch}
            onChange={(e) =>
              setLocal({ ...settings, auto_resume_on_launch: e.target.checked })
            }
          />
          <span>启动时自动恢复未完成的下载</span>
        </label>

        {!mobile && (
          <label className="check-row">
            <input
              type="checkbox"
              checked={settings.clipboard_watch}
              onChange={(e) =>
                setLocal({ ...settings, clipboard_watch: e.target.checked })
              }
            />
            <span>监听剪贴板（复制下载链接自动弹出新建任务）</span>
          </label>
        )}

        {!mobile && (
          <label className="check-row">
            <input
              type="checkbox"
              checked={settings.close_to_tray}
              onChange={(e) =>
                setLocal({ ...settings, close_to_tray: e.target.checked })
              }
            />
            <span>关闭窗口时最小化到托盘（后台继续下载）</span>
          </label>
        )}

        <div className="settings-actions">
          <button className="btn primary" onClick={save} disabled={saving}>
            {saving ? "保存中…" : saved ? "已保存 ✓" : "保存设置"}
          </button>
        </div>
      </div>

      <div className="settings-card">
        <h3>应用更新</h3>
        <div className="update-row">{renderUpdate()}</div>
        <p className="dim-note">
          应用会自动检查新版本（GitHub Releases 发布通道），更新包经签名校验后安装。
        </p>
      </div>

      <div className="settings-card">
        <h3>百度网盘登录</h3>
        {baiduStatus?.logged_in ? (
          <div className="update-row">
            <span className="upd-ok">
              已登录 ✓（{baiduStatus.saved_at
                ? new Date(baiduStatus.saved_at).toLocaleString()
                : "会话有效"}
              ）
            </span>
            <button
              className="btn ghost"
              onClick={baiduLogoutAction}
              disabled={baiduBusy}
            >
              退出登录
            </button>
          </div>
        ) : (
          <div className="update-row">
            <span className="dim">未登录</span>
            <button
              className="btn primary"
              onClick={baiduLoginAction}
              disabled={baiduBusy}
            >
              {baiduBusy ? "等待登录…" : "登录百度网盘"}
            </button>
          </div>
        )}
        {baiduMsg && <p className="dim-note">{baiduMsg}</p>}
        <p className="dim-note">
          登录后即可用你自己的账号直接下载百度网盘分享内容（提取码自动识别）。
          会话 cookie 只保存在本机（0600 权限），不经过任何第三方。
        </p>
      </div>

      <div className="settings-card muted">
        <h3>夸克网盘登录</h3>
        {quarkStatus?.logged_in ? (
          <div className="update-row">
            <span className="upd-ok">
              已登录 ✓（{quarkStatus.saved_at
                ? new Date(quarkStatus.saved_at).toLocaleString()
                : "会话有效"}
              ）
            </span>
            <button
              className="btn ghost"
              onClick={quarkLogoutAction}
              disabled={quarkBusy}
            >
              退出登录
            </button>
          </div>
        ) : (
          <div className="update-row">
            <span className="dim">未登录</span>
            <button
              className="btn primary"
              onClick={quarkLoginAction}
              disabled={quarkBusy}
            >
              {quarkBusy ? "等待登录…" : "登录夸克网盘"}
            </button>
          </div>
        )}
        {quarkMsg && <p className="dim-note">{quarkMsg}</p>}
        <p className="dim-note">
          登录后即可用你自己的账号下载夸克网盘分享内容（匿名可下小文件，登录后大文件自动转存后直下）。
          会话 cookie 只保存在本机（0600 权限），不经过任何第三方。
        </p>
      </div>

      <div className="settings-card muted">
        <h3>123 云盘登录</h3>
        {panStatus?.logged_in ? (
          <div className="update-row">
            <span className="upd-ok">
              已登录 ✓（{panStatus.saved_at
                ? new Date(panStatus.saved_at).toLocaleString()
                : "会话有效"}
              ）
            </span>
            <button
              className="btn ghost"
              onClick={panLogoutAction}
              disabled={panBusy}
            >
              退出登录
            </button>
          </div>
        ) : (
          <div className="update-row">
            <span className="dim">未登录</span>
            <button
              className="btn primary"
              onClick={panLoginAction}
              disabled={panBusy}
            >
              {panBusy ? "等待登录…" : "登录 123 云盘"}
            </button>
          </div>
        )}
        {panMsg && <p className="dim-note">{panMsg}</p>}
        <p className="dim-note">
          登录后即可用你自己的账号下载 123 云盘分享内容（注册用户每月 10GB 免费提取流量）。
          会话 cookie 只保存在本机（0600 权限），不经过任何第三方。
        </p>
      </div>

      <div className="settings-card muted">
        <h3>关于多连接加速</h3>
        <p>
          多连接下载需要服务器支持「断点续传」（Accept-Ranges）。新建任务时会自动探测，
          不支持的服务器会自动降级为单连接下载。
        </p>
        <p>部分网站会对单个连接限速，此时提高连接数可以显著提升速度。</p>
        <p className="version-line">
          飞流下载 {appVersion ? `v${appVersion}` : ""} · Tauri + Rust 引擎
        </p>
      </div>
    </div>
  );
}
