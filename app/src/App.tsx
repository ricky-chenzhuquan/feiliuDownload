import { useEffect, useRef, useState } from "react";
import Sidebar, { type View } from "./components/Sidebar";
import TaskCard from "./components/TaskCard";
import NewTaskModal from "./components/NewTaskModal";
import SettingsView from "./components/SettingsView";
import { IconAlert, IconPause, IconPlay, IconPlus } from "./components/Icons";
import {
  checkForUpdate,
  clearCompletedTasks,
  isMobile,
  listTasks,
  notify,
  onTaskEvent,
  pauseAllTasks,
  readClipboard,
  resumeAllTasks,
  installUpdate,
  type Settings,
  type TaskSnapshot,
  type TaskStatus,
  type UpdateInfo,
} from "./lib/api";

const SAMPLE_WINDOW = 60; // 速度曲线采样点数（约 30 秒）

export default function App() {
  const [tasks, setTasks] = useState<TaskSnapshot[]>([]);
  const [view, setView] = useState<View>("active");
  const [modalOpen, setModalOpen] = useState(false);
  const [modalUrl, setModalUrl] = useState("");
  const [update, setUpdate] = useState<UpdateInfo | null>(null);
  const [installing, setInstalling] = useState(false);
  const [mobile, setMobile] = useState(false);
  const [clipboardWatch, setClipboardWatch] = useState(false);

  const speedSamples = useRef<Map<string, number[]>>(new Map());
  const prevStatus = useRef<Map<string, TaskStatus>>(new Map());
  const lastClipboard = useRef<string>("");

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    listTasks().then((ts) => {
      setTasks(ts);
      for (const t of ts) prevStatus.current.set(t.id, t.status);
    }).catch(console.error);
    const p = onTaskEvent((e) => {
      if (e.type === "list") {
        setTasks(e.tasks);
        for (const t of e.tasks) prevStatus.current.set(t.id, t.status);
      } else {
        const prev = prevStatus.current.get(e.task.id);
        // 速度采样（下载中）
        if (e.task.status === "downloading") {
          const arr = speedSamples.current.get(e.task.id) ?? [];
          arr.push(e.task.speed_bps);
          if (arr.length > SAMPLE_WINDOW) arr.shift();
          speedSamples.current.set(e.task.id, arr);
        } else {
          speedSamples.current.delete(e.task.id);
        }
        // 系统通知：完成 / 失败（跳过应用启动时的初始状态）
        if (prev && prev !== e.task.status) {
          if (e.task.status === "completed") {
            void notify("下载完成 ✓", e.task.filename);
          } else if (e.task.status === "failed") {
            void notify("下载失败 ✕", `${e.task.filename}${e.task.error ? "：" + e.task.error : ""}`);
          }
        }
        prevStatus.current.set(e.task.id, e.task.status);

        setTasks((p) => {
          const i = p.findIndex((t) => t.id === e.task.id);
          if (i === -1) {
            // 已删除任务迟到的进度事件不再“复活”
            return p;
          }
          const next = [...p];
          next[i] = e.task;
          return next;
        });
      }
    });
    p.then((u) => (unlisten = u));
    isMobile().then(setMobile);
    return () => unlisten?.();
  }, []);

  // 剪贴板监听：窗口聚焦时检测到新下载链接自动弹出新建任务
  useEffect(() => {
    if (mobile) return;
    let active = true;
    const check = async () => {
      if (!active || !document.hasFocus() || !clipboardWatch || modalOpen) return;
      const text = await readClipboard();
      const t = text?.trim() ?? "";
      // 从分享文案中提取链接（如【标题】https://...）
      const m = t.match(/https?:\/\/[!-~]+/i);
      const u = (m ? m[0] : t).trim();
      if (u && u !== lastClipboard.current && /^(https?:\/\/\S+|magnet:\?\S+)/i.test(u)) {
        lastClipboard.current = u;
        setModalUrl(u);
        setModalOpen(true);
      }
    };
    const iv = setInterval(check, 1500);
    void check();
    return () => {
      active = false;
      clearInterval(iv);
    };
  }, [mobile, clipboardWatch, modalOpen]);

  // 启动加载设置（用于剪贴板监听开关）+ 静默检查更新
  useEffect(() => {
    checkForUpdate()
      .then((u) => {
        if (u.available) setUpdate(u);
      })
      .catch(() => {});
  }, []);

  const onSettingsLoaded = (s: Settings) => setClipboardWatch(s.clipboard_watch);

  const doInstall = async () => {
    setInstalling(true);
    try {
      await installUpdate(); // 成功后应用自动重启
    } catch (e) {
      alert(String(e));
    } finally {
      setInstalling(false);
    }
  };

  const visible = tasks.filter((t) =>
    view === "completed" ? t.status === "completed" : t.status !== "completed",
  );
  const hasActive = tasks.some((t) => t.status === "downloading" || t.status === "queued");
  const hasResumable = tasks.some((t) => t.status === "paused" || t.status === "failed");

  return (
    <div className="app-shell">
      <Sidebar view={view} onView={setView} tasks={tasks} />
      <main className="content">
        {view === "settings" ? (
          <SettingsView onLoaded={onSettingsLoaded} />
        ) : (
          <>
            {update && (
              <div className="update-banner">
                <IconAlert size={16} />
                <span>
                  发现新版本 <b>v{update.version}</b>
                  {update.notes ? ` — ${update.notes}` : ""}
                </span>
                <button className="btn primary sm" onClick={doInstall} disabled={installing}>
                  {installing ? "下载中…" : "立即更新"}
                </button>
                <button className="icon-btn" onClick={() => setUpdate(null)} title="忽略">
                  ✕
                </button>
              </div>
            )}

            <div className="content-head">
              <h1>{view === "completed" ? "已完成" : "下载中"}</h1>
              <div className="head-actions">
                {view === "active" && hasActive && (
                  <button className="btn ghost" onClick={() => pauseAllTasks().catch(() => {})}>
                    <IconPause size={15} /> 全部暂停
                  </button>
                )}
                {view === "active" && hasResumable && (
                  <button className="btn ghost" onClick={() => resumeAllTasks().catch(() => {})}>
                    <IconPlay size={15} /> 全部继续
                  </button>
                )}
                {view === "completed" &&
                  tasks.some((t) => t.status === "completed") && (
                    <button
                      className="btn ghost"
                      onClick={async () => {
                        const n = await clearCompletedTasks().catch(() => 0);
                        if (n > 0) void notify("已清除", `${n} 个已完成任务已移出列表（文件保留）`);
                      }}
                    >
                      清除已完成
                    </button>
                  )}
                <button className="btn primary" onClick={() => { setModalUrl(""); setModalOpen(true); }}>
                  <IconPlus size={16} /> 新建下载
                </button>
              </div>
            </div>

            <div className="task-list">
              {visible.length === 0 ? (
                <div className="empty-state">
                  <div className="empty-icon">⚡</div>
                  <div className="empty-title">
                    {view === "completed" ? "还没有完成的任务" : "暂无下载任务"}
                  </div>
                  <div className="empty-sub">
                    粘贴一个下载链接，开始高速多线程下载
                  </div>
                  <button className="btn primary" onClick={() => { setModalUrl(""); setModalOpen(true); }}>
                    <IconPlus size={16} /> 新建下载
                  </button>
                </div>
              ) : (
                visible.map((t) => (
                  <TaskCard key={t.id} task={t} samples={speedSamples.current.get(t.id) ?? []} />
                ))
              )}
            </div>
          </>
        )}
      </main>

      <NewTaskModal
        open={modalOpen}
        initialUrl={modalUrl}
        onClose={() => setModalOpen(false)}
        onAdded={() => setView("active")}
      />
    </div>
  );
}
