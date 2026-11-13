import type { TaskSnapshot } from "../lib/api";
import { formatSpeed } from "../lib/format";
import { IconBolt, IconCheck, IconDownload, IconSettings, Logo } from "./Icons";

export type View = "active" | "completed" | "settings";

interface Props {
  view: View;
  onView: (v: View) => void;
  tasks: TaskSnapshot[];
}

export default function Sidebar({ view, onView, tasks }: Props) {
  const activeCount = tasks.filter((t) => t.status !== "completed").length;
  const doneCount = tasks.filter((t) => t.status === "completed").length;
  const totalSpeed = tasks
    .filter((t) => t.status === "downloading")
    .reduce((s, t) => s + t.speed_bps, 0);

  return (
    <aside className="sidebar">
      <div className="brand">
        <Logo size={34} />
        <div className="brand-text">
          <div className="brand-name">飞流下载</div>
          <div className="brand-sub">FeiLiu Downloader</div>
        </div>
      </div>

      <nav className="nav">
        <button
          className={`nav-item ${view === "active" ? "active" : ""}`}
          onClick={() => onView("active")}
        >
          <IconDownload />
          <span>下载中</span>
          {activeCount > 0 && <span className="badge">{activeCount}</span>}
        </button>
        <button
          className={`nav-item ${view === "completed" ? "active" : ""}`}
          onClick={() => onView("completed")}
        >
          <IconCheck />
          <span>已完成</span>
          {doneCount > 0 && <span className="badge">{doneCount}</span>}
        </button>
        <button
          className={`nav-item ${view === "settings" ? "active" : ""}`}
          onClick={() => onView("settings")}
        >
          <IconSettings />
          <span>设置</span>
        </button>
      </nav>

      <div className="sidebar-footer">
        {totalSpeed > 0 && (
          <div className="speed-chip">
            <IconBolt size={14} />
            <span>{formatSpeed(totalSpeed)}</span>
          </div>
        )}
        <div className="version">v0.1.0</div>
      </div>
    </aside>
  );
}
