import { useEffect, useRef, useState } from "react";
import {
  addTask,
  addNeteasePlaylist,
  addBilibiliCollection,
  addBandcampAlbum,
  addBilibiliBangumi,
  addLanzouFolder,
  getSettings,
  isMobile,
  pickFolder,
  pickTorrentFile,
  probeMirrors,
  probeUrl,
  type MirrorProbe,
  type ProbeResult,
} from "../lib/api";
import { formatBytes, formatSpeed } from "../lib/format";
import { IconAlert, IconBolt, IconClipboard, IconClose, IconFolder, IconLink } from "./Icons";

interface Props {
  open: boolean;
  initialUrl?: string;
  onClose: () => void;
  onAdded: () => void;
}

export default function NewTaskModal({ open, initialUrl, onClose, onAdded }: Props) {
  const [url, setUrl] = useState("");
  const [probing, setProbing] = useState(false);
  const [probe, setProbe] = useState<ProbeResult | null>(null);
  const [probeError, setProbeError] = useState<string | null>(null);
  const [filename, setFilename] = useState("");
  const [dir, setDir] = useState("");
  const [mirrors, setMirrors] = useState("");
  const [mirrorProbes, setMirrorProbes] = useState<MirrorProbe[] | null>(null);
  const [batchMsg, setBatchMsg] = useState<string | null>(null);
  const [batching, setBatching] = useState(false);
  const [mirrorProbing, setMirrorProbing] = useState(false);
  const [connections, setConnections] = useState(16);
  const [submitting, setSubmitting] = useState(false);
  const [mobile, setMobile] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);
  const [batchLimit, setBatchLimit] = useState(30);
  const debounceRef = useRef<number | null>(null);
  const mirrorDebounceRef = useRef<number | null>(null);

  useEffect(() => {
    isMobile().then(setMobile);
    if (open) {
      getSettings().then((s) => {
        setDir((d) => d || s.download_dir);
        setConnections(s.max_connections);
        setBatchLimit(s.batch_limit);
      });
      if (initialUrl) {
        setUrl(initialUrl);
        void doProbeRef.current(initialUrl);
      } else if (!mobile) {
        // 打开弹窗时剪贴板里若已是链接 → 自动预填并探测（替代手动粘贴）
        void (async () => {
          const { readClipboard } = await import("../lib/api");
          const text = await readClipboard();
          if (!text) return;
          const u = extractFirstUrl(text);
          if (/^https?:\/\/\S+$/i.test(u) || /^magnet:\?/i.test(u)) {
            setUrl(u);
            if (/^https?:/i.test(u)) void doProbeRef.current(u);
          }
        })();
      }
      setTimeout(() => inputRef.current?.focus(), 50);
    } else {
      setUrl("");
      setProbe(null);
      setProbeError(null);
      setFilename("");
      setMirrors("");
      setMirrorProbes(null);
      setProbing(false);
      setSubmitting(false);
    }
  }, [open]);

  /** 从任意粘贴文本中提取第一个链接（兼容【标题】https://... 分享格式） */
  const extractFirstUrl = (raw: string): string => {
    const t = raw.trim();
    if (/^https?:\/\/[!-~]+$/i.test(t)) return t;
    // URL 仅含 ASCII 可见字符：中文文案（逗号/书名号/空格外字符）一律截断
    const m = t.match(/https?:\/\/[!-~]+/i);
    return m ? m[0] : t;
  };

  const doProbe = async (raw: string) => {
    const u = extractFirstUrl(raw);
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
  const doProbeRef = useRef(doProbe);
  doProbeRef.current = doProbe;

  const parseMirrors = (raw: string, main?: string): string[] =>
    raw
      .split("\n")
      .map((s) => s.trim())
      .filter((s) => /^https?:\/\/\S+/i.test(s) && (!main || s !== main));

  const onUrlChange = (v: string) => {
    const extracted = extractFirstUrl(v);
    setUrl(extracted);
    setProbe(null);
    setProbeError(null);
    if (debounceRef.current) window.clearTimeout(debounceRef.current);
    if (/^https?:\/\/.+/i.test(extracted)) {
      debounceRef.current = window.setTimeout(() => doProbe(extracted), 600);
    }
  };

  // 镜像测速：粘贴镜像后自动采样，展示可达性与速度
  useEffect(() => {
    if (!open || !probe) return;
    const list = parseMirrors(mirrors, probe.url);
    if (mirrorDebounceRef.current) window.clearTimeout(mirrorDebounceRef.current);
    if (list.length === 0) {
      setMirrorProbes(null);
      return;
    }
    mirrorDebounceRef.current = window.setTimeout(async () => {
      setMirrorProbing(true);
      try {
        const r = await probeMirrors(list);
        r.sort((a, b) => (b.ok ? b.speed_bps : -1) - (a.ok ? a.speed_bps : -1));
        setMirrorProbes(r);
      } catch {
        setMirrorProbes(null);
      } finally {
        setMirrorProbing(false);
      }
    }, 700);
  }, [mirrors, probe, open]);

  const batchAdd = async () => {
    if (!probe || batching) return;
    setBatching(true);
    try {
      const kind = probe.batch_kind;
      const invokeBatch =
        kind === "bilibili_collection"
          ? addBilibiliCollection(probe.url, dir.trim() || undefined, connections)
          : kind === "lanzou_folder"
            ? addLanzouFolder(probe.url, dir.trim() || undefined, connections)
            : kind === "bandcamp_album"
              ? addBandcampAlbum(probe.url, dir.trim() || undefined, connections)
              : kind === "bilibili_bangumi"
                ? addBilibiliBangumi(probe.url, dir.trim() || undefined, connections)
                : addNeteasePlaylist(probe.url, dir.trim() || undefined, connections);
      const r = await invokeBatch;
      const unit =
      kind === "netease_playlist" || kind === "bandcamp_album" ? "首" : "个";
      setBatchMsg(
        `已添加 ${r.added} ${unit}，跳过 ${r.skipped} ${unit}（失败/重复）` +
          (r.truncated ? `；较长已截取前 ${batchLimit} ${unit}` : ""),
      );
      onAdded();
    } catch (e) {
      setProbeError(String(e));
    } finally {
      setBatching(false);
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
    if (submitting) return;
    const u = extractFirstUrl(url);
    if (!/^https?:\/\/.+/i.test(u) && !u.startsWith("magnet:") && !/\.torrent$/i.test(u)) {
      return;
    }
    setSubmitting(true);
    try {
      if (u.startsWith("magnet:") || /\.torrent$/i.test(u)) {
        // BT 任务：直接创建，无需探测
        await addTask(u, [], undefined, dir.trim() || undefined, undefined);
        onAdded();
        onClose();
        return;
      }
      if (!probe) return;
      if (probe.notice) {
        setProbeError(probe.notice);
        return;
      }
      // 有测速结果时按速度从快到慢排序镜像
      const list = mirrorProbes
        ? mirrorProbes.filter((m) => m.ok).map((m) => m.url)
        : parseMirrors(mirrors, probe.url);
      await addTask(
        probe.url,
        list,
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

  const isMagnet = url.trim().startsWith("magnet:");
  const isTorrentFile = /\.torrent$/i.test(url.trim());
  const isTorrentTask = isMagnet || isTorrentFile;

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
          <label>下载链接（支持直链 / B 站视频页 / 磁力链 / .torrent）</label>
          <div className="url-row">
            <input
              ref={inputRef}
              className="text-input"
              placeholder="粘贴直链、B 站视频页链接或磁力链 magnet:…"
              value={url}
              onChange={(e) => onUrlChange(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && submit()}
            />
            <button className="btn ghost" title="粘贴" onClick={paste}>
              <IconClipboard size={15} /> 粘贴
            </button>
          </div>
          {!mobile && (
            <div className="torrent-row">
              <button
                className="btn ghost"
                onClick={async () => {
                  const p = await pickTorrentFile();
                  if (p) onUrlChange(p);
                }}
              >
                🧲 选择 .torrent 文件…
              </button>
              <span className="torrent-note">
                磁力/种子将创建 BT 任务，下载到保存目录
              </span>
            </div>
          )}
        </div>

        {isTorrentTask && (
          <div className="probe-hint">
            🧲 BT 任务：{isMagnet ? "磁力链接" : "本地种子文件"}，元数据获取后显示文件名与大小
          </div>
        )}

        {probing && <div className="probe-hint">正在探测文件信息…</div>}

        {probeError && (
          <div className="probe-error">
            <IconAlert size={15} /> {probeError}
          </div>
        )}

        {probe && probe.notice && (
          <div className="probe-error notice-box">
            <IconAlert size={15} /> <div>
              <b>{probe.filename}</b>
              <div className="notice-text">{probe.notice}</div>
              {probe.batch_kind && !batchMsg && (
                <button
                  className="btn primary sm batch-btn"
                  onClick={batchAdd}
                  disabled={batching}
                >
                  {batching
                    ? "解析添加中…"
                    : probe.batch_kind === "bilibili_collection"
                      ? `⚡ 一键添加合集/系列视频（前 ${batchLimit} 个）`
                      : probe.batch_kind === "lanzou_folder"
                        ? `⚡ 一键添加文件夹文件（前 ${batchLimit} 个）`
                        : probe.batch_kind === "bandcamp_album"
                          ? `⚡ 一键添加专辑曲目（前 ${batchLimit} 首）`
                          : probe.batch_kind === "bilibili_bangumi"
                            ? `⚡ 一键添加番剧集（前 ${batchLimit} 集）`
                            : "⚡ 一键添加免费歌曲（上限 30 首）"}
                </button>
              )}
              {batchMsg && <div className="notice-ok">{batchMsg}</div>}
            </div>
          </div>
        )}

        {probe && !probe.notice && (
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
                  {parseMirrors(mirrors, probe.url).length > 0
                    ? `${parseMirrors(mirrors, probe.url).length} 个镜像`
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
              {mirrorProbing && <div className="mirror-hint">正在对镜像测速（采样 256KB）…</div>}
              {mirrorProbes && (
                <div className="mirror-probe-list">
                  {mirrorProbes.map((m) => (
                    <div key={m.url} className={`mirror-probe ${m.ok ? "ok" : "bad"}`}>
                      <span className="mp-url" title={m.url}>
                        {m.ok ? "✓" : "✕"} {m.url}
                      </span>
                      <span className="mp-speed">
                        {m.ok ? formatSpeed(m.speed_bps) : "不可用"}
                      </span>
                    </div>
                  ))}
                </div>
              )}
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
        {probe && probe.notice && (
          <div className="modal-actions">
            <button className="btn ghost" onClick={onClose}>
              关闭
            </button>
          </div>
        )}
      </div>
    </div>
  );
}
