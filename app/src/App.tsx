import { useEffect, useState } from "react";
import Sidebar, { type View } from "./components/Sidebar";
import TaskCard from "./components/TaskCard";
import NewTaskModal from "./components/NewTaskModal";
import SettingsView from "./components/SettingsView";
import { IconAlert, IconPause, IconPlay, IconPlus } from "./components/Icons";
import {
  listTasks,
  onTaskEvent,
  pauseAllTasks,
  resumeAllTasks,
  checkForUpdate,
  installUpdate,
  type TaskSnapshot,
  type UpdateInfo,
} from "./lib/api";

export default function App() {
  const [tasks, setTasks] = useState<TaskSnapshot[]>([]);
  const [view, setView] = useState<View>("active");
  const [modalOpen, setModalOpen] = useState(false);
  const [update, setUpdate] = useState<UpdateInfo | null>(null);
  const [installing, setInstalling] = useState(false);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    listTasks().then(setTasks).catch(console.error);
    const p = onTaskEvent((e) => {
      if (e.type === "list") {
        setTasks(e.tasks);
      } else {
        setTasks((prev) => {
          const i = prev.findIndex((t) => t.id === e.task.id);
          if (i === -1) return [e.task, ...prev];
          const next = [...prev];
          next[i] = e.task;
          return next;
        });
      }
    });
    p.then((u) => (unlisten = u));
    return () => unlisten?.();
  }, []);

  // 启动后静默检查更新，有新版本时顶部提示
  useEffect(() => {
    checkForUpdate()
      .then((u) => {
        if (u.available) setUpdate(u);
      })
      .catch(() => {});
  }, []);

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
          <SettingsView />
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
                <button className="btn primary" onClick={() => setModalOpen(true)}>
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
                  <button className="btn primary" onClick={() => setModalOpen(true)}>
                    <IconPlus size={16} /> 新建下载
                  </button>
                </div>
              ) : (
                visible.map((t) => <TaskCard key={t.id} task={t} />)
              )}
            </div>
          </>
        )}
      </main>

      <NewTaskModal
        open={modalOpen}
        onClose={() => setModalOpen(false)}
        onAdded={() => setView("active")}
      />
    </div>
  );
}
