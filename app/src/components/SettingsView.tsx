import { useEffect, useState } from "react";
import { getSettings, pickFolder, setSettings, type Settings } from "../lib/api";
import { IconFolder } from "./Icons";

export default function SettingsView() {
  const [settings, setLocal] = useState<Settings | null>(null);
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);

  useEffect(() => {
    getSettings().then(setLocal);
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

  return (
    <div className="settings-page">
      <div className="settings-card">
        <h3>下载设置</h3>

        <div className="field">
          <label>默认保存位置</label>
          <div className="url-row">
            <input className="text-input" value={settings.download_dir} readOnly />
            <button className="btn ghost" onClick={chooseDir}>
              <IconFolder size={15} /> 选择…
            </button>
          </div>
        </div>

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

        <div className="settings-actions">
          <button className="btn primary" onClick={save} disabled={saving}>
            {saving ? "保存中…" : saved ? "已保存 ✓" : "保存设置"}
          </button>
        </div>
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
