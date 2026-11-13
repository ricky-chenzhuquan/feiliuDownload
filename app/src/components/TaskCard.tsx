import { useState } from "react";
import type { TaskSnapshot } from "../lib/api";
import { pauseTask, resumeTask, deleteTask, revealPath } from "../lib/api";
import { formatBytes, formatEta, formatSpeed, kindOf, KIND_COLORS } from "../lib/format";
import {
  IconAlert,
  IconFolder,
  IconPause,
  IconPlay,
  IconTrash,
} from "./Icons";

const STATUS_LABEL: Record<TaskSnapshot["status"], string> = {
  queued: "排队中",
  downloading: "下载中",
  paused: "已暂停",
  merging: "合并中",
  completed: "已完成",
  failed: "失败",
};

export default function TaskCard({ task }: { task: TaskSnapshot }) {
  const [confirmDel, setConfirmDel] = useState(false);
  const kind = kindOf(task.filename);
  const colors = KIND_COLORS[kind];
  const ext = task.filename.includes(".") ? task.filename.split(".").pop()!.slice(0, 4) : "?";

  const percent =
    task.total_size > 0
      ? Math.min(100, (task.downloaded / task.total_size) * 100)
      : null;
  const running = task.status === "downloading";
  const indeterminate = task.total_size === 0 && running;

  const onToggle = async () => {
    if (task.status === "downloading" || task.status === "queued") {
      await pauseTask(task.id).catch(() => {});
    } else if (task.status !== "merging") {
      await resumeTask(task.id).catch(() => {});
    }
  };

  const onDelete = async () => {
    if (!confirmDel) {
      setConfirmDel(true);
      setTimeout(() => setConfirmDel(false), 2500);
      return;
    }
    const deleteFile = task.status === "completed";
    await deleteTask(task.id, deleteFile).catch(() => {});
  };

  return (
    <div className={`task-card status-${task.status}`}>
      <div className="task-icon" style={{ background: colors.bg, color: colors.fg }}>
        <span>{ext.toUpperCase()}</span>
      </div>

      <div className="task-main">
        <div className="task-title-row">
          <span className="task-name" title={task.save_path}>
            {task.filename}
          </span>
          <span
            className={`status-chip chip-${task.status}`}
          >
            {STATUS_LABEL[task.status]}
          </span>
        </div>

        <div className={`progress-track ${indeterminate ? "indet" : ""}`}>
          <div
            className="progress-fill"
            style={{ width: percent != null ? `${percent}%` : "100%" }}
          />
        </div>

        <div className="task-stats">
          {percent != null ? (
            <>
              <span className="pct">{percent.toFixed(1)}%</span>
              <span className="dot">·</span>
              <span>
                {formatBytes(task.downloaded)} / {formatBytes(task.total_size)}
              </span>
            </>
          ) : (
            <span>{formatBytes(task.downloaded)}</span>
          )}
          {running && (
            <>
              <span className="dot">·</span>
              <span className="speed">{formatSpeed(task.speed_bps)}</span>
            </>
          )}
          {running && task.eta_seconds != null && (
            <>
              <span className="dot">·</span>
              <span>剩余 {formatEta(task.eta_seconds)}</span>
            </>
          )}
          {running && task.total_size > 0 && (
            <>
              <span className="dot">·</span>
              <span className="conns">
                {task.connections} 连接
                {task.sources > 1 ? ` · ${task.sources} 源` : ""}
              </span>
            </>
          )}
          {task.error && (
            <span className="task-error" title={task.error}>
              <IconAlert size={13} /> {task.error}
            </span>
          )}
        </div>
      </div>

      <div className="task-actions">
        {task.status === "completed" && (
          <button
            className="icon-btn"
            title="打开所在文件夹"
            onClick={() => revealPath(task.save_path).catch(() => {})}
          >
            <IconFolder />
          </button>
        )}
        {(task.status === "downloading" ||
          task.status === "queued" ||
          task.status === "paused" ||
          task.status === "failed") && (
          <button
            className="icon-btn primary"
            title={task.status === "paused" || task.status === "failed" ? "继续下载" : "暂停"}
            onClick={onToggle}
          >
            {task.status === "paused" || task.status === "failed" ? (
              <IconPlay />
            ) : (
              <IconPause />
            )}
          </button>
        )}
        <button
          className={`icon-btn danger ${confirmDel ? "confirm" : ""}`}
          title={confirmDel ? "再次点击确认删除" : "删除任务"}
          onClick={onDelete}
        >
          <IconTrash />
        </button>
      </div>
    </div>
  );
}
