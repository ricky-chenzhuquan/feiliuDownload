import { useEffect, useState } from "react";
import {
  checkForUpdate,
  getSettings,
  isMobile,
  pickFolder,
  setSettings,
  installUpdate,
  type Settings,
  type UpdateInfo,
} from "../lib/api";
import { IconFolder } from "./Icons";

interface Props {
  onLoaded?: (s: Settings) => void;
}

export default function SettingsView({ onLoaded }: Props) {
  const [settings, setLocal] = useState<Settings | null>(null);
  const [mobile, setMobile] = useState(false);
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);
  const [updateState, setUpdateState] = useState<
    "idle" | "checking" | UpdateInfo | "error"
  >("idle");
  const [installing, setInstalling] = useState(false);

  useEffect(() => {
    getSettings().then((s) => {
      setLocal(s);
      onLoaded?.(s);
    });
    isMobile().then(setMobile);
  }, []);

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

      <div className="settings-card muted">
        <h3>关于多连接加速</h3>
        <p>
          多连接下载需要服务器支持「断点续传」（Accept-Ranges）。新建任务时会自动探测，
          不支持的服务器会自动降级为单连接下载。
        </p>
        <p>部分网站会对单个连接限速，此时提高连接数可以显著提升速度。</p>
        <p className="version-line">飞流下载 v0.1.0 · Tauri + Rust 引擎</p>
      </div>
    </div>
  );
}
