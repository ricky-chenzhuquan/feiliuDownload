import { useEffect, useState } from "react";
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

async function copyText(t: string) {
  try {
    await navigator.clipboard.writeText(t);
  } catch {
    /* 剪贴板不可用时忽略 */
  }
}

/** 实时速度 sparkline：60 点滑动窗口（约 30 秒） */
function SpeedSpark({ data }: { data: number[] }) {
  if (data.length < 2) return null;
  const w = 120;
  const h = 26;
  const max = Math.max(...data, 1);
  const pts = data
    .map((v, i) => {
      const x = (i / (data.length - 1)) * w;
      const y = h - 1 - (v / max) * (h - 3);
      return `${x.toFixed(1)},${y.toFixed(1)}`;
    })
    .join(" ");
  const last = data[data.length - 1];
  const area = `0,${h - 1} ${pts} ${w},${h - 1}`;
  return (
    <svg width={w} height={h} className="spark">
      <polygon points={area} fill="rgba(91,140,255,0.10)" />
      <polyline points={pts} fill="none" stroke="#5b8cff" strokeWidth="1.6" />
      <circle cx={w} cy={h - 1 - (last / max) * (h - 3)} r="2" fill="#22d3ee" />
    </svg>
  );
}

const STATUS_LABEL: Record<TaskSnapshot["status"], string> = {
  queued: "排队中",
  downloading: "下载中",
  paused: "已暂停",
  merging: "合并中",
  completed: "已完成",
  failed: "失败",
};

export default function TaskCard({
  task,
  samples,
}: {
  task: TaskSnapshot;
  samples: number[];
}) {
  const [confirmDel, setConfirmDel] = useState(false);
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);

  // 右键菜单打开时点击任意处关闭
  useEffect(() => {
    if (!menu) return;
    const close = () => setMenu(null);
    window.addEventListener("click", close);
    return () => window.removeEventListener("click", close);
  }, [menu]);

  const kind = kindOf(task.filename);
  const colors = KIND_COLORS[kind];
  const ext = task.filename.includes(".")
    ? task.filename.split(".").pop()!.slice(0, 4)
    : "?";

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
    <>
      <div
        className={`task-card status-${task.status}`}
        onContextMenu={(e) => {
          e.preventDefault();
          setMenu({ x: e.clientX, y: e.clientY });
        }}
      >
        <div className="task-icon" style={{ background: colors.bg, color: colors.fg }}>
          <span>{ext.toUpperCase()}</span>
        </div>

        <div className="task-main">
          <div className="task-title-row">
            <span className="task-name" title={task.save_path}>
              {task.filename}
            </span>
            {task.kind === "torrent" && <span className="kind-chip">BT</span>}
            <span className={`status-chip chip-${task.status}`}>
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
          {running && <SpeedSpark data={samples} />}
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

      {menu && (
        <div
          className="ctx-menu"
          style={{ left: menu.x, top: menu.y }}
          onClick={(e) => e.stopPropagation()}
        >
          <button onClick={() => void copyText(task.url)}>复制链接</button>
          <button onClick={() => void copyText(task.filename)}>复制文件名</button>
          {task.status === "completed" && (
            <button onClick={() => void revealPath(task.save_path).catch(() => {})}>
              打开所在文件夹
            </button>
          )}
        </div>
      )}
    </>
  );
}
