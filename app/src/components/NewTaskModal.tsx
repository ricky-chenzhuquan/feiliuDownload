import { useEffect, useRef, useState } from "react";
import {
  addTask,
  getSettings,
  isMobile,
  pickFolder,
  probeUrl,
  type ProbeResult,
} from "../lib/api";
import { formatBytes } from "../lib/format";
import { IconAlert, IconBolt, IconClipboard, IconClose, IconFolder, IconLink } from "./Icons";

interface Props {
  open: boolean;
  onClose: () => void;
  onAdded: () => void;
}

export default function NewTaskModal({ open, onClose, onAdded }: Props) {
  const [url, setUrl] = useState("");
  const [probing, setProbing] = useState(false);
  const [probe, setProbe] = useState<ProbeResult | null>(null);
  const [probeError, setProbeError] = useState<string | null>(null);
  const [filename, setFilename] = useState("");
  const [dir, setDir] = useState("");
  const [mirrors, setMirrors] = useState("");
  const [connections, setConnections] = useState(16);
  const [submitting, setSubmitting] = useState(false);
  const [mobile, setMobile] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);
  const debounceRef = useRef<number | null>(null);

  useEffect(() => {
    isMobile().then(setMobile);
    if (open) {
      getSettings().then((s) => {
        setDir((d) => d || s.download_dir);
        setConnections(s.max_connections);
      });
      setTimeout(() => inputRef.current?.focus(), 50);
    } else {
      setUrl("");
      setProbe(null);
      setProbeError(null);
      setFilename("");
      setMirrors("");
      setProbing(false);
      setSubmitting(false);
    }
  }, [open]);

  const doProbe = async (raw: string) => {
    const u = raw.trim();
    if (!/^https?:\/\/.+/i.test(u)) return;
    setProbing(true);
    setProbeError(null);
    try {
      const r = await probeUrl(u);
      setProbe(r);
      setFilename(r.filename);
    } catch (e) {
      setProbe(null);
      setProbeError(String(e));
    } finally {
      setProbing(false);
    }
  };

  const onUrlChange = (v: string) => {
    setUrl(v);
    setProbe(null);
    setProbeError(null);
    if (debounceRef.current) window.clearTimeout(debounceRef.current);
    if (/^https?:\/\/.+/i.test(v.trim())) {
      debounceRef.current = window.setTimeout(() => doProbe(v), 600);
    }
  };

  const paste = async () => {
    try {
      const text = await navigator.clipboard.readText();
      if (text) onUrlChange(text.trim());
    } catch {
      /* 剪贴板不可用时忽略 */
    }
  };

  const chooseDir = async () => {
    const p = await pickFolder();
    if (p) setDir(p);
  };

  const submit = async () => {
    if (!probe || submitting) return;
    setSubmitting(true);
    try {
      const mirrorList = mirrors
        .split("\n")
        .map((s) => s.trim())
        .filter((s) => /^https?:\/\/.+/i.test(s) && s !== probe.url);
      await addTask(
        probe.url,
        mirrorList,
        filename.trim() || undefined,
        dir.trim() || undefined,
        connections,
      );
      onAdded();
      onClose();
    } catch (e) {
      setProbeError(String(e));
    } finally {
      setSubmitting(false);
    }
  };

  if (!open) return null;

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal-head">
          <IconLink />
          <h2>新建下载任务</h2>
          <button className="icon-btn" onClick={onClose}>
            <IconClose />
          </button>
        </div>

        <div className="field">
          <label>下载链接</label>
          <div className="url-row">
            <input
              ref={inputRef}
              className="text-input"
              placeholder="粘贴 http/https 直链，例如 https://example.com/file.zip"
              value={url}
              onChange={(e) => onUrlChange(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && submit()}
            />
            <button className="btn ghost" onClick={paste}>
              <IconClipboard size={15} /> 粘贴
            </button>
          </div>
        </div>

        {probing && <div className="probe-hint">正在探测文件信息…</div>}

        {probeError && (
          <div className="probe-error">
            <IconAlert size={15} /> {probeError}
          </div>
        )}

        {probe && (
          <>
            <div className="field">
              <label>保存文件名</label>
              <input
                className="text-input"
                value={filename}
                onChange={(e) => setFilename(e.target.value)}
              />
            </div>

            <div className="probe-meta">
              <div className="meta-item">
                <span className="meta-label">文件大小</span>
                <span className="meta-value">
                  {probe.total_size > 0 ? formatBytes(probe.total_size) : "未知"}
                </span>
              </div>
              <div className="meta-item">
                <span className="meta-label">断点续传</span>
                <span className="meta-value">
                  {probe.accept_ranges ? (
                    <span className="badge-ok">
                      <IconBolt size={13} /> 支持，可多线程加速
                    </span>
                  ) : (
                    <span className="badge-no">不支持，将单连接下载</span>
                  )}
                </span>
              </div>
            </div>

            <div className="field">
              <label>
                镜像地址（可选，每行一个，多源并行叠加加速）
                <span className="label-hint">
                  {mirrors
                    ? `${mirrors.split("\n").filter((s) => /^https?:\/\/\S+/.test(s.trim()) && s.trim() !== probe.url).length} 个镜像`
                    : ""}
                </span>
              </label>
              <textarea
                className="text-input mirror-input"
                placeholder="同一文件的其它线路/镜像直链，例如另一个 CDN 的地址"
                rows={2}
                value={mirrors}
                onChange={(e) => setMirrors(e.target.value)}
              />
            </div>

            <div className="field">
              <label>连接数（越大越快，取决于服务器限速）</label>
              <select
                className="select-input"
                value={connections}
                onChange={(e) => setConnections(Number(e.target.value))}
              >
                {[1, 4, 8, 16, 24, 32, 48, 64].map((n) => (
                  <option key={n} value={n}>
                    {n} 连接
                  </option>
                ))}
              </select>
            </div>

            {!mobile && (
              <div className="field">
                <label>保存位置</label>
                <div className="url-row">
                  <input className="text-input" value={dir} readOnly />
                  <button className="btn ghost" onClick={chooseDir}>
                    <IconFolder size={15} /> 选择…
                  </button>
                </div>
              </div>
            )}

            <div className="modal-actions">
              <button className="btn ghost" onClick={onClose}>
                取消
              </button>
              <button className="btn primary" onClick={submit} disabled={submitting}>
                {submitting ? "添加中…" : "添加下载"}
              </button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
