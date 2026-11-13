export function formatBytes(n: number, digits = 1): string {
  if (!Number.isFinite(n) || n < 0) return "--";
  if (n < 1024) return `${n} B`;
  const units = ["KB", "MB", "GB", "TB", "PB"];
  let v = n / 1024;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v.toFixed(digits)} ${units[i]}`;
}

export function formatSpeed(bps: number): string {
  if (!Number.isFinite(bps) || bps <= 0) return "0 KB/s";
  return `${formatBytes(bps)}/s`;
}

export function formatEta(seconds: number | null): string {
  if (seconds == null || !Number.isFinite(seconds) || seconds < 0) return "--";
  if (seconds < 60) return `${Math.ceil(seconds)} 秒`;
  const m = Math.floor(seconds / 60);
  if (m < 60) return `${m} 分 ${Math.floor(seconds % 60)} 秒`;
  const h = Math.floor(m / 60);
  return `${h} 时 ${m % 60} 分`;
}

export function extOf(filename: string): string {
  const i = filename.lastIndexOf(".");
  return i > 0 ? filename.slice(i + 1).toLowerCase() : "";
}

export type FileKind = "video" | "audio" | "image" | "archive" | "doc" | "other";

export function kindOf(filename: string): FileKind {
  const e = extOf(filename);
  if (["mp4", "mkv", "avi", "mov", "flv", "wmv", "webm", "ts", "m3u8", "rmvb"].includes(e))
    return "video";
  if (["mp3", "flac", "wav", "aac", "ogg", "m4a"].includes(e)) return "audio";
  if (["jpg", "jpeg", "png", "gif", "webp", "bmp", "svg", "heic"].includes(e)) return "image";
  if (["zip", "rar", "7z", "tar", "gz", "bz2", "xz", "dmg", "iso", "exe", "msi", "pkg"].includes(e))
    return "archive";
  if (["pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "txt", "epub"].includes(e))
    return "doc";
  return "other";
}

export const KIND_COLORS: Record<FileKind, { bg: string; fg: string; label: string }> = {
  video: { bg: "rgba(139,92,246,.18)", fg: "#b39dfc", label: "视频" },
  audio: { bg: "rgba(52,211,153,.18)", fg: "#6ee7b7", label: "音频" },
  image: { bg: "rgba(251,191,36,.18)", fg: "#fcd34d", label: "图片" },
  archive: { bg: "rgba(96,165,250,.18)", fg: "#93c5fd", label: "压缩包" },
  doc: { bg: "rgba(45,212,191,.18)", fg: "#7dd3fc", label: "文档" },
  other: { bg: "rgba(148,163,184,.16)", fg: "#cbd5e1", label: "文件" },
};

export function timeShort(ms: number): string {
  const d = new Date(ms);
  return `${d.getMonth() + 1}/${d.getDate()} ${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
}
