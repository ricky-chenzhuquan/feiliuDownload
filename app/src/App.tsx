import { useEffect, useState } from "react";
import Sidebar, { type View } from "./components/Sidebar";
import TaskCard from "./components/TaskCard";
import NewTaskModal from "./components/NewTaskModal";
import SettingsView from "./components/SettingsView";
import { IconPlus } from "./components/Icons";
import { listTasks, onTaskEvent, type TaskSnapshot } from "./lib/api";

export default function App() {
  const [tasks, setTasks] = useState<TaskSnapshot[]>([]);
  const [view, setView] = useState<View>("active");
  const [modalOpen, setModalOpen] = useState(false);

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

  const visible = tasks.filter((t) =>
    view === "completed" ? t.status === "completed" : t.status !== "completed",
  );

  return (
    <div className="app-shell">
      <Sidebar view={view} onView={setView} tasks={tasks} />
      <main className="content">
        {view === "settings" ? (
          <SettingsView />
        ) : (
          <>
            <div className="content-head">
              <h1>{view === "completed" ? "已完成" : "下载中"}</h1>
              <button className="btn primary" onClick={() => setModalOpen(true)}>
                <IconPlus size={16} /> 新建下载
              </button>
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
